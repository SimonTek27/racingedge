# Migrating racingedge onto Renzora

Renzora ([renzora.com](https://renzora.com/), [engine](https://github.com/renzora/engine))
is a game engine **and visual editor built on Bevy**. This document records the
verified facts about integrating it, what is already done in this repository, and
the remaining steps. It is a migration plan, not a claim that the game runs under
Renzora today.

## Verified facts

Gathered 2026-10-28 from the Renzora repository and crates.io, because the answer
decides what "alongside Bevy" can even mean:

| Question | Answer |
| --- | --- |
| Is there a `renzora` library to depend on? | **No.** The crates.io `renzora` 0.5.0 crate is a **CLI binary** (`has_lib: false`, sole bin `renzora`): `renzora new/init/run/build/check/add/publish`. |
| What is the engine? | A git **workspace**: `renzora_app` (the `renzora` binary), `crates/renzora` (contracts rlib, unpublished, `0.1.0`), `renzora_runtime`, dozens of `renzora_*` plugin crates and vendored Bevy ecosystem forks. Built with their `cargo renzora` xtask, which stages `dist/`. |
| How does a plugin register? | A normal Bevy `Plugin`. Compiled-in plugins (`crates/`) declare `renzora::add!(MyPlugin);`; `cargo renzora sync` scans those lines and regenerates the `[dependencies]` edge plus the `add_plugins` list. Native plugins (`plugins/`, installed from the marketplace) declare `renzora::plugin!(Thing);` and are dylibs. |
| Bevy version | Released builds (r1-alpha7) are on **Bevy 0.19.1**; `main`'s `[workspace.dependencies]` already pins **`bevy = "0.20"`**, i.e. the same series we track. |
| Bevy feature set | `default-features = false` with a hand-written maximal list that deliberately **omits `bevy_audio`** — "Renzora's audio is its own (`plugins/audio`)". |
| Plugin ABI | Plugin dylibs bind to ONE shared `bevy_dylib`/`renzora_dylib` (`dynamic_linking` on by default), because Rust `TypeId`s differ between separately linked copies of Bevy. |
| License | **MIT OR Apache-2.0** — compatible with our GPL-3.0-or-later; code may be ported with attribution. |
| Project layout | `renzora new my-game` clones the engine; the CLI drives a pinned Docker toolchain. Scenes are `.bsn` (`.ron` legacy); scripts are `.lua` / `.rs`. |

Two consequences are load-bearing:

1. **Nothing Renzora ships can be added as a dependency here.** Its contracts
   crate is unpublished and every member inherits `bevy`/`renzora` from *its*
   workspace, so a plugin only builds inside a Renzora checkout.
2. **`bevy_audio` is not part of a Renzora build.** Our playback layer
   (`simulator::audio_playback`, `AudioPlayer`/`AudioSink`) cannot run there
   unmodified; the mix models can, because they are plain data.

## Done in this repository

* **One plugin for the whole game layer.** `racingedge::plugins::RacingEdgeGamePlugin`
  composes `PhysicsPlugin` + `GamePlugin`. A host adds that single plugin; the
  binary already does, and the Renzora wrapper does too.
* **The audio split.** `game-audio` is a default feature; with
  `--no-default-features` the crate builds without `simulator::audio_playback`
  and `CarSoundPlugin` installs no `bevy_audio` voices, while `CarSound` /
  `CommonSound` (the mix models) stay available for a host mixer.
* **The Renzora wrapper crate** in [`renzora/racingedge_plugin/`](../renzora/racingedge_plugin):
  `RacingEdgePlugin` + `renzora::add!(RacingEdgePlugin, Runtime);`. It is
  deliberately outside this package (`exclude = ["renzora/"]`, own `[workspace]`)
  because its `bevy = { workspace = true }` and `renzora = { path = "../renzora" }`
  only resolve inside a Renzora checkout. **CI does not build it.**

## Remaining steps

1. **Get a Renzora checkout and build it.**
   ```sh
   cargo install renzora
   renzora new racingedge-renzora
   cd racingedge-renzora && renzora init && renzora run
   ```
   Linux needs the dev headers from Renzora's README (`libx11-dev`,
   `libwayland-dev`, `libxkbcommon-dev`, `libasound2-dev`, `libudev-dev`); the
   sandbox this repo was prepared in cannot install them, so the engine has never
   been compiled here.
2. **Drop the wrapper in.** Copy `renzora/racingedge_plugin/` to
   `engine/crates/racingedge_game/`, then `cargo renzora sync` (regenerates the
   link edge and `plugins.rs`) and `cargo renzora` to build and launch.
   For local iteration switch the `racingedge` dependency from `git` to `path`.
3. **Bridge the audio.** Re-express `simulator::audio_playback` on Renzora's
   `AudioLink`/`renzora_audio` (sound ids, voices, mixer) instead of
   `bevy_audio`. Until this exists the Renzora build is silent: it keeps
   `CarSound`/`CommonSound` mix weights and no voices.
4. **Decide the scene story.** The game currently spawns its own car, ground
   plane, light and camera in code. Under Renzora those belong in a `.bsn` scene
   the editor owns, leaving the plugin to attach `VehicleState` and the drive
   systems to a scene-provided entity. That is a design decision, not a port,
   because it changes where the car's transform comes from.
5. **Input.** Our driving input reads `ButtonInput<KeyCode>` directly; Renzora
   has its own input plugin and settings/rebinding. Reuse theirs or keep ours.
6. **Keep the Bevy binary working.** `src/main.rs` stays the pure-Bevy runner so
   CI (`windows-latest`), the CLI tests and the C API keep their baseline; the
   Renzora path is additive.

## Open issues worth knowing before committing

* Renzora is **early alpha** ("expect bugs, incomplete features, and breaking
  changes between versions"), and released binaries are still on Bevy **0.19.1**
  while `main` is 0.20 — so a plugin built against `main` will not load in a
  released editor.
* Plugin loading needs the shared dylib build; a *lean export* (static Bevy)
  cannot load one and needs the plugin compiled in instead.
* Our physics is a bespoke 1 kHz loop (`simulator::r#loop`); Renzora brings its
  own physics plugin (avian) and frame scheduling. Running both means deciding
  which one owns the transform, since our `integrate_transform` writes it.
