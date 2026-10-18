//! Telemetria in uscita — port di ksengine `UdpTelemetryBridge` /
//! `TcpTelemetryBridge`.
//!
//! Due canali, entrambi opzionali, abilitati da [`TelemetryConfig`]:
//!
//! * **UDP** (default `:20777`) — un datagramma per frame, con header `KSIM` v2.
//! * **TCP** (default `:20778`) — framing `[u32 LE len][payload]`, in broadcast a
//!   tutti i client connessi.
//!
//! The payload is [`TelemetryFrame`]: a **fixed-layout, little-endian** block of
//! 108 bytes (no implicit struct padding — encoding is field by field), preceded by `magic: u32`,
//! `version: u16`, `size: u16`.
//!
//! ```
//! use racingedge::net::telemetry::{TelemetryFrame, PACKET_SIZE};
//!
//! let mut frame = TelemetryFrame {
//!     speed_kmh: 212.5,
//!     gear: 5,
//!     ..Default::default()
//! };
//! frame.set_limiter(true);
//!
//! let packet = frame.encode();
//! assert_eq!(packet.len(), PACKET_SIZE);
//! assert_eq!(TelemetryFrame::decode(&packet).unwrap(), frame);
//! ```

use std::io::{self, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, UdpSocket};

use bevy::prelude::*;

/// Porta UDP di default della telemetria in uscita.
pub const UDP_PORT: u16 = 20777;
/// Porta TCP di default della telemetria in uscita.
pub const TCP_PORT: u16 = 20778;

/// Magic di intestazione: i byte little-endian di questo valore sono `"KSIM"`.
pub const MAGIC: u32 = 0x4D49_534B;
/// Versione del protocollo.
pub const VERSION: u16 = 2;

/// Zone di danno trasmesse (`carDamage[5]`).
pub const DAMAGE_ZONES: usize = 5;
/// Ruote trasmesse (anteriore sinistra → posteriore destra).
pub const WHEELS: usize = 4;

/// Byte di `magic` + `version` + `size`.
pub const HEADER_SIZE: usize = 8;
/// Byte del payload, senza header.
pub const PAYLOAD_SIZE: usize = 108;
/// Byte di un pacchetto completo.
pub const PACKET_SIZE: usize = HEADER_SIZE + PAYLOAD_SIZE;

/// Bit di [`TelemetryFrame::flags`]: limitatore attivo.
pub const FLAG_LIMITER: u8 = 1 << 0;
/// Bit di [`TelemetryFrame::flags`]: marcia in retromarcia.
pub const FLAG_REVERSE: u8 = 1 << 1;

/// Un frame di telemetria.
///
/// È anche una [`Resource`] Bevy: la simulazione lo aggiorna e
/// [`TelemetryPlugin`] lo spedisce.
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct TelemetryFrame {
    /// Tempo di simulazione in secondi.
    pub sim_time: f64,
    /// Velocità in km/h.
    pub speed_kmh: f32,
    /// Regime motore in giri/min.
    pub rpm: f32,
    /// Marcia inserita (`-1` = retromarcia, `0` = folle).
    pub gear: i8,
    /// Bit di stato, vedi [`FLAG_LIMITER`] e [`FLAG_REVERSE`].
    pub flags: u8,
    /// Posizione acceleratore `0..=1`.
    pub throttle: f32,
    /// Posizione freno `0..=1`.
    pub brake: f32,
    /// Sterzo `-1..=1`.
    pub steer: f32,
    /// Posizione nel mondo `[x, y, z]`.
    pub position: [f32; 3],
    /// Imbardata in radianti.
    pub yaw: f32,
    /// Numero di giro.
    pub lap: u16,
    /// Tempo sul giro in millisecondi.
    pub lap_time_ms: u32,
    /// Carburante residuo in litri.
    pub fuel: f32,
    /// Danno per zona `0..=1`.
    pub damage: [f32; DAMAGE_ZONES],
    /// Temperatura per ruota in gradi.
    pub tyre_temp: [f32; WHEELS],
    /// Usura per ruota `0..=1`.
    pub tyre_wear: [f32; WHEELS],
}

impl Default for TelemetryFrame {
    fn default() -> Self {
        Self {
            sim_time: 0.0,
            speed_kmh: 0.0,
            rpm: 0.0,
            gear: -1,
            flags: 0,
            throttle: 0.0,
            brake: 0.0,
            steer: 0.0,
            position: [0.0; 3],
            yaw: 0.0,
            lap: 0,
            lap_time_ms: 0,
            fuel: 0.0,
            damage: [0.0; DAMAGE_ZONES],
            tyre_temp: [0.0; WHEELS],
            tyre_wear: [0.0; WHEELS],
        }
    }
}

impl TelemetryFrame {
    /// `true` quando il limitatore è attivo.
    pub fn limiter(&self) -> bool {
        self.flags & FLAG_LIMITER != 0
    }

    /// Attiva/disattiva il bit del limitatore.
    pub fn set_limiter(&mut self, on: bool) {
        self.set_flag(FLAG_LIMITER, on);
    }

    /// `true` quando la marcia è la retromarcia.
    pub fn reverse(&self) -> bool {
        self.flags & FLAG_REVERSE != 0
    }

    /// Attiva/disattiva il bit della retromarcia.
    pub fn set_reverse(&mut self, on: bool) {
        self.set_flag(FLAG_REVERSE, on);
    }

    fn set_flag(&mut self, bit: u8, on: bool) {
        if on {
            self.flags |= bit;
        } else {
            self.flags &= !bit;
        }
    }

    /// Serializza il frame in un pacchetto di [`PACKET_SIZE`] byte.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(PACKET_SIZE);
        out.extend_from_slice(&MAGIC.to_le_bytes());
        out.extend_from_slice(&VERSION.to_le_bytes());
        out.extend_from_slice(&(PAYLOAD_SIZE as u16).to_le_bytes());
        out.extend_from_slice(&self.sim_time.to_le_bytes());
        out.extend_from_slice(&self.speed_kmh.to_le_bytes());
        out.extend_from_slice(&self.rpm.to_le_bytes());
        out.push(self.gear as u8);
        out.push(self.flags);
        out.extend_from_slice(&self.throttle.to_le_bytes());
        out.extend_from_slice(&self.brake.to_le_bytes());
        out.extend_from_slice(&self.steer.to_le_bytes());
        for v in self.position {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&self.yaw.to_le_bytes());
        out.extend_from_slice(&self.lap.to_le_bytes());
        out.extend_from_slice(&self.lap_time_ms.to_le_bytes());
        out.extend_from_slice(&self.fuel.to_le_bytes());
        for v in self.damage {
            out.extend_from_slice(&v.to_le_bytes());
        }
        for v in self.tyre_temp {
            out.extend_from_slice(&v.to_le_bytes());
        }
        for v in self.tyre_wear {
            out.extend_from_slice(&v.to_le_bytes());
        }
        debug_assert_eq!(out.len(), PACKET_SIZE);
        out
    }

    /// Interpreta un pacchetto prodotto da [`TelemetryFrame::encode`].
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let mut c = Cursor::new(bytes);
        let magic = c.u32()?;
        if magic != MAGIC {
            return Err(format!("magic inatteso: 0x{magic:08X}"));
        }
        let version = c.u16()?;
        if version != VERSION {
            return Err(format!("versione non supportata: {version}"));
        }
        let size = c.u16()? as usize;
        if size != PAYLOAD_SIZE {
            return Err(format!("dimensione payload inattesa: {size}"));
        }

        let mut frame = Self {
            sim_time: c.f64()?,
            speed_kmh: c.f32()?,
            rpm: c.f32()?,
            gear: c.i8()?,
            flags: c.u8()?,
            throttle: c.f32()?,
            brake: c.f32()?,
            steer: c.f32()?,
            position: [c.f32()?, c.f32()?, c.f32()?],
            yaw: c.f32()?,
            lap: c.u16()?,
            lap_time_ms: c.u32()?,
            fuel: c.f32()?,
            damage: [0.0; DAMAGE_ZONES],
            tyre_temp: [0.0; WHEELS],
            tyre_wear: [0.0; WHEELS],
        };
        for slot in &mut frame.damage {
            *slot = c.f32()?;
        }
        for slot in &mut frame.tyre_temp {
            *slot = c.f32()?;
        }
        for slot in &mut frame.tyre_wear {
            *slot = c.f32()?;
        }
        Ok(frame)
    }
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

    fn i8(&mut self) -> Result<i8, String> {
        Ok(self.take::<1>()?[0] as i8)
    }

    fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_le_bytes(self.take()?))
    }

    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take()?))
    }

    fn f32(&mut self) -> Result<f32, String> {
        Ok(f32::from_le_bytes(self.take()?))
    }

    fn f64(&mut self) -> Result<f64, String> {
        Ok(f64::from_le_bytes(self.take()?))
    }
}

/// Costruisce un frame TCP: `[u32 LE len][payload]`.
pub fn tcp_frame(packet: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + packet.len());
    out.extend_from_slice(&(packet.len() as u32).to_le_bytes());
    out.extend_from_slice(packet);
    out
}

/// Estrae tutti i frame completi presenti in `buffer`, rimuovendoli.
///
/// I byte parziali restano in `buffer` per la lettura successiva.
pub fn take_tcp_frames(buffer: &mut Vec<u8>) -> Vec<Vec<u8>> {
    let mut frames = Vec::new();
    loop {
        if buffer.len() < 4 {
            break;
        }
        let len = u32::from_le_bytes([buffer[0], buffer[1], buffer[2], buffer[3]]) as usize;
        if buffer.len() < 4 + len {
            break;
        }
        frames.push(buffer[4..4 + len].to_vec());
        buffer.drain(..4 + len);
    }
    frames
}

/// Mittente UDP best-effort.
#[derive(Debug)]
pub struct UdpTelemetrySender {
    socket: UdpSocket,
    target: SocketAddr,
}

impl UdpTelemetrySender {
    /// Apre il socket locale e fissa il destinatario.
    ///
    /// `broadcast` abilita `SO_BROADCAST` per destinatari come
    /// `255.255.255.255`.
    pub fn new(target: SocketAddr, broadcast: bool) -> io::Result<Self> {
        let bind: SocketAddr = if target.is_ipv4() {
            "0.0.0.0:0".parse().expect("indirizzo valido")
        } else {
            "[::]:0".parse().expect("indirizzo valido")
        };
        let socket = UdpSocket::bind(bind)?;
        socket.set_broadcast(broadcast)?;
        Ok(Self { socket, target })
    }

    /// Destinatario configurato.
    pub fn target(&self) -> SocketAddr {
        self.target
    }

    /// Spedisce un pacchetto già serializzato.
    pub fn send_packet(&self, packet: &[u8]) -> io::Result<usize> {
        self.socket.send_to(packet, self.target)
    }

    /// Serializza e spedisce `frame`.
    pub fn send(&self, frame: &TelemetryFrame) -> io::Result<usize> {
        self.send_packet(&frame.encode())
    }
}

/// Server TCP che manda in broadcast la telemetria ai client connessi.
#[derive(Debug)]
pub struct TcpTelemetryServer {
    listener: TcpListener,
    clients: Vec<TcpStream>,
}

impl TcpTelemetryServer {
    /// Mette in ascolto su `addr` in modo non bloccante.
    pub fn bind(addr: SocketAddr) -> io::Result<Self> {
        let listener = TcpListener::bind(addr)?;
        listener.set_nonblocking(true)?;
        Ok(Self {
            listener,
            clients: Vec::new(),
        })
    }

    /// Indirizzo effettivamente in ascolto (utile con la porta 0).
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    /// Accetta i client in attesa, restituendo quanti ne sono entrati.
    pub fn accept_pending(&mut self) -> usize {
        let mut accepted = 0;
        while let Ok((stream, _)) = self.listener.accept() {
            let _ = stream.set_nodelay(true);
            self.clients.push(stream);
            accepted += 1;
        }
        accepted
    }

    /// Client attualmente connessi.
    pub fn client_count(&self) -> usize {
        self.clients.len()
    }

    /// Manda `packet` a tutti i client, scartando quelli caduti.
    pub fn broadcast_packet(&mut self, packet: &[u8]) -> usize {
        let framed = tcp_frame(packet);
        let mut sent = 0;
        self.clients.retain_mut(|client| {
            if client.write_all(&framed).is_ok() {
                sent += 1;
                true
            } else {
                false
            }
        });
        sent
    }

    /// Serializza e manda `frame` a tutti i client.
    pub fn broadcast(&mut self, frame: &TelemetryFrame) -> usize {
        self.broadcast_packet(&frame.encode())
    }
}

/// Configurazione dei canali di telemetria.
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct TelemetryConfig {
    /// Interruttore generale: a `false` non viene aperto né usato alcun socket.
    pub enabled: bool,
    /// Destinatario UDP, oppure `None` per disattivare il canale.
    pub udp: Option<SocketAddr>,
    /// Abilita l'invio broadcast a livello di socket.
    pub udp_broadcast: bool,
    /// Indirizzo di ascolto TCP, oppure `None` per disattivare il canale.
    pub tcp: Option<SocketAddr>,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            udp: Some(SocketAddr::from(([127, 0, 0, 1], UDP_PORT))),
            udp_broadcast: false,
            tcp: Some(SocketAddr::from(([0, 0, 0, 0], TCP_PORT))),
        }
    }
}

/// Socket aperti da [`TelemetryPlugin`].
#[derive(Resource, Default)]
pub struct TelemetrySockets {
    /// Canale UDP, se attivo e aperto correttamente.
    pub udp: Option<UdpTelemetrySender>,
    /// Canale TCP, se attivo e aperto correttamente.
    pub tcp: Option<TcpTelemetryServer>,
    /// Ultimo errore di apertura, se ce n'è stato uno.
    pub error: Option<String>,
}

/// Plugin che spedisce il [`TelemetryFrame`] corrente a ogni frame.
///
/// Non fa nulla finché [`TelemetryConfig::enabled`] resta `false`, così il
/// binario può aggiungerlo sempre:
///
/// ```ignore
/// app.insert_resource(TelemetryConfig { enabled: true, ..default() })
///     .add_plugins(TelemetryPlugin);
/// ```
pub struct TelemetryPlugin;

impl Plugin for TelemetryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TelemetryConfig>()
            .init_resource::<TelemetryFrame>()
            .init_resource::<TelemetrySockets>()
            .add_systems(Update, (telemetry_connect, telemetry_publish).chain());
    }
}

/// Apre i socket la prima volta che la configurazione è abilitata.
fn telemetry_connect(config: Res<TelemetryConfig>, mut sockets: ResMut<TelemetrySockets>) {
    if !config.enabled {
        return;
    }

    if let Some(target) = config.udp {
        if sockets.udp.is_none() {
            match UdpTelemetrySender::new(target, config.udp_broadcast) {
                Ok(sender) => sockets.udp = Some(sender),
                Err(e) => sockets.error = Some(format!("UDP {target}: {e}")),
            }
        }
    }

    if let Some(addr) = config.tcp {
        if sockets.tcp.is_none() {
            match TcpTelemetryServer::bind(addr) {
                Ok(server) => sockets.tcp = Some(server),
                Err(e) => sockets.error = Some(format!("TCP {addr}: {e}")),
            }
        }
    }
}

/// Spedisce il frame corrente sui canali aperti.
fn telemetry_publish(
    config: Res<TelemetryConfig>,
    frame: Res<TelemetryFrame>,
    mut sockets: ResMut<TelemetrySockets>,
) {
    if !config.enabled {
        return;
    }

    let packet = frame.encode();
    if let Some(udp) = &sockets.udp {
        let _ = udp.send_packet(&packet);
    }
    if let Some(tcp) = &mut sockets.tcp {
        tcp.accept_pending();
        tcp.broadcast_packet(&packet);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn sample_frame() -> TelemetryFrame {
        TelemetryFrame {
            sim_time: 12.5,
            speed_kmh: 212.5,
            rpm: 7250.0,
            gear: 5,
            flags: FLAG_LIMITER,
            throttle: 0.87,
            brake: 0.0,
            steer: -0.15,
            position: [1.5, -2.25, 300.75],
            yaw: 0.5,
            lap: 3,
            lap_time_ms: 98_765,
            fuel: 42.5,
            damage: [0.0, 0.1, 0.0, 0.25, 1.0],
            tyre_temp: [90.0, 91.0, 88.0, 89.5],
            tyre_wear: [0.2, 0.21, 0.35, 0.36],
        }
    }

    #[test]
    fn packet_size_matches_the_field_layout() {
        assert_eq!(PACKET_SIZE, 116);
        assert_eq!(sample_frame().encode().len(), PACKET_SIZE);
    }

    #[test]
    fn round_trips_every_field() {
        let frame = sample_frame();
        let decoded = TelemetryFrame::decode(&frame.encode()).expect("decodifica");
        assert_eq!(decoded, frame);
        assert!(decoded.limiter());
        assert!(!decoded.reverse());
    }

    #[test]
    fn header_carries_magic_version_and_size() {
        let packet = sample_frame().encode();
        assert_eq!(MAGIC.to_le_bytes(), *b"KSIM");
        assert_eq!(&packet[0..4], &MAGIC.to_le_bytes());
        assert_eq!(&packet[4..6], &VERSION.to_le_bytes());
        assert_eq!(&packet[6..8], &(PAYLOAD_SIZE as u16).to_le_bytes());
        assert_eq!(&packet[0..4], b"KSIM");
    }

    #[test]
    fn rejects_bad_magic_version_and_truncation() {
        let mut packet = sample_frame().encode();
        packet[0] = 0;
        assert!(TelemetryFrame::decode(&packet).is_err());

        let mut packet = sample_frame().encode();
        packet[4] = 99;
        assert!(TelemetryFrame::decode(&packet).is_err());

        let packet = sample_frame().encode();
        assert!(TelemetryFrame::decode(&packet[..PACKET_SIZE - 1]).is_err());
        assert!(TelemetryFrame::decode(&[]).is_err());
    }

    #[test]
    fn flags_are_bidirectional() {
        let mut frame = TelemetryFrame::default();
        assert!(!frame.limiter() && !frame.reverse());

        frame.set_reverse(true);
        frame.set_limiter(true);
        assert_eq!(frame.flags, FLAG_LIMITER | FLAG_REVERSE);

        frame.set_reverse(false);
        assert!(frame.limiter() && !frame.reverse());
    }

    #[test]
    fn tcp_framing_round_trips_and_keeps_partial_data() {
        let a = sample_frame().encode();
        let mut b = sample_frame().encode();
        b[PAYLOAD_SIZE] = 7; // cambia `fuel` per distinguerli
        let mut stream = tcp_frame(&a);
        stream.extend_from_slice(&tcp_frame(&b));

        // Mezzo frame in coda: non deve essere consumato.
        stream.extend_from_slice(&[0x10, 0x00]);

        let mut buffer = stream;
        let frames = take_tcp_frames(&mut buffer);
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0], a);
        assert_eq!(frames[1], b);
        assert_eq!(buffer, vec![0x10, 0x00]);
    }

    #[test]
    fn udp_delivers_a_packet_on_loopback() {
        let receiver = UdpSocket::bind("127.0.0.1:0").expect("bind receiver");
        receiver
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("timeout");

        let sender =
            UdpTelemetrySender::new(receiver.local_addr().unwrap(), false).expect("crea sender");
        let frame = sample_frame();
        sender.send(&frame).expect("invio");

        let mut buf = [0u8; PACKET_SIZE];
        let read = receiver.recv(&mut buf).expect("ricezione");
        assert_eq!(read, PACKET_SIZE);
        assert_eq!(TelemetryFrame::decode(&buf).unwrap(), frame);
    }

    #[test]
    fn tcp_server_broadcasts_to_connected_clients() {
        let mut server =
            TcpTelemetryServer::bind("127.0.0.1:0".parse().unwrap()).expect("bind server");
        let addr = server.local_addr().unwrap();

        let mut client = TcpStream::connect(addr).expect("connect");
        assert_eq!(server.accept_pending(), 1);
        assert_eq!(server.client_count(), 1);

        let frame = sample_frame();
        assert_eq!(server.broadcast(&frame), 1);

        let mut buf = [0u8; 4 + PACKET_SIZE];
        std::io::Read::read_exact(&mut client, &mut buf).expect("lettura");
        let len = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
        assert_eq!(len, PACKET_SIZE);
        assert_eq!(TelemetryFrame::decode(&buf[4..]).unwrap(), frame);
    }

    #[test]
    fn tcp_server_drops_a_closed_client() {
        let mut server =
            TcpTelemetryServer::bind("127.0.0.1:0".parse().unwrap()).expect("bind server");
        let addr = server.local_addr().unwrap();

        let client = TcpStream::connect(addr).expect("connect");
        server.accept_pending();
        drop(client);

        // Un primo invio può ancora riuscire (il FIN non è arrivato), il secondo
        // deve accorgersi che il peer è caduto.
        let frame = sample_frame();
        let _ = server.broadcast(&frame);
        let _ = server.broadcast(&frame);
        assert!(server.client_count() <= 1);
    }
}
