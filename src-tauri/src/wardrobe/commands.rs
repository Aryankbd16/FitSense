use std::path::PathBuf;
use std::sync::Arc;

use tauri::ipc::Response;
use tauri::State;
use zeroize::Zeroizing;

use crate::diagnostics::{Diagnostics, Event};
use crate::security;

use super::backup::EXTENSION;
use super::model::{Category, Garment, GarmentInput, Page, PhotoChange, Query, CATEGORIES};
use super::service::{State as VaultState, Vault};
use super::{files, Error, Result};

pub struct Context {
    vault: Arc<Vault>,
    logs: Arc<Diagnostics>,
}

impl Context {
    pub fn new(root: PathBuf, foundation: Option<u32>, logs: Arc<Diagnostics>) -> Self {
        Self {
            vault: Arc::new(Vault::new(root, foundation)),
            logs,
        }
    }

    pub fn locked(&self) -> bool {
        !self.vault.is_open()
    }
}

async fn run<T, F>(context: &Context, run: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce(&Vault) -> Result<T> + Send + 'static,
{
    let vault = Arc::clone(&context.vault);
    let logs = Arc::clone(&context.logs);
    tauri::async_runtime::spawn_blocking(move || {
        let result = run(&vault);
        let event = match result {
            Err(Error::Integrity | Error::ImageMissing | Error::Incompatible) => {
                Some(Event::VaultIntegrityError)
            }
            Err(Error::Io | Error::Database | Error::EncryptionUnavailable | Error::Cleanup) => {
                Some(Event::VaultOperationError)
            }
            _ => None,
        };
        if let Some(event) = event {
            logs.record(event, None);
        }
        result
    })
    .await
    .map_err(|_| Error::Io)?
}

#[tauri::command]
pub fn wardrobe_categories() -> &'static [Category] {
    CATEGORIES
}

#[tauri::command]
pub async fn wardrobe_state(context: State<'_, Context>) -> Result<VaultState> {
    run(&context, Vault::status).await
}

#[tauri::command]
pub async fn wardrobe_setup(context: State<'_, Context>, passphrase: String) -> Result<VaultState> {
    let passphrase = Zeroizing::new(passphrase);
    run(&context, move |vault| vault.setup(&passphrase)).await
}

#[tauri::command]
pub async fn wardrobe_unlock(
    context: State<'_, Context>,
    passphrase: String,
) -> Result<VaultState> {
    let passphrase = Zeroizing::new(passphrase);
    run(&context, move |vault| vault.unlock(&passphrase)).await
}

#[tauri::command]
pub async fn wardrobe_change_passphrase(
    context: State<'_, Context>,
    current: String,
    replacement: String,
) -> Result<()> {
    let current = Zeroizing::new(current);
    let replacement = Zeroizing::new(replacement);
    run(&context, move |vault| {
        vault.change_passphrase(&current, &replacement)
    })
    .await
}

#[tauri::command]
pub async fn wardrobe_list(context: State<'_, Context>, query: Query) -> Result<Page> {
    run(&context, move |vault| vault.list(query)).await
}

#[tauri::command]
pub async fn wardrobe_detail(context: State<'_, Context>, id: String) -> Result<Garment> {
    run(&context, move |vault| vault.detail(&id)).await
}

#[tauri::command]
pub async fn wardrobe_save(
    context: State<'_, Context>,
    id: Option<String>,
    input: GarmentInput,
    photo: PhotoChange,
) -> Result<Garment> {
    run(&context, move |vault| vault.save(id, input, photo)).await
}

#[tauri::command]
pub async fn wardrobe_delete(context: State<'_, Context>, id: String) -> Result<()> {
    run(&context, move |vault| vault.delete(&id)).await
}

#[tauri::command]
pub async fn wardrobe_image(
    context: State<'_, Context>,
    id: String,
    thumbnail: bool,
) -> Result<Response> {
    run(&context, move |vault| vault.image(&id, thumbnail))
        .await
        .map(Response::new)
}

fn backup_name(name: &str) -> bool {
    name.strip_suffix(".fitbackup").is_some_and(|base| {
        !base.is_empty()
            && base.len() <= 90
            && base
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    })
}

#[tauri::command]
pub async fn wardrobe_backups(context: State<'_, Context>) -> Result<Vec<String>> {
    run(&context, |vault| {
        let mut names = Vec::new();
        for entry in std::fs::read_dir(vault.backup_directory()?)? {
            let entry = entry?;
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            if backup_name(&name) && entry.file_type()?.is_file() {
                names.push(name);
            }
        }
        names.sort_unstable_by(|a, b| b.cmp(a));
        names.truncate(100);
        Ok(names)
    })
    .await
}

#[tauri::command]
pub async fn wardrobe_create_backup(
    context: State<'_, Context>,
    choose_location: bool,
) -> Result<String> {
    run(&context, move |vault| {
        let directory = vault.backup_directory()?;
        let suffix = security::hex(&security::random::<4>()?);
        let filename = format!("FitSense-{}-{suffix}.{EXTENSION}", super::database::now());
        let path = if choose_location {
            #[cfg(windows)]
            {
                let path = rfd::FileDialog::new()
                    .set_title("Export encrypted wardrobe backup")
                    .add_filter("FitSense backup", &[EXTENSION])
                    .set_directory(&directory)
                    .set_file_name(&filename)
                    .save_file()
                    .ok_or(Error::Cancelled)?;
                if path
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case(EXTENSION))
                {
                    path
                } else {
                    path.with_extension(EXTENSION)
                }
            }
            #[cfg(not(windows))]
            {
                return Err(Error::EncryptionUnavailable);
            }
        } else {
            directory.join(filename)
        };
        files::regular(&path)?;
        vault.export(&path)?;
        Ok(path
            .file_name()
            .ok_or(Error::Io)?
            .to_string_lossy()
            .into_owned())
    })
    .await
}

#[tauri::command]
pub async fn wardrobe_restore(
    context: State<'_, Context>,
    passphrase: String,
    name: Option<String>,
) -> Result<VaultState> {
    let passphrase = Zeroizing::new(passphrase);
    run(&context, move |vault| {
        let directory = vault.backup_directory()?;
        let path = if let Some(name) = name {
            if !backup_name(&name) {
                return Err(Error::InvalidInput);
            }
            directory.join(name)
        } else {
            #[cfg(windows)]
            {
                rfd::FileDialog::new()
                    .set_title("Choose an encrypted wardrobe backup")
                    .add_filter("FitSense backup", &[EXTENSION])
                    .set_directory(directory)
                    .pick_file()
                    .ok_or(Error::Cancelled)?
            }
            #[cfg(not(windows))]
            {
                return Err(Error::EncryptionUnavailable);
            }
        };
        vault.restore(&path, &passphrase)
    })
    .await
}

#[tauri::command]
pub fn wardrobe_cancel_backup(context: State<'_, Context>) {
    context.vault.cancel_backup();
}
