//! Script engine facade over rhai (port of ksengine `engine/Scripting/ScriptHost.*`
//! and `luaScript/LuaScriptHost.*`).
//!
//! The C++ side never touches the Lua state directly; here the engine never
//! leaks its [`rhai::Engine`]/[`rhai::AST`] either — callers only see the
//! methods on [`ScriptHost`].

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use rhai::{Dynamic, Engine, ImmutableString, Module, Scope, AST};

/// Maximum number of `ks::log` lines kept (matches ksengine's ring of 512).
pub const LOG_CAPACITY: usize = 512;

/// A compiled, executed mod script.
struct Script {
    path: String,
    ast: AST,
}

type NumberStore = Arc<Mutex<HashMap<String, f64>>>;
type LogRing = Arc<Mutex<Vec<String>>>;
type HandlerMap = Arc<Mutex<HashMap<String, Vec<String>>>>;

/// Facade over the rhai engine: run scripts, read/write shared numbers and
/// dispatch events to mod handlers.
pub struct ScriptHost {
    engine: Engine,
    scope: Scope<'static>,
    scripts: Vec<Script>,
    numbers: NumberStore,
    logs: LogRing,
    handlers: HandlerMap,
    initialized: bool,
    last_error: String,
}

impl ScriptHost {
    /// Creates an uninitialized host. Call [`initialize`](Self::initialize)
    /// (or any `eval`/`run_file`, which initializes lazily) before use.
    #[must_use]
    pub fn new() -> Self {
        Self {
            engine: Engine::new(),
            scope: Scope::new(),
            scripts: Vec::new(),
            numbers: Arc::new(Mutex::new(HashMap::new())),
            logs: Arc::new(Mutex::new(Vec::new())),
            handlers: Arc::new(Mutex::new(HashMap::new())),
            initialized: false,
            last_error: String::new(),
        }
    }

    /// Installs the `ks` prelude. Idempotent; returns `false` only if a previous
    /// initialization left the host unusable.
    pub fn initialize(&mut self) -> bool {
        if self.initialized {
            return true;
        }
        let mut module = Module::new();

        let logs = Arc::clone(&self.logs);
        module.set_native_fn("log", move |message: ImmutableString| {
            push_log(&logs, message.to_string());
            Ok(())
        });

        let numbers = Arc::clone(&self.numbers);
        module.set_native_fn(
            "get",
            move |name: ImmutableString| -> Result<f64, Box<rhai::EvalAltResult>> {
                let value = numbers
                    .lock()
                    .map(|store| store.get(name.as_str()).copied().unwrap_or(0.0))
                    .unwrap_or(0.0);
                Ok(value)
            },
        );

        let numbers = Arc::clone(&self.numbers);
        module.set_native_fn("set", move |name: ImmutableString, value: f64| {
            if let Ok(mut store) = numbers.lock() {
                store.insert(name.to_string(), value);
            }
            Ok(())
        });

        let handlers = Arc::clone(&self.handlers);
        module.set_native_fn(
            "on",
            move |event: ImmutableString, handler: ImmutableString| {
                if let Ok(mut map) = handlers.lock() {
                    map.entry(event.to_string())
                        .or_default()
                        .push(handler.to_string());
                }
                Ok(())
            },
        );

        module.set_native_fn(
            "version",
            || -> Result<ImmutableString, Box<rhai::EvalAltResult>> {
                Ok(ImmutableString::from(env!("CARGO_PKG_VERSION")))
            },
        );

        self.engine.register_static_module("ks", module.into());
        self.initialized = true;
        true
    }

    /// Drops all scripts and state, leaving the host uninitialized.
    pub fn shutdown(&mut self) {
        self.engine = Engine::new();
        self.scope = Scope::new();
        self.scripts.clear();
        self.initialized = false;
        if let Ok(mut store) = self.numbers.lock() {
            store.clear();
        }
        if let Ok(mut logs) = self.logs.lock() {
            logs.clear();
        }
        if let Ok(mut handlers) = self.handlers.lock() {
            handlers.clear();
        }
        self.last_error.clear();
    }

    /// Whether the `ks` prelude has been installed.
    #[must_use]
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Evaluates a chunk (or expression). Returns `false` and records
    /// [`last_error`](Self::last_error) on a parse or run error.
    pub fn eval(&mut self, code: &str) -> bool {
        self.initialize();
        match self.engine.compile(code) {
            Ok(ast) => match self.engine.run_ast_with_scope(&mut self.scope, &ast) {
                Ok(()) => {
                    self.last_error.clear();
                    true
                }
                Err(error) => {
                    self.last_error = error.to_string();
                    false
                }
            },
            Err(error) => {
                self.last_error = error.to_string();
                false
            }
        }
    }

    /// Reads, compiles and runs a script file. A compiled script's global
    /// functions stay callable afterwards even if the run itself failed.
    pub fn run_file(&mut self, path: impl AsRef<Path>) -> bool {
        self.initialize();
        let path = path.as_ref();
        let code = match std::fs::read_to_string(path) {
            Ok(code) => code,
            Err(error) => {
                self.last_error = format!("{}: {error}", path.display());
                return false;
            }
        };
        self.run_script(&path.display().to_string(), &code)
    }

    /// Compiles and runs an in-memory script under a display `path`.
    pub fn run_script(&mut self, path: &str, code: &str) -> bool {
        self.initialize();
        let ast = match self.engine.compile(code) {
            Ok(ast) => ast,
            Err(error) => {
                self.last_error = format!("{path}: {error}");
                return false;
            }
        };
        let result = self.engine.run_ast_with_scope(&mut self.scope, &ast);
        self.scripts.push(Script {
            path: path.to_string(),
            ast,
        });
        match result {
            Ok(()) => {
                self.last_error.clear();
                true
            }
            Err(error) => {
                self.last_error = format!("{path}: {error}");
                false
            }
        }
    }

    /// The paths of every script compiled so far, in load order.
    #[must_use]
    pub fn script_paths(&self) -> Vec<String> {
        self.scripts
            .iter()
            .map(|script| script.path.clone())
            .collect()
    }

    /// Reads a shared number, if set.
    #[must_use]
    pub fn get_number(&self, name: &str) -> Option<f64> {
        self.numbers.lock().ok()?.get(name).copied()
    }

    /// Writes a shared number (the `ks.get` / `ks.set` store).
    pub fn set_number(&mut self, name: &str, value: f64) -> bool {
        match self.numbers.lock() {
            Ok(mut store) => {
                store.insert(name.to_string(), value);
                true
            }
            Err(_) => false,
        }
    }

    /// Whether any loaded script defines a global function `name`.
    #[must_use]
    pub fn has_function(&self, name: &str) -> bool {
        self.scripts
            .iter()
            .any(|script| ast_has_function(&script.ast, name))
    }

    /// Calls the global function `name` with the given numeric arguments.
    /// Returns `false` when the function is missing or raised an error.
    pub fn call_function(&mut self, name: &str, args: &[f64]) -> bool {
        if !self.has_function(name) {
            self.last_error = format!("function not found: {name}");
            return false;
        }
        let args: Vec<Dynamic> = args.iter().map(|value| Dynamic::from(*value)).collect();
        self.call_in_scripts(name, args)
    }

    /// Dispatches an engine event: calls every `ks.on("<event>", …)` handler
    /// plus the conventional global `on_<event>`, passing the numbers first and
    /// then the strings. Having no handler is success.
    pub fn emit_event(&mut self, event: &str, numbers: &[f64], strings: &[String]) -> bool {
        let mut names = vec![format!("on_{event}")];
        if let Ok(map) = self.handlers.lock() {
            if let Some(list) = map.get(event) {
                names.extend(list.iter().cloned());
            }
        }

        let mut args: Vec<Dynamic> = numbers.iter().map(|value| Dynamic::from(*value)).collect();
        args.extend(strings.iter().map(|value| Dynamic::from(value.clone())));

        let mut error = None;
        for name in &names {
            for script in &self.scripts {
                if ast_has_function(&script.ast, name) {
                    if let Err(raised) = self.engine.call_fn::<Dynamic>(
                        &mut self.scope,
                        &script.ast,
                        name.as_str(),
                        args.clone(),
                    ) {
                        error = Some(raised.to_string());
                    }
                }
            }
        }
        match error {
            Some(message) => {
                self.last_error = message;
                false
            }
            None => {
                self.last_error.clear();
                true
            }
        }
    }

    /// Drains the `ks.log` lines collected since the last call.
    pub fn take_log(&mut self) -> Vec<String> {
        match self.logs.lock() {
            Ok(mut logs) => std::mem::take(&mut *logs),
            Err(_) => Vec::new(),
        }
    }

    /// The message from the last failed call, or empty after a successful one.
    #[must_use]
    pub fn last_error(&self) -> &str {
        &self.last_error
    }

    fn call_in_scripts(&mut self, name: &str, args: Vec<Dynamic>) -> bool {
        let mut called = false;
        let mut error = None;
        for script in &self.scripts {
            if ast_has_function(&script.ast, name) {
                called = true;
                if let Err(raised) =
                    self.engine
                        .call_fn::<Dynamic>(&mut self.scope, &script.ast, name, args.clone())
                {
                    error = Some(raised.to_string());
                }
            }
        }
        match error {
            Some(message) => {
                self.last_error = message;
                false
            }
            None => {
                self.last_error.clear();
                called
            }
        }
    }
}

impl Default for ScriptHost {
    fn default() -> Self {
        Self::new()
    }
}

fn ast_has_function(ast: &AST, name: &str) -> bool {
    ast.iter_functions().any(|metadata| metadata.name == name)
}

fn push_log(logs: &LogRing, message: String) {
    if let Ok(mut ring) = logs.lock() {
        if ring.len() >= LOG_CAPACITY {
            ring.remove(0);
        }
        ring.push(message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initialize_is_idempotent() {
        let mut host = ScriptHost::new();
        assert!(!host.is_initialized());
        assert!(host.initialize());
        assert!(host.initialize());
        assert!(host.is_initialized());
    }

    #[test]
    fn eval_get_set_roundtrip() {
        let mut host = ScriptHost::new();
        host.set_number("speed", 100.0);
        assert!(host.eval("ks::set(\"speed\", ks::get(\"speed\") + 10.0)"));
        assert_eq!(host.get_number("speed"), Some(110.0));
        assert_eq!(host.get_number("missing"), None);
        assert!(host.last_error().is_empty());
    }

    #[test]
    fn version_is_exposed() {
        let mut host = ScriptHost::new();
        assert!(host.eval("ks::log(ks::version());"));
        assert_eq!(host.take_log(), vec![env!("CARGO_PKG_VERSION").to_string()]);
    }

    #[test]
    fn has_and_call_function() {
        let mut host = ScriptHost::new();
        assert!(host.run_script("f.rhai", "fn boost(mult) { ks::set(\"out\", mult * 2.0); }"));
        assert!(host.has_function("boost"));
        assert!(!host.has_function("nope"));
        assert!(host.call_function("boost", &[3.0]));
        assert_eq!(host.get_number("out"), Some(6.0));
    }

    #[test]
    fn call_missing_function_fails() {
        let mut host = ScriptHost::new();
        assert!(!host.call_function("ghost", &[]));
        assert!(host.last_error().contains("ghost"));
    }

    #[test]
    fn emit_event_passes_numbers_then_strings() {
        let mut host = ScriptHost::new();
        assert!(host.run_script(
            "lap.rhai",
            "fn on_lap(speed, message) { ks::log(message); ks::set(\"speed\", speed); }",
        ));
        assert!(host.emit_event("lap", &[42.0], &["fast".to_string()]));
        assert_eq!(host.get_number("speed"), Some(42.0));
        assert_eq!(host.take_log(), vec!["fast".to_string()]);
    }

    #[test]
    fn ks_on_registers_named_handler() {
        let mut host = ScriptHost::new();
        assert!(host.run_script(
            "on.rhai",
            "fn my_handler() { ks::set(\"fired\", 1.0); }\nks::on(\"beep\", \"my_handler\");",
        ));
        assert!(host.emit_event("beep", &[], &[]));
        assert_eq!(host.get_number("fired"), Some(1.0));
    }

    #[test]
    fn dispatch_without_handler_is_success() {
        let mut host = ScriptHost::new();
        assert!(host.emit_event("nobody_listens", &[1.0], &[]));
    }

    #[test]
    fn log_ring_is_bounded() {
        let mut host = ScriptHost::new();
        assert!(host.eval("for i in 0..600 { ks::log(\"x\"); }"));
        assert_eq!(host.take_log().len(), LOG_CAPACITY);
        assert!(host.take_log().is_empty());
    }

    #[test]
    fn syntax_error_sets_last_error() {
        let mut host = ScriptHost::new();
        assert!(!host.run_script("bad.rhai", "fn ("));
        assert!(host.last_error().starts_with("bad.rhai:"));
        assert!(host.script_paths().is_empty());
    }

    #[test]
    fn shutdown_resets_state() {
        let mut host = ScriptHost::new();
        host.set_number("a", 1.0);
        assert!(host.run_script("s.rhai", "fn f() {}"));
        host.shutdown();
        assert!(!host.is_initialized());
        assert_eq!(host.get_number("a"), None);
        assert!(!host.has_function("f"));
    }
}
