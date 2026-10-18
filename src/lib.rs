//! racingedge — a Rust racing game and modding suite.
//!
//! The crate exposes two building blocks used by both the game binary and the
//! modding CLI tools:
//!
//! * [`editor`] — the content-modding editor suite (the audio editor today),
//!   built on the formats recovered by REA reverse engineering.
//! * [`plugins`] — the game-side Bevy plugins (physics).
//! * [`net`] — runtime networking and telemetry (ksengine port).
//! * [`simulator`] — the runtime vehicle simulator (ksengine port).
//! * [`c_api`] — the C ABI for headless hosts (`include/racingedge.h`).

pub mod c_api;
pub mod editor;
pub mod formats;
pub mod net;
pub mod plugins;
pub mod scripting;
pub mod simulator;
