# REA findings — FSB5 codec of the stock authoring tool (v2.03.13)

Reverse engineering performed with [REA](https://github.com/morluto/rea)
(`rea-agents` 6.5.0) using **Ghidra 12.1.4** as the native analysis provider.

## Target inventory

| Component | Size | Role |
| --- | --- | --- |
| `FMOD Studio.exe` | 77 KB | launcher |
| `fmodstudio.exe` | 75 KB | launcher |
| `fmodstudioui.dll` | 23 MB | main authoring UI + engine |
| `libfsbvorbis64.dll` | 1.7 MB | **FSB Vorbis codec** (decode/encode) |
| `opus.dll` | 472 KB | Opus codec |
| `xmaencoder64.dll` | 118 KB | XMA encoder |
| `libfsbvorbis64.dll` | 1.7 MB | Vorbis FSB codec |

## Key finding 1 — the FSB Vorbis codec is stock Xiph libvorbis

`rea analyze libfsbvorbis64.dll` reported 452 procedures and 195 strings.
A `rea search --kind procedures` for codec keywords returned the complete
**stock `libvorbis` public API** (SHA-256
`b9bd6cd999f52bec64324e7d447bef8cbf215b0b00c0e03b7b5b47f2a021a63b`):

```
vorbis_info_init            vorbis_info_clear        vorbis_info_blocksize
vorbis_comment_init         vorbis_comment_clear     vorbis_comment_add
vorbis_comment_add_tag      vorbis_comment_query     vorbis_comment_query_count
vorbis_analysis_init        vorbis_analysis_buffer   vorbis_analysis_wrote
vorbis_analysis_blockout    vorbis_analysis_headerout vorbis_analysis
vorbis_bitrate_addblock     vorbis_bitrate_flushpacket
vorbis_synthesis_init       vorbis_synthesis_blockin vorbis_synthesis_pcmout
vorbis_synthesis_read       vorbis_synthesis_restart vorbis_synthesis
vorbis_synthesis_idheader   vorbis_synthesis_headerin
vorbis_synthesis_halfrate   vorbis_synthesis_halfrate_p
vorbis_synthesis_trackonly  vorbis_synthesis_lapout
vorbis_synthesis_pcmout     vorbis_packet_blocksize
vorbis_granule_time         vorbis_window
vorbis_dsp_clear            vorbis_block_init        vorbis_block_clear
vorbis_encode_init          vorbis_encode_init_vbr   vorbis_encode_setup_init
vorbis_encode_setup_managed vorbis_encode_setup_vbr  vorbis_encode_ctl
vorbis_version_string
```

### What this means

* The tool does **not** ship a custom Vorbis implementation. FSB5 `VORBIS` samples
  are ordinary Vorbis streams (identification + comment + setup headers, then
  audio packets) decoded by the reference `libvorbis`.
* The embedded vendor strings pin the exact upstream release:

  ```
  "Xiph.Org libVorbis I 20101101 (Schaufenugget)"
  "Xiph.Org libVorbis 1.3.2"
  ```

  i.e. the authoring tool ships **libvorbis 1.3.2 verbatim**.
* The `.fsb` container is only a **wrapper**: it stores the two Vorbis headers
  once (in the per-sample `VORBISDATA` metadata chunk) and then a sequence of
  length-delimited audio packets, each prefixed with a CRC32.
* Consequence for the Rust editor: FSB Vorbis samples can be decoded with a
  stock Rust Vorbis decoder such as [`lewton`](https://crates.io/crates/lewton)
  or [`vorbis`](https://crates.io/crates/vorbis) — no vendor-specific DSP is
  required.
* The same DLL also exports the **complete encoder** API
  (`vorbis_encode_init_vbr`, `vorbis_analysis_buffer/wrote/blockout/headerout`,
  `vorbis_bitrate_addblock/flushpacket`, `vorbis_comment_*`).
  [`ramod::vorbis_encode`](https://github.com/SimonTek27/RaMOD/blob/main/src/vorbis_encode.rs) therefore calls the bundled
  libvorbisenc through raw FFI (`LoadLibraryW` + `GetProcAddress`) to encode
  PCM → Vorbis **without adding a crate**. Each VBR quality level reproduces one
  of the 161 bundled setup headers, so every encoded bank stays decodable by the
  reference decoder; this was verified by decoding the output with the same DLL
  (mono and stereo).

## Key finding 2 — FSB5 container layout (cross-checked)

The container layout implemented in [`ramod::fsb`](https://github.com/SimonTek27/RaMOD/blob/main/src/fsb.rs) matches the
publicly documented FSB5 binary template and the packet/header split above:

* Header (60 bytes for version ≥ 1): magic `FSB5`, version, sample count,
  sample-header size, name-table size, data size, mode.
* Per-sample header: a packed 64-bit word
  (`extraParams:1`, `frequency:4`, `twoChannels:1`, `dataOffset:28` in 16-byte
  units, `samples:30`), followed by optional metadata chunks:
  `CHANNELS`, `FREQUENCY`, `LOOP`, `XMASEEK`, `DSPCOEFF`, `XWMADATA`,
  `VORBISDATA` (11).
* Name table: `u32` offsets + NUL-terminated names, padded so the data section
  starts on a **32-byte boundary** (`60 + sampleHeaderSize + nameTableSize` is a
  multiple of 32 in every reference bank examined).
* Data section: sample payloads, positions taken from the per-sample
  `dataOffset`. Every sample is **32-byte aligned** (all observed `dataOffset`
  values are multiples of 32, i.e. even 16-byte units) and the section is padded
  up to a 32-byte boundary (`dataSize % 32 == 0`).
* Every reference-written sample carries a final 4-byte metadata chunk of type
  **13** whose payload is a little-endian `f32` peak/volume value (`32767/32768`
  for a full-scale sample, `0.1` for a quiet ambience). It looks like authoring
  metadata and is not needed to decode the audio; this crate does not emit it
  and still round-trips real banks byte-for-byte.
* The header's 8-byte `zero` field is usually zero (it is `1` in the review
  `showroom.bank`), while the 16-byte `hash` and 8-byte `dummy` fields hold
  reference build values. All three are preserved on re-serialisation and
  ignored by the loader.

For `VORBIS` banks each packet in the data section is length-prefixed with a
`u16` size followed by the raw packet bytes; the `VORBISDATA` chunk carries the
shared setup-header key and a (sparse) seek table. See key finding 3.

## Key finding 3 — FSB5 Vorbis packet framing and setup headers

The authoritative reference *decoder* (`reference/reference_vorbis.py`, the
`python-fsb5` `vorbis.rebuild` routine) and a genuine reference bank agree on
the exact wire format:

```
VORBISDATA chunk payload:
    u32 setup_crc32                       // keys one of the setup headers
    repeat { u32 offset; u32 granulePos } // packet seek table (may be sparse)

VORBIS sample data section:
    repeat {
        u16 size;                         // 0 terminates the stream
        u8  data[size];                   // raw Vorbis packet (no flags byte)
    }
```

* **There is no separate flags byte.** The first byte after the length prefix is
  the first byte of the Vorbis packet itself — its low bit is Vorbis' own
  audio/header discriminator, which is zero for every audio packet. Verified
  against a real bank extracted from `showroom.bank`: feeding its packets to the
  stock `libfsbvorbis64.dll` rejects **2108 / 5142** packets when a flags
  byte is assumed, but **0 / 5142** when all `size` bytes are treated as the
  packet. *Caveat:* the 010-Editor template `reference_FSB5.bt` declares
  `ubyte audio:1; ubyte r:7; ubyte data[size-1]`, which is incorrect here; the
  Python reference decoder is the correct one.
* The seek table maps a byte offset (into the sample data section) to the granule
  position of the packet that starts there. The reference table is **sparse** —
  roughly one entry per second of audio — while this crate writes one entry per
  packet; both are valid, and every offset points at a packet boundary.

* The format does **not** store the Vorbis setup header in the bank. It ships a
  fixed set of **161 pre-built setup headers** (identification of the codebook)
  and stores only a **CRC32 key** in the `VORBISDATA` chunk. The setup headers
  are recovered from a lookup table indexed by that CRC32.
* The key is the **standard zlib CRC32** (polynomial `0xedb88320`, reflected,
  init/final `0xffffffff`) of the **complete setup packet**, including its
  `0x05 "vorbis"` signature bytes. Verified: every key in
  `reference/reference_vorbis_headers.py` equals `zlib.crc32(payload)`, and
  `vorbis::zlib_crc32` reproduces it (see the `zlib_crc_matches_bundled_header_keys`
  test). This makes it possible to import an arbitrary Ogg Vorbis file: compute
  the CRC32 of its setup packet and accept it when it matches a bundled header.
* The setup headers were produced by libvorbis 1.3.2, so Ogg files authored with
  a compatible libvorbis build carry matching setup headers and can be wrapped
  into an FSB5 `VORBIS` sample (`vorbis::import_ogg_to_bank`).
* The identification header must be synthesised from the sample's channel
  count and sample rate (the bank does not store block sizes; the format uses the
  short/long block sizes 256 / 2048).
* The comment header is empty.

### Implication for the Rust editor

A complete FSB Vorbis decode pipeline is therefore:

1. Parse the bank with [`ramod::fsb`](https://github.com/SimonTek27/RaMOD/blob/main/src/fsb.rs) → sample metadata + packets.
2. Read `setup_crc32` from `VORBISDATA` and load the matching setup header.
3. Feed the synthesised identification header, an empty comment header and the
   setup header into a Rust Vorbis decoder
   ([`lewton`](https://crates.io/crates/lewton), or the `vorbis`/`ogg` crates),
   then stream the audio packets from `Fsb5::vorbis_packets` (in `ramod`).
4. Re-encode with the same setup-header id to keep byte-compatibility, or emit
   a standalone Ogg Vorbis file for other tools.

The 161 setup-header keys observed in the reference table are stored in
`reference/reference_vorbis_headers.py` and converted to the compact binary
asset `assets/vorbis_setup_headers.bin` by
`tools/build_vorbis_setup_headers.py`. The Rust loader now lives in the
[`ramod`](https://github.com/SimonTek27/RaMOD) crate (`src/vorbis.rs`), which
racingedge re-exports through `src/editor/audio/`.

## Reproducing the analysis

```cmd
tools\rea_analysis.cmd
```

or manually (see [`docs/REA_GUIDE.md`](docs/REA_GUIDE.md)):

```cmd
npx -y rea-agents@latest analyze "C:\Program Files\FMOD SoundSystem\FMOD Studio 2.03.13\libfsbvorbis64.dll" --format json
npx -y rea-agents@latest search  "C:\Program Files\FMOD SoundSystem\FMOD Studio 2.03.13\libfsbvorbis64.dll" --pattern "vorbis|sample|decode|channel|packet|codec|fsb" --kind procedures --mode regex --format json
```

Raw evidence is stored in [`evidence/`](evidence/):

* `fmod_fsbvorbis_analysis.json` — binary overview (452 procedures, 195 strings)
* `rea_procedures.json` — full libvorbis API inventory
* `rea_vorbis_strings.json` — codec string literals (identifies libvorbis 1.3.2

## Not yet analysed

`fmodstudioui.dll` (23 MB) exceeds REA's ~330 s Ghidra budget and aborts with
`provider_timeout`. Deeper verification of the FSB *container* code would
need an incremental `--snapshot` run; the container format is already confirmed
independently by the FSB5 binary template in `reference/`.
