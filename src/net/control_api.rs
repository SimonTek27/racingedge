//! API di controllo bidirezionale in stile InSim su TCP `:20780` — port di
//! ksengine `ExternalControlApi.h` (e `docs/CONTROL_API.md`).
//!
//! Un client alla volta, testo UTF-8 una riga per messaggio (`\n` o `\r\n`).
//! Il verbo è reso maiuscolo e gli argomenti sono separati da spazi (nessun
//! quoting). All'accettazione il server manda una riga di saluto
//! (`OK ksim control API v1`, oppure `... AUTH_REQUIRED` se è configurato un
//! token). `PING`/`AUTH` sono gestiti dal canale; ogni altro verbo è delegato a
//! un [`ControlHandler`] applicativo.
//!
//! ```
//! use racingedge::net::control_api::{parse_control_line, ControlSession, NullControlHandler};
//!
//! let cmd = parse_control_line("session time_attack 10");
//! assert_eq!(cmd.verb, "SESSION");
//! assert_eq!(cmd.args, vec!["time_attack", "10"]);
//!
//! let mut session = ControlSession::new();
//! let replies = session.process_line("PING", &mut NullControlHandler);
//! assert_eq!(replies, vec!["PONG\n"]);
//! ```

use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};

/// Porta TCP di default dell'API di controllo.
pub const DEFAULT_CONTROL_PORT: u16 = 20780;

/// Un comando di controllo analizzato (port di `ControlCommand`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ControlCommand {
    /// La riga originale.
    pub raw: String,
    /// Il verbo, reso maiuscolo.
    pub verb: String,
    /// Gli argomenti separati da spazi.
    pub args: Vec<String>,
}

/// Analizza una riga di comando (port di `parseControlLine`).
pub fn parse_control_line(line: &str) -> ControlCommand {
    let mut parts = line.split_whitespace();
    let verb = parts.next().unwrap_or("").to_ascii_uppercase();
    ControlCommand {
        raw: line.to_string(),
        verb,
        args: parts.map(str::to_string).collect(),
    }
}

/// Collettore delle righe in uscita da un comando.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ControlOut {
    lines: Vec<String>,
}

impl ControlOut {
    /// Aggiunge una riga di risposta (garantendo il terminatore).
    pub fn reply(&mut self, line: impl Into<String>) {
        self.push(line.into());
    }

    /// Aggiunge un evento, prefissato con `EVT `.
    pub fn event(&mut self, event: impl Into<String>) {
        self.push(format!("EVT {}", event.into()));
    }

    /// Le righe raccolte.
    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    /// Estrae le righe raccolte.
    pub fn take(&mut self) -> Vec<String> {
        std::mem::take(&mut self.lines)
    }

    fn push(&mut self, mut line: String) {
        if !line.ends_with('\n') {
            line.push('\n');
        }
        self.lines.push(line);
    }
}

/// Gestore applicativo dei verbi (diverso da `PING`/`AUTH`).
///
/// L'applicazione risponde e emette eventi tramite [`ControlOut`].
pub trait ControlHandler {
    fn handle_control(&mut self, cmd: &ControlCommand, out: &mut ControlOut);
}

/// Gestore nullo: risponde come l'upstream quando nessun handler è agganciato.
pub struct NullControlHandler;

impl ControlHandler for NullControlHandler {
    fn handle_control(&mut self, _cmd: &ControlCommand, out: &mut ControlOut) {
        out.reply("ERR unknown handler");
    }
}

/// Stato di autenticazione e dispatcher dei comandi (parte pura, senza socket).
#[derive(Debug, Default)]
pub struct ControlSession {
    token: String,
    auth_required: bool,
    authenticated: bool,
}

impl ControlSession {
    /// Crea una sessione aperta (nessun token).
    pub fn new() -> Self {
        Self::default()
    }

    /// Configura il token: un valore non vuoto attiva `AUTH_REQUIRED`
    /// (port di `setAuthToken`).
    pub fn set_auth_token(&mut self, token: &str) {
        self.token = token.to_string();
        self.auth_required = !token.is_empty();
        self.authenticated = false;
    }

    /// Forza la richiesta di autenticazione.
    pub fn set_auth_required(&mut self, on: bool) {
        self.auth_required = on;
    }

    /// Token configurato.
    pub fn token(&self) -> &str {
        &self.token
    }

    /// `true` quando l'autenticazione non è richiesta o è già avvenuta.
    pub fn is_authenticated(&self) -> bool {
        !self.auth_required || self.authenticated
    }

    /// Azzera lo stato di autenticazione (port di `clearAuth`).
    pub fn clear_auth(&mut self) {
        self.authenticated = false;
    }

    /// Riga di saluto da mandare all'accettazione (port del corpo di
    /// `acceptIfNeeded`).
    pub fn greeting(&self) -> &'static str {
        if self.auth_required {
            "OK ksim control API v1 AUTH_REQUIRED"
        } else {
            "OK ksim control API v1"
        }
    }

    /// Prepara la sessione per un nuovo client: autenticata se non serve token.
    pub fn on_client_connected(&mut self) {
        self.authenticated = !self.auth_required;
    }

    /// Gestisce una riga e ritorna le righe di risposta (port del corpo di
    /// `poll`).
    pub fn process_line(&mut self, line: &str, handler: &mut dyn ControlHandler) -> Vec<String> {
        let trimmed = line.trim_end_matches(['\r', ' ']);
        if trimmed.is_empty() {
            return Vec::new();
        }
        let cmd = parse_control_line(trimmed);
        let mut out = ControlOut::default();
        match cmd.verb.as_str() {
            "PING" => out.reply("PONG"),
            "AUTH" => {
                let provided = cmd.args.first().map(String::as_str).unwrap_or("");
                if !self.auth_required || provided == self.token {
                    self.authenticated = true;
                    out.reply("OK AUTH");
                } else {
                    self.authenticated = false;
                    out.reply("ERR AUTH");
                }
            }
            _ => {
                if self.auth_required && !self.authenticated {
                    out.reply("ERR UNAUTH - send AUTH <token>");
                } else {
                    handler.handle_control(&cmd, &mut out);
                }
            }
        }
        out.take()
    }
}

/// API di controllo su TCP (port di `ExternalControlApi`).
#[derive(Debug, Default)]
pub struct ExternalControlApi {
    listener: Option<TcpListener>,
    client: Option<TcpStream>,
    rx: Vec<u8>,
    session: ControlSession,
    port: u16,
    error: Option<String>,
}

impl ExternalControlApi {
    /// Crea l'API non avviata.
    pub fn new() -> Self {
        Self {
            port: DEFAULT_CONTROL_PORT,
            ..Default::default()
        }
    }

    /// Configura il token di autenticazione.
    pub fn set_auth_token(&mut self, token: &str) {
        self.session.set_auth_token(token);
    }

    /// Forza la richiesta di autenticazione.
    pub fn set_auth_required(&mut self, on: bool) {
        self.session.set_auth_required(on);
    }

    /// `true` quando il client è autenticato (o l'auth non serve).
    pub fn is_authenticated(&self) -> bool {
        self.session.is_authenticated()
    }

    /// Mette in ascolto su `0.0.0.0:port` in modo non bloccante (port di
    /// `start`).
    pub fn start(&mut self, port: u16) -> io::Result<()> {
        if self.listener.is_some() {
            return Ok(());
        }
        let listener = TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], port)))?;
        listener.set_nonblocking(true)?;
        self.listener = Some(listener);
        self.port = port;
        Ok(())
    }

    /// Chiude client e listener (port di `stop`).
    pub fn stop(&mut self) {
        self.close_client();
        self.listener = None;
    }

    /// Porta configurata.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Indirizzo locale, se in ascolto.
    pub fn local_addr(&self) -> Option<SocketAddr> {
        self.listener.as_ref().and_then(|l| l.local_addr().ok())
    }

    /// Ultimo errore, se presente.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// `true` quando un client è connesso.
    pub fn has_client(&self) -> bool {
        self.client.is_some()
    }

    /// Accetta un client in attesa, manda il saluto e prepara la sessione
    /// (port di `acceptIfNeeded`).
    pub fn accept_if_needed(&mut self) {
        if self.listener.is_none() || self.client.is_some() {
            return;
        }
        let Some(listener) = &self.listener else {
            return;
        };
        match listener.accept() {
            Ok((stream, _)) => {
                let _ = stream.set_nonblocking(true);
                let _ = stream.set_nodelay(true);
                self.client = Some(stream);
                self.session.on_client_connected();
                let greeting = self.session.greeting();
                self.send_line(greeting);
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    /// Riceve le righe disponibili e le dispaccia, scrivendo le risposte
    /// (port di `poll`).
    pub fn poll(&mut self, handler: &mut dyn ControlHandler) {
        self.accept_if_needed();
        if self.client.is_none() {
            return;
        }

        let mut closed = false;
        {
            let client = self.client.as_mut().expect("client presente");
            let mut buf = [0u8; 512];
            loop {
                match client.read(&mut buf) {
                    Ok(0) => {
                        closed = true;
                        break;
                    }
                    Ok(n) => self.rx.extend_from_slice(&buf[..n]),
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                    Err(_) => {
                        closed = true;
                        break;
                    }
                }
            }
        }
        if closed {
            self.close_client();
            return;
        }

        let mut outputs: Vec<String> = Vec::new();
        for line in take_lines(&mut self.rx) {
            outputs.extend(self.session.process_line(&line, handler));
        }
        for line in outputs {
            self.send_line(line.trim_end());
        }
    }

    /// Manda una riga al client (port di `sendLine`).
    pub fn send_line(&mut self, line: &str) {
        let Some(client) = &mut self.client else {
            return;
        };
        let mut msg = line.to_string();
        if !msg.ends_with('\n') {
            msg.push('\n');
        }
        let _ = client.write_all(msg.as_bytes());
    }

    /// Manda un evento al client (port di `emitEvent`).
    pub fn emit_event(&mut self, event: &str) {
        self.send_line(&format!("EVT {event}"));
    }

    fn close_client(&mut self) {
        self.client = None;
        self.rx.clear();
        self.session.clear_auth();
    }
}

fn take_lines(rx: &mut Vec<u8>) -> Vec<String> {
    let mut lines = Vec::new();
    while let Some(pos) = rx.iter().position(|&b| b == b'\n') {
        let line: Vec<u8> = rx.drain(..=pos).collect();
        let text = String::from_utf8_lossy(&line[..line.len() - 1]).into_owned();
        lines.push(text);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;
    use std::time::Duration;

    #[derive(Default)]
    struct RecordingHandler {
        seen: Vec<String>,
        reply: String,
    }

    impl ControlHandler for RecordingHandler {
        fn handle_control(&mut self, cmd: &ControlCommand, out: &mut ControlOut) {
            self.seen.push(cmd.verb.clone());
            if self.reply.is_empty() {
                out.reply(format!("ERR unknown {}", cmd.verb));
            } else {
                let reply = self.reply.clone();
                out.reply(reply);
            }
        }
    }

    #[test]
    fn parse_uppercases_verb_and_splits_args() {
        let cmd = parse_control_line("session time_attack 10");
        assert_eq!(cmd.verb, "SESSION");
        assert_eq!(cmd.args, vec!["time_attack", "10"]);
        assert_eq!(cmd.raw, "session time_attack 10");

        let empty = parse_control_line("   ");
        assert_eq!(empty.verb, "");
        assert!(empty.args.is_empty());
    }

    #[test]
    fn session_handles_ping() {
        let mut session = ControlSession::new();
        let replies = session.process_line("ping", &mut NullControlHandler);
        assert_eq!(replies, vec!["PONG\n"]);
    }

    #[test]
    fn session_dispatches_other_verbs_to_handler() {
        let mut session = ControlSession::new();
        let mut handler = RecordingHandler::default();
        let replies = session.process_line("SESSION RACE 5", &mut handler);
        assert_eq!(handler.seen, vec!["SESSION"]);
        assert_eq!(replies, vec!["ERR unknown SESSION\n"]);
    }

    #[test]
    fn token_requires_auth_before_other_verbs() {
        let mut session = ControlSession::new();
        session.set_auth_token("sekret");
        let mut handler = RecordingHandler::default();

        assert_eq!(
            session.process_line("SESSION RACE", &mut handler),
            vec!["ERR UNAUTH - send AUTH <token>\n"]
        );
        assert!(handler.seen.is_empty());

        assert_eq!(
            session.process_line("AUTH wrong", &mut handler),
            vec!["ERR AUTH\n"]
        );
        assert!(!session.is_authenticated());

        assert_eq!(
            session.process_line("AUTH sekret", &mut handler),
            vec!["OK AUTH\n"]
        );
        assert!(session.is_authenticated());

        let replies = session.process_line("SESSION RACE", &mut handler);
        assert_eq!(handler.seen, vec!["SESSION"]);
        assert_eq!(replies, vec!["ERR unknown SESSION\n"]);
    }

    #[test]
    fn empty_lines_are_ignored() {
        let mut session = ControlSession::new();
        assert!(session
            .process_line("  ", &mut NullControlHandler)
            .is_empty());
        assert!(session
            .process_line("\r", &mut NullControlHandler)
            .is_empty());
    }

    #[test]
    fn out_prefixes_events_and_takes_lines() {
        let mut out = ControlOut::default();
        out.reply("OK RESULT");
        out.event("LAP time=92.413000");
        assert_eq!(
            out.lines(),
            &[
                "OK RESULT\n".to_string(),
                "EVT LAP time=92.413000\n".to_string()
            ]
        );
        let taken = out.take();
        assert_eq!(taken.len(), 2);
        assert!(out.lines().is_empty());
    }

    #[test]
    fn greeting_reflects_auth_mode() {
        let mut session = ControlSession::new();
        assert_eq!(session.greeting(), "OK ksim control API v1");
        session.set_auth_token("t");
        assert_eq!(session.greeting(), "OK ksim control API v1 AUTH_REQUIRED");
    }

    #[test]
    fn loopback_ping_without_auth() {
        let mut api = ExternalControlApi::new();
        api.start(0).unwrap();
        let addr = loopback(&api);

        let mut client = TcpStream::connect(addr).unwrap();
        let mut handler = NullControlHandler;

        let mut greeting = String::new();
        for _ in 0..200 {
            api.poll(&mut handler);
            greeting = read_line_maybe(&mut client);
            if !greeting.is_empty() {
                break;
            }
        }
        assert_eq!(greeting, "OK ksim control API v1");

        client.write_all(b"PING\n").unwrap();
        let mut pong = String::new();
        for _ in 0..200 {
            api.poll(&mut handler);
            pong = read_line_maybe(&mut client);
            if !pong.is_empty() {
                break;
            }
        }
        assert_eq!(pong, "PONG");
        assert!(api.has_client());
    }

    #[test]
    fn loopback_auth_flow() {
        let mut api = ExternalControlApi::new();
        api.set_auth_token("sekret");
        api.start(0).unwrap();
        let addr = loopback(&api);

        let mut client = TcpStream::connect(addr).unwrap();
        let mut handler = RecordingHandler {
            reply: "OK SESSION RACE".to_string(),
            ..Default::default()
        };

        let mut greeting = String::new();
        for _ in 0..200 {
            api.poll(&mut handler);
            greeting = read_line_maybe(&mut client);
            if !greeting.is_empty() {
                break;
            }
        }
        assert_eq!(greeting, "OK ksim control API v1 AUTH_REQUIRED");

        client.write_all(b"SESSION RACE\n").unwrap();
        let mut unauth = String::new();
        for _ in 0..200 {
            api.poll(&mut handler);
            unauth = read_line_maybe(&mut client);
            if !unauth.is_empty() {
                break;
            }
        }
        assert!(unauth.starts_with("ERR UNAUTH"));

        client.write_all(b"AUTH sekret\n").unwrap();
        let mut ok = String::new();
        for _ in 0..200 {
            api.poll(&mut handler);
            ok = read_line_maybe(&mut client);
            if !ok.is_empty() {
                break;
            }
        }
        assert_eq!(ok, "OK AUTH");

        client.write_all(b"SESSION RACE 10\n").unwrap();
        let mut reply = String::new();
        for _ in 0..200 {
            api.poll(&mut handler);
            reply = read_line_maybe(&mut client);
            if !reply.is_empty() {
                break;
            }
        }
        assert_eq!(reply, "OK SESSION RACE");
        assert_eq!(handler.seen, vec!["SESSION"]);
    }

    #[test]
    fn only_one_client_is_served_at_a_time() {
        let mut api = ExternalControlApi::new();
        api.start(0).unwrap();
        let addr = loopback(&api);

        let _first = TcpStream::connect(addr).unwrap();
        let _second = TcpStream::connect(addr).unwrap();
        let mut handler = NullControlHandler;
        for _ in 0..100 {
            api.poll(&mut handler);
            if api.has_client() {
                break;
            }
            sleep(Duration::from_millis(5));
        }
        assert!(api.has_client());

        // A second accept must not replace the live client.
        api.accept_if_needed();
        assert!(api.has_client());
    }

    fn loopback(api: &ExternalControlApi) -> SocketAddr {
        SocketAddr::from(([127, 0, 0, 1], api.local_addr().unwrap().port()))
    }

    fn read_line_maybe(stream: &mut TcpStream) -> String {
        stream.set_nonblocking(true).unwrap();
        let mut line = Vec::new();
        let mut byte = [0u8; 1];
        loop {
            match stream.read(&mut byte) {
                Ok(0) => break,
                Ok(_) => {
                    if byte[0] == b'\n' {
                        break;
                    }
                    line.push(byte[0]);
                }
                Err(_) => break,
            }
        }
        String::from_utf8_lossy(&line)
            .trim_end_matches('\r')
            .to_string()
    }
}
