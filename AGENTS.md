# Agent guide — racingedge

Instructions for AI agents and contributors working in this repository.

## Rule 1 — Bevy comes first, always

**Bevy updates are always a priority, and this project always adapts to Bevy.**

RacingEdge tracks the **latest released Bevy**. When a new Bevy version ships:

1. Upgrading Bevy takes precedence over any feature work in progress.
2. The migration is done by **changing our code to match Bevy** — never by
   pinning, patching, vendoring or forking Bevy, and never by leaving a module
   out of the build because it no longer compiles.
3. The whole workspace must end up compiling on the new version: `src/editor/`,
   `src/net/`, `src/plugins/` **and** `src/simulator/`, which is declared by
   `src/lib.rs`.
4. The upgrade is only finished when `cargo fmt --all -- --check`,
   `cargo clippy --locked --all-targets -- -D warnings`,
   `cargo test --locked --all-targets` and `cargo test --locked --doc` are all
   green (the two test commands cannot be combined: cargo rejects `--doc`
   together with target-selection flags).

Corollaries:

* `Cargo.toml` pins Bevy with a caret requirement on the current major-minor
  series; bump it as soon as the version is available.
* Never reintroduce a module that is excluded from the build to dodge a
  migration. A module that does not compile is a bug, not a state.
* When an API is renamed or removed, follow the official Bevy migration guide
  for that release; do not guess.
* Write code using the current Bevy idioms (required components, `TextFont` /
  `TextColor`, `Mesh3d` / `MeshMaterial3d`, `Time::delta_secs`, `Color::srgb`)
  so the next upgrade is smaller.

## Rule 2 — Ports from ksengine

`racingedge` is the Rust/Bevy re-implementation of the concepts proven in the
C++ [`ksengine`](https://github.com/SimonTek27/ksengine) project. Both are
GPL-3.0, so code may be ported directly. Keep the attribution comment used by
`src/simulator/tire.rs` ("port of ksengine …") on everything ported.

The full inventory of what can be ported lives in
[`docs/KSENGINE_PORT_BACKLOG.md`](docs/KSENGINE_PORT_BACKLOG.md).

## Rule 3 — One implementation, shared

Format logic lives in one place and is shared by the game binary and the CLI:
the FSB5/Vorbis middleware is in the [`ramod`](https://github.com/SimonTek27/RaMOD)
crate, and everything else lives in the `racingedge` library
(`src/editor/`, `src/net/`) so `src/main.rs` and `src/bin/rea_bridge.rs` use the
same code.

## Quality bar

* No new dependencies without a reason; prefer what is already in the tree.
* Every parser gets unit tests with real-world-shaped fixtures.
* `#[cfg(test)]` module tests plus at least one doc test for each public entry
  point that has an example.
* CI (`windows-latest`) is the reference platform: FMOD's `libfsbvorbis64.dll`
  (through `ramod`) and the `.cmd` REA scripts are Windows-only.
