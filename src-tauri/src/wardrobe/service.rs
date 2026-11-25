use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Mutex, MutexGuard};

use rusqlite::{Connection, TransactionBehavior};
use serde::Serialize;
use tempfile::TempDir;
use zeroize::Zeroizing;

use crate::security::{self, Secret, RECOVERY_LEN};

use super::database::{self, DATABASE};
use super::model::{Garment, GarmentInput, Page, PhotoChange, Query};
use super::{backup, files, media, Error, Result};

pub const RECOVERY: &str = "recovery.key";
const LOCAL: &str = "local.key";
const STAGE: &str = ".fitsense-stage-";

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum State {
    Setup,
    Recovery,
    Open {
        #[serde(rename = "cleanupPending")]
        cleanup_pending: bool,
    },
}

pub struct Vault {
    workspace: PathBuf,
    foundation: Option<u32>,
    state: Mutex<Runtime>,
    open: AtomicBool,
    backup_state: AtomicU8,
}

#[derive(Default)]
struct Runtime {
    lease: Option<File>,
    attempted_local: bool,
    store: Option<Store>,
}

struct Store {
    db: Connection,
    master: Secret,
    media_key: Secret,
    cleanup_pending: bool,
}

impl Store {
    fn open(root: &Path, master: Secret, phase: u32) -> Result<Self> {
        media::directories(root, false)?;
        let mut db = database::open(&root.join(DATABASE), &master, false)?;
        database::migrate(&mut db, phase)?;
        let media_key = security::derive(&master, b"images")?;
        let mut store = Self {
            db,
            master,
            media_key,
            cleanup_pending: false,
        };
        store.collect(root);
        Ok(store)
    }

    fn collect(&mut self, root: &Path) {
        self.cleanup_pending = database::all_assets(&self.db)
            .and_then(|assets| media::collect_unreferenced(root, &assets))
            .is_err();
    }
}

impl Vault {
    pub fn new(workspace: PathBuf, foundation: Option<u32>) -> Self {
        Self {
            workspace,
            foundation,
            state: Mutex::new(Runtime::default()),
            open: AtomicBool::new(false),
            backup_state: AtomicU8::new(0),
        }
    }

    pub fn is_open(&self) -> bool {
        self.open.load(Ordering::Acquire)
    }

    fn base(&self) -> PathBuf {
        self.workspace.join("private")
    }
    fn root(&self) -> PathBuf {
        self.base().join("vault")
    }

    pub fn backup_directory(&self) -> Result<PathBuf> {
        let directory = self.workspace.join("backups");
        if files::directory(&directory)? {
            Ok(directory)
        } else {
            Err(Error::Io)
        }
    }

    fn runtime(&self) -> Result<MutexGuard<'_, Runtime>> {
        let mut runtime = self.state.lock().map_err(|_| Error::Busy)?;
        if runtime.lease.is_none() {
            if self.foundation != Some(1) {
                return Err(Error::EncryptionUnavailable);
            }
            let dir = self.base();
            files::ensure_directory(&dir)?;
            let lock = files::lease(&dir.join("vault.lock"))?;
            // Staging folders contain ciphertext only and can be discarded after a crash.
            for entry in fs::read_dir(&dir)? {
                let entry = entry?;
                if entry.file_name().to_str().is_some_and(|name| {
                    name.strip_prefix(STAGE).is_some_and(|suffix| {
                        suffix.len() == 6 && suffix.bytes().all(|b| b.is_ascii_alphanumeric())
                    })
                }) && files::directory(&entry.path())?
                {
                    fs::remove_dir_all(entry.path()).map_err(|_| Error::Cleanup)?;
                }
            }
            runtime.lease = Some(lock);
        }
        Ok(runtime)
    }

    fn stage(&self) -> Result<TempDir> {
        Ok(tempfile::Builder::new()
            .prefix(STAGE)
            .tempdir_in(self.base())?)
    }

    fn publish(&self, stage: TempDir) -> Result<()> {
        if files::directory(&self.root())? {
            return Err(Error::Exists);
        }
        files::move_path(stage.path(), &self.root(), false)
    }

    fn activate(&self, runtime: &mut Runtime, store: Store) -> State {
        let cleanup_pending = store.cleanup_pending;
        runtime.store = Some(store);
        self.open.store(true, Ordering::Release);
        State::Open { cleanup_pending }
    }

    pub fn status(&self) -> Result<State> {
        let mut runtime = self.runtime()?;
        if let Some(store) = &runtime.store {
            return Ok(State::Open {
                cleanup_pending: store.cleanup_pending,
            });
        }
        let root = self.root();
        if !files::directory(&root)? {
            database::verify_provider()?;
            return Ok(State::Setup);
        }
        if !runtime.attempted_local {
            runtime.attempted_local = true;
            match files::read(&root.join(LOCAL), 16 * 1024, Error::LocalKey)
                .and_then(|wrapped| security::unprotect_local(&wrapped))
            {
                Ok(master) => match Store::open(&root, master, 1) {
                    Ok(store) => return Ok(self.activate(&mut runtime, store)),
                    // A copied vault can hold a stale but locally decodable wrapper.
                    Err(Error::Integrity) => (),
                    Err(error) => return Err(error),
                },
                Err(Error::LocalKey) => (),
                Err(error) => return Err(error),
            }
        }
        Ok(State::Recovery)
    }

    pub fn setup(&self, passphrase: &str) -> Result<State> {
        let mut runtime = self.runtime()?;
        if files::directory(&self.root())? {
            return Err(Error::Exists);
        }
        database::verify_provider()?;
        let master = Zeroizing::new(security::random()?);
        let envelope = security::recovery(&master, passphrase)?;
        let protected = security::protect_local(&master)?;
        let stage = self.stage()?;
        media::directories(stage.path(), true)?;
        let mut db = database::open(&stage.path().join(DATABASE), &master, true)?;
        database::migrate(&mut db, 1)?;
        database::verify_integrity(&db)?;
        drop(db);
        files::write(&stage.path().join(RECOVERY), &envelope, false)?;
        files::write(&stage.path().join(LOCAL), &protected, false)?;
        self.publish(stage)?;
        let store = Store::open(&self.root(), master, 1)?;
        Ok(self.activate(&mut runtime, store))
    }

    pub fn unlock(&self, passphrase: &str) -> Result<State> {
        let mut runtime = self.runtime()?;
        let root = self.root();
        if !files::directory(&root)? {
            return Err(Error::Locked);
        }
        let envelope = files::read(&root.join(RECOVERY), RECOVERY_LEN, Error::Integrity)?;
        let master = security::recover(&envelope, passphrase)?;
        let store = Store::open(&root, master, 1)?;
        files::write(
            &root.join(LOCAL),
            &security::protect_local(&store.master)?,
            true,
        )?;
        Ok(self.activate(&mut runtime, store))
    }

    pub fn change_passphrase(&self, current: &str, new: &str) -> Result<()> {
        let runtime = self.runtime()?;
        let store = runtime.store.as_ref().ok_or(Error::Locked)?;
        let path = self.root().join(RECOVERY);
        let envelope = files::read(&path, RECOVERY_LEN, Error::Integrity)?;
        security::verify_recovery(&envelope, &store.master)?;
        security::recover(&envelope, current)?;
        files::write(&path, &security::recovery(&store.master, new)?, true)
    }

    pub fn list(&self, query: Query) -> Result<Page> {
        let runtime = self.runtime()?;
        database::list(&runtime.store.as_ref().ok_or(Error::Locked)?.db, query)
    }

    pub fn detail(&self, id: &str) -> Result<Garment> {
        let runtime = self.runtime()?;
        database::detail(&runtime.store.as_ref().ok_or(Error::Locked)?.db, id)
    }

    pub fn save(
        &self,
        id: Option<String>,
        input: GarmentInput,
        photo: PhotoChange,
    ) -> Result<Garment> {
        let mut runtime = self.runtime()?;
        let store = runtime.store.as_mut().ok_or(Error::Locked)?;
        let input = input.validated()?;
        let root = self.root();
        media::directories(&root, false)?;
        let is_new = id.is_none();
        let id = match id {
            Some(id) => id,
            None => security::hex(&security::random::<16>()?),
        };
        let operation = (|| {
            let tx = store
                .db
                .transaction_with_behavior(TransactionBehavior::Immediate)?;
            database::write_garment(&tx, &id, &input, is_new)?;
            if !matches!(&photo, PhotoChange::Keep) {
                tx.execute("DELETE FROM garment_images WHERE garment_id=?1", [&id])?;
            }
            if let PhotoChange::Upload(encoded) = photo {
                let image_id = security::hex(&security::random::<16>()?);
                let prepared = media::prepare(&encoded, &image_id)?;
                prepared.write(&root, &store.media_key)?;
                prepared.link(&tx, &id, &image_id)?;
            }
            let garment = database::detail(&tx, &id)?;
            tx.commit()?;
            Ok(garment)
        })();
        store.collect(&root);
        operation
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        let mut runtime = self.runtime()?;
        let store = runtime.store.as_mut().ok_or(Error::Locked)?;
        let tx = store
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if tx.execute("DELETE FROM garments WHERE id=?1", [id])? != 1 {
            return Err(Error::NotFound);
        }
        tx.commit()?;
        store.collect(&self.root());
        Ok(())
    }

    pub fn image(&self, id: &str, thumbnail: bool) -> Result<Vec<u8>> {
        let runtime = self.runtime()?;
        let store = runtime.store.as_ref().ok_or(Error::Locked)?;
        let root = self.root();
        media::directories(&root, false)?;
        let kind = if thumbnail { "thumbnail" } else { "display" };
        database::garment_asset(&store.db, id, kind)?.decrypt(&root, &store.media_key)
    }

    pub fn cancel_backup(&self) {
        let _ = self
            .backup_state
            .compare_exchange(1, 2, Ordering::AcqRel, Ordering::Acquire);
    }

    fn backup_operation(&self) -> Result<backup::Operation<'_>> {
        backup::Operation::start(&self.backup_state)
    }

    pub fn export(&self, target: &Path) -> Result<()> {
        let operation = self.backup_operation()?;
        let runtime = self.runtime()?;
        let store = runtime.store.as_ref().ok_or(Error::Locked)?;
        backup::export(
            &self.root(),
            target,
            &store.db,
            &store.master,
            &store.media_key,
            &operation,
        )
    }

    pub fn restore(&self, source: &Path, passphrase: &str) -> Result<State> {
        let operation = self.backup_operation()?;
        let mut runtime = self.runtime()?;
        if files::directory(&self.root())? {
            return Err(Error::Exists);
        }
        database::verify_provider()?;
        let stage = self.stage()?;
        let master = backup::extract(source, stage.path(), passphrase, &operation)?;
        files::write(
            &stage.path().join(LOCAL),
            &security::protect_local(&master)?,
            false,
        )?;
        operation.check()?;
        self.publish(stage)?;
        let store = Store::open(&self.root(), master, 1)?;
        Ok(self.activate(&mut runtime, store))
    }
}

#[cfg(all(test, windows))]
mod tests {
    use std::sync::Arc;

    use base64::{engine::general_purpose::STANDARD, Engine};
    use tempfile::TempDir;

    use super::*;

    const PASS: &str = "orchard rain coastal trail";
    const NEXT: &str = "separate orchard picture passphrase";

    fn fixture() -> (TempDir, Vault) {
        let dir = tempfile::tempdir().unwrap();
        let storage = crate::storage::Storage {
            root: dir.path().into(),
            mode: crate::storage::Mode::Portable,
        };
        storage.initialize_directories().unwrap();
        let version = storage.initialize_database().unwrap();
        (dir, Vault::new(storage.root, Some(version)))
    }

    fn garment(name: &str) -> GarmentInput {
        GarmentInput {
            name: name.into(),
            category: "casual-shirts".into(),
            primary_colour: "Indigo".into(),
            notes: "private wardrobe details".into(),
            seasons: vec!["Autumn".into()],
            ..Default::default()
        }
    }

    fn photo() -> PhotoChange {
        PhotoChange::Upload(STANDARD.encode(media::sample_png()))
    }

    #[test]
    fn encrypted_crud_integrity_and_rollback() {
        let (dir, vault) = fixture();
        assert_eq!(vault.status().unwrap(), State::Setup);
        assert!(matches!(
            vault.save(None, garment("Before setup"), PhotoChange::Keep),
            Err(Error::Locked)
        ));
        assert!(matches!(
            Vault::new(dir.path().into(), Some(1)).status(),
            Err(Error::Busy)
        ));
        assert!(matches!(vault.setup(PASS).unwrap(), State::Open { .. }));
        let item = vault.save(None, garment("Indigo shirt"), photo()).unwrap();
        let display = vault.image(&item.id, false).unwrap();
        assert!(display.starts_with(b"\xff\xd8"));
        let result = vault
            .list(Query {
                search: "indigo".into(),
                category: "casual-shirts".into(),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(result.garments[0].id, item.id);
        assert!(vault
            .list(Query {
                group: "Accessories".into(),
                ..Default::default()
            })
            .unwrap()
            .garments
            .is_empty());
        assert!(matches!(
            vault.save(
                Some(item.id.clone()),
                garment("Should roll back"),
                PhotoChange::Upload("broken".into())
            ),
            Err(Error::InvalidImage)
        ));
        assert_eq!(vault.detail(&item.id).unwrap().details.name, "Indigo shirt");
        assert_eq!(vault.image(&item.id, false).unwrap(), display);
        let updated = vault
            .save(Some(item.id), garment("Updated shirt"), photo())
            .unwrap();
        assert_eq!(updated.added, item.added);
        assert_eq!(
            vault.detail(&updated.id).unwrap().details.name,
            "Updated shirt"
        );
        for (variant, _) in media::VARIANTS {
            assert_eq!(
                fs::read_dir(vault.root().join(variant.directory()))
                    .unwrap()
                    .count(),
                1
            );
        }
        let database = fs::read(vault.root().join(DATABASE)).unwrap();
        assert!(!database
            .windows(b"Updated shirt".len())
            .any(|part| part == b"Updated shirt"));
        let technical = Connection::open(dir.path().join("db/foundation.sqlite3")).unwrap();
        assert_eq!(
            technical
                .pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))
                .unwrap(),
            1
        );
        vault.delete(&updated.id).unwrap();
        assert!(matches!(vault.detail(&updated.id), Err(Error::NotFound)));
        for (variant, _) in media::VARIANTS {
            assert_eq!(
                fs::read_dir(vault.root().join(variant.directory()))
                    .unwrap()
                    .count(),
                0
            );
        }
    }

    #[test]
    fn portable_copy_recovers_without_source_dpapi_and_rewraps() {
        let (source_dir, source) = fixture();
        source.setup(PASS).unwrap();
        let item = source
            .save(None, garment("Copied jacket"), photo())
            .unwrap();
        let root = source.root();
        let originals = source.image(&item.id, false).unwrap();
        drop(source);

        let (dest_dir, dest) = fixture();
        files::ensure_directory(&dest.base()).unwrap();
        files::ensure_directory(&dest.root()).unwrap();
        media::directories(&dest.root(), true).unwrap();
        for name in [DATABASE, RECOVERY] {
            fs::copy(root.join(name), dest.root().join(name)).unwrap();
        }
        for (variant, _) in media::VARIANTS {
            for file in fs::read_dir(root.join(variant.directory())).unwrap() {
                let file = file.unwrap();
                fs::copy(
                    file.path(),
                    dest.root().join(variant.directory()).join(file.file_name()),
                )
                .unwrap();
            }
        }
        assert_eq!(dest.status().unwrap(), State::Recovery);
        assert!(matches!(dest.unlock(NEXT), Err(Error::WrongPassphrase)));
        dest.unlock(PASS).unwrap();
        assert_eq!(dest.image(&item.id, false).unwrap(), originals);
        assert!(dest.root().join(LOCAL).is_file());
        assert!(matches!(
            dest.change_passphrase(NEXT, PASS),
            Err(Error::WrongPassphrase)
        ));
        dest.change_passphrase(PASS, NEXT).unwrap();
        drop(dest);
        let restarted = Vault::new(dest_dir.path().into(), Some(1));
        assert!(matches!(restarted.status().unwrap(), State::Open { .. }));
        assert!(matches!(
            restarted.unlock(PASS),
            Err(Error::WrongPassphrase)
        ));
        restarted.unlock(NEXT).unwrap();
        assert!(source_dir.path().join("private/vault/local.key").is_file());
    }

    #[test]
    fn backup_restores_photos_and_rejects_tampering_overwrite_and_missing_assets() {
        let (source_dir, source) = fixture();
        let (target_dir, target) = fixture();
        let (bad_dir, bad) = fixture();
        source.setup(PASS).unwrap();
        let item = source
            .save(None, garment("Backup jacket"), photo())
            .unwrap();
        let backup = source_dir.path().join("backups/complete.fitbackup");
        source.export(&backup).unwrap();
        assert_eq!(source.export(&backup), Err(Error::Exists));
        assert!(matches!(
            target.restore(&backup, NEXT),
            Err(Error::WrongPassphrase)
        ));
        assert!(!target.root().exists());
        target.restore(&backup, PASS).unwrap();
        assert_eq!(target.detail(&item.id).unwrap().details, item.details);
        assert_eq!(
            target.image(&item.id, true).unwrap(),
            source.image(&item.id, true).unwrap()
        );
        assert!(matches!(target.restore(&backup, PASS), Err(Error::Exists)));
        assert!(target_dir.path().join("private/vault/local.key").is_file());

        let mut bytes = fs::read(&backup).unwrap();
        let index = bytes.len() - 40;
        bytes[index] ^= 1;
        let amended = bad_dir.path().join("backups/tampered.fitbackup");
        fs::write(&amended, bytes).unwrap();
        assert!(matches!(bad.restore(&amended, PASS), Err(Error::Integrity)));
        assert!(!bad.root().exists());
        let thumbnail = source.root().join("images/thumbnails");
        let asset = fs::read_dir(thumbnail).unwrap().next().unwrap().unwrap();
        fs::remove_file(asset.path()).unwrap();
        let incomplete = source_dir.path().join("backups/incomplete.fitbackup");
        assert_eq!(source.export(&incomplete), Err(Error::ImageMissing));
        assert!(!incomplete.exists());
    }

    #[test]
    fn cancelled_restore_never_publishes_staging_data() {
        let (source_dir, source) = fixture();
        let (_target_dir, target) = fixture();
        source.setup(PASS).unwrap();
        let package = source_dir.path().join("backups/cancel.fitbackup");
        source.export(&package).unwrap();
        let target = Arc::new(target);
        let operation = Arc::clone(&target);
        let worker = std::thread::spawn(move || operation.restore(&package, PASS));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while target.backup_state.load(Ordering::Acquire) != 1 && !worker.is_finished() {
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        target.cancel_backup();
        assert!(matches!(worker.join().unwrap(), Err(Error::Cancelled)));
        assert!(!target.root().exists());
        assert_eq!(target.status().unwrap(), State::Setup);
    }
}
