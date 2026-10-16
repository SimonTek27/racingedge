//! Rete e telemetria.
//!
//! Port dei canali di ksengine (`UdpTelemetryBridge`, `TcpTelemetryBridge`,
//! `CarStateSync`, `ServerDiscovery`, `ExternalControlApi`, …). Per ora è
//! implementata la **telemetria in uscita**; gli altri canali elencati nel
//! [backlog ksengine](../../docs/KSENGINE_PORT_BACKLOG.md) trovano posto qui.
//!
//! * [`telemetry`] — telemetria in uscita su UDP `:20777` e TCP `:20778`.

pub mod telemetry;
