use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OpenFlags, OptionalExtension, Row, TransactionBehavior};
use zeroize::Zeroizing;

use crate::security::{derive, hex, Secret};

use super::files;
use super::media::Asset;
use super::model::{Garment, GarmentInput, Page, Query, Sort, CATEGORIES};
use super::{Error, Result};

pub const SCHEMA: u32 = 2;
pub const DATABASE: &str = "wardrobe.sqlite3";
const APP_ID: i32 = i32::from_be_bytes(*b"FITS");
const PAGE_SIZE: usize = 24;

const GARMENT_SELECT: &str = "SELECT g.id, g.added_at, g.changed_at,
    EXISTS(SELECT 1 FROM garment_images i WHERE i.garment_id=g.id),
    g.name,g.category_id,m.primary_colour,m.secondary_colours,m.pattern,m.fit,
    m.sleeve,m.fabric,m.formality,m.seasons,m.notes
    FROM garments g JOIN garment_metadata m ON g.id=m.garment_id
    JOIN garment_categories c ON c.id=g.category_id";

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn probe(db: &Connection) -> Result<()> {
    let version = db
        .query_row("PRAGMA cipher_version", [], |row| row.get::<_, String>(0))
        .map_err(|_| Error::EncryptionUnavailable)?;
    if !version.starts_with("4.") {
        return Err(Error::EncryptionUnavailable);
    }
    db.pragma_update(None, "cipher_log_level", "NONE")?;
    Ok(())
}

pub fn verify_provider() -> Result<()> {
    probe(&Connection::open_in_memory()?)
}

pub fn open(path: &Path, master: &Secret, create: bool) -> Result<Connection> {
    if files::regular(path)?.is_some() == create {
        return Err(if create {
            Error::Exists
        } else {
            Error::Integrity
        });
    }
    let mut flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    if create {
        flags |= OpenFlags::SQLITE_OPEN_CREATE;
    }
    let db = Connection::open_with_flags(path, flags)?;
    probe(&db)?;
    let key = derive(master, b"sqlcipher")?;
    let encoded = Zeroizing::new(hex(key.as_ref()));
    let raw = Zeroizing::new(format!("x'{}'", encoded.as_str()));
    db.pragma_update(None, "key", raw.as_str())?;
    let cipher_status = db
        .pragma_query_value(None, "cipher_status", |row| row.get::<_, String>(0))
        .map_err(|_| Error::EncryptionUnavailable)?;
    let provider = db
        .pragma_query_value(None, "cipher_provider", |row| row.get::<_, String>(0))
        .map_err(|_| Error::EncryptionUnavailable)?;
    if cipher_status != "1" || !provider.eq_ignore_ascii_case("openssl") {
        return Err(Error::EncryptionUnavailable);
    }
    db.pragma_update(None, "cipher_plaintext_header_size", 0)?;
    db.pragma_update(None, "cipher_use_hmac", true)?;
    db.pragma_update(None, "cipher_memory_security", true)?;
    db.pragma_update(None, "temp_store", "MEMORY")?;
    db.pragma_update(None, "trusted_schema", false)?;
    db.query_row("SELECT count(*) FROM sqlite_schema", [], |row| {
        row.get::<_, i64>(0)
    })
    .map_err(|_| Error::Integrity)?;
    if !create
        && db.pragma_query_value(None, "application_id", |row| row.get::<_, i32>(0))? != APP_ID
    {
        return Err(Error::Integrity);
    }
    let journal: String = db.pragma_query_value(None, "journal_mode", |row| row.get(0))?;
    if journal != "delete" {
        return Err(Error::Incompatible);
    }
    db.pragma_update(None, "synchronous", "FULL")?;
    db.pragma_update(None, "foreign_keys", true)?;
    db.pragma_update(None, "secure_delete", true)?;
    Ok(db)
}

pub fn migrate(db: &mut Connection, phase_one: u32) -> Result<()> {
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let version: u32 = tx.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version > SCHEMA {
        return Err(Error::Incompatible);
    }
    if version == 0 {
        if phase_one != 1 {
            return Err(Error::Incompatible);
        }
        tx.execute_batch(include_str!("schema/001.sql"))?;
        tx.pragma_update(None, "application_id", APP_ID)?;
        tx.execute(
            "INSERT INTO storage_metadata VALUES (1, ?1, ?2)",
            params![phase_one, now()],
        )?;
        for category in CATEGORIES {
            tx.execute(
                "INSERT INTO garment_categories VALUES (?1, ?2, ?3)",
                params![category.id, category.group, category.label],
            )?;
        }
    }
    if version < 2 {
        tx.execute_batch(include_str!("schema/002.sql"))?;
    }
    if version < SCHEMA {
        tx.pragma_update(None, "user_version", SCHEMA)?;
    }
    tx.commit()?;
    Ok(())
}

pub fn verify_integrity(db: &Connection) -> Result<()> {
    if db
        .prepare("PRAGMA cipher_integrity_check")?
        .query([])?
        .next()?
        .is_some()
        || db.query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))? != "ok"
        || db
            .prepare("PRAGMA foreign_key_check")?
            .query([])?
            .next()?
            .is_some()
    {
        return Err(Error::Integrity);
    }
    let incomplete: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM garments g LEFT JOIN garment_metadata m ON g.id=m.garment_id WHERE m.garment_id IS NULL)
             OR EXISTS(SELECT 1 FROM garment_images i LEFT JOIN image_assets a ON i.id=a.image_id
                       GROUP BY i.id HAVING count(a.kind) <> 3)",
        [], |row| row.get(0))?;
    if incomplete {
        return Err(Error::Integrity);
    }
    Ok(())
}

fn parse_list(row: &Row<'_>, index: usize) -> rusqlite::Result<Vec<String>> {
    let json: String = row.get(index)?;
    serde_json::from_str(&json).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })
}

fn garment(row: &Row<'_>) -> rusqlite::Result<Garment> {
    Ok(Garment {
        id: row.get(0)?,
        added: row.get(1)?,
        changed: row.get(2)?,
        photo: row.get(3)?,
        details: GarmentInput {
            name: row.get(4)?,
            category: row.get(5)?,
            primary_colour: row.get(6)?,
            secondary_colours: parse_list(row, 7)?,
            pattern: row.get(8)?,
            fit: row.get(9)?,
            sleeve: row.get(10)?,
            fabric: row.get(11)?,
            formality: row.get(12)?,
            seasons: parse_list(row, 13)?,
            notes: row.get(14)?,
        },
    })
}

pub fn detail(db: &Connection, id: &str) -> Result<Garment> {
    db.query_row(&format!("{GARMENT_SELECT} WHERE g.id=?1"), [id], garment)
        .optional()?
        .ok_or(Error::NotFound)
}

pub fn list(db: &Connection, filter: Query) -> Result<Page> {
    if filter.search.len() > 256 || filter.category.len() > 40 || filter.group.len() > 24 {
        return Err(Error::InvalidInput);
    }
    let sort = match filter.sort {
        Sort::Newest => "g.added_at DESC,g.id",
        Sort::Oldest => "g.added_at,g.id",
        Sort::Name => "g.name COLLATE NOCASE,g.id",
    };
    let mut statement = db.prepare(&format!("{GARMENT_SELECT}
        WHERE (?1='' OR instr(lower(g.name || ' ' || c.label || ' ' || m.primary_colour || ' ' ||
            m.secondary_colours || ' ' || m.pattern || ' ' || m.fabric || ' ' || m.notes), lower(?1))>0)
        AND (?2='' OR g.category_id=?2) AND (?3='' OR c.group_name=?3)
        ORDER BY {sort} LIMIT ?4 OFFSET ?5"))?;
    let mut garments = statement
        .query_map(
            params![
                filter.search.trim(),
                filter.category,
                filter.group,
                (PAGE_SIZE + 1) as u32,
                filter.offset
            ],
            garment,
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let has_more = garments.len() > PAGE_SIZE;
    garments.truncate(PAGE_SIZE);
    Ok(Page { garments, has_more })
}

pub fn write_garment(db: &Connection, id: &str, input: &GarmentInput, new: bool) -> Result<()> {
    let timestamp = now();
    if new {
        db.execute(
            "INSERT INTO garments VALUES (?1,?2,?3,?4,?4)",
            params![id, input.name, input.category, timestamp],
        )?;
    } else if db.execute(
        "UPDATE garments SET name=?2, category_id=?3, changed_at=?4 WHERE id=?1",
        params![id, input.name, input.category, timestamp],
    )? != 1
    {
        return Err(Error::NotFound);
    }
    let colours =
        serde_json::to_string(&input.secondary_colours).map_err(|_| Error::InvalidInput)?;
    let seasons = serde_json::to_string(&input.seasons).map_err(|_| Error::InvalidInput)?;
    db.execute(
        "INSERT INTO garment_metadata VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
        ON CONFLICT(garment_id) DO UPDATE SET primary_colour=excluded.primary_colour,
        secondary_colours=excluded.secondary_colours, pattern=excluded.pattern, fit=excluded.fit,
        sleeve=excluded.sleeve, fabric=excluded.fabric, formality=excluded.formality,
        seasons=excluded.seasons, notes=excluded.notes",
        params![
            id,
            input.primary_colour,
            colours,
            input.pattern,
            input.fit,
            input.sleeve,
            input.fabric,
            input.formality,
            seasons,
            input.notes
        ],
    )?;
    Ok(())
}

const ASSET_FIELDS: &str = "a.image_id, a.kind, a.relative_path, a.mime, a.width, a.height, a.byte_count, a.sha256, a.processing_version";

pub fn all_assets(db: &Connection) -> Result<Vec<Asset>> {
    let mut statement = db.prepare(&format!(
        "SELECT {ASSET_FIELDS} FROM image_assets a ORDER BY a.relative_path"
    ))?;
    let assets = statement
        .query_map([], Asset::from_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(assets)
}

pub fn garment_asset(db: &Connection, id: &str, variant: &str) -> Result<Asset> {
    db.query_row(
        &format!(
            "SELECT {ASSET_FIELDS} FROM image_assets a JOIN garment_images i ON a.image_id=i.id
        WHERE i.garment_id=?1 AND a.kind=?2"
        ),
        [id, variant],
        Asset::from_row,
    )
    .optional()?
    .ok_or(Error::NotFound)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn migration_from_one_preserves_data_and_constraints() {
        let dir = tempfile::tempdir().unwrap();
        let secret = Zeroizing::new(crate::security::random().unwrap());
        let path = dir.path().join(DATABASE);
        let mut db = open(&path, &secret, true).unwrap();
        db.execute_batch(include_str!("schema/001.sql")).unwrap();
        db.pragma_update(None, "application_id", APP_ID).unwrap();
        db.pragma_update(None, "user_version", 1).unwrap();
        db.execute_batch(
            "INSERT INTO storage_metadata VALUES (1,1,1);
            INSERT INTO garment_categories VALUES ('t-shirts','Upper wear','T-shirts');",
        )
        .unwrap();
        let input = GarmentInput {
            name: "Preserved".into(),
            category: "t-shirts".into(),
            ..Default::default()
        };
        write_garment(&db, ID, &input, true).unwrap();

        migrate(&mut db, 1).unwrap();
        assert_eq!(
            db.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))
                .unwrap(),
            SCHEMA
        );
        assert_eq!(detail(&db, ID).unwrap().details, input);
        assert!(db
            .execute("UPDATE garments SET category_id='missing'", [])
            .is_err());
        verify_integrity(&db).unwrap();
    }

    #[test]
    fn correct_key_required_and_bad_schema_or_corruption_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let correct = Zeroizing::new(crate::security::random().unwrap());
        let wrong = Zeroizing::new(crate::security::random().unwrap());
        let path = dir.path().join(DATABASE);
        let mut db = open(&path, &correct, true).unwrap();
        migrate(&mut db, 1).unwrap();
        verify_integrity(&db).unwrap();
        let keyless = Connection::open(&path).unwrap();
        assert!(keyless
            .query_row("SELECT COUNT(*) FROM garments", [], |row| row
                .get::<_, i64>(0))
            .is_err());
        drop(keyless);
        assert!(matches!(open(&path, &wrong, false), Err(Error::Integrity)));
        assert!(!std::fs::read(&path)
            .unwrap()
            .starts_with(b"SQLite format 3\0"));
        db.pragma_update(None, "user_version", 99).unwrap();
        assert_eq!(migrate(&mut db, 1), Err(Error::Incompatible));
        drop(db);
        let mut bytes = std::fs::read(&path).unwrap();
        bytes[32] ^= 1;
        std::fs::write(&path, bytes).unwrap();
        assert!(matches!(
            open(&path, &correct, false),
            Err(Error::Integrity)
        ));
    }
}
