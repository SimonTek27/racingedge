# Recon map — RaMOD vs FMOD / Wwise / GoldWave

Produced with the method (and the `parity.py` tool) from
[replica-skill](https://github.com/Jakeschincariol/replica-skill): recon →
architect → build → diff. The subject is **RaMOD**
([repo](https://github.com/SimonTek27/RaMOD)), the references are the commercial
audio stacks, and the goal is *parity where it matters, then beat them on the
things their users complain about* — clean-room: features and flows, not their
code, assets or trade dress.

## Scope

| | |
| --- | --- |
| Product | RaMOD, an open-source FSB5/FMOD-class audio stack for Rust and Bevy |
| Slice | The three jobs the goal names: **runtime mixing** (FMOD/Wwise), **bank authoring** (FMOD Studio / Wwise), **waveform editing** (GoldWave) |
| For | Bevy and Renzora projects that cannot ship FMOD or Wwise (licence, cost, console SDKs, GPL, or a platform that those vendors do not target) |
| Not in scope | Their authoring GUI design, bundled content, console SDKs, partner networks — `skip` rows in the matrix |

## Sources

| Source | What it gave |
| --- | --- |
| [RaMOD README](https://github.com/SimonTek27/RaMOD/blob/main/README.md) | shipped features (parse/build/edit/extract/import/encode), dependency policy |
| [RaMOD ROADMAP](https://github.com/SimonTek27/RaMOD/blob/main/ROADMAP.md) | the 50-item gap list this matrix is graded against (items 1–50) |
| [RaMOD ARCHITECTURE](https://github.com/SimonTek27/RaMOD/blob/main/ARCHITECTURE.md) | module boundaries (runtime, Bevy, editor) |
| [RaMOD docs/RENZORA.md](https://github.com/SimonTek27/RaMOD/blob/main/docs/RENZORA.md) | the Renzora integration that exists today, and its Bevy 0.19 pin |
| [RaMOD Cargo.toml](https://github.com/SimonTek27/RaMOD/blob/main/Cargo.toml) | workspace members (`editor`, `runtime`, `bevy_ramod`, `waveform`) and feature flags |
| [GoldWave features](https://www.goldwave.com/features.php) | first-hand editor feature list (control window, effects, batch, effect chain, noise reduction, expression evaluator) |
| FMOD Studio and Wwise | **not fetched this session** — their product pages render client-side and returned no readable text. Their rows in `features.csv` record long-documented, non-controversial capabilities (event system, buses, DSP, 3D, virtual voices, profiling, snapshots, live connect) from their public documentation. Treat those cells as vendor-documented, not re-verified today. |

## Module inventory (the "screens" of a library)

| ID | Module | Job | Evidence |
| --- | --- | --- | --- |
| M01 | `src/fsb.rs`, `src/fsb_write.rs` | FSB5 read/write, self-verifying writer | README, Cargo.toml |
| M02 | `src/vorbis.rs`, `src/vorbis_encode.rs`, `src/vorbis_decode.rs` | Ogg muxing, pure-Rust encode/decode | features `encoder-rust`, `decoder-rust` |
| M03 | `src/error.rs` | error types | ROADMAP #3 open (richer context) |
| M04 | `runtime/` | mixer, cpal device | workspace member; ROADMAP Phase 1 |
| M05 | `editor/` | authoring tool | workspace member; scope unverified |
| M06 | `waveform/` | waveform view | workspace member; scope unverified |
| M07 | `bevy_ramod/` | Bevy plugin, ECS components | README, docs/RENZORA.md |
| M08 | `renzora_plugin/` | Renzora native plugin (marketplace id `ramod`) | docs/RENZORA.md |
| M09 | `src/bin/ramod.rs` | CLI (`list`, `info`, `extract`) | README |

## Flows (the core loops)

```
F01 Game plays an event and hears it          M01 bank -> M04 mixer -> device
F02 Designer edits a bank in place            M01 -> RAMOD editor -> M01
F03 User edits audio and writes it back       M06 waveform -> effects -> M01
F04 Bevy game swaps bevy_audio for RaMOD      M07 -> M04
F05 Renzora game swaps Kira for RaMOD         M08 -> M04
F06 Live tweak while the game runs            editor <-> :17420 <-> M04
```

Happy-path click counts do not apply to a library; the equivalent metric is
**calls to first sound**: RaMOD is `replace_renzora_audio_with_ramod()` + one
spawn (2 in `docs/RENZORA.md`) versus FMOD/Wwise's bank-load + event-instance +
parameter calls with a proprietary engine linked.

## Inferred data model (confidence)

* **Fsb5** → header (magic, version, counts, mode), samples[], name table, data — *high* (documented layout).
* **SoundSample** → name, frequency, channels, frames, loop range, codec chunk, offset — *high*.
* **Rebank/event** (`bevy_ramod`) → event id, layers, rpm/parameter curves — *medium* (docs + code, not inspected).
* **Bus** → master/sfx/music tree — *medium* (constants exist; hierarchy depth unverified).
* **Voice** → clip, bus, spatial position, priority, virtual flag — *low*: ROADMAP #15 shows voice state does not exist yet.

## What cannot be cloned (`skip`)

* Console platform SDKs and FMOD/Wwise's licensed runtime binaries.
* Their authoring UI's look, bundled demo content, marketplace assets and docs.
* The vendor network effects (integration guides, middleware partnerships).

## Size

S–L depending on slice. Runtime parity alone (mixer, virtual voices, DSP graph,
3D, events, parameters) is the quarter-scale piece; the GoldWave-style editor is
another; bank authoring and the Renzora path are largely **done**.

## Now

Run `python3 replica/parity.py replica/features.csv` for the number, then build
the missing **must** rows in order — the first three are virtual voices, the DSP
node graph, and named parameters with automation.
