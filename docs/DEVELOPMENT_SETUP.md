# Development setup

## Windows prerequisites

- Windows 10 or Windows 11 on x64. Use PowerShell and an interactive desktop for the native smoke test.
- Node.js 22.20 or newer in the 22.x line, or Node.js 24+. The repository pins frontend dependencies in `package-lock.json`.
- Rust stable 1.90 or newer with the `x86_64-pc-windows-msvc` target. This workspace was verified with Rust 1.100.0. Install the Visual Studio Desktop development with C++ workload and a Windows 10 or 11 SDK. `rustup target add x86_64-pc-windows-msvc` adds the target if needed.
- Native Windows Perl on PATH for the bundled OpenSSL build. Set `OPENSSL_SRC_PERL` to an absolute path if needed. The MSYS Perl interpreter from Git for Windows uses a different path model. NASM is optional. `OPENSSL_RUST_USE_NASM=0` explicitly selects the supported C-only OpenSSL build. No Perl, NASM or OpenSSL installation is required at application runtime.
- Microsoft Edge WebView2 Evergreen Runtime. Established Windows environments typically have it. Tauri's installer is configured to embed Microsoft's offline redistributable for machines where it is absent. [Tauri Windows prerequisites](https://v2.tauri.app/start/prerequisites/) · [Tauri offline installer](https://v2.tauri.app/distribute/windows-installer/#offline-installer)
- A Python 3.12+ x64 executable for development and tests, or the bundled interpreter staged below. Release builds always use the packaged interpreter.

## Install build dependencies and run

Use the project root for these commands. The first setup pulls dependencies from their upstream registries. Later builds and tests can use the cached inputs.

```powershell
npm.cmd ci
cargo fetch --locked --manifest-path src-tauri/Cargo.toml --target x86_64-pc-windows-msvc
./scripts/Stage-Worker.ps1 -Download
$env:FITSENSE_PYTHON = (Resolve-Path './src-tauri/runtime/worker/python.exe').Path
npm.cmd run tauri -- dev
```

`Stage-Worker.ps1` verifies a pinned SHA-256 from the CPython release page before it expands the runtime and copies `worker.py`. For offline staging, pass `-ArchivePath` to a local copy of the official archive or place it in `.build/downloads/`. A local development interpreter may be used by setting `FITSENSE_PYTHON` to its absolute path. Development mode uses a separate Windows data directory. [Python 3.13.17 release and checksum](https://www.python.org/downloads/release/python-31317/)

## Build the Windows distributions

```powershell
./scripts/Stage-Worker.ps1
npm.cmd run tauri -- build --target x86_64-pc-windows-msvc --config src-tauri/tauri.distribution.conf.json
./scripts/New-Portable.ps1
./scripts/Test-Portable.ps1 -CheckForcedExit
./scripts/Test-Wardrobe.ps1
```

The NSIS installer is produced under `src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/`. A fresh packaging machine may need to fetch NSIS tooling and the Microsoft offline WebView2 installer. The setup embeds that redistributable for offline installation. Production signing requires a separate signing configuration and key.

The portable archive is produced under `artifacts/`. It contains `FitSense.exe`, the `FitSense.portable` marker and `runtime/worker` with Python. It needs an existing WebView2 Evergreen installation or the separate Microsoft offline installation package. The archive has no installed data snapshot. Extract it before launching.

To build just the Windows executable for the portable artifact, use `npm.cmd run tauri -- build --target x86_64-pc-windows-msvc --no-bundle`, followed by `./scripts/New-Portable.ps1`.
