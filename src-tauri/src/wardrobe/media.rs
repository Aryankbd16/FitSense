use std::collections::HashSet;
use std::fs;
use std::io::Cursor;
use std::path::Path;

use base64::{engine::general_purpose::STANDARD, Engine};
use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, GenericImageView, ImageDecoder, ImageFormat, ImageReader, Limits};
use rusqlite::{params, Connection, Row};
use sha2::{Digest, Sha256};

use crate::images::{reference, Variant};
use crate::security::{seal, unseal, Secret};

use super::{files, Error, Result};

pub const MAX_UPLOAD: usize = 12 * 1024 * 1024;
const MAX_DIMENSION: u32 = 6000;
const MAX_PIXELS: u64 = 20_000_000;
const VERSION: u32 = 1;
const MAGIC: &[u8; 8] = b"FSIMAGE1";
pub const MAX_ENCRYPTED: u64 = MAX_UPLOAD as u64 + 48;
pub const VARIANTS: [(Variant, &str); 3] = [
    (Variant::Original, "original"),
    (Variant::Display, "display"),
    (Variant::Thumbnail, "thumbnail"),
];

pub struct Asset {
    image: String,
    kind: String,
    pub path: String,
    mime: String,
    width: u32,
    height: u32,
    size: usize,
    hash: Vec<u8>,
    version: u32,
}

impl Asset {
    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            image: row.get(0)?,
            kind: row.get(1)?,
            path: row.get(2)?,
            mime: row.get(3)?,
            width: row.get(4)?,
            height: row.get(5)?,
            size: row.get::<_, u32>(6)? as usize,
            hash: row.get(7)?,
            version: row.get(8)?,
        })
    }

    fn validate(&self) -> Result<()> {
        if self.version != VERSION
            || self.size == 0
            || self.size > MAX_UPLOAD
            || !matches!(self.mime.as_str(), "image/jpeg" | "image/png")
            || (self.kind != "original" && self.mime != "image/jpeg")
            || !supported_dimensions(self.width, self.height)
            || self.hash.len() != 32
            || !VARIANTS.iter().any(|(variant, kind)| {
                *kind == self.kind && reference(*variant, &self.image).as_ref() == Some(&self.path)
            })
        {
            return Err(Error::Integrity);
        }
        Ok(())
    }

    fn aad(&self) -> Vec<u8> {
        [MAGIC.as_slice(), self.path.as_bytes()].concat()
    }

    pub fn decrypt(&self, root: &Path, key: &Secret) -> Result<Vec<u8>> {
        self.validate()?;
        let boxed = files::read(&root.join(&self.path), self.size + 48, Error::ImageMissing)?;
        self.check_ciphertext(&boxed, key)
    }

    pub fn verified_ciphertext(&self, root: &Path, key: &Secret) -> Result<Vec<u8>> {
        self.validate()?;
        let boxed = files::read(&root.join(&self.path), self.size + 48, Error::ImageMissing)?;
        self.check_ciphertext(&boxed, key)?;
        Ok(boxed)
    }

    fn check_ciphertext(&self, boxed: &[u8], key: &Secret) -> Result<Vec<u8>> {
        let ciphertext = boxed
            .strip_prefix(MAGIC.as_slice())
            .ok_or(Error::Integrity)?;
        let plaintext = unseal(key, &self.aad(), ciphertext)?;
        if plaintext.len() != self.size || Sha256::digest(&plaintext).as_slice() != self.hash {
            return Err(Error::Integrity);
        }
        Ok(plaintext)
    }
}

pub fn valid_path(path: &str) -> bool {
    path.rsplit_once('/').is_some_and(|(_, id)| {
        VARIANTS
            .iter()
            .any(|(variant, _)| reference(*variant, id).as_deref() == Some(path))
    })
}

pub fn prepare(encoded: &str, id: &str) -> Result<Prepared> {
    if encoded.is_empty() || encoded.len() > MAX_UPLOAD.div_ceil(3) * 4 {
        return Err(Error::InvalidImage);
    }
    let bytes = STANDARD.decode(encoded).map_err(|_| Error::InvalidImage)?;
    prepare_bytes(bytes, id)
}

fn supported_dimensions(width: u32, height: u32) -> bool {
    width > 0
        && height > 0
        && width <= MAX_DIMENSION
        && height <= MAX_DIMENSION
        && u64::from(width) * u64::from(height) <= MAX_PIXELS
}

pub struct Prepared(Vec<(Asset, Vec<u8>)>);

fn prepare_bytes(bytes: Vec<u8>, id: &str) -> Result<Prepared> {
    if bytes.is_empty() || bytes.len() > MAX_UPLOAD {
        return Err(Error::InvalidImage);
    }
    let format = image::guess_format(&bytes).map_err(|_| Error::InvalidImage)?;
    let mime = match format {
        ImageFormat::Jpeg => "image/jpeg",
        ImageFormat::Png => "image/png",
        _ => return Err(Error::InvalidImage),
    };
    let mut reader = ImageReader::with_format(Cursor::new(&bytes), format);
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    limits.max_alloc = Some(96 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader.into_decoder().map_err(|_| Error::InvalidImage)?;
    let (width, height) = decoder.dimensions();
    if !supported_dimensions(width, height) {
        return Err(Error::InvalidImage);
    }
    let orientation = decoder.orientation().map_err(|_| Error::InvalidImage)?;
    let mut image = DynamicImage::from_decoder(decoder).map_err(|_| Error::InvalidImage)?;
    image.apply_orientation(orientation);
    let derived =
        [(1200, 86), (320, 80)].map(|(edge, quality)| jpeg_variant(&image, edge, quality));
    let [display, thumbnail] = derived;
    let files = [(bytes, width, height, mime), display?, thumbnail?];
    VARIANTS
        .into_iter()
        .zip(files)
        .map(|((variant, kind), (bytes, width, height, mime))| {
            let path = reference(variant, id).ok_or(Error::InvalidInput)?;
            Ok((
                Asset {
                    image: id.to_owned(),
                    kind: kind.to_owned(),
                    path,
                    mime: mime.to_owned(),
                    width,
                    height,
                    size: bytes.len(),
                    hash: Sha256::digest(&bytes).to_vec(),
                    version: VERSION,
                },
                bytes,
            ))
        })
        .collect::<Result<Vec<_>>>()
        .map(Prepared)
}

fn jpeg_variant(
    image: &DynamicImage,
    edge: u32,
    quality: u8,
) -> Result<(Vec<u8>, u32, u32, &'static str)> {
    let resized = image.thumbnail(edge, edge);
    let (width, height) = resized.dimensions();
    let mut bytes = Vec::new();
    JpegEncoder::new_with_quality(&mut bytes, quality)
        .encode_image(&resized.to_rgb8())
        .map_err(|_| Error::InvalidImage)?;
    Ok((bytes, width, height, "image/jpeg"))
}

impl Prepared {
    pub fn write(&self, root: &Path, key: &Secret) -> Result<()> {
        for (asset, bytes) in &self.0 {
            let encrypted = seal(key, &asset.aad(), bytes)?;
            let boxed = [MAGIC.as_slice(), &encrypted].concat();
            files::write(&root.join(&asset.path), &boxed, false)?;
        }
        Ok(())
    }

    pub fn link(&self, db: &Connection, garment: &str, image: &str) -> Result<()> {
        db.execute(
            "INSERT INTO garment_images VALUES (?1, ?2)",
            [image, garment],
        )?;
        for (asset, _) in &self.0 {
            db.execute(
                "INSERT INTO image_assets VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                params![
                    asset.image,
                    asset.kind,
                    asset.path,
                    asset.mime,
                    asset.width,
                    asset.height,
                    asset.size as u32,
                    asset.hash,
                    asset.version,
                ],
            )?;
        }
        Ok(())
    }
}

pub fn directories(root: &Path, create: bool) -> Result<()> {
    let paths = ["images"]
        .into_iter()
        .chain(VARIANTS.map(|(variant, _)| variant.directory()));
    for directory in paths {
        let path = root.join(directory);
        if create {
            files::ensure_directory(&path)?;
        } else if !files::directory(&path)? {
            return Err(Error::Integrity);
        }
    }
    Ok(())
}

pub fn collect_unreferenced(root: &Path, assets: &[Asset]) -> Result<()> {
    directories(root, false)?;
    let mut keep = HashSet::new();
    for asset in assets {
        asset.validate()?;
        keep.insert(asset.path.as_str());
    }
    for (variant, _) in VARIANTS {
        for item in fs::read_dir(root.join(variant.directory()))? {
            let item = item?;
            let name = item.file_name();
            let remove = name.to_str().is_some_and(|name| {
                reference(variant, name).is_some_and(|path| !keep.contains(path.as_str()))
                    || name.strip_prefix(".fitsense-write-").is_some_and(|suffix| {
                        suffix.len() == 6 && suffix.bytes().all(|b| b.is_ascii_alphanumeric())
                    })
            });
            if remove && files::regular(&item.path())?.is_some() {
                fs::remove_file(item.path())?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
pub fn sample_png() -> Vec<u8> {
    use image::{ImageEncoder, RgbImage};
    let image = RgbImage::from_pixel(360, 240, image::Rgb([68, 87, 101]));
    let mut bytes = Vec::new();
    image::codecs::png::PngEncoder::new(&mut bytes)
        .write_image(&image, 360, 240, image::ExtendedColorType::Rgb8)
        .unwrap();
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn validates_preserves_and_downsizes() {
        let upload = sample_png();
        let images = prepare_bytes(upload.clone(), ID).unwrap();
        assert_eq!(images.0[0].1, upload);
        assert_eq!((images.0[2].0.width, images.0[2].0.height), (320, 213));
        assert!(prepare_bytes(vec![0; MAX_UPLOAD + 1], ID).is_err());
        assert!(prepare_bytes(b"JPEG".to_vec(), ID).is_err());
        assert!(!supported_dimensions(6000, 6000));
    }

    #[test]
    fn detects_missing_tampered_and_incorrectly_hashed_ciphertext() {
        let workspace = tempfile::tempdir().unwrap();
        let mut images = prepare_bytes(sample_png(), ID).unwrap();
        let secret = zeroize::Zeroizing::new(crate::security::random().unwrap());
        let root = workspace.path();
        directories(root, true).unwrap();
        let asset = &images.0[0].0;
        assert!(matches!(
            asset.decrypt(root, &secret),
            Err(Error::ImageMissing)
        ));
        images.write(root, &secret).unwrap();
        assert_eq!(asset.decrypt(root, &secret).unwrap(), sample_png());
        let path = root.join(&asset.path);
        let mut ciphertext = fs::read(&path).unwrap();
        assert!(!ciphertext.starts_with(b"\x89PNG"));
        ciphertext[28] ^= 1;
        fs::write(path, ciphertext).unwrap();
        assert!(matches!(
            asset.decrypt(root, &secret),
            Err(Error::Integrity)
        ));
        images.0[1].0.hash[0] ^= 1;
        assert!(matches!(
            images.0[1].0.decrypt(root, &secret),
            Err(Error::Integrity)
        ));
    }
}
