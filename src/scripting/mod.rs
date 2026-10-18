//! Mod SDK scripting (port of ksengine `engine/Scripting/`, roadmap 3.4).
//!
//! ksengine embeds Lua and exposes a small prelude (`ks.on` / `ks.get` /
//! `ks.set` / `ks.log` / `ks.version`) plus an event `dispatch`. RacingEdge
//! keeps the same shape but runs scripts on [`rhai`], a pure-Rust engine, so
//! the crate builds with no C toolchain.
//!
//! * [`ScriptHost`] — the engine-facing facade. Everything else talks to this
//!   type only, so swapping the backend never ripples past this module.
//! * [`mod_sdk`] — the mod-facing helpers: script discovery/load order
//!   ([`load_mod_scripts`]), event [`dispatch`], [`take_log`] and the
//!   [`ScriptHotReloader`].
//!
//! Mod scripts live in `<modRoot>/scripts/*.rhai` (or `<modRoot>/*.rhai` when
//! there is no `scripts/` folder) and are run in filename order. A script opts
//! into an event by defining a global `fn on_<event>(numbers…, strings…)`, or by
//! registering any function name with `ks::on("<event>", "<fn>")`.
//!
//! ```
//! use racingedge::scripting::ScriptHost;
//!
//! let mut host = ScriptHost::new();
//! assert!(host.initialize());
//! host.set_number("speed", 100.0);
//! assert!(host.eval("ks::set(\"speed\", ks::get(\"speed\") + 10.0)"));
//! assert_eq!(host.get_number("speed"), Some(110.0));
//! ```

pub mod mod_sdk;
pub mod script_host;

pub use mod_sdk::{dispatch, install, load_mod_scripts, take_log, LoadReport, ScriptHotReloader};
pub use script_host::ScriptHost;
