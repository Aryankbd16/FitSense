# Local security

## Database and photographs

FitSense checks for SQLCipher before accepting private data. It derives a raw SQLCipher key, checks that the connection is keyed and uses the bundled OpenSSL provider, and reads the keyed schema before admitting writes. The private vault uses authenticated encrypted pages, in-memory SQLite temporary storage, a DELETE rollback journal, full synchronization and foreign keys. The Phase 1 technical database is kept separately. An encryption or integrity failure leaves the private files untouched and reports a fixed error code. SQLCipher's own diagnostic output is disabled.

Each photo variant is independently encrypted and authenticated with XChaCha20-Poly1305. A fresh random 192-bit nonce and authenticated version plus relative reference bind the contents to a storage slot. The master key uses HKDF purpose separation for the database, images and backup. Files use opaque names. The database stores dimensions, content hashes and the processing version. Originals may contain metadata from their source. Derived JPEGs carry only re-encoded pixels.

The renderer uses the local Tauri protocol and scoped commands. WebView2 runs in InPrivate mode with general autofill disabled. For previews, Rust verifies and decrypts only the requested photo. React creates an in-memory blob URL and revokes it when leaving the view. FitSense does not create a decrypted photo temp file, password preference, or wardrobe cache. Diagnostic logs contain fixed error categories and timings. [SQLCipher API](https://www.zetetic.net/sqlcipher/sqlcipher-api/)

## Setup, recovery and passphrase changes

1. Open **Wardrobe** and choose a long, unique recovery passphrase of at least 12 characters. Keep it away from the workspace, for example in a trusted password manager or offline record.
2. FitSense generates a 256-bit key using the OS CSPRNG. It writes a Windows CurrentUser DPAPI wrapper so the current account can open the vault. It also writes a separately authenticated recovery envelope. The passphrase derives a wrapping key through Argon2id v19 with 64 MiB, three passes, one lane and a 128-bit random salt. The envelope stores encrypted key material and its verification tag.
3. For portable migration, close FitSense before copying the directory and keep the complete `data/private/vault` contents together. If the local DPAPI wrapper cannot open, Wardrobe prompts for the recovery passphrase. FitSense validates the recovered key against the encrypted database and only then writes a new local DPAPI wrapper. A failed passphrase or invalid database leaves the files unchanged. Consult the completion report for the Windows environments tested.
4. In **Backup & recovery**, enter the current phrase and confirm a replacement. FitSense rewraps the same vault key with fresh salt and nonce. Already-exported backups still use their original passphrase. Changing a passphrase does not revoke old backups or rotate an already disclosed master key.

DPAPI CurrentUser normally ties the local wrapper to the originating Windows account and computer. Loss of both a working Windows scope and the passphrase makes the vault unrecoverable. FitSense provides no bypass. [Windows DPAPI](https://learn.microsoft.com/en-us/windows/win32/api/dpapi/nf-dpapi-cryptprotectdata) · [Argon2 RFC](https://www.rfc-editor.org/rfc/rfc9106.html)

## Backup and restore

**Back up to workspace** saves an encrypted package in the local backups folder. **Export to another folder** opens a native Windows save dialog. The package contains only an encrypted manifest, passphrase recovery envelope, SQLCipher snapshot and authenticated image ciphertext. The local DPAPI wrapper stays with its vault. The exporter verifies the DB and each photo and refuses an existing destination. An interrupted export cannot create a valid-looking final file.

To restore, open **Wardrobe → Restore an existing backup** from an unused workspace, select the file, and enter the passphrase in effect when that archive was created. FitSense validates format, schema, whole-package MAC, database integrity and photo inventory and authentication in an encrypted staging directory. It creates a fresh Windows DPAPI wrapper before atomically publishing the vault. An existing vault is never overwritten by this flow. Keep at least one verified backup in a different location.

## Residual risks

An unlocked Windows session or compromised process can access data FitSense is authorized to read. The OS may page memory or capture crash dumps, and plaintext originals selected from outside FitSense remain under the user's control. The application does not control storage firmware, Windows indexing and backup policies, other copies of the portable folder or old backup packages. Ciphertext file sizes and filesystem timestamps can be observed. Passphrase protection depends on the strength of the chosen phrase. The cryptographic components use upstream libraries. Phase 2 does not include an independent security audit or protection against rollback to a previously valid complete vault snapshot.
