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

## 0. Resolved: the whole `src/simulator/` tree is now compiled

`src/lib.rs` declares `pub mod simulator;`, so the entire simulator tree
(physics, tyre, engine, aero, suspension, brakes, drivetrain, electronics,
cockpit, car data, AC import, car sound, rebank, LUT) is part of the library, the
game binary and the CLI, and is built and tested by CI.

The blocker was that `src/simulator/` was written against a *newer* Bevy than
the then-pinned `bevy = "0.11"`, while `src/editor/audio/*` and
`src/plugins/*` used 0.11 APIs (`PbrBundle`, `TextBundle::from_section`,
`AudioBundle`, `Time::delta_seconds`). The repo held two incompatible halves.

**Resolution:** `bevy` was upgraded to the latest release (**0.20**) and *our
code was changed to match Bevy*, as `AGENTS.md` Rule 1 requires — no pinning,
patching, vendoring or forking:

* `editor/audio/plugin.rs`, `editor/audio/browser.rs` and `plugins/physics.rs`
  moved to required components (`Mesh3d`/`MeshMaterial3d`, `AudioPlayer`),
  `Node`-based UI with `Text`/`TextFont`/`TextColor`/`FontSize`, `ButtonInput`,
  `Color::srgb`, and picking events (`PointerOver`/`PointerPress`/`PointerClick`)
  instead of the deprecated `Interaction` component.
* the simulator tree needed only small fixes — `Query::single`,
  `FontSize::Px(..)`, `Text`'s `Deref<Target = String>`, and a borrow split in
  `car_sound.rs`.

Two migration gotchas worth remembering:

* Bevy 0.20 requires `rust-version = 1.97.1`; local and CI toolchains are ≥ 1.99.
* a **stale `Cargo.lock`** from the 0.11 era resolved *two* incompatible
  `windows` crates (`0.58`, pulled by `gilrs`, and `0.62`, pulled by
  `gpu-allocator`/`wgpu-hal`) and broke the `wgpu-hal` DX12 build. Regenerating
  the lock (`cargo generate-lockfile`) unifies them on `0.62`.

---

## Delivered so far

| Item | Where it landed | Tests |
| --- | --- | --- |
| A2 track data | `src/editor/track/` (`data.rs`, `surfaces.rs`, `mod.rs`) | 15 unit tests |
| A9 `sounds.ini` | `src/editor/audio/sounds_ini.rs` | 7 unit tests |
| A14 JSON fonts | `src/simulator/fonts.rs` — JSON bitmap-font definitions for cockpit digital instruments (per-glyph `width`, `x_offset`, `y_offset`, text measurement) | 5 unit tests + 2 doc tests |
| B11 timing | `src/simulator/timing.rs` (`LapTimer` = `phys_LapTimer`, `LapSectorTimer`), `src/simulator/personal_best.rs` (`PersonalBestStore`, file-backed `.pb`) | 16 + 9 unit tests + 2 doc tests |
| B10 strategy | `src/simulator/strategy.rs` — `StrategySimulator`: fuel planning + pit-stop recommendation | 7 unit tests |
| D13 user stats | `src/simulator/user_stats.rs` — `DriverStats` persisted to `stats.json` (camel-case keys, partial-merge load) | 6 unit tests + 1 doc test |
| D2 sessions | `src/simulator/session.rs` (`GameSessionMode`, `SessionStartParams`, `RaceFlag`, `RaceSession`), `src/simulator/session_flow.rs` (`SessionFlow`) | 8 + 10 unit tests + 1 doc test |
| D1 FeatureHub | `src/simulator/feature_hub.rs` — Bevy `Resource` composing mode/params, `SessionFlow`, `PersonalBestStore` and `TrackLimitsMonitor`; lap→PB event strings | 7 unit tests + 1 doc test |
| D3 track limits | `src/simulator/track_limits.rs` — lateral/centerline off-track detection, grace + cooldown, escalating time/drive-through/stop-go/DQ penalties | 10 unit tests + 1 doc test |
| B4/B5 differential + gearbox | `src/simulator/drivetrain.rs` — `DifferentialModel` (open/LSD clutch/viscous/Torsen/locked/active, preload, power/coast ramps, locking transfer, thermal) and `GearboxModel` (ratios, final drive, auto-shift, wheel torque) | 12 unit tests |
| B6 aero draft | `src/simulator/aero_draft.rs` — `DraftState` + `compute_draft` slipstream/tow | 6 unit tests |
| B1 tyre wear | `src/simulator/tire_wear.rs` — `TireWearSystem` (thermal surface/core/carcass, wear + graining + blistering, compound presets) and `TireFlatSpot` | 11 unit tests |
| B2 brake thermal | `src/simulator/brake_thermal.rs` — `BrakeThermalModel` (per-corner disc/pad temp + fade) and `BrakeWearSystem` (fade types, pad/disc wear, warpage; thermal types renamed `BrakeWearThermal*` per upstream's own note) | 10 unit tests |
| B3 damage | `src/simulator/damage.rs` — rF2-style `DamageSystem` (11 zones, component damage, impact distribution, CG shift, repair), `Rf2DamageModel` impulse bridge, `MechanicalDamage` continuous wear, `DamageTelemetry` sample | 17 unit tests |
| D9 camera | `src/simulator/camera.rs` — `CameraController` cockpit/chase/free, head-bob, chase lag, free-fly, RH view + projection | 8 unit tests |
| D5 pit/garage | `src/simulator/pit.rs` — `GarageExitController` phase machine (block reasons, pit limiter, exit cone), `PitLaneQueue` (leave/enter spacing + priority), `PitLaneCollision` (OBB SAT, walls, damage impulses, sync helpers), `PitLaneRepair` (job planning from `RepairData`, fuel/tyres, abort) and `PitStrategyState` mapping | 12 unit tests |
| D8 input + FFB | `src/simulator/input.rs` (joystick/keyboard mapping, `key_name`/`key_from_name`, flat-ini persistence, `InputManager` with dead-zone/gamma curves, edge-triggered shifts, injection and a `ControllerBackend` trait for HW polling) + `src/simulator/ffb.rs` (`FfbBridge` aligning-moment→torque via `MagicFormulaTire`, `FfbOutput` soft-FFB with optional `FfbActuator`) | 14 + 6 unit tests |
| E4 telemetry out | `src/net/telemetry.rs` (UDP `:20777`, TCP `:20778`, `TelemetryPlugin`) | 9 unit tests + 1 doc test |
| I1 CI | `.github/workflows/ci.yml` (Windows, `fmt` + `clippy -D warnings` + `test`) | — |
| — | `src/editor/ini.rs` — ordered INI reader shared by A2/A9 | 6 unit tests |
| **Bevy 0.20** | `Cargo.toml`/`Cargo.lock`, `src/main.rs`, `editor/audio/*`, `plugins/physics.rs`, all of `simulator/*` (§0) | whole tree compiles, lints clean |

Green baseline: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
`cargo test --all-targets` (246 unit tests) and `cargo test --doc` (9 doc tests)
all pass.

---

## 1. Already ported

| RacingEdge | ksengine reference |
| --- | --- |
| `src/simulator/tire.rs` | `src/engine/physics/KsTireModel.*` (Magic Formula) |
| `src/simulator/engine.rs`, `lut.rs` | `src/engine/physics/EngineModel.*`, `loadPowerLut` |
| `src/simulator/drivetrain.rs` | `src/engine/physics/GearboxModel.*`, `DifferentialModel.*` |
| `src/simulator/aero_draft.rs` | `src/engine/physics/AeroDraft.h` |
| `src/simulator/tire_wear.rs` | `src/engine/physics/TireWearSystem.*`, `TireFlatSpot.h` |
| `src/simulator/brake_thermal.rs` | `src/engine/physics/BrakeThermalModel.*`, `BrakeWearSystem.*` |
| `src/simulator/damage.rs` | `src/engine/physics/DamageSystem.*`, `Rf2DamageModel.h`, `MechanicalDamage.h`, `DamageTelemetry.h` |
| `src/simulator/camera.rs` | `src/simulator/CameraController.*` |
| `src/simulator/pit.rs` | `src/simulator/GarageExit.h`, `PitLaneQueue.h`, `PitLaneCollision.h`, `PitLaneRepair.h`, `PitStrategyBridge.h` |
| `src/simulator/input.rs` | `src/simulator/InputManager.{h,cpp}`, `src/engine/devices/xinput/XInputDevice.*`, `DirectInputJoystick.*` |
| `src/simulator/ffb.rs` | `src/simulator/FfbOutput.h`, `src/engine/devices/FFBBridge.{h,cpp}` |
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
| A4 ✅ | `formats/ksanim.rs` — `.ksanim` node-animation container (binary, v0/v1/v2) | `FileFormat/KsAnim.*` | S |
| A5 | `formats/acd.rs` — `data.acd` (ROT decipher + key) | `FileFormat/AcdReader.*` | M |
| A6 | `formats/setup.rs` — setup `.ini` + `setup.json` per car | `Simulator/SetupFile.h`, `SetupJson.h` | S/M |
| A7 | `formats/replay.rs` — replay record/playback | `Simulator/ReplayRecorder.*` | M |
| A8 | `formats/telemetry_log.rs` — telemetry dump/read (v2, damage channels) | `physics/TelemetryPhysics.*`, `UdpTelemetryBridge.h` | S |
| A9 ✅ | `editor/audio/sounds_ini.rs` — `sounds.ini` event routing parser | `Simulator/SoundsIniParser.*` | S |
| A10 | `formats/vehicle_meta.rs` — `materials.txt`, `team.ini`, `upgrades.ini`, livery bundle | `engine/vehicle/VehicleUpgradeSystem.h`, `VehicleAppearanceBundle.h` | S/M |
| A11 | `formats/csp_config.rs` — CSP-style key/value config | `engine/Config/CspConfigParser.*` | S |
| A12 | CLI: **FSPRO ↔ KSAudio ↔ FSB** conversion | `FileFormat/FSPROImporter/Exporter.*`, `KSAudio*` | M |
| A13 | CLI: KN5 baker → runtime format + texture extraction | `FileFormat/Kn5Baker.*`, `tools/kn5baker` | M/L |
| A14 ✅ | `simulator/fonts.rs` — JSON bitmap-font definitions for cockpit digital instruments (per-glyph `width`, `x_offset`, `y_offset`, text measurement) | — |

## 3. Physics — extend `src/simulator/`

| # | Module to write | ksengine reference | Effort |
| --- | --- | --- | --- |
| B1 ✅ | `tire_wear.rs` — tyre wear + flat spots | `TireWearSystem.*`, `TireFlatSpot.h` | M |
| B2 ✅ | `brake_thermal.rs` — brake temperature + pad/disc wear | `BrakeThermalModel.*`, `BrakeWearSystem.*` | M |
| B3 ✅ | `damage.rs` — rF2 damage zones, impact impulses, damage telemetry | `DamageSystem.*`, `Rf2DamageModel.h`, `MechanicalDamage.h`, `DamageTelemetry.h` | M/L |
| B4 ✅ | Full differential: preload, power/coast ramps, LSD | `DifferentialModel.*` | S/M |
| B5 ✅ | Gearbox: ratios, final drive, auto-shift, wheel torque | `GearboxModel.*` | S |
| B6 ✅ | `aero_draft.rs` — slipstream / tow | `AeroDraft.h` | S |
| B7 | Suspension kinematics (camber/toe vs travel, ARB, ride height) | `SuspensionKinematics.h` | M |
| B8 | `track_surface.rs` — dynamic grip grid: rubbering, marbles, per-sector wetness | `TrackSurface.h`, `TrackPhysics.*` | M/L |
| B9 | `weather.rs` — weather keyframes, grip vs wetness, track temperature | `WeatherPhysics.*`, `WeatherConfig.*`, `physics/weather/` | M |
| B10 ✅ | `strategy.rs` — fuel planning + pit strategy (ksengine has no separate `FuelSystem`; fuel lives in `StrategySimulator`, so the port keeps it together) | `StrategySimulator.*` | S/M |
| B11 ✅ | `timing.rs` — lap/sector timer + per-track/car personal bests (`PersonalBestStore` in `personal_best.rs`) | `phys_LapTimer.*`, `LapSectorTimer.h`, `PersonalBestStore.h` | S/M |
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
| D1 ✅ | `feature_hub.rs` — Bevy resource (session, flow, PB). Discovery/control sockets attach with E2/E3, track limits with D3, weather with B9, replay with D4 | `Simulator/FeatureHub.h` | M |
| D2 ✅ | `session.rs` + `session_flow.rs` — practice/qualifying/race, flags, penalties, countdown, grid publish, standings | `RaceSession*.*`, `RaceFlag.h`, `SessionFlow.h` | M/L |
| D3 ✅ | `track_limits.rs` — track cut, penalties (pit speeding still to do) | `TrackLimitsMonitor.h` | M |
| D4 | Replay recorder + playback | `ReplayRecorder.*` | M |
| D5 ✅ | Garage/pit state machine: garage exit → queue → collision → repair → strategy | `GarageExit.*`, `PitLaneQueue/Collision/Repair.h`, `PitStrategyBridge.h` | M/L |
| D6 | Setup garage UI + setup persistence per car/track | `SetupGarage.*`, `ApplySetup.h` | M |
| D7 | Multi-car manager + AI traffic (spline following) | `MultiCarManager.*`, `AIController.*`, `AIDriver.h` | L |
| D8 ✅ | Input & FFB: gamepad, DirectInput wheels, force feedback output (HW polling/actuators behind traits, attached by the app) | `InputManager.*`, `devices/*`, `FfbOutput.h` | M/L |
| D9 ✅ | `camera.rs` — cockpit / cinematic / replay cameras | `CameraController.*` | S |
| D10 | HUD + telemetry overlay in Bevy UI | `DashboardOverlay.*`, `TelemetryOverlay.*`, `RaceTelemetryHud.h` | M |
| D11 | Menus: pause, settings, key rebind, device settings | `GameMenuOverlay.*`, `KeyRebindOverlay.h` | M |
| D12 | `content_library.rs` — scan installed content/mods | `ContentLibrary.h`, `tools/ksmmm` | M |
| D13 ✅ | `user_stats.rs` — career stats: wins, poles, podiums, best lap (the `DriverProfile` helpers land with the D11 menus) | `UserStats.h` | S |

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

**Phase 1 — close the AC vertical.** A2 ✅, A9 ✅, B10 ✅, B11 ✅, D1 ✅, D13 ✅,
E4 ✅, I1 ✅. **Done.**

**Phase 2 — driving experience.** D2 ✅, D3 ✅, B1 ✅, B2 ✅, B3 ✅, B4 ✅, B5 ✅,
B6 ✅, D9 ✅, D5 ✅, D8 ✅, D6, D10, C1, C2.

**Phase 3 — depth and platform.** A1, A3, A7, B3, B7, B8, B9, B14, D4, D7, E1,
E2, E3, E5, H1, H2, H3.

**Phase 4 — editor and extended modding.** A12, A13, C3–C7, F1–F5, G1–G6,
E6–E8.

## High-value / low-cost starting points

1. **E4 telemetry out** — makes RacingEdge compatible with existing dashboards
   and telemetry tools straight away.
2. **A2 + A9** (`data.json`, `surfaces.ini`, `sounds.ini`) — completes the AC
   track/car content pipeline started by `ac_import.rs`.
3. **B10 + B11 ✅** (fuel and timing/PB) — a complete race loop with lap times
   and pit stops.
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
