//! The audio editor.
//!
//! A thin facade over the [`ramod`] FSB5 audio middleware so the game binary and
//! the `rea_bridge` CLI share exactly one implementation. The FSB5 container
//! parsing/writing, the Vorbis ↔ Ogg tooling and the PCM → Vorbis encoder all
//! live in [`ramod`]; this module re-exports them under
//! `racingedge::editor::audio` and adds the Bevy integration:
//!
//! * [`fsb`] — FSB5 container parser: header, per-sample headers, metadata
//!   chunks (`CHANNELS`, `FREQUENCY`, `LOOP`, `VORBISDATA`, …), name table and
//!   sample payloads.
//! * [`fsb_write`] — FSB5 writer ([`Fsb5Builder`]), in-place [`BankEditor`] and
//!   the RIFF/WAVE reader.
//! * [`vorbis`] — Vorbis ↔ Ogg: the bundled setup-header table, the Ogg
//!   muxer/demuxer, FSB → Ogg export and Ogg → FSB import.
//! * [`vorbis_encode`] — PCM → Vorbis encoding driven through FMOD's own
//!   `libfsbvorbis64.dll` encoder.
//! * [`plugin`] — the Bevy `AudioPlugin`: loads banks at runtime and exposes
//!   preview playback (needs `bevy_audio`, so it is behind `game-audio`).
//! * [`browser`] — the Bevy `BankBrowserPlugin`: an in-game panel listing every
//!   loaded bank and sample (same gate: it previews through `bevy_audio`).
//! * [`sounds_json`] — the `sounds.json` event map (`SoundsJson`).
//!
//! The parsing/editing logic lives in [`ramod`] rather than in the game so the
//! binary and the `rea_bridge` CLI share exactly one implementation.

pub use ramod::{fsb, fsb_write, vorbis, vorbis_encode};

// The Bevy playback half of the editor needs `bevy_audio` for its assets,
// voices and previews, so it follows the `game-audio` feature. A host that
// brings its own mixer (Renzora) keeps every parser and writer here and loses
// only the in-engine preview — see `docs/RENZORA_MIGRATION.md`.
#[cfg(feature = "game-audio")]
pub mod browser;
#[cfg(feature = "game-audio")]
pub mod plugin;
pub mod sounds_json;

#[cfg(feature = "game-audio")]
pub use browser::BankBrowserPlugin;
pub use fsb::{Fsb5, SoundFormat};
pub use fsb_write::{BankEditor, Fsb5Builder, Wav};
#[cfg(feature = "game-audio")]
pub use plugin::AudioPlugin;
pub use sounds_json::{SoundEvent, SoundsJson};
