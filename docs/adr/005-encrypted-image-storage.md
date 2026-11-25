# ADR 005 · Store immutable authenticated image variants

Rust accepts bounded JPEG and PNG uploads, checks the real format and dimensions before decoding, applies orientation to derived images, and emits a JPEG display image and thumbnail. It retains the original byte sequence. Decoding and encryption use memory. The browser receives bounded binary variants as transient blob URLs.

Store assets by opaque relative reference in the private vault. XChaCha20-Poly1305 uses a fresh 192-bit random nonce for each object. The format version and reference are associated data, so moving a ciphertext to another slot fails verification. Store expected SHA-256, byte and pixel sizes, variant, MIME type and processing version in SQLCipher. Validate authentication and hash before returning an image.

Publish immutable encrypted files before the linking transaction. A successful replacement or deletion commits references first. Cleanup removes only well-formed, unreferenced names under known directories. A vault lock serializes cleanup and publishing. Unreferenced ciphertext left by a crash is collected on the next open.
