//! `racingedge-server` — headless dedicated server (port of ksengine
//! `tools/ks_server` and `SimulatorServerApp`).
//!
//! No renderer: it drives the simulation with a built-in pace driver, hosts LAN
//! discovery (`:20779`) and the control API (`:20780`), and spawns an optional
//! AI field. Exit codes: `0` completed or interrupted, `1` `--duration`
//! exceeded, `2` bad usage.

use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use racingedge::simulator::server::{parse_args, ConfigError, HeadlessServer};

/// The fixed server tick (20 Hz) fed to the simulation.
const SERVER_SLICE: f64 = 0.05;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let config = match parse_args(&args) {
        Ok(config) => config,
        Err(ConfigError::Help(text)) => {
            print!("{text}");
            return ExitCode::SUCCESS;
        }
        Err(ConfigError::Invalid(message)) => {
            eprintln!("racingedge-server: {message}");
            return ExitCode::from(2);
        }
    };

    let stop = Arc::new(AtomicBool::new(false));
    install_ctrl_handler(Arc::clone(&stop));

    let mut server = HeadlessServer::new(config.clone());
    if !server.start() {
        eprintln!(
            "racingedge-server: warning: services partially unavailable (discovery={} control={})",
            server.discovery_started(),
            server.control_started()
        );
    }
    eprintln!(
        "racingedge-server: name={:?} laps={} time={}s countdown={:.0}s track={:.0}m duration={:.0}s{}",
        config.name,
        config.laps,
        config.time_limit,
        config.countdown,
        config.track_length,
        config.duration,
        if config.fast { " [fast]" } else { "" }
    );
    eprintln!(
        "racingedge-server: running (discovery :{}, control :{})",
        config.discovery_port, config.control_port
    );

    let realtime = !config.fast;
    let mut next_status = if config.status_every > 0.0 {
        config.status_every
    } else {
        f64::INFINITY
    };
    let mut sim_clock = 0.0;
    let mut last = Instant::now();
    let mut exit = ExitCode::SUCCESS;

    while !stop.load(Ordering::SeqCst) {
        let report = server.tick(SERVER_SLICE);
        sim_clock += SERVER_SLICE;

        if report.finished {
            println!(
                "SESSION COMPLETE: laps={} time={:.1}s",
                report.lap, report.session_time
            );
            break;
        }
        if sim_clock >= config.duration {
            eprintln!(
                "racingedge-server: TIMEOUT after {:.1}s sim (phase={})",
                sim_clock, report.phase
            );
            exit = ExitCode::from(1);
            break;
        }
        if sim_clock >= next_status {
            println!("[status] {}", server.status_line());
            next_status += config.status_every;
        }
        if realtime {
            let target = Duration::from_secs_f64(SERVER_SLICE);
            let elapsed = last.elapsed();
            if elapsed < target {
                std::thread::sleep(target - elapsed);
            }
            last = Instant::now();
        }
    }

    if stop.load(Ordering::SeqCst) {
        eprintln!("racingedge-server: interrupted, shutting down cleanly");
    }
    server.stop();
    exit
}

/// Installs a Ctrl+C / console-close handler that flips `stop`.
#[cfg(windows)]
fn install_ctrl_handler(stop: Arc<AtomicBool>) {
    static STOP: OnceLock<Arc<AtomicBool>> = OnceLock::new();
    let _ = STOP.set(stop);

    extern "system" fn handler(ctrl_type: u32) -> i32 {
        if matches!(ctrl_type, 0..=2) {
            if let Some(stop) = STOP.get() {
                stop.store(true, Ordering::SeqCst);
            }
            return 1;
        }
        0
    }

    extern "system" {
        fn SetConsoleCtrlHandler(handler: Option<extern "system" fn(u32) -> i32>, add: i32) -> i32;
    }
    // SAFETY: registering a valid console handler for the process lifetime.
    unsafe {
        SetConsoleCtrlHandler(Some(handler), 1);
    }
}

/// Installs a SIGINT handler that flips `stop`.
#[cfg(unix)]
fn install_ctrl_handler(stop: Arc<AtomicBool>) {
    static STOP: OnceLock<Arc<AtomicBool>> = OnceLock::new();
    let _ = STOP.set(stop);

    extern "C" fn handler(_signal: i32) {
        if let Some(stop) = STOP.get() {
            stop.store(true, Ordering::SeqCst);
        }
    }

    extern "C" {
        fn signal(signal: i32, handler: extern "C" fn(i32)) -> usize;
    }
    // SAFETY: installing SIGINT (2) with a valid handler for the process lifetime.
    unsafe {
        signal(2, handler);
    }
}

#[cfg(not(any(windows, unix)))]
fn install_ctrl_handler(_stop: Arc<AtomicBool>) {}
