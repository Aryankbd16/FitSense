# ADR 002 · Gate personal persistence on encryption

**Status** Accepted Phase 1 boundary. Phase 2 implements it through [ADR 003](003-database-encryption.md) and [ADR 004](004-key-management-and-recovery.md).

Plain SQLite with Windows file permissions supports a technical schema but cannot protect a copied database. BitLocker or EFS can add host protection, but each depends on machine configuration and offers no FitSense-managed recovery for portable copies. Per-field encryption can leave metadata, schema and paging artifacts exposed and complicates queries. SQLCipher provides an implementable page-level path for the SQLite database. Photographs and backups require authenticated encryption of their own.

Keep the Phase 1 foundation database separate from any future private database. Require a verified encrypted provider and a successfully unlocked random key before private persistence opens or creates files. Use a user-scoped Windows DPAPI wrapper for the installed profile, with a separate recovery envelope for export and migration. Validate Windows x64 builds, licensing, correct/wrong-key behavior and the handling of temporary data before activation. [SQLCipher API](https://www.zetetic.net/sqlcipher/sqlcipher-api/index.html) · [Windows DPAPI](https://learn.microsoft.com/en-us/windows/win32/api/dpapi/nf-dpapi-cryptprotectdata)
