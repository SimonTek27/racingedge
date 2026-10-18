//! Mod SDK helpers (port of ksengine `engine/Scripting/ModSdk.*` and
//! `HotReloadManager.*`).
//!
//! A mod is a content folder (track/car) whose `scripts/*.rhai` files are run
//! at content load time. Scripts are independent: one failing file is reported
//! but never stops the others, and a mod with no scripts at all is not an
//! error.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::script_host::ScriptHost;

/// The result of [`load_mod_scripts`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LoadReport {
    /// The script paths that ran, in load order.
    pub loaded: Vec<String>,
    /// `"<path>: <message>"` for each script that failed.
    pub errors: Vec<String>,
}

impl LoadReport {
    /// Whether every script that was found ran without error.
    #[must_use]
    pub fn ok(&self) -> bool {
        self.errors.is_empty()
    }
}

/// Installs the mod SDK prelude into `host` (idempotent). Returns `false` only
/// when the host cannot be initialized.
pub fn install(host: &mut ScriptHost) -> bool {
    host.initialize()
}

/// Runs the mod scripts under `mod_root`:
///
/// * `mod_root/scripts/*.rhai` when `scripts/` exists, else `mod_root/*.rhai`;
/// * sorted by filename so load order is deterministic.
///
/// A `mod_root` that is not a directory is an error; having no scripts at all
/// is not.
pub fn load_mod_scripts(host: &mut ScriptHost, mod_root: impl AsRef<Path>) -> LoadReport {
    let mod_root = mod_root.as_ref();
    let mut report = LoadReport::default();
    if !mod_root.is_dir() {
        report
            .errors
            .push(format!("{}: not a directory", mod_root.display()));
        return report;
    }
    for path in collect_scripts(mod_root) {
        if host.run_file(&path) {
            report.loaded.push(path.display().to_string());
        } else {
            report
                .errors
                .push(format!("{}: {}", path.display(), host.last_error()));
        }
    }
    report
}

/// Dispatches an engine event to every handler (see
/// [`ScriptHost::emit_event`]).
pub fn dispatch(host: &mut ScriptHost, event: &str, numbers: &[f64], strings: &[String]) -> bool {
    host.emit_event(event, numbers, strings)
}

/// `ks.log` lines since the last call (bounded ring of 512).
pub fn take_log(host: &mut ScriptHost) -> Vec<String> {
    host.take_log()
}

/// Watches mod directories and re-runs scripts whose file changed (port of
/// ksengine `HotReloadManager`).
#[derive(Debug, Default)]
pub struct ScriptHotReloader {
    roots: Vec<PathBuf>,
    seen: BTreeMap<PathBuf, SystemTime>,
}

impl ScriptHotReloader {
    /// Creates an empty reloader.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a mod root (or a `scripts/` folder) to watch.
    pub fn watch_dir(&mut self, dir: impl AsRef<Path>) {
        self.roots.push(dir.as_ref().to_path_buf());
    }

    /// Re-runs every new or changed script, returning the reloaded paths.
    pub fn poll(&mut self, host: &mut ScriptHost) -> Vec<PathBuf> {
        let mut reloaded = Vec::new();
        for root in self.roots.clone() {
            for path in collect_scripts(&root) {
                let modified = std::fs::metadata(&path)
                    .and_then(|meta| meta.modified())
                    .ok();
                let unchanged = matches!(
                    (self.seen.get(&path), modified),
                    (Some(previous), Some(now)) if *previous == now
                );
                if unchanged {
                    continue;
                }
                if host.run_file(&path) {
                    reloaded.push(path.clone());
                }
                if let Some(now) = modified {
                    self.seen.insert(path, now);
                }
            }
        }
        reloaded
    }

    /// The number of scripts being tracked.
    #[must_use]
    pub fn tracked(&self) -> usize {
        self.seen.len()
    }
}

/// Lists the script files under `root` in deterministic load order.
fn collect_scripts(root: &Path) -> Vec<PathBuf> {
    let scripts_dir = root.join("scripts");
    let dir = if scripts_dir.is_dir() {
        scripts_dir
    } else {
        root.to_path_buf()
    };
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().map(|ext| ext == "rhai").unwrap_or(false))
        .collect();
    files.sort();
    files
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn temp_dir(tag: &str) -> PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "racingedge_script_{tag}_{}_{id}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn load_report_ok_tracks_errors() {
        let report = LoadReport::default();
        assert!(report.ok());
        let report = LoadReport {
            loaded: vec!["a".into()],
            errors: vec!["b: boom".into()],
        };
        assert!(!report.ok());
    }

    #[test]
    fn load_mod_scripts_reads_scripts_dir_in_order() {
        let root = temp_dir("order");
        let scripts = root.join("scripts");
        std::fs::create_dir_all(&scripts).unwrap();
        std::fs::write(scripts.join("b.rhai"), "ks::set(\"b\", 2.0);").unwrap();
        std::fs::write(scripts.join("a.rhai"), "ks::set(\"a\", 1.0);").unwrap();
        std::fs::write(scripts.join("ignore.txt"), "not a script").unwrap();

        let mut host = ScriptHost::new();
        let report = load_mod_scripts(&mut host, &root);
        assert!(report.ok(), "{:?}", report.errors);
        assert_eq!(report.loaded.len(), 2);
        assert!(report.loaded[0].ends_with("a.rhai"));
        assert_eq!(host.get_number("a"), Some(1.0));
        assert_eq!(host.get_number("b"), Some(2.0));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn load_mod_scripts_falls_back_to_root() {
        let root = temp_dir("root");
        std::fs::write(root.join("one.rhai"), "ks::set(\"one\", 1.0);").unwrap();

        let mut host = ScriptHost::new();
        let report = load_mod_scripts(&mut host, &root);
        assert!(report.ok());
        assert_eq!(report.loaded.len(), 1);
        assert_eq!(host.get_number("one"), Some(1.0));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn load_mod_scripts_reports_bad_file_but_keeps_going() {
        let root = temp_dir("mixed");
        std::fs::write(root.join("a.rhai"), "ks::set(\"a\", 1.0);").unwrap();
        std::fs::write(root.join("b.rhai"), "ks::set(\"b\", ").unwrap();
        std::fs::write(root.join("c.rhai"), "ks::set(\"c\", 3.0);").unwrap();

        let mut host = ScriptHost::new();
        let report = load_mod_scripts(&mut host, &root);
        assert!(!report.ok());
        assert_eq!(report.loaded.len(), 2);
        assert_eq!(report.errors.len(), 1);
        assert!(report.errors[0].contains("b.rhai"));
        assert_eq!(host.get_number("c"), Some(3.0));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_root_is_an_error() {
        let missing = temp_dir("gone").join("nope");
        let mut host = ScriptHost::new();
        let report = load_mod_scripts(&mut host, &missing);
        assert!(!report.ok());
        assert!(report.loaded.is_empty());
        assert!(report.errors[0].contains("not a directory"));
    }

    #[test]
    fn empty_mod_folder_is_not_an_error() {
        let root = temp_dir("empty");
        let mut host = ScriptHost::new();
        let report = load_mod_scripts(&mut host, &root);
        assert!(report.ok());
        assert!(report.loaded.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn install_initializes_host() {
        let mut host = ScriptHost::new();
        assert!(install(&mut host));
        assert!(host.is_initialized());
    }

    #[test]
    fn hot_reloader_runs_then_reloads_on_change() {
        let root = temp_dir("hot");
        std::fs::write(root.join("r.rhai"), "ks::set(\"v\", 1.0);").unwrap();

        let mut host = ScriptHost::new();
        let mut reloader = ScriptHotReloader::new();
        reloader.watch_dir(&root);

        assert_eq!(reloader.poll(&mut host).len(), 1);
        assert_eq!(host.get_number("v"), Some(1.0));
        assert_eq!(reloader.tracked(), 1);
        assert!(reloader.poll(&mut host).is_empty());

        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(root.join("r.rhai"), "ks::set(\"v\", 9.0);").unwrap();
        assert_eq!(reloader.poll(&mut host).len(), 1);
        assert_eq!(host.get_number("v"), Some(9.0));

        let _ = std::fs::remove_dir_all(&root);
    }
}
