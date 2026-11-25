# Phase 2 implementation plan

1. Preserve the Phase 1 workspace resolver, technical database, React shell and worker lifecycle. Verify SQLCipher on Windows before connecting personal-data commands.
2. Add a private Rust vault with versioned migrations, Windows DPAPI, and an independent passphrase recovery envelope. Test wrong keys, integrity and failure behavior first.
3. Add transactional garment storage, catalogue validation, and an encrypted photo pipeline with immutable files and orphan cleanup.
4. Implement authenticated local backup and restore into an unused vault with a staged, atomic publication step. Test interrupted and tampered restores.
5. Extend the existing React navigation with Wardrobe and connect setup, management and recovery through scoped Tauri commands. Keep Python independent.
6. Run the application and the existing and new suites offline. Update the completion report with actual results for installed, portable and cross-account operation.
