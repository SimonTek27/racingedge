# Port backlog — what `ksengine` has that RacingEdge can grow into

RacingEdge is the Rust/[Bevy](https://bevyengine.org) re-implementation of the
concepts proven in the C++ **[ksengine](https://github.com/SimonTek27/ksengine)**
project (engine + `SimulatorApp` runtime + `ksEditor` suite). Both projects are
GPL-3.0, so code and data structures can be ported directly.

This document is the planning inventory: everything ksengine does today, mapped
to a concrete RacingEdge module, with an effort estimate. It is intentionally
descriptive — it is not a commitment, and nothing here is scheduled.

**Effort legend** — `S` = well under a day · `M` = one to three days ·
`L` = more than three days.

---

## 0. Known blocker: the `src/simulator/` tree is not compiled

`src/lib.rs` only declares `editor` and `plugins`; nothing declares
`mod simulator`, so the whole `src/simulator/` tree (13 modules: physics, tyre,
aero, suspension, brakes, drivetrain, electronics, cockpit, car data, AC import,
car sound, rebank, LUT) is **dead code today** — `cargo build` and
`cargo test` never see it.

Wiring it in (`pub mod simulator;`) fails with **38 errors**, because the tree
was written against a *newer* Bevy than the pinned `bevy = "0.11"`:

| Symptom | Bevy version it belongs to |
| --- | --- |
| `Text::new`, `TextFont`, `TextColor` | 0.15+ |
| `Mesh3d`, `MeshMaterial3d` (no `PbrBundle`) | 0.15+ |
| `Cylinder`, `Cuboid` in the prelude | 0.14+ |
| `Time::delta_secs` / `elapsed_secs` | 0.14+ |
| `Color::srgb` | 0.14+ |
| `Node { position_type, top, left }` | 0.14+ |

Meanwhile `src/editor/audio/*` and `src/plugins/*` use 0.11 APIs
(`PbrBundle`, `TextBundle::from_section`, `Time::delta_seconds`). So the repo
currently holds two incompatible halves.

**P0 decision to make:** either

1. **upgrade** `bevy` to the version the simulator targets (0.15/0.16) and fix
   `editor/audio/*` + `plugins/physics.rs` for the new API, then add
   `pub mod simulator;` — this unlocks ~150 KB of existing game code; or
2. **port the simulator tree down** to 0.11 (mechanical, but touches 5+ files
   and loses the newer-API work).

Until that is resolved, new code goes in the modules that *do* compile
(`editor/*`, `net/*`, `plugins/*`) — which is what the items below marked
*delivered* did.

---

## Delivered so far

| Item | Where it landed | Tests |
| --- | --- | --- |
| A2 track data | `src/editor/track/` (`data.rs`, `surfaces.rs`, `mod.rs`) | 15 unit tests |
| A9 `sounds.ini` | `src/editor/audio/sounds_ini.rs` | 7 unit tests |
| E4 telemetry out | `src/net/telemetry.rs` (UDP `:20777`, TCP `:20778`, `TelemetryPlugin`) | 9 unit tests + 1 doc test |
| I1 CI | `.github/workflows/ci.yml` (Windows, `fmt` + `clippy -D warnings` + `test`) | — |
| — | `src/editor/ini.rs` — ordered INI reader shared by A2/A9 | 6 unit tests |

Green baseline: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`
and `cargo test --all-targets` all pass (49 tests).

---

## 1. Already ported

| RacingEdge | ksengine reference |
| --- | --- |
| `src/simulator/tire.rs` | `src/engine/physics/KsTireModel.*` (Magic Formula) |
| `src/simulator/engine.rs`, `lut.rs` | `src/engine/physics/EngineModel.*`, `loadPowerLut` |
| `src/simulator/drivetrain.rs` | `src/engine/physics/GearboxModel.*`, `DifferentialModel.*` (partial) |
| `src/simulator/aero.rs`, `suspension.rs`, `brakes.rs`, `electronics.rs` | `AeroModel`, `SuspensionModel`, `BrakeThermalModel`, `HybridSystem`/`ErsDrcController` (all partial) |
| `src/simulator/ac_import.rs`, `car_data.rs` | `FileFormat/INIParser.*`, `CarPhysicsJson.*`, `adapters/assetto_corsa/TrackDataJsonLoader.h` |
| `src/simulator/car_sound.rs`, `rebank.rs`, `cockpit.rs` | `src/simulator/SimulatorAudio.*`, `VehicleAudioHook.h`, `AudioBankManager.*`, `DashboardOverlay.*` |
| `src/editor/audio/`, `src/bin/rea_bridge.rs` | `FileFormat/BankParser*`, `BankWriter*`, `FSPROImporter/Exporter.*`, `KSAudioImporter/Exporter.*` |

---

## 2. Formats — read/write

| # | Module to write | ksengine reference | Effort |
| --- | --- | --- | --- |
| A1 | `formats/kn5.rs` — KN5 reader (magic `sc6969`, v4/5/6) → Bevy meshes/materials | `FileFormat/Kn5Reader.*` | L |
| A2 ✅ | `editor/track/{data.rs,surfaces.rs}` — `<track>/data/data.json` + `surfaces.ini` grip grid | `TrackDataJsonLoader.h`, `AcSurfacesLoader.*` | S |
| A3 | `formats/ai_spline.rs` — AI spline reader/writer (magic `0x00414900` v1) | `AiFileReader/Writer.*`, `FileFormat/AiSpline.*` | S/M |
| A4 | `formats/ksanim.rs` — `.ksanim` animation | `FileFormat/KsAnim.*` | S |
| A5 | `formats/acd.rs` — `data.acd` (ROT decipher + key) | `FileFormat/AcdReader.*` | M |
| A6 | `formats/setup.rs` — setup `.ini` + `setup.json` per car | `Simulator/SetupFile.h`, `SetupJson.h` | S/M |
| A7 | `formats/replay.rs` — replay record/playback | `Simulator/ReplayRecorder.*` | M |
| A8 | `formats/telemetry_log.rs` — telemetry dump/read (v2, damage channels) | `physics/TelemetryPhysics.*`, `UdpTelemetryBridge.h` | S |
| A9 ✅ | `editor/audio/sounds_ini.rs` — `sounds.ini` event routing parser | `Simulator/SoundsIniParser.*` | S |
| A10 | `formats/vehicle_meta.rs` — `materials.txt`, `team.ini`, `upgrades.ini`, livery bundle | `engine/vehicle/VehicleUpgradeSystem.h`, `VehicleAppearanceBundle.h` | S/M |
| A11 | `formats/csp_config.rs` — CSP-style key/value config | `engine/Config/CspConfigParser.*` | S |
| A12 | CLI: **FSPRO ↔ KSAudio ↔ FSB** conversion | `FileFormat/FSPROImporter/Exporter.*`, `KSAudio*` | M |
| A13 | CLI: KN5 baker → runtime format + texture extraction | `FileFormat/Kn5Baker.*`, `tools/kn5baker` | M/L |

## 3. Physics — extend `src/simulator/`

| # | Module to write | ksengine reference | Effort |
| --- | --- | --- | --- |
| B1 | `tire_wear.rs` — tyre wear + flat spots | `TireWearSystem.*`, `TireFlatSpot.h` | M |
| B2 | `brake_thermal.rs` — brake temperature + pad/disc wear | `BrakeThermalModel.*`, `BrakeWearSystem.*` | M |
| B3 | `damage.rs` — rF2 damage zones, impact impulses, damage telemetry | `DamageSystem.*`, `Rf2DamageModel.h`, `MechanicalDamage.h`, `DamageTelemetry.h` | M/L |
| B4 | Full differential: preload, power/coast ramps, LSD | `DifferentialModel.*` | S/M |
| B5 | Full gearbox: clutch, shift time, rev matching, drivetrain loss | `GearboxModel.*` | S |
| B6 | `aero_draft.rs` — slipstream / tow | `AeroDraft.h` | S |
| B7 | Suspension kinematics (camber/toe vs travel, ARB, ride height) | `SuspensionKinematics.h` | M |
| B8 | `track_surface.rs` — dynamic grip grid: rubbering, marbles, per-sector wetness | `TrackSurface.h`, `TrackPhysics.*` | M/L |
| B9 | `weather.rs` — weather keyframes, grip vs wetness, track temperature | `WeatherPhysics.*`, `WeatherConfig.*`, `physics/weather/` | M |
| B10 | `fuel.rs` + `strategy.rs` — fuel burn and pit strategy | `StrategySimulator.*` | S/M |
| B11 | `timing.rs` — lap/sector timer + per-track/car personal bests | `phys_LapTimer.*`, `LapSectorTimer.h`, `PersonalBestStore.h` | S/M |
| B12 | `physics_validate.rs` — validator + golden reference tests | `PhysicsValidator.*`, `PhysicsGolden.h` | S/M |
| B13 | `physics_profile.rs` — physics/frame profiler | `PhysicsProfiler.*`, `GpuProfiler.*` | S |
| B14 | `loop.rs` — deterministic fixed timestep (1 kHz, as kssim) | `SimulationLoop::m_physicsDt` | M |

## 4. Audio — extend `car_sound.rs` / `rebank.rs`

| # | Module to write | ksengine reference | Effort |
| --- | --- | --- | --- |
| C1 | `audio3d.rs` — doppler, distance attenuation, bearing, surface rolling noise | `Simulator/Audio3D.h`, `CarEventVolumes.h` | M |
| C2 | `track_audio.rs` — track ambience, crowd, pit | `Simulator/TrackAudioManager.*` | S/M |
| C3 | `mixer.rs` — buses/VCAs, ducking, node graph | `Audio/AudioMixer.*`, `VCAMixerSystem.*`, `AudioGraph.*` | M |
| C4 | `rpm_profile.rs` — layer capture/profiling from real banks | `Audio/RPMProfile.*`, `RPMRecorder.*` | S/M |
| C5 | Reverb zones with delay lines | `Audio/AudioEffects*.*` | M |
| C6 | `car_acoustics.rs` — cabin resonance, interior/exterior filtering | `Audio/CarAcoustics.*` | M |
| C7 | CLI: build `.rebank` from a WAV folder + normalise + loop-point detection | `Audio/AudioBankManager.*` | S/M |

## 5. Runtime and session (Bevy)

| # | Module to write | ksengine reference | Effort |
| --- | --- | --- | --- |
| D1 | `FeatureHub` as the single resource (session, weather, limits, PB, API) | `Simulator/FeatureHub.h` | M |
| D2 | Sessions and flags: practice/qualifying/race, flags, countdown, standing start | `RaceSession*.*`, `RaceFlag.h`, `SessionFlow.h` | M/L |
| D3 | `track_limits.rs` — track cut, penalties, pit speeding | `TrackLimitsMonitor.h` | M |
| D4 | Replay recorder + playback | `ReplayRecorder.*` | M |
| D5 | Garage/pit state machine: garage exit → queue → collision → repair → strategy | `GarageExit.*`, `PitLaneQueue/Collision/Repair.h`, `PitStrategyBridge.h` | M/L |
| D6 | Setup garage UI + setup persistence per car/track | `SetupGarage.*`, `ApplySetup.h` | M |
| D7 | Multi-car manager + AI traffic (spline following) | `MultiCarManager.*`, `AIController.*`, `AIDriver.h` | L |
| D8 | Input & FFB: gamepad, DirectInput wheels, force feedback output | `InputManager.*`, `devices/*`, `FfbOutput.h` | M/L |
| D9 | `camera.rs` — cockpit / cinematic / replay cameras | `CameraController.*` | S |
| D10 | HUD + telemetry overlay in Bevy UI | `DashboardOverlay.*`, `TelemetryOverlay.*`, `RaceTelemetryHud.h` | M |
| D11 | Menus: pause, settings, key rebind, device settings | `GameMenuOverlay.*`, `KeyRebindOverlay.h` | M |
| D12 | `content_library.rs` — scan installed content/mods | `ContentLibrary.h`, `tools/ksmmm` | M |
| D13 | `user_stats.rs` — career stats: wins, poles, podiums, best lap | `UserStats.h` | S |

## 6. Networking and telemetry

| # | Module to write | ksengine reference | Effort |
| --- | --- | --- | --- |
| E1 | `net/car_state_sync.rs` — UDP, magic `KSCC`, :40001, 20 Hz + remote interpolation (32-slot ring, 100 ms) | `CarStateSync.h`, `RemoteCarInterpolator.h` | M |
| E2 | LAN discovery (magic `KSDI`, :20779) + HTTP matchmaking | `ServerDiscovery.h`, `Matchmaking.h` | S/M |
| E3 | Control API on TCP :20780 (`SESSION/WEATHER/TIME/LIMITS/PB/REPLAY/RESULT/PING/AUTH`, `EVT` events) | `ExternalControlApi.h`, `docs/CONTROL_API.md` | M |
| E4 ✅ | Telemetry out: UDP :20777 and TCP :20778 (`[u32 LE len][payload]`) | `UdpTelemetryBridge.h`, `TcpTelemetryBridge.h` | S/M |
| E5 | AC shared memory (`Local\acpmf_physics`, `acpmf_graphics`, `acpmf_static`) for external apps | `adapters/assetto_corsa/AcSharedMemoryPublisher.*` | M |
| E6 | Inbound AC channels: listener :20747, commands :9666, remote telemetry :9996 | `UdpTelemetryListener.*`, `UdpCommandListener.h`, `AcRemoteTelemetry.h` | M |
| E7 | Optional MQTT bridge | `Simulator/MqttClient.*` | S |
| E8 | Reliable-UDP netcode (Rust: `laminar`/`renet`) | `engine/network/ksnet/` | L |

## 7. CLI and tooling (`rea_bridge` or new binaries)

| # | Module to write | ksengine reference | Effort |
| --- | --- | --- | --- |
| F1 | `track-scaffold` / `track-validate` / `--spline-info` | `tools/tracktool` | S/M |
| F2 | `tire-normalize` — normalise tyre curves from real LUTs | `tools/kstirenorm`, `physics/TireNormalizeTool.*` | S |
| F3 | `physics-profile` / `vs-profile` | `tools/ksphprofile`, `tools/ksvsprofile` | S |
| F4 | Headless mod manager (`list` / `install` / `verify`) | `tools/ksmmm` | M |
| F5 | Batch conversion + sample grafting (`--into`) — already on the README roadmap | `Audio/AudioFormatConverter.*` | S |

## 8. Editor suite (`racingedge::editor`, beyond `audio`)

| # | Module to write | ksengine reference | Effort |
| --- | --- | --- | --- |
| G1 | Physics editor (Pacejka, suspension, aero, powertrain) with a Bevy UI | `sdk/kseditor`, `physics/TireCurveEditor.*` | M/L |
| G2 | Telemetry/replay viewer and analyser | `TelemetryOverlay.*`, `ReplayRecorder.*` | M |
| G3 | Livery/paint editor (DDS/TGA) | `kseditor` paint module, `material/TexturePaint.*` | L |
| G4 | Modeler (KN5/FBX view + edit) | `engine/mesh/*`, `Kn5Reader.*` | L |
| G5 | Event/career editor + server config editor | `kseditor` event/server modules | M |
| G6 | Showroom / presentation mode | `kseditor` showroom module | M |

## 9. Scripting and APIs

| # | Module to write | ksengine reference | Effort |
| --- | --- | --- | --- |
| H1 | Mod SDK scripting (`on`/`dispatch`/`get`/`set`) with `mlua` or `rhai`, hot reload | `engine/Scripting/ModSdk*`, `ScriptHost.*`, `HotReloadManager.*` | M |
| H2 | C API (`racingedge.h` + `cbindgen`) for headless hosts | `include/ksengine_c.h`, `engine/api/ksengine_c.cpp` | M |
| H3 | Headless server binary | `tools/ks_server`, `Simulator/SimulatorServerApp.cpp` | M |

## 10. Infrastructure

| # | Module to write | ksengine reference | Effort |
| --- | --- | --- | --- |
| I1 ✅ | GitHub Actions CI: `cargo fmt --check`, `clippy -D warnings`, `cargo test` | README build badge | S |
| I2 | Golden/determinism tests + byte-exact format round trips | `test_determinism`, `PhysicsGolden.h` | S/M |

---

## Suggested sequence

**Phase 1 — close the AC vertical.** A2, A9, B10, B11, D1, D13, E4, I1.

**Phase 2 — driving experience.** D2, D3, D5, D6, D8, D9, D10, B1, B2, B4, B5,
B6, C1, C2.

**Phase 3 — depth and platform.** A1, A3, A7, B3, B7, B8, B9, B14, D4, D7, E1,
E2, E3, E5, H1, H2, H3.

**Phase 4 — editor and extended modding.** A12, A13, C3–C7, F1–F5, G1–G6,
E6–E8.

## High-value / low-cost starting points

1. **E4 telemetry out** — makes RacingEdge compatible with existing dashboards
   and telemetry tools straight away.
2. **A2 + A9** (`data.json`, `surfaces.ini`, `sounds.ini`) — completes the AC
   track/car content pipeline started by `ac_import.rs`.
3. **B10 + B11** (fuel and timing/PB) — a complete race loop with lap times and
   pit stops.
4. **D1 + D2** (FeatureHub + session/flags) — a real "game" skeleton around the
   physics.
5. **I1 CI** — protects everything that follows.

## Porting notes

* Both projects are GPL-3.0; `src/simulator/tire.rs` already carries the
  attribution comment used for ports.
* ksengine has no Rust dependency to reuse: the runtime is Win32 + Vulkan and
  the editor is Qt6. Porting means re-implementing behaviour in Bevy, not
  linking.
* The FSB5/audio side of ksengine (`FileFormat/BankParser*`, `FSPRO`,
  `KSAudio`) overlaps with the `ramod` crate; new work there belongs in `ramod`,
  not here.
* Known ksengine inconsistencies worth not copying: stale `src/core/*` paths in
  several docs, `SimulationLoop_FeatureMethods.cpp` is not part of any build
  target, and `adapters/assetto_corsa/FSPROImporter.h` is a stub (the real
  parser lives in `engine/FileFormat/`).
