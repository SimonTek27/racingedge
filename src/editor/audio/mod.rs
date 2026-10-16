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
//! * [`plugin`] — the Bevy [`AudioPlugin`]: loads banks at runtime and exposes
//!   preview playback.
//! * [`browser`] — the Bevy [`BankBrowserPlugin`]: an in-game panel listing every
//!   loaded bank and sample.
//! * [`sounds_ini`] — the `sounds.ini` event map (`SoundsIni`).
//!
//! The parsing/editing logic lives in [`ramod`] rather than in the game so the
//! binary and the `rea_bridge` CLI share exactly one implementation.

pub use ramod::{fsb, fsb_write, vorbis, vorbis_encode};

pub mod browser;
pub mod plugin;
pub mod sounds_ini;

pub use browser::BankBrowserPlugin;
pub use fsb::{Fsb5, SoundFormat};
pub use fsb_write::{BankEditor, Fsb5Builder, Wav};
pub use plugin::AudioPlugin;
pub use sounds_ini::{SoundEvent, SoundsIni};
