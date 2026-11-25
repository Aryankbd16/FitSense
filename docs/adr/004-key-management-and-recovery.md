# ADR 004 · Pair user-scoped DPAPI with a recovery passphrase

Use OS randomness to generate a 256-bit master key. Derive independent SQLCipher, image and backup-authentication keys with HKDF-SHA-256 and versioned purpose labels. Protect the local wrapper with Windows CurrentUser DPAPI and suppress Windows credential dialogs. The local wrapper is optional in exported backups and is excluded from the backup format.

Use Argon2id v19, 64 MiB memory, three passes, one lane, a fresh 128-bit salt, and a 256-bit wrapping key. Wrap the master key with XChaCha20-Poly1305 and bind the format header and salt as associated data. Passphrases are never persisted. On another account, the recovery passphrase authenticates the envelope, the database verifies the recovered key, and FitSense installs that account's DPAPI wrapper. Changing the passphrase rewraps the same master key with fresh salt and nonce. Existing backups still need the passphrase in use at export.

Machine-scope protection would let other accounts unwrap the local key. DPAPI alone does not provide portable recovery. Losing both the local DPAPI scope and the recovery passphrase means losing access. [Windows DPAPI](https://learn.microsoft.com/en-us/windows/win32/api/dpapi/nf-dpapi-cryptprotectdata) · [Argon2 recommendations](https://www.rfc-editor.org/rfc/rfc9106.html)
