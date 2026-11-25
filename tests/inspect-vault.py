"""Create a synthetic image fixture and prove a finished vault is unreadable as SQLite."""

import argparse
import mmap
import sqlite3
import struct
import zlib
from pathlib import Path


def chunk(name, payload):
    return struct.pack(">I", len(payload)) + name + payload + struct.pack(">I", zlib.crc32(name + payload))


def fixture(path):
    header = struct.pack(">IIBBBBB", 400, 260, 8, 2, 0, 0, 0)
    pixels = (b"\0" + bytes([59, 73, 95]) * 400) * 260
    path.write_bytes(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header)
                     + chunk(b"IDAT", zlib.compress(pixels)) + chunk(b"IEND", b""))


def verify(path):
    database = path / "wardrobe.sqlite3"
    raw = database.read_bytes()
    assert not raw.startswith(b"SQLite format 3\0")
    assert b"Offline linen shirt" not in raw
    try:
        with sqlite3.connect(database.as_uri() + "?mode=ro", uri=True) as connection:
            connection.execute("SELECT * FROM garments").fetchall()
    except sqlite3.DatabaseError as error:
        assert "not a database" in str(error)
    else:
        raise AssertionError("Stock SQLite read private wardrobe data")
    for variant in ("originals", "display", "thumbnails"):
        assets = list((path / "images" / variant).iterdir())
        assert len(assets) == 1, f"Wrong encrypted asset inventory for {variant}"
        found = assets[0].read_bytes()
        assert found.startswith(b"FSIMAGE1"), f"Missing encrypted asset header for {variant}"
        assert b"\x89PNG\r\n\x1a\n" not in found
    sentinels = (
        b"Offline linen shirt",
        b"Offline indigo shirt",
        b"FitSense smoke test recovery phrase",
    )
    for file in path.parents[1].rglob("*"):
        if file.is_file() and file.stat().st_size:
            with file.open("rb") as handle, mmap.mmap(handle.fileno(), 0, access=mmap.ACCESS_READ) as contents:
                assert all(contents.find(sentinel) < 0 for sentinel in sentinels), f"Plaintext in {file.name}"
    print("Stock SQLite rejected the database. All photo variants are encrypted and no fixture secrets appeared in workspace files.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--fixture", type=Path)
    group.add_argument("--verify", type=Path)
    options = parser.parse_args()
    if options.fixture:
        fixture(options.fixture)
    else:
        verify(options.verify.resolve())
