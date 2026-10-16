"""Convert the reference FMOD/libvorbis setup-header table into a compact
binary asset usable by the racingedge Rust audio editor.

Input : reference_vorbis_headers.py  (a Python dict: crc32 -> setup header bytes)
Output: assets/vorbis_setup_headers.bin

Binary format (little-endian):
    u32 entry_count
    repeat entry_count times:
        u32 setup_crc32
        u32 length
        u8  bytes[length]
"""

from __future__ import annotations

import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "reference" / "reference_vorbis_headers.py"
TARGET = ROOT / "assets" / "vorbis_setup_headers.bin"


def load_table() -> dict[int, bytes]:
    namespace: dict[str, object] = {}
    source_text = SOURCE.read_text(encoding="utf-8-sig")
    exec(compile(source_text, str(SOURCE), "exec"), namespace)
    lookup = namespace.get("lookup")
    if not isinstance(lookup, dict):
        raise SystemExit(f"{SOURCE} did not define a dict named 'lookup'")
    return lookup


def main() -> int:
    table = load_table()
    TARGET.parent.mkdir(parents=True, exist_ok=True)

    lengths = [len(v) for v in table.values()]
    with TARGET.open("wb") as handle:
        handle.write(struct.pack("<I", len(table)))
        for crc32, payload in table.items():
            handle.write(struct.pack("<II", crc32 & 0xFFFFFFFF, len(payload)))
            handle.write(payload)

    total = sum(lengths)
    print(f"entries      : {len(table)}")
    print(f"payload bytes: {total}")
    print(f"min/max entry: {min(lengths)} / {max(lengths)}")
    print(f"asset size   : {TARGET.stat().st_size} bytes")
    print(f"written      : {TARGET}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
