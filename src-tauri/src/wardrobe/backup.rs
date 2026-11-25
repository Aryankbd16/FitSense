use std::collections::BTreeSet;
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter, ErrorKind, Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU8, Ordering};

use hmac::{Hmac, Mac};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::security::{self, Secret, RECOVERY_LEN};

use super::database::{self, DATABASE, SCHEMA};
use super::service::RECOVERY;
use super::{files, media, Error, Result};

const MAGIC: &[u8; 8] = b"FSBACK01";
const MAX_SIZE: u64 = 8 * 1024 * 1024 * 1024;
const MAX_DATABASE: u64 = 1024 * 1024 * 1024;
const MAX_MANIFEST: usize = 3 * 1024 * 1024;
const MAX_FILES: usize = 30_001;
const TAG_SIZE: usize = 32;
pub const EXTENSION: &str = "fitbackup";

pub struct Operation<'a>(&'a AtomicU8);

impl<'a> Operation<'a> {
    pub fn start(state: &'a AtomicU8) -> Result<Self> {
        state
            .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| Error::Busy)?;
        Ok(Self(state))
    }

    pub fn check(&self) -> Result<()> {
        if self.0.load(Ordering::Acquire) == 2 {
            Err(Error::Cancelled)
        } else {
            Ok(())
        }
    }
}

impl Drop for Operation<'_> {
    fn drop(&mut self) {
        self.0.store(0, Ordering::Release);
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema: u32,
    created_at: i64,
    files: Vec<Entry>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    path: String,
    size: u64,
}

fn key_mac(master: &Secret) -> Result<Hmac<Sha256>> {
    Hmac::new_from_slice(security::derive(master, b"backup authentication")?.as_ref())
        .map_err(|_| Error::EncryptionUnavailable)
}

fn manifest_key(master: &Secret) -> Result<Secret> {
    security::derive(master, b"backup manifest")
}

fn write_mac(output: &mut impl Write, mac: &mut Hmac<Sha256>, bytes: &[u8]) -> Result<()> {
    output.write_all(bytes)?;
    mac.update(bytes);
    Ok(())
}

fn read_exact(input: &mut impl Read, buffer: &mut [u8]) -> Result<()> {
    input.read_exact(buffer).map_err(|err| {
        if err.kind() == ErrorKind::UnexpectedEof {
            Error::Integrity
        } else {
            err.into()
        }
    })
}

fn read_mac(input: &mut impl Read, mac: &mut Hmac<Sha256>, buffer: &mut [u8]) -> Result<()> {
    read_exact(input, buffer)?;
    mac.update(buffer);
    Ok(())
}

fn stream(
    input: &mut impl Read,
    output: &mut impl Write,
    size: u64,
    mac: &mut Hmac<Sha256>,
    operation: &Operation<'_>,
) -> Result<()> {
    let mut buffer = vec![0; 128 * 1024];
    let mut left = size;
    while left > 0 {
        operation.check()?;
        let length = left.min(buffer.len() as u64) as usize;
        let chunk = &mut buffer[..length];
        read_mac(input, mac, chunk)?;
        output.write_all(chunk)?;
        left -= length as u64;
    }
    Ok(())
}

impl Manifest {
    fn size(&self) -> Result<u64> {
        if self.schema == 0 || self.schema > SCHEMA {
            return Err(Error::Incompatible);
        }
        if self.files.len() > MAX_FILES
            || self
                .files
                .first()
                .is_none_or(|entry| entry.path != DATABASE)
        {
            return Err(Error::Integrity);
        }
        let mut seen = BTreeSet::new();
        let mut size = 0u64;
        for entry in &self.files {
            let limit = if entry.path == DATABASE {
                MAX_DATABASE
            } else if media::valid_path(&entry.path) {
                media::MAX_ENCRYPTED
            } else {
                return Err(Error::Integrity);
            };
            if entry.size == 0 || entry.size > limit || !seen.insert(&entry.path) {
                return Err(Error::Integrity);
            }
            size = size.checked_add(entry.size).ok_or(Error::Integrity)?;
            if size > MAX_SIZE {
                return Err(Error::Integrity);
            }
        }
        Ok(size)
    }
}

fn entry(root: &Path, path: String) -> Result<Entry> {
    let size = files::regular(&root.join(&path))?
        .ok_or(Error::ImageMissing)?
        .len();
    Ok(Entry { path, size })
}

pub fn export(
    root: &Path,
    target: &Path,
    db: &Connection,
    master: &Secret,
    media_key: &Secret,
    operation: &Operation<'_>,
) -> Result<()> {
    files::regular(target)?.map_or(Ok(()), |_| Err(Error::Exists))?;
    if [
        "wardrobe.sqlite3-journal",
        "wardrobe.sqlite3-wal",
        "wardrobe.sqlite3-shm",
    ]
    .iter()
    .any(|name| root.join(name).exists())
    {
        return Err(Error::Integrity);
    }
    database::verify_integrity(db)?;
    media::directories(root, false)?;
    let envelope = files::read(&root.join(RECOVERY), RECOVERY_LEN, Error::Integrity)?;
    security::verify_recovery(&envelope, master)?;
    let assets = database::all_assets(db)?;
    let mut entries = vec![entry(root, DATABASE.to_owned())?];
    for asset in &assets {
        if !media::valid_path(&asset.path) {
            return Err(Error::Integrity);
        }
        entries.push(entry(root, asset.path.clone())?);
    }
    let manifest = Manifest {
        schema: SCHEMA,
        created_at: database::now(),
        files: entries,
    };
    let payload_size = manifest.size()?;
    let encoded = serde_json::to_vec(&manifest).map_err(|_| Error::Integrity)?;
    let boxed = security::seal(&manifest_key(master)?, MAGIC, &encoded)?;
    let overhead = MAGIC.len() + RECOVERY_LEN + 4 + TAG_SIZE;
    if boxed.len() > MAX_MANIFEST || payload_size + (boxed.len() + overhead) as u64 > MAX_SIZE {
        return Err(Error::Integrity);
    }
    let mut temporary = tempfile::Builder::new()
        .prefix(".fitsense-backup-")
        .tempfile_in(target.parent().ok_or(Error::Io)?)?;
    let mut mac = key_mac(master)?;
    {
        let mut output = BufWriter::new(temporary.as_file_mut());
        for bytes in [
            MAGIC.as_slice(),
            &envelope,
            &(boxed.len() as u32).to_le_bytes(),
            &boxed,
        ] {
            write_mac(&mut output, &mut mac, bytes)?;
        }
        let mut db_file = File::open(root.join(DATABASE))?;
        stream(
            &mut db_file,
            &mut output,
            manifest.files[0].size,
            &mut mac,
            operation,
        )?;
        if db_file.read(&mut [0])? != 0 {
            return Err(Error::Integrity);
        }
        for (asset, entry) in assets.iter().zip(&manifest.files[1..]) {
            operation.check()?;
            let bytes = asset.verified_ciphertext(root, media_key)?;
            if bytes.len() as u64 != entry.size {
                return Err(Error::Integrity);
            }
            write_mac(&mut output, &mut mac, &bytes)?;
        }
        output.write_all(&mac.finalize().into_bytes())?;
        output.flush()?;
    }
    operation.check()?;
    files::publish_file(temporary, target, false)
}

pub fn extract(
    source: &Path,
    stage: &Path,
    passphrase: &str,
    operation: &Operation<'_>,
) -> Result<Secret> {
    let size = files::regular(source)?.ok_or(Error::NotFound)?.len();
    if size > MAX_SIZE {
        return Err(Error::Integrity);
    }
    let mut input = BufReader::new(File::open(source)?);
    let mut magic = [0; 8];
    read_exact(&mut input, &mut magic)?;
    if &magic != MAGIC {
        return Err(Error::Incompatible);
    }
    let mut envelope = [0; RECOVERY_LEN];
    read_exact(&mut input, &mut envelope)?;
    operation.check()?;
    let master = security::recover(&envelope, passphrase)?;
    let mut mac = key_mac(&master)?;
    mac.update(MAGIC);
    mac.update(&envelope);
    let mut length = [0; 4];
    read_mac(&mut input, &mut mac, &mut length)?;
    let length = u32::from_le_bytes(length) as usize;
    if !(40..=MAX_MANIFEST).contains(&length) {
        return Err(Error::Integrity);
    }
    let mut boxed = vec![0; length];
    read_mac(&mut input, &mut mac, &mut boxed)?;
    let encoded = security::unseal(&manifest_key(&master)?, MAGIC, &boxed)?;
    let manifest: Manifest = serde_json::from_slice(&encoded).map_err(|_| Error::Integrity)?;
    let expected = (MAGIC.len() + RECOVERY_LEN + 4 + length + TAG_SIZE) as u64 + manifest.size()?;
    if size != expected {
        return Err(Error::Integrity);
    }
    media::directories(stage, true)?;
    for entry in &manifest.files {
        operation.check()?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(stage.join(&entry.path))?;
        stream(&mut input, &mut output, entry.size, &mut mac, operation)?;
        output.sync_all()?;
    }
    let mut tag = [0; TAG_SIZE];
    read_exact(&mut input, &mut tag)?;
    mac.verify_slice(&tag).map_err(|_| Error::Integrity)?;
    if input.read(&mut [0])? != 0 {
        return Err(Error::Integrity);
    }

    let mut db = database::open(&stage.join(DATABASE), &master, false)?;
    let schema: u32 = db.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if schema != manifest.schema {
        return Err(Error::Integrity);
    }
    database::migrate(&mut db, 1)?;
    database::verify_integrity(&db)?;
    let assets = database::all_assets(&db)?;
    let expected_paths: BTreeSet<_> = manifest
        .files
        .iter()
        .skip(1)
        .map(|entry| &entry.path)
        .collect();
    let actual_paths: BTreeSet<_> = assets.iter().map(|asset| &asset.path).collect();
    if expected_paths != actual_paths {
        return Err(Error::Integrity);
    }
    let media_key = security::derive(&master, b"images")?;
    for asset in &assets {
        operation.check()?;
        asset.decrypt(stage, &media_key)?;
    }
    drop(db);
    files::write(&stage.join(RECOVERY), &envelope, false)?;
    Ok(master)
}
