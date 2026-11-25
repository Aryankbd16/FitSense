use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::path::Path;

use tempfile::NamedTempFile;

use super::{Error, Result};

fn redirected(metadata: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

pub fn directory(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !redirected(&metadata) => Ok(true),
        Ok(_) => Err(Error::Integrity),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

pub fn ensure_directory(path: &Path) -> Result<()> {
    if !directory(path)? {
        match fs::create_dir(path) {
            Ok(()) => (),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => (),
            Err(error) => return Err(error.into()),
        }
    }
    if directory(path)? {
        Ok(())
    } else {
        Err(Error::Io)
    }
}

pub fn regular(path: &Path) -> Result<Option<Metadata>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() && !redirected(&metadata) => Ok(Some(metadata)),
        Ok(_) => Err(Error::Integrity),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

pub fn lease(path: &Path) -> Result<File> {
    regular(path)?;
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT;
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let lock = options.open(path)?;
    if redirected(&lock.metadata()?) {
        return Err(Error::Integrity);
    }
    lock.try_lock().map_err(|_| Error::Busy)?;
    Ok(lock)
}

pub fn read(path: &Path, max: usize, missing: Error) -> Result<Vec<u8>> {
    if regular(path)?.ok_or(missing)?.len() > max as u64 {
        return Err(Error::Integrity);
    }
    let mut bytes = Vec::new();
    File::open(path)?
        .take(max as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > max {
        return Err(Error::Integrity);
    }
    Ok(bytes)
}

pub fn publish_file(file: NamedTempFile, path: &Path, replace: bool) -> Result<()> {
    file.as_file().sync_all()?;
    let temporary = file.into_temp_path();
    move_path(&temporary, path, replace)
}

pub fn move_path(source: &Path, target: &Path, replace: bool) -> Result<()> {
    #[cfg(windows)]
    let result = {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{
            MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
        };
        let wide = |path: &Path| {
            path.as_os_str()
                .encode_wide()
                .chain(std::iter::once(0))
                .collect::<Vec<_>>()
        };
        let mut flags = MOVEFILE_WRITE_THROUGH;
        if replace {
            flags |= MOVEFILE_REPLACE_EXISTING;
        }
        if unsafe { MoveFileExW(wide(source).as_ptr(), wide(target).as_ptr(), flags) } == 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(())
        }
    };
    #[cfg(not(windows))]
    let result = if !replace && target.exists() {
        Err(ErrorKind::AlreadyExists.into())
    } else {
        fs::rename(source, target)
    };
    result.map_err(|error| {
        if error.kind() == ErrorKind::AlreadyExists {
            Error::Exists
        } else {
            Error::Io
        }
    })
}

pub fn write(path: &Path, bytes: &[u8], replace: bool) -> Result<()> {
    regular(path)?;
    let mut temp = tempfile::Builder::new()
        .prefix(".fitsense-write-")
        .tempfile_in(path.parent().ok_or(Error::Io)?)?;
    temp.write_all(bytes)?;
    publish_file(temp, path, replace)
}
