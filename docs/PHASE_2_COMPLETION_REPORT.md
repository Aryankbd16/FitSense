# FitSense V1 · Phase 2 completion report

## Delivered

FitSense 0.2.0 extends the existing Tauri shell with a Wardrobe view, secure initialization, manual garment registration, photo previews, browsing, search, category filters, sorting, metadata editing, confirmed deletion and encrypted backup and restore. The catalogue contains the 18 approved men's subcategories. Overview, the supervised Python worker, its IPC protocol and Diagnostics remain available. Wardrobe storage is independent of Python.

## Database and encryption

Schema 2 consists of `storage_metadata`, `garment_categories`, `garments`, `garment_metadata`, `garment_images` and `image_assets`. The first migration creates and seeds these tables. The second adds browse indexes. The plaintext Phase 1 database remains a separate technical marker and its version is recorded in the private vault. SQLCipher validates the keyed connection and crypto provider before private writes, checks page authentication during integrity verification, and keeps temporary SQLite data in memory.

An OS-generated master key is protected with Windows CurrentUser DPAPI. HKDF-SHA-256 separates key purposes. An Argon2id-derived key and XChaCha20-Poly1305 independently wrap the master key for passphrase recovery. Passphrase changes authenticate the current envelope before replacing it. Older exports keep their original passphrase.

The encrypted image layer preserves JPEG and PNG originals and caches re-encoded display and thumbnail variants. Each file has an independent nonce, an authenticated relative reference and a content hash recorded in SQLCipher. Image replacement and deletion commit reference changes before encrypted file cleanup. The UI requests images by garment ID and renders them in memory.

Backups stream the encrypted database and referenced asset files into a package with an encrypted manifest and a whole-package HMAC. Restore verifies the package, schema, database, and image inventory before publishing a staged vault. It refuses to overwrite an existing vault.

## Project and dependencies

| Path | Responsibility |
| --- | --- |
| `src/wardrobe/` | Wardrobe screens, typed Tauri bridge, form and workflow tests |
| `src-tauri/src/wardrobe/` | Vault lifecycle, migrations, scoped commands, photo validation and backup |
| `src-tauri/src/security.rs` | Crypto boundaries, recovery envelopes and Windows key protection |
| `tests/` and `scripts/` | Shared WebView2 test client, native wardrobe checks, stock SQLite inspection and packaging |
| `docs/` and `licenses/` | Design decisions, verification, and SQLCipher and OpenSSL notices |

The native binding changed from rusqlite 0.37 to 0.43.0 and libsqlite3-sys 0.40.0 with SQLCipher 4.20.0, SQLite 3.54.1, and bundled OpenSSL 3.6.3. Phase 2 adds Argon2, RustCrypto AEAD and MAC libraries, OS randomness and key zeroization support, image 0.25.10 for JPEG/PNG, and rfd 0.17.2 for Windows file selection. React, Tauri and the Python runtime versions are unchanged.

## Verification

The release was tested on November 24, 2026, on Windows 11 x64 build 26300 with WebView2 157.0.4341.60. The automated suites ran offline. The end-to-end smoke tests blocked external WebView hostname resolution and observed only Tauri-local requests.

| Check | Result |
| --- | --- |
| Rust tests on Windows 11 x64 | 21 passed on the final SQLCipher dependency stack |
| React tests | 9 passed |
| Python worker subprocess tests | 4 passed |
| Static checks | Strict TypeScript and Vite production build, Rust formatting, and Clippy with warnings treated as errors passed |
| Existing portable worker and diagnostics smoke test | Passed on the Phase 2 release |
| Native portable wardrobe | Passed from a newly extracted ZIP with spaces in the path. |
| Native installed-mode wardrobe | Passed with a release executable without the portable marker, using the actual Windows per-user directory. The test moved its synthetic fixture back into the workspace on exit. |
| Separate Windows account or machine | Pending. This run has one Windows account and no Windows Sandbox. |

## Acceptance criteria

| Criterion | Evidence |
| --- | --- |
| 1 · Encrypt personal data | The independent Python SQLite reader rejected the native vault, and the workspace scan found no plaintext fixture records or passphrase. |
| 2 · Test key management | DPAPI round trip and tampering checks, incorrect key rejection, Argon2id envelope recovery, and passphrase change passed. |
| 3 · Recover portable data | A fresh extracted portable directory unlocked a copied vault without its source DPAPI wrapper, generated a local wrapper and displayed the recovered photo. Verification under a different Windows identity remains open. |
| 4 · Encrypt all photo variants | The native release wrote encrypted original, display and thumbnail files. Rust tests checked authentication, missing files and incorrect hashes. |
| 5 · Migrate data safely | Phase 1's technical marker was preserved and recorded in the new vault. Migration from encrypted schema 1 to 2 retained existing garment data and enforced foreign keys. |
| 6–10 · Create, edit, delete, find and preserve image associations | Installed and portable desktop workflows passed. An invalid photo rolled back edits, and confirmed deletion removed the three associated encrypted files. |
| 11–12 · Back up and restore records and photos | Desktop backups from both modes restored into fresh portable workspaces with visible photos. Tests rejected wrong passphrases, changed bytes, missing assets and overwrite attempts. Cancellation left no published vault. |
| 13–14 · Installed and portable storage | Both roots were exercised in the released Windows binary and checked by the backend tests. |
| 15 · Pass automated checks | 34 tests across Rust, React and Python, plus the native workflows and Phase 1 smoke check. |
| 16 · Preserve Phase 1 | Overview, diagnostics, bundled Python, readiness, graceful shutdown and worker termination after forced parent exit passed. |
| 17 · Operate offline | The wardrobe, restoration and diagnostics worked with external WebView hostname resolution disabled. |
| 18 · Document implementation and limits | This report, updated architecture, storage, security, setup and testing documents, and ADRs 003 through 006. |

## Windows builds

| Artifact under `artifacts/` | Result | SHA-256 |
| --- | --- | --- |
| `FitSense-0.2.0-windows-x64-portable.zip` | Built and tested after extraction | `c096fef3fccb49fdc9ac29f59558dd49d06b7e0a16185cd4ffd4dd18c962306f` |
| `FitSense-0.2.0-windows-x64-setup.exe` | Built unsigned with bundled Python and the offline Microsoft WebView2 redistributable | `6eff7b701c3d7aa582cc5a079c63c58c6137e2ca5d90f69e702ba115b655261b` |

The release links OpenSSL and SQLCipher statically. Inspection of the Windows imports found no separate OpenSSL or SQLite DLL. The MSVC linker reported missing debug symbols for vendored OpenSSL objects and produced the runnable release. The distribution includes the SQLCipher and OpenSSL licenses.

The native test output is under `.build/wardrobe smoke 233b8064170d42c0b7954b6a9bda8700/` and `.build/wardrobe smoke 57bc4df4c2ed4f64b41490b0422b376a/`. The Windows installer was packaged and its content manifest checked. Clean-machine installation was outside the verified test environment.

## Limitations and remaining security work

- Confirm portable recovery with separate Windows credentials or a different computer before claiming that boundary is validated.
- Validate clean-machine NSIS installation and Windows 10. The Phase 2 installer is unsigned.
- The wardrobe supports one original photo per garment. Restore requires an unused vault.
- Phase 2 has no independent security audit or 4 GB benchmark. The [security notes](SECURITY.md) cover memory, local account compromise and backup rollback risks.

[Schema and file formats](DATA_STORAGE.md) · [Security and recovery](SECURITY.md) · [Repeatable tests](TESTING.md)
