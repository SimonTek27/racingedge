//! Scoperta dei server in LAN via broadcast UDP — port di ksengine
//! `ServerDiscovery.h`.
//!
//! Il pacchetto [`DiscoveryPacket`] (magic `0x4B534449`, porta di default
//! `:20779`) è **fixed-layout little-endian**: `magic: u32`, `msg: u8`,
//! `session_type: u8`, `port/players/max_players: u16`, poi tre stringhe a
//! lunghezza fissa (`name[48]`, `track[48]`, `host[32]`), per un totale di 140
//! byte.
//!
//! Un host si annuncia in broadcast ogni 2 s e risponde alle `Query`; i client
//! mandano una `Query` e raccolgono [`ServerListEntry`], scartando quelle non
//! viste da più di 8 s.
//!
//! ```
//! use racingedge::net::discovery::{DiscoveryMsg, DiscoveryPacket};
//!
//! let mut packet = DiscoveryPacket::new(DiscoveryMsg::Announce);
//! packet.name = "My Server".to_string();
//! packet.port = 7777;
//! let decoded = DiscoveryPacket::decode(&packet.encode()).unwrap();
//! assert_eq!(decoded.name, "My Server");
//! assert_eq!(decoded.port, 7777);
//! ```

use std::io;
use std::net::{SocketAddr, UdpSocket};

/// Magic di [`DiscoveryPacket`] (byte big-endian `"KSDI"`).
pub const DISCOVERY_MAGIC: u32 = 0x4B53_4449;
/// Porta UDP di default della scoperta server.
pub const DISCOVERY_PORT: u16 = 20779;
/// Byte del campo `name`.
pub const NAME_LEN: usize = 48;
/// Byte del campo `track`.
pub const TRACK_LEN: usize = 48;
/// Byte del campo `host`.
pub const HOST_LEN: usize = 32;
/// Byte di un pacchetto completo.
pub const DISCOVERY_PACKET_SIZE: usize = 4 + 1 + 1 + 2 + 2 + 2 + NAME_LEN + TRACK_LEN + HOST_LEN;
/// Intervallo fra due annunci dell'host (s).
pub const ANNOUNCE_INTERVAL: f64 = 2.0;
/// Dopo questo tempo senza annunci un server sparisce (s).
pub const SERVER_TIMEOUT: f64 = 8.0;

/// Tipo di messaggio di scoperta (port di `DiscoveryMsg`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DiscoveryMsg {
    #[default]
    Query = 1,
    Announce = 2,
    Reply = 3,
}

impl DiscoveryMsg {
    fn from_u8(v: u8) -> Option<Self> {
        match v {
            1 => Some(Self::Query),
            2 => Some(Self::Announce),
            3 => Some(Self::Reply),
            _ => None,
        }
    }
}

/// Un pacchetto di scoperta (port di `DiscoveryPacket`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DiscoveryPacket {
    pub magic: u32,
    pub msg: u8,
    pub session_type: u8,
    pub port: u16,
    pub players: u16,
    pub max_players: u16,
    pub name: String,
    pub track: String,
    pub host: String,
}

impl DiscoveryPacket {
    /// Crea un pacchetto col magic corretto e il messaggio indicato.
    pub fn new(msg: DiscoveryMsg) -> Self {
        Self {
            magic: DISCOVERY_MAGIC,
            msg: msg as u8,
            ..Default::default()
        }
    }

    /// Messaggio decodificato, se riconosciuto.
    pub fn message(&self) -> Option<DiscoveryMsg> {
        DiscoveryMsg::from_u8(self.msg)
    }

    /// Serializza il pacchetto in [`DISCOVERY_PACKET_SIZE`] byte.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(DISCOVERY_PACKET_SIZE);
        out.extend_from_slice(&self.magic.to_le_bytes());
        out.push(self.msg);
        out.push(self.session_type);
        out.extend_from_slice(&self.port.to_le_bytes());
        out.extend_from_slice(&self.players.to_le_bytes());
        out.extend_from_slice(&self.max_players.to_le_bytes());
        put_fixed(&mut out, &self.name, NAME_LEN);
        put_fixed(&mut out, &self.track, TRACK_LEN);
        put_fixed(&mut out, &self.host, HOST_LEN);
        debug_assert_eq!(out.len(), DISCOVERY_PACKET_SIZE);
        out
    }

    /// Interpreta un pacchetto prodotto da [`DiscoveryPacket::encode`].
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let mut c = Cursor::new(bytes);
        let magic = c.u32()?;
        if magic != DISCOVERY_MAGIC {
            return Err(format!("magic inatteso: 0x{magic:08X}"));
        }
        Ok(Self {
            magic,
            msg: c.u8()?,
            session_type: c.u8()?,
            port: c.u16()?,
            players: c.u16()?,
            max_players: c.u16()?,
            name: c.fixed(NAME_LEN)?,
            track: c.fixed(TRACK_LEN)?,
            host: c.fixed(HOST_LEN)?,
        })
    }
}

/// Una voce dell'elenco server (port di `ServerListEntry`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ServerListEntry {
    pub name: String,
    pub host: String,
    pub track: String,
    pub port: u16,
    pub session_type: u8,
    pub players: i32,
    pub max_players: i32,
    pub last_seen_sec: f64,
}

/// Scoperta server LAN (port di `ServerDiscovery`).
#[derive(Debug, Default)]
pub struct ServerDiscovery {
    socket: Option<UdpSocket>,
    port: u16,
    host_mode: bool,
    host_name: String,
    host_track: String,
    host_port: u16,
    host_session: u8,
    host_players: i32,
    host_max: i32,
    servers: Vec<ServerListEntry>,
    last_announce: f64,
    started: bool,
}

impl ServerDiscovery {
    /// Crea una scoperta non avviata sulla porta di default.
    pub fn new() -> Self {
        Self {
            port: DISCOVERY_PORT,
            host_port: 40000,
            host_max: 16,
            last_announce: f64::NEG_INFINITY,
            ..Default::default()
        }
    }

    /// Imposta la porta (da chiamare prima di [`Self::start`]).
    pub fn set_port(&mut self, port: u16) {
        self.port = port;
    }

    /// Porta configurata.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Avvia il socket in ascolto (port di `start`).
    pub fn start(&mut self, announce_as_host: bool) -> io::Result<()> {
        if self.socket.is_some() {
            return Ok(());
        }
        let socket = UdpSocket::bind(SocketAddr::from(([0, 0, 0, 0], self.port)))?;
        socket.set_nonblocking(true)?;
        socket.set_broadcast(true)?;
        self.socket = Some(socket);
        self.host_mode = announce_as_host;
        self.started = true;
        Ok(())
    }

    /// Chiude il socket.
    pub fn stop(&mut self) {
        self.socket = None;
        self.started = false;
    }

    /// `true` dopo un [`Self::start`] riuscito.
    pub fn is_running(&self) -> bool {
        self.started
    }

    /// `true` quando la scoperta si annuncia come host.
    pub fn host_mode(&self) -> bool {
        self.host_mode
    }

    /// Costruisce il pacchetto di annuncio corrente.
    pub fn announce(&self) -> DiscoveryPacket {
        self.make_announce()
    }

    /// Indirizzo locale, se il socket è aperto.
    pub fn local_addr(&self) -> Option<SocketAddr> {
        self.socket.as_ref().and_then(|s| s.local_addr().ok())
    }

    /// Informazioni annunciate dall'host (port di `setHostInfo`).
    pub fn set_host_info(
        &mut self,
        name: &str,
        track: &str,
        port: u16,
        session_type: u8,
        players: i32,
        max_players: i32,
    ) {
        self.host_name = name.to_string();
        self.host_track = track.to_string();
        self.host_port = port;
        self.host_session = session_type;
        self.host_players = players;
        self.host_max = max_players;
        self.host_mode = true;
    }

    /// Manda subito una `Query` in broadcast (port di `queryLan`).
    pub fn query_lan(&self) {
        self.send_broadcast(&DiscoveryPacket::new(DiscoveryMsg::Query));
    }

    /// Un passo: annuncia se host, raccoglie i pacchetti e scade i server
    /// vecchi (port di `tick`). `now` è il tempo locale monotono in secondi.
    pub fn tick(&mut self, now: f64) {
        if self.socket.is_none() {
            self.expire(now);
            return;
        }
        if self.host_mode && now - self.last_announce > ANNOUNCE_INTERVAL {
            self.last_announce = now;
            self.send_broadcast(&self.make_announce());
        }

        let mut received: Vec<(Vec<u8>, SocketAddr)> = Vec::new();
        if let Some(socket) = &self.socket {
            let mut buf = [0u8; DISCOVERY_PACKET_SIZE];
            loop {
                match socket.recv_from(&mut buf) {
                    Ok((n, from)) => received.push((buf[..n].to_vec(), from)),
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                    Err(_) => break,
                }
            }
        }
        for (bytes, from) in received {
            if let Ok(packet) = DiscoveryPacket::decode(&bytes) {
                self.handle_packet(&packet, from, now);
            }
        }
        self.expire(now);
    }

    /// Gestisce un pacchetto già decodificato (port del corpo di `tick`).
    pub fn handle_packet(&mut self, packet: &DiscoveryPacket, from: SocketAddr, now: f64) {
        match packet.message() {
            Some(DiscoveryMsg::Query) => {
                if self.host_mode {
                    let announce = self.make_announce();
                    if let Some(socket) = &self.socket {
                        let _ = socket.send_to(&announce.encode(), from);
                    }
                }
            }
            Some(DiscoveryMsg::Announce | DiscoveryMsg::Reply) => {
                let host = if packet.host.is_empty() {
                    from.ip().to_string()
                } else {
                    packet.host.clone()
                };
                self.upsert(ServerListEntry {
                    name: packet.name.clone(),
                    host,
                    track: packet.track.clone(),
                    port: packet.port,
                    session_type: packet.session_type,
                    players: packet.players as i32,
                    max_players: packet.max_players as i32,
                    last_seen_sec: now,
                });
            }
            None => {}
        }
    }

    /// Elenco dei server visti di recente.
    pub fn servers(&self) -> &[ServerListEntry] {
        &self.servers
    }

    fn make_announce(&self) -> DiscoveryPacket {
        let mut p = DiscoveryPacket::new(DiscoveryMsg::Announce);
        p.session_type = self.host_session;
        p.port = self.host_port;
        p.players = self.host_players.max(0) as u16;
        p.max_players = self.host_max.max(0) as u16;
        p.name = self.host_name.clone();
        p.track = self.host_track.clone();
        p
    }

    fn send_broadcast(&self, packet: &DiscoveryPacket) {
        if let Some(socket) = &self.socket {
            let bcast = SocketAddr::from(([255, 255, 255, 255], self.port));
            let _ = socket.send_to(&packet.encode(), bcast);
        }
    }

    fn upsert(&mut self, entry: ServerListEntry) {
        if let Some(existing) = self
            .servers
            .iter_mut()
            .find(|s| s.host == entry.host && s.port == entry.port)
        {
            *existing = entry;
        } else {
            self.servers.push(entry);
        }
    }

    fn expire(&mut self, now: f64) {
        self.servers
            .retain(|e| now - e.last_seen_sec <= SERVER_TIMEOUT);
    }
}

fn put_fixed(out: &mut Vec<u8>, s: &str, len: usize) {
    let bytes = s.as_bytes();
    let n = bytes.len().min(len);
    out.extend_from_slice(&bytes[..n]);
    out.resize(out.len() + (len - n), 0);
}

/// Lettore sequenziale con controllo dei limiti.
struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }

    fn take<const N: usize>(&mut self) -> Result<[u8; N], String> {
        let end = self.at + N;
        let slice = self
            .bytes
            .get(self.at..end)
            .ok_or_else(|| format!("pacchetto troncato: servono {} byte", N))?;
        self.at = end;
        let mut buf = [0u8; N];
        buf.copy_from_slice(slice);
        Ok(buf)
    }

    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take::<1>()?[0])
    }

    fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_le_bytes(self.take()?))
    }

    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take()?))
    }

    fn fixed(&mut self, len: usize) -> Result<String, String> {
        let end = self.at + len;
        let slice = self
            .bytes
            .get(self.at..end)
            .ok_or_else(|| format!("stringa troncata: servono {len} byte"))?;
        self.at = end;
        let stop = slice.iter().position(|&b| b == 0).unwrap_or(len);
        Ok(String::from_utf8_lossy(&slice[..stop]).into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;
    use std::time::Duration;

    fn announce(name: &str, host: &str, port: u16) -> DiscoveryPacket {
        let mut p = DiscoveryPacket::new(DiscoveryMsg::Announce);
        p.name = name.to_string();
        p.track = "ks_barcelona".to_string();
        p.host = host.to_string();
        p.port = port;
        p.players = 3;
        p.max_players = 16;
        p.session_type = 2;
        p
    }

    fn from(port: u16) -> SocketAddr {
        SocketAddr::from(([127, 0, 0, 1], port))
    }

    #[test]
    fn packet_is_140_bytes() {
        assert_eq!(DISCOVERY_PACKET_SIZE, 140);
        assert_eq!(
            DiscoveryPacket::new(DiscoveryMsg::Query).encode().len(),
            140
        );
    }

    #[test]
    fn packet_round_trips_including_padding() {
        let packet = announce("Server", "10.0.0.5", 7777);
        let bytes = packet.encode();
        assert_eq!(bytes.len(), DISCOVERY_PACKET_SIZE);
        assert_eq!(DiscoveryPacket::decode(&bytes).unwrap(), packet);
    }

    #[test]
    fn packet_header_carries_magic_and_message() {
        let bytes = DiscoveryPacket::new(DiscoveryMsg::Query).encode();
        assert_eq!(&bytes[0..4], &DISCOVERY_MAGIC.to_le_bytes());
        assert_eq!(DISCOVERY_MAGIC.to_be_bytes(), *b"KSDI");
        assert_eq!(bytes[4], DiscoveryMsg::Query as u8);
    }

    #[test]
    fn packet_rejects_bad_magic_and_truncation() {
        let mut bytes = announce("S", "h", 1).encode();
        bytes[0] = 0;
        assert!(DiscoveryPacket::decode(&bytes).is_err());
        let bytes = announce("S", "h", 1).encode();
        assert!(DiscoveryPacket::decode(&bytes[..DISCOVERY_PACKET_SIZE - 1]).is_err());
        assert!(DiscoveryPacket::decode(&[]).is_err());
    }

    #[test]
    fn long_strings_are_truncated_on_wire() {
        let long = "x".repeat(100);
        let mut p = DiscoveryPacket::new(DiscoveryMsg::Announce);
        p.name = long.clone();
        let decoded = DiscoveryPacket::decode(&p.encode()).unwrap();
        assert_eq!(decoded.name.len(), NAME_LEN);
    }

    #[test]
    fn announce_is_ingested_and_upserted() {
        let mut disc = ServerDiscovery::new();
        disc.handle_packet(&announce("A", "10.0.0.1", 7777), from(3000), 0.0);
        disc.handle_packet(&announce("B", "10.0.0.2", 7777), from(3001), 0.0);
        assert_eq!(disc.servers().len(), 2);

        // Same host+port updates in place.
        disc.handle_packet(&announce("A2", "10.0.0.1", 7777), from(3000), 1.0);
        assert_eq!(disc.servers().len(), 2);
        let a = disc
            .servers()
            .iter()
            .find(|s| s.host == "10.0.0.1")
            .unwrap();
        assert_eq!(a.name, "A2");
        assert_eq!(a.last_seen_sec, 1.0);
    }

    #[test]
    fn missing_host_falls_back_to_sender_ip() {
        let mut disc = ServerDiscovery::new();
        disc.handle_packet(&announce("A", "", 7777), from(4000), 0.0);
        assert_eq!(disc.servers()[0].host, "127.0.0.1");
    }

    #[test]
    fn servers_expire_after_timeout() {
        let mut disc = ServerDiscovery::new();
        disc.handle_packet(&announce("A", "10.0.0.1", 7777), from(3000), 0.0);
        disc.tick(5.0);
        assert_eq!(disc.servers().len(), 1);
        disc.tick(9.0);
        assert!(disc.servers().is_empty());
    }

    #[test]
    fn query_is_ignored_when_not_host() {
        let mut disc = ServerDiscovery::new();
        disc.handle_packet(&DiscoveryPacket::new(DiscoveryMsg::Query), from(3000), 0.0);
        assert!(disc.servers().is_empty());
    }

    #[test]
    fn host_replies_to_query_over_udp() {
        let mut host = ServerDiscovery::new();
        host.set_port(0);
        host.start(true).unwrap();
        host.set_host_info("My Host", "ks_monza", 7777, 2, 1, 12);

        let client = UdpSocket::bind("127.0.0.1:0").unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let host_addr = host.local_addr().unwrap();
        let target = SocketAddr::from(([127, 0, 0, 1], host_addr.port()));

        let query = DiscoveryPacket::new(DiscoveryMsg::Query).encode();
        client.send_to(&query, target).unwrap();

        let mut got = None;
        for _ in 0..200 {
            host.tick(0.0);
            let mut buf = [0u8; DISCOVERY_PACKET_SIZE];
            if let Ok((n, _)) = client.recv_from(&mut buf) {
                got = Some(DiscoveryPacket::decode(&buf[..n]).unwrap());
                break;
            }
            sleep(Duration::from_millis(5));
        }
        let reply = got.expect("nessuna risposta dall'host");
        assert_eq!(reply.message(), Some(DiscoveryMsg::Announce));
        assert_eq!(reply.name, "My Host");
        assert_eq!(reply.track, "ks_monza");
        assert_eq!(reply.port, 7777);
        assert_eq!(reply.max_players, 12);
    }
}
