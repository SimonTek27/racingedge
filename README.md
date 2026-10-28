# racingedge

A Rust / [Bevy](https://bevyengine.org) racing game plus a **content-modding
suite**.

The suite currently targets **FSB5 sound banks** — the audio containers produced
by the stock content-authoring tool — so custom audio content can be inspected,
extracted and re-authored.

[![CI](https://github.com/SimonTek27/racingedge/actions/workflows/ci.yml/badge.svg)](https://github.com/SimonTek27/racingedge/actions/workflows/ci.yml)
[![License: GPL-3.0](https://img.shields.io/badge/license-GPL--3.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.97.1%2B-orange.svg)](https://www.rust-lang.org)

## Layout

| Path | Purpose |
| --- | --- |
| `src/lib.rs` | Library root shared by the game and the CLI tools |
| `src/editor/audio/mod.rs` | Facade that re-exports the [`ramod`](https://github.com/SimonTek27/RaMOD) FSB5 middleware |
| `src/editor/audio/sounds_json.rs` | `sounds.json` event map (per-car / per-track audio routing) |
| `src/editor/track/` | Track data: `data.json` metadata and the `surfaces.ini` grip grid |
| `src/editor/ini.rs` | Ordered INI reader shared by the content editors |
| `src/formats/` | Binary format readers shared by the game and the CLI: `.ksanim` node animations, `.kn5` AC mesh containers, `.ai` spline lines and replay files |
| `src/net/telemetry.rs` | Telemetry out: UDP `:20777` and TCP `:20778` (`KSIM` v2 frames) |
| `src/net/car_state_sync.rs` | LAN car-state sync: UDP `:40001` (`0x4B534353` packets) + remote interpolation |
| `src/net/discovery.rs` | LAN server discovery: UDP `:20779` (`0x4B534449` Query/Announce/Reply) |
| `src/net/matchmaking.rs` | LAN + soft-fail HTTP lobby registry (`GET`/`POST /servers`), browser rows |
| `src/net/control_api.rs` | InSim-style control API on TCP `:20780` (auth + `EVT` events) |
| `src/net/ac_shared_memory.rs` | Assetto Corsa `Local\acpmf_*` shared-memory pages for third-party apps |
| `src/scripting/` | Mod SDK scripting on [`rhai`](https://rhai.rs): `ScriptHost` (`ks.log/get/set/on/version`, event dispatch) + mod loading and hot reload |
| `src/editor/audio/plugin.rs` | Bevy `AudioPlugin`: loads FSB banks into playable assets |
| `src/editor/audio/browser.rs` | Bevy `BankBrowserPlugin`: in-game bank browser |
| `src/simulator/` | Vehicle runtime: physics, tyre (Magic Formula + wear/flat spots), engine, aero (incl. slipstream/draft), suspension (incl. kinematic LUTs), brakes (thermal + pad/disc wear), drivetrain (differential + gearbox), electronics, cockpit HUD, AC import, car audio, lap/sector timing, personal bests, fuel/pit strategy, career stats, session/flag state, track limits, rF2-style damage, cockpit/chase/free camera, pit-lane flow (garage exit, queue, collisions, service), input mapping + soft force feedback, car setup (model + per-car/track persistence), 3D audio math (doppler/pan/distance),
track ambience events from `audio_config.ini`, race HUD/telemetry overlay, dynamic track-surface grip grid (rubbering/marbles/wetness) with banking/elevation models, weather evolution, a 1 kHz fixed-step loop, a replay runtime and a multi-car manager with AI spline following |
| `src/plugins/` | Game-side Bevy plugins: `physics` (drive loop) and `game` (scene, chase camera, HUD assembly) |
| `src/main.rs` | The game binary (`racingedge`) |
| `src/bin/rea_bridge.rs` | The modding CLI (`rea_bridge`) |
| `src/bin/racingedge_server.rs` | The headless dedicated server (`racingedge-server`) |
| `src/simulator/server.rs` | Headless session loop, config (INI + CLI) and control handler shared by the server binary |
| `src/c_api.rs` | The C ABI (`extern "C"`) for headless hosts, mirrored by `include/racingedge.h` |
| `assets/vorbis_setup_headers.bin` | Provenance copy of the 161 bundled setup headers (the runtime asset lives in `ramod`) |
| `tools/build_vorbis_setup_headers.py` | Regenerates that asset from `reference/` |
| `docs/KSENGINE_PORT_BACKLOG.md` | What [`ksengine`](https://github.com/SimonTek27/ksengine) has that this project can grow into |

The FSB5 parser/writer, the Vorbis ↔ Ogg tooling and the PCM → Vorbis encoder
live in the **[`ramod`](https://github.com/SimonTek27/RaMOD)** crate (a git
dependency), so the game and the CLI share exactly one implementation;
`src/editor/audio/` re-exports it and adds the Bevy integration. Mod scripting
uses the pure-Rust [`rhai`](https://rhai.rs) engine (built with its `sync`
feature) so no C toolchain is required.

## Build & test

```sh
cargo build
cargo test
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
```

CI runs all four on Windows (`.github/workflows/ci.yml`). Both binaries build
cleanly with **no warnings**. The FSB5/Vorbis/encoder unit tests now live in the
[`ramod`](https://github.com/SimonTek27/RaMOD) crate; this repository's tests
cover the Bevy audio bridge (sample → WAV/Ogg, summaries), the browser panel
(row labels, reactive list rebuild), the CLI helpers and the content/telemetry
parsers.

> **Note** — the project tracks the **latest released Bevy** and adapts to it
> (see `AGENTS.md` "Rule 1"); it currently builds against **Bevy 0.20**.
> `src/simulator/` is wired into the crate, compiled/tested by CI, and assembled
> into the game binary by `src/plugins/`: `PhysicsPlugin` steps the real
> `VehicleState` on the 1 kHz loop with keyboard input and the auto-shifted
> `GearboxModel`, and `GamePlugin` adds the scene, the `CameraController`-driven
> camera, the race HUD and the car audio (engine layer mix, cockpit / exterior
> split, tyre rolling and skid layers). The mix is audible: `audio_playback`
> gives every engine layer and the two tyre loops a Bevy `AudioPlayer` and drives
> its `AudioSink` volume and pitch from the layer weights, so engine note, wind-up
> and tyre rumble follow the car. Everything else in the port backlog is tracked
> in [docs/KSENGINE_PORT_BACKLOG.md](docs/KSENGINE_PORT_BACKLOG.md).

## Driving the game

```sh
cargo run --release
```

| Key | Action |
| --- | --- |
| `W` / `↑` | Throttle |
| `S` / `↓` | Brake |
| `A` `D` / `←` `→` | Steer |
| `C` | Chase ⇄ cockpit camera (also moves the engine mix interior) |
| `F2` | Cycle race HUD mode |
| `F1` | FSB bank browser |
| `P` | Preview the first playable bank sample |

The gearbox shifts automatically from the car's own ratios.

The whole game layer is one plugin, `racingedge::plugins::RacingEdgeGamePlugin`
(vehicle physics + scene, camera, HUD and audio), so it can be hosted by another
Bevy runner. Renzora — a Bevy-based engine and editor — is the intended second
host: `renzora/racingedge_plugin/` holds the wrapper crate and
[docs/RENZORA_MIGRATION.md](docs/RENZORA_MIGRATION.md) the verified facts and the
remaining steps. Build with `--no-default-features` to drop the `game-audio`
playback layer (Renzora ships its own mixer and never enables `bevy_audio`).

## The `rea_bridge` CLI

```sh
cargo run --bin rea_bridge -- --help
```

| Command | Description |
| --- | --- |
| `fsb-info <bank.fsb>` | Parse a bank and print every sample (rate, channels, frames, loop, Vorbis setup id) |
| `fsb-extract <bank.fsb> <outdir>` | Extract every sample (WAV for PCM, Ogg for Vorbis, raw otherwise) |
| `fsb-ogg <bank.fsb> <outdir>` | Rebuild every Vorbis sample as a playable `.ogg` file |
| `fsb-pack <out.fsb> <wav...>` | Build an FSB5 bank from one or more PCM `.wav` files |
| `fsb-edit <in.fsb> <out.fsb> [opts]` | Edit a bank in place: rename, set loop, change rate, replace PCM audio |
| `fsb-import-ogg <out.fsb> <in.ogg>` | Build a Vorbis bank from an `.ogg` file (`--name <name>`) |
| `fsb-encode <out.fsb> <in.wav>` | Encode a `.wav` to a Vorbis bank using the stock encoder (`--name`, `--quality 0-100`) |

Example:

```sh
cargo run --bin rea_bridge -- fsb-info Master.fsb
```

```
=== FSB5 bank: Master.fsb ===
version            : 1
num samples        : 12
data size          : 482311 bytes
format             : vorbis (mode 15)

[  0] engine_idle             44100 Hz  2 ch   132300 frames  3.000s   98120 bytes
        loop: 0..132300
        vorbis: setup id 118203318  packets 212  seek entries 212
```

Packing edited audio back into a bank:

```sh
cargo run --bin rea_bridge -- fsb-pack engine.fsb idle.wav limiter.wav
# wrote engine.fsb (2 sample(s), 9184 bytes, pcm16)
```

Editing an existing bank and re-serialising it:

```sh
cargo run --bin rea_bridge -- fsb-edit Master.fsb Master_patched.fsb \
    --rename 3=engine_idle --loop 3=0:132300 --freq 7=22050 \
    --replace 5=hit.wav --clear-loop 2
```

Every metadata chunk the editor does not manage (e.g. `VORBISDATA`, `XMASEEK`,
`DSPCOEFF`), the header fields and all other samples are preserved byte-for-byte.

Samples are laid out exactly as the reference tool writes them: every sample is
32-byte aligned inside the data section, the section itself starts on a 32-byte
boundary (the name table absorbs the padding so `60 + sampleHeaderSize +
nameTableSize` is a multiple of 32) and is padded to the same boundary
(`dataOffset` counts 16-byte units, so each offset is an even unit count).
Every built bank is parsed back before it is written, so a successful build is
guaranteed to be readable by the suite.

## What works today

* **Parse** FSB5 banks: header, per-sample headers, metadata chunks
  (`CHANNELS`, `FREQUENCY`, `LOOP`, `VORBISDATA`, …), name table and data.
* **Extract** samples as playable audio: RIFF/WAVE for PCM banks, and fully
  rebuilt **Ogg Vorbis** files for Vorbis banks (identification, comment and
  setup headers re-synthesised, packets re-muxed into CRC-correct Ogg pages).
* **Build** FSB5 PCM banks from WAV files (`PCM8/16/24/32`, mono to multichannel,
  loop points, non-standard sample rates). The writer self-verifies by parsing
  its own output, and a WAV → bank → WAV round trip is byte-identical.
* **Import** Ogg Vorbis into a bank (`fsb-import-ogg`): the Ogg's packets and
  setup header are demuxed (every page CRC32 checked) and re-framed as an FSB5
  `VORBIS` sample — each packet stored as a `u16` length followed by the raw
  Vorbis bytes, exactly as the reference tool writes them. The setup-header
  CRC32 and a per-packet seek table are written into `VORBISDATA`. The setup
  header must be one of the 161 bundled headers (the format stores only the
  CRC32 key), so Ogg files from a compatible libvorbis build are accepted;
  anything else is
  rejected with the offending id.
* **Encode** PCM → Vorbis (`fsb-encode`): the bundled `libfsbvorbis64.dll`
  exports the *full* libvorbisenc encoder, so the tool drives it directly through
  Windows FFI (located automatically, or via `RAMOD_VORBIS_ENCODER`) — no
  Vorbis crate is added. Each VBR quality reproduces one of the 161 bundled
  setup headers, so the resulting bank is decodable by the reference decoder;
  mono and stereo output were round-trip decoded with the same libvorbis to
  confirm it.
* **Edit** existing banks in place (`BankEditor`): rename samples, set/clear
  loop regions, change sample rates, replace PCM payloads from WAV files, and
  create banks from scratch. Codec metadata chunks, header fields and untouched
  samples are preserved, and re-serialisation is idempotent.
* **Hear the car**: `simulator::audio_playback` turns the `CarSound` mix into
  sound — one looping `AudioPlayer` per engine layer (the interior set in
  cockpit view, the exterior set otherwise) plus synthesised tyre rolling and
  skid loops — and updates each voice's volume and playback speed from the layer
  weights and surface volumes every frame. The engine set the camera does not
  select is paused rather than mixed at zero gain, the cheap half of FMOD-style
  virtual voices.
* **Play** banks in-game: the `AudioPlugin` loads FSB banks into Bevy
  `AudioSource` assets (WAV for PCM, Ogg for Vorbis), so samples can be previewed
  and driven by gameplay. Point it at a bank with the `RACINGEDGE_BANKS`
  environment variable (semicolon-separated paths), or drop `.fsb` files in
  `assets/audio/`.
* **Browse** them in-game: an `F1` bank-browser panel lists every loaded bank
  and sample (rate, channels, duration, loop, playable container) and plays a
  sample when its row is clicked (`P` previews the first playable sample). The
  list rebuilds automatically whenever the library changes.
* **Read track and audio metadata**: `Track::discover` loads a track's
  `data.json` (name, spline length, width) and `surfaces.ini` grip grid, and
  `SoundsJson` parses a car's or track's `sounds.json` event map — the same data
  the reference engine uses.
* **Read AC meshes**: `formats::kn5` parses the Assetto Corsa `.kn5` container
  (magic `sc6969`, versions 4/5/6): textures, materials (with shader properties
  and texture mappings) and the Base/Mesh/SkinnedMesh node hierarchy, with
  world matrices accumulated from Base ancestors and every read bounds-checked.
  `to_bevy_mesh` turns a parsed mesh into a Bevy `Mesh` (positions/normals/uv/
  indices). Texture payloads are skipped unless `keep_texture_data` is set.
* **Read AI lines**: `formats::ai_spline` reads and writes Assetto Corsa
  `fast_lane.ai` splines (magic `0x00414900`, version 1) with the same byte
  layout as the reference editor, falling back to a comma-separated text file
  when the magic is absent. It derives cumulative distance and the closed-loop
  flag the same way the reference does.
* **Record and play replays**: `formats::replay` implements the `ReplayRecorder`
  (`RPLY` v1): 60 Hz frame recording, save/load, and playback with speed control,
  seeking and linear interpolation between frames (gear snaps to the nearer one).
* **Stream telemetry**: `TelemetryPlugin` broadcasts `KSIM` v2 frames (116 bytes,
  speed/rpm/gear/inputs/position/lap/fuel/damage/tyres) on UDP `:20777` and TCP
  `:20778`, with the same wire format and ports as the reference engine, so
  existing dashboards and telemetry tools can consume them. Off by default:
  opt in with `TelemetryConfig { enabled: true, ..default() }`.
* **Sync cars over LAN**: `car_state_sync` ports the ksengine `CarStateSync`
  broadcast (magic `0x4B534353`, UDP `:40001`, host/client roles, 20 Hz rate
  limit) and the `RemoteCarInterpolator` (32-slot ring, 100 ms delay, velocity
  extrapolation) so remote cars stay smooth despite jitter. Off by default.
* **Find servers**: `discovery` ports the `ServerDiscovery` broadcast (magic
  `0x4B534449`, UDP `:20779`, Query/Announce/Reply, 2 s announcements, 8 s
  expiry) and `matchmaking` merges the LAN list with an optional HTTP lobby
  registry (`POST`/`GET /servers`), degrading gracefully when no lobby URL is
  configured.
* **Control the sim remotely**: `control_api` serves the InSim-style TCP
  channel on `:20780` (one client, UTF-8 line protocol, optional `AUTH` token),
  handling `PING`/`AUTH` itself and dispatching the rest to a `ControlHandler`
  that replies with `OK`/`ERR` lines and `EVT` events.
* **Feed third-party AC apps**: `ac_shared_memory` publishes the AC-compatible
  `acpmf_physics` / `acpmf_graphics` / `acpmf_static` pages (Live Timers,
  SimTools, dashboards) with the 592-byte mapping floor AC readers expect, and
  falls back to an in-process mirror when no mapping can be created.
* **Script mods**: `scripting::ScriptHost` runs mod `.rhai` scripts against the
  `ks` prelude (`ks.log`/`ks.get`/`ks.set`/`ks.on`/`ks.version`), and
  `load_mod_scripts` + `dispatch` drive events to `on_<event>` handlers with a
  filename-ordered load and a `ScriptHotReloader` for live reload.
* **Run a headless server**: `racingedge-server` (built on `simulator::server`)
  drives a session with a built-in pace driver, hosts LAN discovery and the
  control API, spawns an optional AI field, and supports an INI config file plus
  command-line overrides:

  ```sh
  # time-limited practice, real-time pacing, default ports
  cargo run --bin racingedge_server -- --time 300

  # 10-lap race with 4 AI cars, announced on the LAN
  cargo run --bin racingedge_server -- --laps 10 --track-length 5793 \
      --ai 4 --announce --name "Monza Practice"

  # fastest possible, custom ports, hard 60 s sim cap
  cargo run --bin racingedge_server -- --fast --duration 60 \
      --discovery-port 20879 --control-port 20880
  ```
* **Embed the engine from C**: `c_api` exports a stable, null-safe `extern "C"`
  surface (`re_engine_create`/`_step`/`_get_state`, session control) declared in
  the hand-written `include/racingedge.h`. The crate builds as a `cdylib`, so a
  host can drive a fixed-step vehicle and session without linking Rust; no
  header generator is used, and a test keeps the header and the exported symbols
  in sync.
* **Time laps and plan races**: `LapTimer` (event-driven) and `LapSectorTimer`
  (AC-style, spline-derived sectors) in `simulator/timing.rs`, a file-backed
  per-track/car `PersonalBestStore` (`.pb`), `StrategySimulator` fuel/pit-stop
  planning, and `DriverStats` persisted to `stats.json`.
* **Run a session**: `FeatureHub` (a Bevy `Resource`) bundles the session mode
  and start parameters, the qualifying → grid → countdown → race → finished
  `SessionFlow`, and the personal-best store, turning a completed lap into the
  same `PB …` / `LAP …` events the reference emits. `RaceSession` tracks flags,
  penalties and phases, and `TrackLimitsMonitor` detects off-track excursions
  (centerline lateral distance or an on-track hint) and escalates them to time,
  drive-through, stop-go and disqualification penalties. `DifferentialModel`
  (open/LSD/Torsen/locked/active with preload, power/coast ramps, locking
  transfer and thermal state) and `GearboxModel` (ratios, final drive,
  auto-shift, wheel torque) round out the drivetrain, and `compute_draft` models
  the slipstream/tow behind a leading car. `TireWearSystem` tracks surface/core/
  carcass temperature with wear, graining, blistering and per-compound grip, and
  `TireFlatSpot` models locked-wheel flat spots; `BrakeThermalModel` and
  `BrakeWearSystem` track disc/pad temperature, fade and pad/disc wear. The
  rF2-style `DamageSystem` tracks 11 body zones plus engine/aero/transmission/
  suspension/brake component damage, distributes collisions by contact point,
  shifts the CG and exposes repair cost/time (with `Rf2DamageModel` impulse and
  `MechanicalDamage` continuous-wear bridges and a telemetry sample). The
  `CameraController` provides cockpit (with head-bob), smoothed chase and free-fly
  modes. The pit-lane stack models the full box flow: `GarageExitController`
  (phase machine, block reasons, pit limiter, exit cone), `PitLaneQueue`
  (leave/enter spacing with player priority), `PitLaneCollision` (OBB SAT
  contacts, corridor walls, damage impulses) and `PitLaneRepair` (job planning
  from `RepairData`, tyres/fuel, abort). `InputManager` handles joystick/wheel
  and rebindable keyboard mapping with dead-zone/gamma curves and edge-triggered
  shifts (hardware polling is behind a `ControllerBackend` trait), and
  `FfbOutput` produces soft force feedback from `FfbBridge`'s aligning-moment
  torque, with an optional `FfbActuator` for real wheels. `SetupGarage` models
  the tabbed car setup (tyres, suspension, aero, brakes, diff) with typed rows,
  keyboard navigation and `SetupParams` derivation, and setups persist per
  car/track through `setup_path`/`save_setup`/`load_setup`. The `audio3d`
  helpers give other-car voices doppler, distance attenuation and bearing pan,
  and `CarEventVolumes` maps CSP-style `[AUDIO_VOLUME]`/`[AUDIO_PITCH]` files
  onto the sound categories. `TrackAudioManager` loads positioned track events
  (crowd, pit, weather) from `audio_config.ini`, driving their triggers from the
  weather and scaling gain per camera. `hud` ports the race HUD/telemetry
  overlay: a `RaceTelemetryHud` model with Off/Compact/Race/Engineer modes,
  history buffers and damage/status lines, plus a Bevy plugin that renders it.
  `SuspensionKinematics` maps damper velocity, travel, camber and bump-steer
  through sampled LUTs and derives the anti-roll bar force. `track_surface`
  keeps a world-space grip grid where rubbering, marbles and wetness evolve per
  cell, alongside a section-based surface model and banking/elevation maths.
  `weather` evolves ambient/track temperature, rain wetness and wind from a
  preset and exposes the resulting grip reduction, aquaplaning risk and wind
  force. `loop` provides the deterministic 1 kHz fixed-timestep accumulator
  (`PHYSICS_DT`) that turns variable frame time into whole physics steps.
  `replay_runtime` drives `ReplayRecorder` from that loop and bridges
  `VehicleState` into replay samples, and `multi_car` adds the multi-car
  manager with an AI spline follower (speed-scheduled lookahead, curvature
  feedforward, lap counting and traffic overtaking) plus grid placement and
  collision detection.

## Roadmap

1. Batch conversion and sample grafting (`--into` to add or replace a sample
   inside an existing bank).
2. Finish the `src/simulator/` runtime in the game binary: car audio
   (`CarSoundPlugin`), the session/`FeatureHub` flow, menus and the garage UI —
   the plumbing is tracked in the [port backlog](docs/KSENGINE_PORT_BACKLOG.md).
3. Continue the ksengine port backlog: the remaining track/model formats,
   the Phase 2 driving-experience items (garage/pit, cameras, HUD),
   multiplayer sync and the AC shared-memory bridge.

## License

Released under the [GNU General Public License v3.0](LICENSE)
(`GPL-3.0-or-later`).

## RaMOD audio parity (measured, not claimed)

RaMOD lives in its own repository; the artifacts below are the recon, the score and the repair
patch produced against its `main`.

- [`replica/features.csv`](replica/features.csv) — feature matrix, each cell with its evidence.
- [`replica/parity.py`](replica/parity.py) — the replica-diff scorer (MIT, from the replica skill).
  Current score: **79.2 / 100**, must-haves 16 of 20.
- [`replica/recon.md`](replica/recon.md) — recon notes, including where the matrix was corrected
  against the real `runtime/` code rather than the roadmap.
- [`replica/ramod-repairs.md`](replica/ramod-repairs.md) — why RaMOD would not build, what was
  reconstructed, and what remains unproven.
- [`replica/ramod-fixes.patch`](replica/ramod-fixes.patch) — the same fixes as a patch against RaMOD.

