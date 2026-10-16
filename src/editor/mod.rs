//! The racingedge content-editing suite.
//!
//! Each submodule is a self-contained editor for one kind of game content,
//! shared by the game binary and the `rea_bridge` modding CLI. The editors are
//! built on top of the formats recovered by REA reverse engineering of the
//! FMOD toolchain.
//!
//! * [`audio`] — the audio editor: FSB5 bank parsing/writing/editing, Vorbis
//!   ↔ Ogg conversion and the in-game bank browser.
//! * [`track`] — track data: `data.json` metadata and the `surfaces.ini` grip
//!   grid.
//! * [`ini`] — the small ordered INI reader shared by the content editors.

pub mod audio;
pub mod ini;
pub mod track;
