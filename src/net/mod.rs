//! Rete e telemetria.
//!
//! Port of ksengine channels (`UdpTelemetryBridge`, `TcpTelemetryBridge`,
//! `CarStateSync`, `ServerDiscovery`, `ExternalControlApi`, …). For now is
//! implemented **outgoing telemetry**; the other channels listed in the
//! [ksengine port backlog](../../docs/KSENGINE_PORT_BACKLOG.md) are placed here.
//!
//! * [`telemetry`] — outgoing telemetry over UDP `:20777` and TCP `:20778`.

pub mod car_state_sync;
pub mod control_api;
pub mod discovery;
pub mod matchmaking;
pub mod shared_memory;
pub mod telemetry;
