# Local data storage

| Mode | Root on Windows x64 |
| --- | --- |
| Installed | `%LOCALAPPDATA%\com.fitsense.desktop` |
| Portable | `data` alongside `FitSense.exe` when a regular `FitSense.portable` marker is alongside the executable |
| Development | `%LOCALAPPDATA%\com.fitsense.development`, unless launched with the portable marker |

The working directory does not decide the data location. Installed data is outside the directory holding executables and runtime resources. A portable root must be writable. A marker directory or symlink is invalid and startup fails.

```text
<workspace>/
  db/foundation.sqlite3
  private/
    vault.lock
    vault/
      wardrobe.sqlite3
      local.key
      recovery.key
      images/{originals,display,thumbnails}/
  backups/*.fitbackup
  logs/session-<pid>-<timestamp>-<segment>.jsonl
  webview/
```

Both distributions use this layout. The Phase 1 foundation file stays readable and contains only its existing technical schema marker. The new SQLCipher database has application ID `FITS` and `user_version = 2`. Migration 1 creates the tables, seeds the 18 approved subcategories, and records the Phase 1 schema marker in encrypted metadata. Migration 2 adds browse indexes. Each pending migration runs inside one transaction. Unknown future versions are rejected.

| Private table | Contents and constraints |
| --- | --- |
| `storage_metadata` | Singleton row with Phase 1 schema marker and vault creation timestamp |
| `garment_categories` | Stable subcategory IDs, display labels and one of four approved groups |
| `garments` | Random 128-bit ID, name, required category FK and UTC creation/update times |
| `garment_metadata` | One-to-one optional details, JSON arrays for secondary colours and seasons, cascading FK |
| `garment_images` | Zero or one original photo per garment, with cascading FK from the garment |
| `image_assets` | Original, display and thumbnail metadata. Each row has a relative reference, MIME type, dimensions, SHA-256, byte length and processing version. |

The asset reference is `images/<variant>/<32 lowercase hex characters>` relative to the private vault. The Phase 1 image directories outside the vault may still exist for compatibility. The wardrobe writes photographs only inside the vault. FitSense parses JPEG and PNG, limits uploads to 12 MiB, 6,000 pixels per edge and 20 megapixels, and preserves original bytes. It creates oriented JPEGs with longest edges of up to 1,200 pixels for display and 320 pixels for thumbnails. Edit and deletion update references in a transaction and then remove unreferenced ciphertext. Opening the vault retries cleanup after interruptions.

The log writer creates independent session files, rolls to a new segment at 256 KiB and expires owned files on startup and rotation. `FITSENSE_LOG_RETENTION_DAYS` accepts 1–90 days and defaults to seven. An invalid value uses the default and records a configuration event. Logs contain Unix timestamps, component, event and optional millisecond timing. Raw stderr, paths, wardrobe details and personal measurements are excluded. A logging failure appears as degraded status. The WebView profile stays within the selected data root.

Use **Wardrobe → Backup & recovery** to create a `.fitbackup` file. It includes the encrypted database, all referenced photos, the recovery envelope and the encrypted backup manifest. Exports use the vault's current recovery passphrase. Restore into an unused vault through Wardrobe setup. Close FitSense before copying an entire portable workspace. Follow the recovery steps in [Security](SECURITY.md).

## Versioned file formats

An image file starts with `FSIMAGE1`, then holds a 24-byte nonce and the ciphertext followed by its 16-byte tag. The authenticated data combines the format marker and relative reference. SQLCipher stores the expected hash and dimensions.

A `.fitbackup` file starts with `FSBACK01`, followed by the 128-byte recovery envelope, a 32-bit little-endian encrypted manifest length, the sealed manifest, and the encrypted files in manifest order. A 32-byte HMAC covers all preceding bytes. The manifest names the schema, creation time, item paths and lengths. Uploads and restores cannot specify arbitrary paths inside the vault. Version 1 accepts at most 30,001 files, a 3 MiB encrypted manifest and an 8 GiB package.
