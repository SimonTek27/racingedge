# RaMOD: what was broken, what changed, what is still unproven

RaMOD is a separate repository (`github.com/SimonTek27/RaMOD`). This work was done on a clone at
`.vendor/ramod` (gitignored) and exported as [`ramod-fixes.patch`](ramod-fixes.patch) — apply it to
RaMOD `main` to reproduce everything below.

## The headline: the workspace did not build at all

`cargo test` on `main` failed before any of this. Not "some features are missing" — the crates could
not be compiled:

| Symptom | Root cause |
| --- | --- |
| `error: this file contains an unclosed delimiter` (runtime) | `runtime/src/runtime_core_b.rs` was an 18-byte file containing `PLACEHOLDER_CORE_B` |
| recovery blobs unusable | `recovery/core_b.z0 + z1` is not a valid zlib stream (`invalid bit length repeat`), and `recovery/core_b.part0` is itself truncated mid-function |
| the split could never parse | even with both parts present, an `impl` block cannot span two `include!` files — rustc requires each included file to be a balanced token stream |
| `error: found invalid character; only '#' is allowed in raw string delimitation` | `runtime/src/live_update.rs` had three `r#{...}"#` literals missing the opening quote |
| `BankName: Default is not satisfied` | `Bank` derives `Default` but `BankName` did not implement it |
| `no method named 'as_str'` for `&(String, f32)` | `SnapshotPlayer::active_name` destructured `|(n)|` from a two-field tuple |
| 3 failing tests in `src/error.rs` | setup-header CRC printed decimal only; `FsbOpenError` had no `Error::source` |

## Fixes in the patch

**Runtime (`runtime/src/`)**

- `runtime_core_a.rs`: the `impl Runtime` block is now closed in its own file; the redundant
  `use` lines that collided with `lib.rs`'s re-exports were removed.
- `runtime_core_b.rs`: rewritten as a self-contained second `impl Runtime`. The recovery fragment
  (`part0`) is preserved as the `render` / profiler / event-registration half, and the methods the
  rest of the workspace calls were reconstructed against the real `EventSystem`, `VoicePool`,
  `Mixer`, `SnapshotPlayer` and `spatial` APIs: `play_event -> Result<u64, RuntimeError>`,
  `release_instance`, `set_listener`, `set_instance_3d`, `set_instance_parameter`, `set_bus_volume_db`,
  `set_bus_mute`, `set_bus_solo`, `trigger_snapshot`, `clear_snapshot`, `instance_count`,
  `sync_event_spatial_gains` (per-voice 3D gain plus Doppler pitch).
- `backend.rs`: `OutputBackend` is now `Send + Sync` so `Runtime` can be a Bevy `Resource`; the cpal
  backend gets a matching, documented `unsafe impl Sync` (the stream is owned and only written from
  the audio callback, the shared ring is mutex-guarded).
- `live_update.rs`, `bank.rs`, `mixer.rs`: the three compile errors above.

**Container / codec (`src/`)**

- `vorbis.rs`: unknown setup-header CRC now prints hex, which is what the test and the docs require.
- `fsb.rs`: `FsbOpenError` gained a real `Error` impl, so `source()` chains instead of returning `None`.

**Waveform editor (`waveform/src/`)**

- `import.rs`: the generic `plane_to_f32<S: Copy>` helper could never work — symphonia does not let you
  name its `Sample` trait from outside the crate. Replaced with a concrete `push_frames` helper plus
  per-branch closures, so all ten sample formats decode.
- `history.rs`: `UndoStack` is `Clone` (required by `WaveBuffer`'s derive).

**Bevy 0.19 → 0.20 (`bevy_ramod/`)**

- `bevy::log` no longer exists: 24 call sites moved to the `log` crate (`log = "0.4"` added).
- `AssetPath::to_path_buf` is gone: `.path().to_path_buf()`.
- `AssetLoader` now requires `TypePath`: derived on `RamodBankLoader`.
- The four examples had an ambiguous `Vec3` (Bevy's vs RaMOD's spatial vector); each now pins
  `use bevy::prelude::Vec3;`.

**Renzora (`renzora_plugin/Cargo.toml`)**

- `bevy_ramod` was resolved from `git = "https://github.com/SimonTek27/RaMOD"` (i.e. whatever HEAD
  happens to be) instead of the crate in this workspace: now `path = "../bevy_ramod"`, still with the
  `renzora` feature.

## Editor pass (this round)

`waveform/src/lib.rs` and `history.rs` gained real undo/redo:

- `EditCommand` now carries the selection and offers two forms: `ReplaceAll` for structural edits
  (cut, paste, delete) and `ReplaceRegion` for in-place effects, so a gain or fade on a long file no
  longer clones the whole buffer per edit.
- `WaveBuffer::redo` and `can_redo` exist; starting a new edit discards the redo branch, as in any
  editor. `UndoStack` exposes `undo_depth`, `redo_depth` and a bounded history.
- Undo/redo swap state through one `apply_command` path, so both directions restore samples and
  selection identically.
- Five new tests: undo then redo of a silence, redo invalidation by a new edit, undo after a
  structural cut (length restored, redo re-cuts), interleaved stereo integrity across a reversed
  region, and the empty-history case.

## Effect chain (this round)

`waveform/src/effects.rs` gained the processing GoldWave-class editors ship, all pure Rust:

- **EQ** — `EqBand` + RBJ peaking biquads (direct form, per-channel state), applied as a chain.
- **Compressor** — `CompressorParams` (threshold, ratio, attack, release, makeup) with a
  stereo-linked peak detector, so the stereo image never shifts.
- **Limiter** — instant attack, 50 ms release, mathematically capped: any frame above the ceiling
  gets exactly `ceiling / peak`, so output cannot exceed it.
- **Time-stretch** — Hann-windowed overlap-add with 50% synthesis hop; duration scales, pitch does
  not.
- **Pitch-shift** — stretch by the pitch factor then resample back, so duration is preserved exactly
  (`resize`/`truncate` to the original frame count).

Integration with undo/redo is deliberate rather than generic: EQ, compressor and limiter are
length-preserving, so they record a **region** command; stretch and pitch change length and record a
**full-buffer** command, with the selection retargeted to the new region so a following edit lands in
the right place.

Seven new tests assert measurable behaviour, not just that code runs: +12 dB on a 1 kHz sine measures
about 4x RMS and undo restores it; a -18 dB cut at 200 Hz leaves 5 kHz within 10%; the compressor
drives a 0.5-peak signal below 0.2; the limiter output never exceeds its ceiling; a 1.5x stretch
lands within 400 frames of 18 000 and undoes to 12 000; an octave-up pitch shift keeps the exact
frame count and increases zero crossings; and a 2x stereo stretch keeps left/right mirrored to 1e-3.

## What was actually run

| Check | Result |
| --- | --- |
| `cargo check -p ramod_runtime --lib --all-features` | pass (was: unclosed delimiter) |
| `cargo test -p ramod --lib` | **47 passed** (was 3 failing) |
| `cargo test -p ramod_waveform --lib` | 2 passed |
| `cargo test -p ramod_runtime --lib --no-default-features` | **6 passed**, including 3 new behavioural tests: `play_event` creates an instance and voice, unknown paths return `Err` and leave nothing behind, and `render_seconds` emits non-zero samples through the mixer |
| `cargo check -p bevy_ramod --all-targets` | pass — library plus all four examples |

## What is still unproven

- **Audible output.** This sandbox has no `libasound`, so any test binary built with the default
  `backend-cpal` feature fails to link (`rust-lld: undefined symbol: snd_pcm_sw_params_current`).
  The runtime tests therefore run on the null backend. Whether cpal actually plays on a real machine
  is untested here.
- **Completeness of the reconstruction.** The lost tail of `runtime_core_b.rs` is gone for good. The
  rebuild covers every method this workspace calls (the compiler is the proof) plus behaviour tests
  for play/render/release, but a method used only by external code would still be missing.
- **Live Update end to end.** The TCP server/client compiles now; it was not exercised against a client.
- **`renzora_plugin`.** It cannot build without an engine checkout at `../../engine/crates/renzora`
  (see `docs/RENZORA.md`); nothing here changes that.
- **Parity with FMOD/Wwise/GoldWave is still 79.2 / 100** (16 of 20 must-haves) per
  `replica/parity.py`. Fixing the build does not create missing editor features; it makes the
  existing ones real and measurable.

## Zoom pass — interactive viewport

The effect chain existed but a selection could only be dragged across the whole file: there was no
zoom anywhere in the tree (`grep -rn zoom --include=*.rs` returned nothing). A 3-minute music bed was
one 800-pixel smear, so the waveform editor could not do the one thing an editor must do.

`waveform/src/view.rs` now has a `Viewport`:

* `start` / `visible` are `f64` frames, not pixels, because a zoomed window rarely lands on a whole
  frame and rounding on every scroll makes the view drift.
* `zoom_at(anchor_frame, factor)` keeps the frame under the cursor fixed — wheel zoom that does not
  lose the edit point.
* `pan_frames` / `pan_px`, `fit`, `zoom_to_selection`, `center_on`, `clamp`, `visible_range`,
  `frame_at_f` / `x_of` for hit-testing.
* `set_total` keeps the view valid when an effect changes the length: a fitted view follows the
  buffer, a zoomed view is clamped inside it instead of pointing past the end.
* Zoom is bounded by the buffer on one side and `MIN_VISIBLE_FRAMES = 16` on the other, so it can
  never collapse to one sample.

`WaveBuffer::overview_range(start_frame, frames, buckets)` paints only the visible span. Without it,
zooming in and out shows an identical picture — the old panel always rasterised the whole buffer into
the same pixels. Second markers are chosen at 1/2/5 × 10^n so the scale stays readable when zoomed.

`editor/src/waveform_panel.rs` wires it up: ctrl+wheel zooms at the pointer, wheel pans, drag-select
converts through the viewport (so a selection is frame-accurate at any zoom), the canvas grew to
160 px, and the toolbar now exposes Apply EQ / Compress / Limit / Shift / Stretch against the effect
APIs plus a Redo button.

### Verified

| Check | Result |
| --- | --- |
| `cargo test -p ramod_waveform --lib` | **27 passed** (10 new: zoom anchor stability, clamping, fit, zoom-to-selection, buffer-length tracking, empty buffer, zoomed envelope, viewport span) |
| `cargo test -p ramod --lib` | 47 passed |
| `cargo fmt --check -p ramod_waveform` | clean |
| `rustfmt --check editor/src/waveform_panel.rs` | clean |
| `cargo clippy -p ramod_waveform --lib` | exit 0 (3 pre-existing warnings in unrelated files) |

### Found while verifying: the editor crate does not build

`cargo check -p ramod_editor` reports **34 errors that predate this pass**, spread over
`audio_bridge.rs` (8), `main.rs` (29 minus my file), `event_editor.rs` (4), `profiler_panel.rs` (4),
`project.rs` (1). `waveform_panel.rs` itself is error- and warning-free.

The shape is the same as the lost `runtime_core_b`: `audio_bridge.rs` is missing ~25 methods the UI
already calls (`play_pcm`, `pump`, `set_parameter`, `master_levels`, `profiler_snapshot`,
`capture_fx_chain`, `apply_fx_chain`, the five `add_bus_*` insert helpers, the snapshot helpers),
and its exported types (`ProfilerSnapshot`, `BusInsertInfo`, `FxPreset`) are gone; `Project` lost
`path`, `add_asset_file`, `add_track`, `add_instrument`; `TrackEntry::new` is missing and
`PlaylistEntry` has no `PartialEq`. So the zoom work is type-checked but **not runnable**, which is
why must-have 15 stays `partial` in the matrix rather than being promoted on unit tests alone.

## Publishing the repairs (and why it is not pushed yet)

The repairs were applied to a **clean checkout of RaMOD `main`**, not just the working clone:

```
git worktree at f941b56  ->  git am replica/ramod-repair.patch  ->  18436a2
```

`replica/ramod-repair.patch` is a `git format-patch` export, so `git am` lands it as one signed-off
commit with its message intact instead of a bare diff. It was re-applied to a second pristine
worktree to confirm the round trip.

| Check in the clean checkout (`f941b56` + patch) | Result |
| --- | --- |
| `git am replica/ramod-repair.patch` | clean, commit `18436a2` |
| `cargo test -p ramod --lib` | 47 passed |
| `cargo test -p ramod_waveform --lib` | 27 passed |
| `cargo test -p ramod_runtime --lib --no-default-features` | 6 passed (null backend) |

**The push is blocked by credentials, not by the code.** This workspace's GitHub credential is
scoped to the connected repository (`racingedge`). `git push` to `SimonTek27/RaMOD` from the clone
fails with `fatal: could not read Username for 'https://github.com'`, and asking for a token to
paste in is not something this workflow should do.

To land it, either connect `SimonTek27/RaMOD` as the repository for a chat and ask for the push
again, or from a local RaMOD clone that is already on `main` at `f941b56`:

```
git am /path/to/ramod-repair.patch
git push origin HEAD:refs/heads/fix/restore-build-and-effects
```

`replica/ramod-fixes.patch` remains as the plain-diff form (verified earlier with `git apply`).
