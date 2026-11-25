# ADR 001 · Use a supervised Python process over stdio

**Status** Accepted for Phase 1.

An in-process Python interpreter would avoid IPC but would tie the desktop process to Python initialization and native-extension failures. A loopback HTTP service would add a listening port, connection lifecycle and server dependencies. Named pipes could support reconnecting multiple clients. FitSense has exactly one Rust parent and one child, and the operating system already provides inherited anonymous pipes.

Use UTF-8 JSON lines over stdin and stdout, a dedicated stderr stream, bounded frames and a versioned request envelope. The supervisor serializes requests and generates correlation IDs. A Windows Job Object with kill-on-close covers abrupt parent exit. Closing stdin lets the worker exit when the pipe disappears. User-triggered retry starts a clean process after a failure, with no automatic crash loop.

Use the official Windows x64 embeddable CPython distribution plus the worker script. Its isolated search path, stable on-disk layout and lack of launch-time extraction suit Phase 1. A PyInstaller one-folder bundle remains an option when native inference dependencies arrive. It can collect dependencies at build time, at the cost of hook maintenance and a second build tool. A self-extracting single-file package adds startup I/O and temporary files that can outlive a crash. Benchmark sidecar startup and test ONNX Runtime wheel DLL loading before taking a packaging dependency for inference. [CPython embeddable package](https://docs.python.org/3.13/using/windows.html#the-embeddable-package) · [PyInstaller operating modes](https://pyinstaller.org/en/stable/operating-mode.html)
