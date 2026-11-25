# Testing

After installing the dependencies in [Development setup](DEVELOPMENT_SETUP.md), run from the repository root with `FITSENSE_PYTHON` set to an absolute path.

```powershell
npm.cmd run build
npm.cmd test
& $env:FITSENSE_PYTHON -B -m unittest discover -s tests -p 'test_*.py' -v
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --release --features tauri/custom-protocol --locked --offline --manifest-path src-tauri/Cargo.toml --target x86_64-pc-windows-msvc
cargo clippy --release --features tauri/custom-protocol --locked --offline --manifest-path src-tauri/Cargo.toml --target x86_64-pc-windows-msvc --all-targets -- -D warnings
```

| Suite | Coverage |
| --- | --- |
| React / Vitest | Phase 1 regressions plus wardrobe navigation, empty and loading states, setup errors, creation, editing, deletion confirmation, search and category filtering |
| Python / `unittest` | A real subprocess, readiness, health and diagnostics correlation, graceful exit and EOF, malformed and oversize input, redaction |
| Rust / `cargo test` | Phase 1 worker, path and logging tests plus SQLCipher migration and integrity, wrong key handling, DPAPI, recovery wrapping, CRUD rollback, authenticated files, corrupt and missing images, backup and restore, wrong passphrases and interrupted restore |

## Native release smoke check

Generate the release archive and run `./scripts/Test-Portable.ps1 -CheckForcedExit`. It extracts the archive to a fresh directory with a space in its name and launches the actual Windows executable using a separate working directory. The test checks the rendered WebView2 page, live local status, the UI-triggered diagnostic response from bundled Python, a 700px layout and graceful worker shutdown. `-CheckForcedExit` also confirms the worker exits if its parent is terminated. The smoke test temporarily enables a loopback-only DevTools endpoint and maps non-local browser hosts to unresolved. It records page network requests during the test and expects only Tauri origins. Screenshots and the portable data directory stay under `.build/`.

A clean-machine installer check should run on Windows 10 and Windows 11 VMs with networking disabled and no WebView2 runtime installed. Install with the NSIS setup and verify the installed path, the per-user storage path, WebView2 initialization, diagnostics and shutdown. The packaged binary smoke test and the clean-machine installer check cover different paths. See [Phase 1 completion report](PHASE_1_COMPLETION_REPORT.md) for the actual validation results.

## Native wardrobe smoke check

```powershell
./scripts/Test-Wardrobe.ps1
./scripts/Test-Wardrobe.ps1 -Installed
```

The script uses a synthetic upload and temporary test directories. It runs the release UI against real Tauri commands, creates and edits a garment with an encrypted photo, filters the wardrobe, backs it up, deletes the garment and restores the backup with its photo. The independent inspector uses ordinary SQLite and expects it to reject the private file. A separate portable copy omits the source DPAPI wrapper, exercises the recovery prompt and checks that a local wrapper is created.

`-Installed` uses the released executable without the portable marker and exercises the actual Windows per-user path. It refuses to start if the current account already has a FitSense workspace. It moves only its fresh fixture workspace into the test run directory on exit. Use a separate Windows account or machine to verify the real DPAPI boundary and recovery before claiming cross-account support. The [Phase 2 completion report](PHASE_2_COMPLETION_REPORT.md) records the environments actually tested.

For the separate-account check, create a portable wardrobe with a synthetic photo under one Windows identity and close FitSense. Copy the entire directory, including the source `local.key`, to a location writable by the destination identity. Start FitSense under the destination identity and verify that it requests the recovery passphrase. Confirm that an incorrect phrase leaves the vault intact. Enter the correct phrase, open the photo, and restart under the destination identity to verify that the new DPAPI wrapper opens the vault. Also restore an exported backup on the destination to verify that path across the same boundary.
