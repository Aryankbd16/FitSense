# FitSense V1 · Phase 1 completion report

## Delivered foundation

- React workspace with Overview and Diagnostics, live status and recovery controls, a desktop layout that adapts to narrow windows, and local fonts and assets.
- A Rust-controlled Python process with readiness, correlation, periodic health checks, structured errors, manual recovery, cancellation on shutdown and bounded cleanup. Python is embedded in the intended release distributions.
- The same SQLite foundation schema and storage layout for installed and portable modes, with an explicit lock on storage of personal data.
- Local structured logs for Rust and Python, with fixed event names, timed operations, size-based rotation and configurable retention.
- Windows x64 NSIS and portable packaging configuration, local build scripts and repeatable automated checks.

## Selected versions

| Component | Version |
| --- | --- |
| App | 0.1.0 |
| Rust compiler and target | 1.100.0 · `x86_64-pc-windows-msvc` |
| Tauri | Rust 2.12.7 · build crate 2.7.6 · CLI 2.12.6 · JS API 2.12.5 |
| React and build tooling | React and React DOM 19.3.1 · TypeScript 5.9.3 · Vite 8.3.8 |
| Frontend test runner | Vitest 5.1.1 |
| Node.js and npm | Node.js 22.20.0 · npm 10.9.3 |
| Worker | Windows x64 embeddable CPython 3.13.17 · IPC protocol v1 |
| SQLite | SQLite 3.50.2 · `rusqlite` 0.37.0 · bundled `libsqlite3-sys` 0.35.0 · foundation schema 1 |

Direct JavaScript dependencies are pinned in `package.json`. `package-lock.json` and `src-tauri/Cargo.lock` capture full resolution. PyTorch and ONNX Runtime will be pinned when inference work begins.

## Acceptance evidence

Verified on November 24, 2026, using Windows 11 x64 build 26300 and WebView2 Evergreen 157.0.4341.60. The native smoke test launched the release from a freshly extracted directory with spaces in its path and used a different working directory.

| Acceptance criterion | Evidence |
| --- | --- |
| 1 · Launch the Windows application | Pass. The optimized x64 Tauri executable launched from the extracted ZIP. |
| 2 · Render the React interface | Pass. WebView2 rendered Overview and Diagnostics, loaded the bundled font and fit a 700px viewport without horizontal overflow. |
| 3 · Launch Python from Rust | Pass. The bundled interpreter returned Python 3.13.17 through the UI. The release has no system Python lookup. |
| 4 · Complete the readiness handshake | Pass. The live UI showed Connected and the local log recorded `worker_ready`. Both the Rust integration test and the Python subprocess test passed. |
| 5 · Check health through actual IPC | Pass. Rust exercised a real `health` request, and the live dashboard displayed its latest round-trip timing. |
| 6 · Correlate IDs | Pass. Tests confirmed distinct IDs and rejected a response with the wrong ID. |
| 7 · Handle worker errors | Pass. Tests covered malformed and oversized messages, wrong IDs, early worker exit, a timeout, cancellation and successful retry. |
| 8 · Shut down the worker gracefully | Pass. Window closure recorded `python_stopped` followed by `app_stopped`, with no forced stop event. A separate run verified that terminating the parent also terminated its worker. |
| 9 · Keep the UI responsive | Pass. The frontend test changed views while diagnostics were pending, and the release UI displayed the live diagnostic result through an asynchronous command. |
| 10 · Initialize storage directories | Pass. An empty portable workspace acquired SQLite schema 1 and local directories. Rust tests covered both storage modes and rejection of unknown schemas. |
| 11 · Log without exposing sensitive data | Pass. Rust tests forced rotation, checked retention and rejected raw stderr. Python subprocess tests verified that malformed private sample text stayed out of responses and logs. |
| 12 · Pass automated tests | Pass. 5 frontend tests, 4 Python subprocess tests, 11 Rust tests and the native release smoke check passed. `tsc`, Vite, `cargo fmt` and Clippy with `-D warnings` passed. |
| 13 · Document the architecture | Pass. Architecture, IPC, storage, security, setup, testing and two ADRs are in `docs/`. |
| 14 · Work without internet access | Pass for the portable runtime on this host. The smoke test blocked external WebView hostname resolution, observed only Tauri-local requests and completed real diagnostics. Rust tests used Cargo offline mode. |

See [README](../README.md) for the project structure and run commands and [Testing](TESTING.md) for repeatable checks.

## Windows artifacts

| Artifact under `artifacts/` | Status | SHA-256 |
| --- | --- | --- |
| `FitSense-0.1.0-windows-x64-portable.zip` | Built and verified after extraction. Includes CPython and needs an existing WebView2 runtime. | `968d7c3faa65124f7e6e36d07092e2725b0cc1c99518ce83da7b4c9577d78e68` |
| `FitSense-0.1.0-windows-x64-setup.exe` | Built unsigned. The generated NSIS manifest includes CPython and the signed Microsoft WebView2 offline redistributable. | `8bd78e5feae5115ae64ecbba48d70a261425c83a352bb7e5e5e76d7d37c416d9` |

The native smoke check wrote screenshots and local logs to `.build/portable smoke 2e64784ecd934d168bc6059d71754824/`. Clean-machine installer execution and Windows 10 validation remain open.

## Release follow-up

- Verify the offline installer on clean Windows 10 and Windows 11 machines, including WebView2 installation and launched data-root selection.
- Add Windows code signing before public distribution.
- Validate SQLCipher, DPAPI key wrapping, authenticated assets and backups, portability and recovery before enabling private storage.
- Benchmark 8 GB and 4 GB systems. Inference and outfit recommendations are later phases.
