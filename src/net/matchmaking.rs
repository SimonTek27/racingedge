//! Matchmaking: scoperta LAN + registro lobby HTTP opzionale — port di
//! ksengine `Matchmaking.h`.
//!
//! La parte LAN riusa [`ServerDiscovery`]; la parte cloud è **soft-fail**: se
//! `lobby_base_url` è vuoto o l'HTTP non è disponibile, l'elenco LAN continua a
//! funzionare. Il registro espone `POST /servers` (registrazione host) e
//! `GET /servers` (elenco JSON).
//!
//! ```
//! use racingedge::net::matchmaking::{Matchmaking, MatchmakingConfig};
//!
//! let mut mm = Matchmaking::new(MatchmakingConfig::default());
//! let rows = mm.browser_rows();
//! assert_eq!(rows, vec!["No servers (LAN query / lobby)".to_string()]);
//! ```

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use super::discovery::{ServerDiscovery, ServerListEntry};

/// Configurazione del matchmaking (port di `MatchmakingConfig`).
#[derive(Clone, Debug, PartialEq)]
pub struct MatchmakingConfig {
    /// URL base del registro lobby, es. `"http://lobby.example.com/api/v1"`.
    pub lobby_base_url: String,
    /// Intervallo fra due query LAN (s).
    pub lan_refresh_sec: f64,
    /// Intervallo fra due query alla lobby (s).
    pub lobby_refresh_sec: f64,
    /// `true` per annunciarsi come host.
    pub announce_as_host: bool,
}

impl Default for MatchmakingConfig {
    fn default() -> Self {
        Self {
            lobby_base_url: String::new(),
            lan_refresh_sec: 2.0,
            lobby_refresh_sec: 10.0,
            announce_as_host: false,
        }
    }
}

/// Matchmaking LAN + lobby (port di `Matchmaking`).
#[derive(Debug, Default)]
pub struct Matchmaking {
    config: MatchmakingConfig,
    discovery: ServerDiscovery,
    host: ServerListEntry,
    servers: Vec<ServerListEntry>,
    lobby_cache: Vec<ServerListEntry>,
    running: bool,
    auth_required: bool,
    last_lan_refresh: f64,
    last_lobby_refresh: f64,
    last_lobby_register: f64,
}

impl Matchmaking {
    /// Crea il matchmaking con la configurazione data.
    pub fn new(config: MatchmakingConfig) -> Self {
        Self {
            host: ServerListEntry {
                max_players: 16,
                ..Default::default()
            },
            config,
            ..Default::default()
        }
    }

    /// Sostituisce la configurazione.
    pub fn set_config(&mut self, config: MatchmakingConfig) {
        self.config = config;
    }

    /// Configurazione corrente.
    pub fn config(&self) -> &MatchmakingConfig {
        &self.config
    }

    /// Scoperta LAN sottostante.
    pub fn discovery(&self) -> &ServerDiscovery {
        &self.discovery
    }

    /// Scoperta LAN sottostante (mutabile).
    pub fn discovery_mut(&mut self) -> &mut ServerDiscovery {
        &mut self.discovery
    }

    /// Avvia la scoperta LAN (port di `start`).
    pub fn start(&mut self) -> bool {
        if self.discovery.start(self.config.announce_as_host).is_err() {
            return false;
        }
        self.running = true;
        true
    }

    /// Ferma la scoperta e svuota l'elenco.
    pub fn stop(&mut self) {
        self.running = false;
        self.discovery.stop();
        self.servers.clear();
    }

    /// `true` quando è attivo.
    pub fn is_running(&self) -> bool {
        self.running
    }

    /// Informazioni host annunciate (port di `setHostInfo`).
    pub fn set_host_info(
        &mut self,
        name: &str,
        track: &str,
        port: u16,
        players: i32,
        max_players: i32,
        session_type: u8,
    ) {
        self.host = ServerListEntry {
            name: name.to_string(),
            track: track.to_string(),
            port,
            players,
            max_players,
            session_type,
            ..Default::default()
        };
        self.discovery
            .set_host_info(name, track, port, session_type, players, max_players);
    }

    /// Richiede o meno l'autenticazione nel browser.
    pub fn set_auth_required(&mut self, value: bool) {
        self.auth_required = value;
    }

    /// `true` quando l'autenticazione è richiesta.
    pub fn auth_required(&self) -> bool {
        self.auth_required
    }

    /// Un passo: annunci/query LAN ed eventuali query/registrazioni lobby
    /// (port di `update`). `now` è il tempo locale monotono in secondi.
    pub fn update(&mut self, _dt: f64, now: f64) {
        if !self.running {
            return;
        }
        self.discovery.tick(now);

        if self.config.announce_as_host
            && !self.config.lobby_base_url.is_empty()
            && now - self.last_lobby_register >= 5.0
        {
            self.last_lobby_register = now;
            self.register_with_lobby();
        }

        if now - self.last_lan_refresh >= self.config.lan_refresh_sec {
            self.last_lan_refresh = now;
            self.discovery.query_lan();
            self.merge_lan_list();
        }

        if !self.config.lobby_base_url.is_empty()
            && now - self.last_lobby_refresh >= self.config.lobby_refresh_sec
        {
            self.last_lobby_refresh = now;
            self.query_lobby();
        }
    }

    /// Elenco dei server (LAN + lobby).
    pub fn servers(&self) -> &[ServerListEntry] {
        &self.servers
    }

    /// Istantanea dell'ultima query LAN (per i test).
    pub fn last_lan_refresh(&self) -> f64 {
        self.last_lan_refresh
    }

    /// Righe del browser server (port di `browserRows`).
    pub fn browser_rows(&self) -> Vec<String> {
        let mut rows: Vec<String> = self
            .servers
            .iter()
            .map(|e| {
                let auth = if self.auth_required { " [auth]" } else { "" };
                format!(
                    "{} | {} | {}/{} | {}:{}{}",
                    e.name, e.track, e.players, e.max_players, e.host, e.port, auth
                )
            })
            .collect();
        if rows.is_empty() {
            rows.push("No servers (LAN query / lobby)".to_string());
        }
        rows
    }

    /// Unisce l'elenco LAN con la cache della lobby (port di `mergeLanList`).
    pub fn merge_lan_list(&mut self) {
        let mut merged = self.discovery.servers().to_vec();
        for server in &self.lobby_cache {
            let known = merged
                .iter()
                .any(|m| m.host == server.host && m.port == server.port);
            if !known {
                merged.push(server.clone());
            }
        }
        self.servers = merged;
    }

    fn register_with_lobby(&self) {
        if self.config.lobby_base_url.is_empty() {
            return;
        }
        let json = format!(
            "{{\"name\":\"{}\",\"track\":\"{}\",\"port\":{},\"players\":{},\"maxPlayers\":{},\"sessionType\":{},\"auth\":{}}}",
            escape_json(&self.host.name),
            escape_json(&self.host.track),
            self.host.port,
            self.host.players,
            self.host.max_players,
            self.host.session_type,
            self.auth_required
        );
        let url = format!("{}/servers", self.config.lobby_base_url);
        let _ = http_request(&url, "POST", &json);
    }

    fn query_lobby(&mut self) {
        if self.config.lobby_base_url.is_empty() {
            return;
        }
        let url = format!("{}/servers", self.config.lobby_base_url);
        let Some(response) = http_request(&url, "GET", "") else {
            return;
        };
        let body = response
            .split_once("\r\n\r\n")
            .map_or(response.as_str(), |(_, b)| b);
        self.parse_lobby(body);
        self.merge_lan_list();
    }

    /// Interpreta il corpo JSON dell'elenco lobby (port di `parseLobbyJson`).
    pub fn parse_lobby(&mut self, body: &str) {
        self.lobby_cache.clear();
        let mut rest = body;
        while let Some(start) = rest.find('{') {
            let Some(end) = rest[start..].find('}') else {
                break;
            };
            let obj = &rest[start..start + end + 1];
            let host = extract_json_string(obj, "host");
            let name = extract_json_string(obj, "name");
            if !host.is_empty() || !name.is_empty() {
                self.lobby_cache.push(ServerListEntry {
                    name,
                    host: if host.is_empty() {
                        "0.0.0.0".to_string()
                    } else {
                        host
                    },
                    track: extract_json_string(obj, "track"),
                    port: extract_json_int(obj, "port", 40000) as u16,
                    players: extract_json_int(obj, "players", 0),
                    max_players: extract_json_int(obj, "maxPlayers", 16),
                    session_type: extract_json_int(obj, "sessionType", 0) as u8,
                    last_seen_sec: 0.0,
                });
            }
            rest = &rest[start + end + 1..];
        }
    }
}

/// Estrae `host`, `port` e `path` da un URL `http://` (port di `httpRequest`).
pub fn parse_http_url(url: &str) -> Option<(String, u16, String)> {
    let rest = url.strip_prefix("http://")?;
    let (host_port, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let (host, port) = match host_port.rsplit_once(':') {
        Some((h, p)) => (h, p.parse::<u16>().ok()?),
        None => (host_port, 80),
    };
    if host.is_empty() {
        return None;
    }
    Some((host.to_string(), port, path.to_string()))
}

/// Esegue una richiesta HTTP/1.0 bloccante; ritorna la risposta completa
/// (header + corpo) o `None` (port di `httpRequest`).
pub fn http_request(url: &str, method: &str, body: &str) -> Option<String> {
    let (host, port, path) = parse_http_url(url)?;
    let addr = (host.as_str(), port).to_socket_addrs().ok()?.next()?;
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(3)).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(3))).ok()?;
    stream
        .set_write_timeout(Some(Duration::from_secs(3)))
        .ok()?;

    let mut request = format!(
        "{method} {path} HTTP/1.0\r\nHost: {host}\r\nUser-Agent: racingedge-matchmaking/1.0\r\nConnection: close\r\n"
    );
    if !body.is_empty() {
        request.push_str(&format!(
            "Content-Type: application/json\r\nContent-Length: {}\r\n",
            body.len()
        ));
    }
    request.push_str("\r\n");
    if !body.is_empty() {
        request.push_str(body);
    }

    stream.write_all(request.as_bytes()).ok()?;
    let mut response = String::new();
    let _ = stream.take(256 * 1024).read_to_string(&mut response);
    if response.is_empty() {
        None
    } else {
        Some(response)
    }
}

/// Sfugge `"` e `\` per un valore JSON.
pub fn escape_json(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c == '"' || c == '\\' {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Estrae il valore stringa di `key` da un oggetto JSON.
pub fn extract_json_string(obj: &str, key: &str) -> String {
    let pattern = format!("\"{key}\"");
    let Some(p) = obj.find(&pattern) else {
        return String::new();
    };
    let Some(colon) = obj[p..].find(':') else {
        return String::new();
    };
    let rest = &obj[p + colon..];
    let Some(open) = rest.find('"') else {
        return String::new();
    };
    let Some(close) = rest[open + 1..].find('"') else {
        return String::new();
    };
    rest[open + 1..open + 1 + close].to_string()
}

/// Estrae il valore intero di `key` da un oggetto JSON, o `default`.
pub fn extract_json_int(obj: &str, key: &str, default: i32) -> i32 {
    let pattern = format!("\"{key}\"");
    let Some(p) = obj.find(&pattern) else {
        return default;
    };
    let Some(colon) = obj[p..].find(':') else {
        return default;
    };
    let rest = obj[p + colon + 1..].trim_start();
    let digits: String = rest
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '-')
        .collect();
    digits.parse().unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::discovery::DiscoveryPacket;

    fn entry(name: &str, host: &str, port: u16) -> ServerListEntry {
        ServerListEntry {
            name: name.to_string(),
            host: host.to_string(),
            track: "ks_monza".to_string(),
            port,
            players: 2,
            max_players: 16,
            ..Default::default()
        }
    }

    #[test]
    fn config_defaults() {
        let cfg = MatchmakingConfig::default();
        assert_eq!(cfg.lan_refresh_sec, 2.0);
        assert_eq!(cfg.lobby_refresh_sec, 10.0);
        assert!(!cfg.announce_as_host);
        assert!(cfg.lobby_base_url.is_empty());
    }

    #[test]
    fn parse_http_url_variants() {
        assert_eq!(
            parse_http_url("http://lobby.example.com/api/v1"),
            Some(("lobby.example.com".to_string(), 80, "/api/v1".to_string()))
        );
        assert_eq!(
            parse_http_url("http://host:8080/servers"),
            Some(("host".to_string(), 8080, "/servers".to_string()))
        );
        assert_eq!(
            parse_http_url("http://host"),
            Some(("host".to_string(), 80, "/".to_string()))
        );
        assert!(parse_http_url("https://host/").is_none());
        assert!(parse_http_url("http:///path").is_none());
    }

    #[test]
    fn escape_and_extract_json() {
        assert_eq!(escape_json(r#"a"b\c"#), r#"a\"b\\c"#);
        let obj = r#"{"name":"S","host":"10.0.0.1","port":7777,"maxPlayers":12,"players":3}"#;
        assert_eq!(extract_json_string(obj, "name"), "S");
        assert_eq!(extract_json_string(obj, "host"), "10.0.0.1");
        assert_eq!(extract_json_int(obj, "port", 0), 7777);
        assert_eq!(extract_json_int(obj, "maxPlayers", 16), 12);
        assert_eq!(extract_json_int(obj, "missing", 9), 9);
    }

    #[test]
    fn parse_lobby_reads_array_and_merges() {
        let body = r#"[{"name":"A","host":"10.0.0.1","track":"ks_monza","port":7777,"players":4,"maxPlayers":16,"sessionType":2},{"name":"B"}]"#;
        let mut mm = Matchmaking::new(MatchmakingConfig::default());
        mm.parse_lobby(body);
        mm.merge_lan_list();
        assert_eq!(mm.servers().len(), 2);
        assert_eq!(mm.servers()[0].host, "10.0.0.1");
        // Entry without host falls back to 0.0.0.0.
        assert_eq!(mm.servers()[1].host, "0.0.0.0");
    }

    #[test]
    fn merge_prefers_lan_and_dedups_by_host_port() {
        let mut mm = Matchmaking::new(MatchmakingConfig::default());
        mm.parse_lobby(r#"[{"name":"Lobby","host":"10.0.0.1","port":7777}]"#);
        // LAN entry for the same host:port must win over the lobby copy.
        inject_lan(&mut mm, &entry("Lan", "10.0.0.1", 7777));
        mm.merge_lan_list();
        assert_eq!(mm.servers().len(), 1);
        assert_eq!(mm.servers()[0].name, "Lan");
    }

    #[test]
    fn browser_rows_show_placeholder_and_auth() {
        let mut mm = Matchmaking::new(MatchmakingConfig::default());
        assert_eq!(
            mm.browser_rows(),
            vec!["No servers (LAN query / lobby)".to_string()]
        );

        inject_lan(&mut mm, &entry("A", "10.0.0.1", 7777));
        mm.merge_lan_list();
        mm.set_auth_required(true);
        let rows = mm.browser_rows();
        assert_eq!(rows.len(), 1);
        assert!(rows[0].starts_with("A | ks_monza | 2/16 | 10.0.0.1:7777"));
        assert!(rows[0].ends_with("[auth]"));
    }

    #[test]
    fn update_is_noop_before_start() {
        let mut mm = Matchmaking::new(MatchmakingConfig::default());
        mm.update(0.016, 100.0);
        assert_eq!(mm.last_lan_refresh(), 0.0);
        assert!(!mm.is_running());
    }

    #[test]
    fn update_refreshes_lan_at_interval() {
        let mut mm = Matchmaking::new(MatchmakingConfig::default());
        mm.discovery_mut().set_port(0);
        assert!(mm.start());
        mm.update(0.016, 1.0);
        assert_eq!(mm.last_lan_refresh(), 0.0);
        mm.update(0.016, 2.5);
        assert_eq!(mm.last_lan_refresh(), 2.5);
        mm.stop();
        assert!(!mm.is_running());
    }

    #[test]
    fn set_host_info_feeds_discovery() {
        let mut mm = Matchmaking::new(MatchmakingConfig {
            announce_as_host: true,
            ..Default::default()
        });
        mm.set_host_info("My Host", "ks_monza", 7777, 1, 12, 2);
        assert!(mm.discovery().host_mode());
        let announce = mm.discovery().announce();
        assert_eq!(announce.name, "My Host");
        assert_eq!(announce.max_players, 12);
        assert_eq!(announce.port, 7777);
    }

    #[test]
    fn discovery_magic_is_shared() {
        let pkt = DiscoveryPacket::new(crate::net::discovery::DiscoveryMsg::Query);
        assert_eq!(pkt.magic, crate::net::discovery::DISCOVERY_MAGIC);
    }

    fn inject_lan(mm: &mut Matchmaking, entry: &ServerListEntry) {
        let mut p = DiscoveryPacket::new(crate::net::discovery::DiscoveryMsg::Announce);
        p.name = entry.name.clone();
        p.host = entry.host.clone();
        p.track = entry.track.clone();
        p.port = entry.port;
        p.players = entry.players.max(0) as u16;
        p.max_players = entry.max_players.max(0) as u16;
        p.session_type = entry.session_type;
        mm.discovery_mut()
            .handle_packet(&p, "127.0.0.1:1".parse().unwrap(), 0.0);
    }
}
