# FitSense

FitSense is an offline-first Windows desktop application for managing a men's wardrobe. Add garments by hand, keep encrypted photos, search and filter the collection, and take encrypted backups. The desktop workspace and supervised Python diagnostics from Phase 1 remain available.

## Run locally

On Windows x64, install Node.js, the Rust MSVC toolchain, Visual Studio C++ build tools, native Windows Perl and WebView2. Stage the pinned embeddable CPython runtime or use a local Python 3.12+ executable for development.

```powershell
npm.cmd ci
./scripts/Stage-Worker.ps1 -Download
$env:FITSENSE_PYTHON = (Resolve-Path './src-tauri/runtime/worker/python.exe').Path
npm.cmd run tauri -- dev
```

Open **Wardrobe** to set a recovery passphrase and initialize the encrypted vault, or restore an existing backup into a new workspace. Open **Diagnostics** and select **Run diagnostics** to check the Python worker. `npm.cmd run dev` opens a browser preview with the desktop connection disabled.

## Test and package

```powershell
npm.cmd run build
npm.cmd test
& $env:FITSENSE_PYTHON -B -m unittest discover -s tests -p 'test_*.py' -v
cargo test --release --features tauri/custom-protocol --locked --offline --manifest-path src-tauri/Cargo.toml --target x86_64-pc-windows-msvc
npm.cmd run tauri -- build --target x86_64-pc-windows-msvc --config src-tauri/tauri.distribution.conf.json
./scripts/New-Portable.ps1
./scripts/Test-Portable.ps1 -CheckForcedExit
./scripts/Test-Wardrobe.ps1
```

After dependencies are cached, the test suites run offline. The first packaging build may fetch NSIS and the offline WebView2 redistributable. See [Development setup](docs/DEVELOPMENT_SETUP.md) for the complete build sequence and how to prime Cargo's cache.

## Project layout

| Path | Responsibility |
| --- | --- |
| `src/` | React workspace, desktop bridge, styling and frontend tests |
| `src-tauri/` | Tauri entry point, SQLite, storage paths, diagnostics and Rust-managed IPC |
| `python-worker/` | Python process entry point and v1 request dispatcher |
| `tests/` | Python subprocess tests and native WebView2 smoke client |
| `scripts/` | Windows worker staging, portable archive and smoke orchestration |
| `docs/` | Architecture, decisions, security, storage, setup and verification |

Keep your recovery passphrase and an encrypted backup outside the workspace. Restoring a backup requires an unused vault. AI models, recommendations and diagnostic export are future work.

[Architecture](docs/ARCHITECTURE.md) · [IPC protocol](docs/IPC_PROTOCOL.md) · [Data storage](docs/DATA_STORAGE.md) · [Security](docs/SECURITY.md) · [Testing](docs/TESTING.md) · [Phase 2 completion report](docs/PHASE_2_COMPLETION_REPORT.md)
