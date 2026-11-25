# ADR 006 · Stream an authenticated backup and publish restores atomically

Export a versioned binary package with an AEAD-encrypted manifest, recovery envelope, complete encrypted database and all referenced encrypted asset files. Use HMAC-SHA-256 under a dedicated derived key for the complete package. Stream and authenticate the files without writing any decrypted data to disk. The archive has fixed bounds and accepts only the encrypted database and known relative asset names. The DPAPI wrapper stays local.

The vault lease and serialized connection prevent writes during export. The DELETE rollback journal avoids dependence on a separate WAL. Require a clean journal and verify the database and every reference before export. Write a temporary encrypted package, sync and publish without replacing an existing backup.

Restore extracts encrypted bytes to a sibling stage. Verify authentication, database version, SQLite integrity, the image inventory and every image tag and hash before publishing. Generate a new DPAPI wrapper on the destination. Restore only into an unused vault, with a single same-volume directory rename. An existing vault is left untouched. Cancellation or failure before publication leaves the active vault untouched.
