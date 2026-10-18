//! Sincronizzazione degli stati auto via UDP — port di ksengine
//! `CarStateSync.h` e `RemoteCarInterpolator.h`.
//!
//! Canale leggero (niente ksnet) per il multiplayer LAN: l'host manda in
//! broadcast i pacchetti [`CarStatePacket`] (magic `0x4B534353`, porta di
//! default `:40001`) a circa 20 Hz; i client li accumulano in
//! [`RemoteCarInterpolator`] e campionano a un ritardo fisso (100 ms) così il
//! jitter non teletrasporta le auto remote.
//!
//! Il formato è **fixed-layout little-endian** (niente padding implicito):
//! header di 16 byte (`magic: u32`, `seq: u16`, `count: u16`, `sim_time: f64`)
//! seguito da `count` blocchi [`CarStatePacked`] da 38 byte.
//!
//! ```rust
//! use racingedge::net::car_state_sync::{CarStatePacket, CarStatePacked};
//!
//! let packet = CarStatePacket::new(1, 1.5, vec![CarStatePacked::default()]);
//! let decoded = CarStatePacket::decode(&packet.encode()).unwrap();
//! assert_eq!(decoded.cars.len(), 1);
//! ```

use std::collections::{HashMap, VecDeque};
use std::io;
use std::net::{IpAddr, SocketAddr, ToSocketAddrs, UdpSocket};

use bevy::prelude::*;

/// Magic di [`CarStatePacket`]: i byte big-endian sono `"KSCS"` (ksengine
/// `kCarStateMagic = 0x4B534353`).
pub const CAR_STATE_MAGIC: u32 = 0x4B53_4353;
/// Porta UDP di default del canale di stato auto.
pub const DEFAULT_SYNC_PORT: u16 = 40001;
/// Numero massimo di auto per pacchetto.
pub const MAX_CARS_PER_PACKET: usize = 16;
/// Frequenza di invio di default (Hz).
pub const DEFAULT_HZ: f32 = 20.0;
/// Byte di una voce auto nel pacchetto.
pub const CAR_STATE_SIZE: usize = 38;
/// Byte dell'header del pacchetto.
pub const PACKET_HEADER_SIZE: usize = 16;
/// Byte del pacchetto massimo (header + 16 auto).
pub const MAX_PACKET_SIZE: usize = PACKET_HEADER_SIZE + MAX_CARS_PER_PACKET * CAR_STATE_SIZE;

/// Bit di [`CarStatePacked::flags`]: auto attiva.
pub const CAR_FLAG_ACTIVE: u8 = 1 << 0;
/// Bit di [`CarStatePacked::flags`]: auto del giocatore locale.
pub const CAR_FLAG_PLAYER: u8 = 1 << 1;

/// Stato di un'auto nel formato sul filo (port di `CarStatePacked`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CarStatePacked {
    pub car_id: u32,
    pub pos: [f32; 3],
    pub heading: f32,
    pub speed: f32,
    pub steer: f32,
    pub throttle: f32,
    pub brake: f32,
    pub gear: u8,
    pub flags: u8,
}

impl CarStatePacked {
    /// Costruisce una voce da uno stato simulato (port di `packFromSim`).
    #[allow(clippy::too_many_arguments)]
    pub fn from_sim(
        id: u32,
        pos: [f32; 3],
        heading: f32,
        speed: f32,
        steer: f32,
        throttle: f32,
        brake: f32,
        gear: i32,
        player: bool,
        active: bool,
    ) -> Self {
        Self {
            car_id: id,
            pos,
            heading,
            speed,
            steer,
            throttle,
            brake,
            gear: gear.clamp(0, 8) as u8,
            flags: (active as u8) | ((player as u8) << 1),
        }
    }

    /// `true` quando l'auto è attiva.
    pub fn active(&self) -> bool {
        self.flags & CAR_FLAG_ACTIVE != 0
    }

    /// `true` quando l'auto è del giocatore locale.
    pub fn is_player(&self) -> bool {
        self.flags & CAR_FLAG_PLAYER != 0
    }

    fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.car_id.to_le_bytes());
        for v in self.pos {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&self.heading.to_le_bytes());
        out.extend_from_slice(&self.speed.to_le_bytes());
        out.extend_from_slice(&self.steer.to_le_bytes());
        out.extend_from_slice(&self.throttle.to_le_bytes());
        out.extend_from_slice(&self.brake.to_le_bytes());
        out.push(self.gear);
        out.push(self.flags);
    }

    fn decode(c: &mut Cursor) -> Result<Self, String> {
        Ok(Self {
            car_id: c.u32()?,
            pos: [c.f32()?, c.f32()?, c.f32()?],
            heading: c.f32()?,
            speed: c.f32()?,
            steer: c.f32()?,
            throttle: c.f32()?,
            brake: c.f32()?,
            gear: c.u8()?,
            flags: c.u8()?,
        })
    }
}

/// Un pacchetto di stato auto (port di `CarStatePacket`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CarStatePacket {
    pub magic: u32,
    pub seq: u16,
    pub sim_time: f64,
    pub cars: Vec<CarStatePacked>,
}

impl CarStatePacket {
    /// Crea un pacchetto con il magic corretto, troncando a
    /// [`MAX_CARS_PER_PACKET`] voci.
    pub fn new(seq: u16, sim_time: f64, mut cars: Vec<CarStatePacked>) -> Self {
        cars.truncate(MAX_CARS_PER_PACKET);
        Self {
            magic: CAR_STATE_MAGIC,
            seq,
            sim_time,
            cars,
        }
    }

    /// Serializza il pacchetto in byte little-endian.
    pub fn encode(&self) -> Vec<u8> {
        let count = self.cars.len().min(MAX_CARS_PER_PACKET);
        let mut out = Vec::with_capacity(PACKET_HEADER_SIZE + count * CAR_STATE_SIZE);
        out.extend_from_slice(&self.magic.to_le_bytes());
        out.extend_from_slice(&self.seq.to_le_bytes());
        out.extend_from_slice(&(count as u16).to_le_bytes());
        out.extend_from_slice(&self.sim_time.to_le_bytes());
        for car in self.cars.iter().take(count) {
            car.encode(&mut out);
        }
        out
    }

    /// Interpreta un pacchetto prodotto da [`CarStatePacket::encode`].
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let mut c = Cursor::new(bytes);
        let magic = c.u32()?;
        if magic != CAR_STATE_MAGIC {
            return Err(format!("magic inatteso: 0x{magic:08X}"));
        }
        let seq = c.u16()?;
        let count = c.u16()? as usize;
        if count > MAX_CARS_PER_PACKET {
            return Err(format!("count fuori range: {count}"));
        }
        let sim_time = c.f64()?;
        let mut cars = Vec::with_capacity(count);
        for _ in 0..count {
            cars.push(CarStatePacked::decode(&mut c)?);
        }
        Ok(Self {
            magic,
            seq,
            sim_time,
            cars,
        })
    }
}

/// Stato auto "ricco" usato dall'interpolatore (port di `net::CarStateData`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CarStateData {
    pub car_id: u32,
    pub frame_number: u32,
    pub pos: [f32; 3],
    pub rot: [f32; 3],
    pub vel: [f32; 3],
    pub speed: f32,
    pub rpm: f32,
    pub gear: i32,
    pub throttle: f32,
    pub brake: f32,
    pub steering: f32,
}

/// Un campione datato dell'interpolatore.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Snapshot {
    t: f64,
    state: CarStateData,
}

/// Buffer a anello per auto + interpolazione client (port di
/// `RemoteCarInterpolator`).
#[derive(Clone, Debug)]
pub struct RemoteCarInterpolator {
    buffers: HashMap<u32, VecDeque<Snapshot>>,
    delay: f64,
    latest_recv: f64,
}

/// Slot di ring buffer per auto (port di `RemoteCarInterpolator::kBufferSize`).
pub const INTERPOLATOR_BUFFER_SIZE: usize = 32;
/// Ritardo di interpolazione di default (100 ms).
pub const DEFAULT_INTERP_DELAY: f64 = 0.100;
/// Estrapolazione massima oltre l'ultimo campione (80 ms).
pub const MAX_EXTRAPOLATE: f64 = 0.080;

impl Default for RemoteCarInterpolator {
    fn default() -> Self {
        Self {
            buffers: HashMap::new(),
            delay: DEFAULT_INTERP_DELAY,
            latest_recv: 0.0,
        }
    }
}

impl RemoteCarInterpolator {
    /// Nuovo interpolatore col ritardo di default.
    pub fn new() -> Self {
        Self::default()
    }

    /// Imposta il ritardo, limitato a `0..=0.5` s.
    pub fn set_delay(&mut self, sec: f64) {
        self.delay = sec.clamp(0.0, 0.5);
    }

    /// Ritardo corrente in secondi.
    pub fn delay(&self) -> f64 {
        self.delay
    }

    /// Svuota tutti i buffer.
    pub fn clear(&mut self) {
        self.buffers.clear();
    }

    /// Dimentica un'auto.
    pub fn remove(&mut self, car_id: u32) {
        self.buffers.remove(&car_id);
        if self.buffers.is_empty() {
            self.latest_recv = 0.0;
        }
    }

    /// Aggiunge un campione. `recv_time` è il tempo locale monotono del client.
    ///
    /// Ritorna `false` se il campione è fuori ordine e va scartato.
    pub fn push(&mut self, car_id: u32, mut state: CarStateData, recv_time: f64) -> bool {
        state.car_id = car_id;
        let buf = self.buffers.entry(car_id).or_default();
        if buf.len() >= INTERPOLATOR_BUFFER_SIZE {
            buf.pop_front();
        }
        if let Some(last) = buf.back() {
            if recv_time + 1e-6 < last.t {
                return false;
            }
        }
        buf.push_back(Snapshot {
            t: recv_time,
            state,
        });
        self.latest_recv = self.latest_recv.max(recv_time);
        true
    }

    /// Campiona un'auto a `render_time` meno il ritardo.
    pub fn sample(&self, car_id: u32, render_time: f64) -> Option<CarStateData> {
        let buf = self.buffers.get(&car_id)?;
        sample_buffer(buf, render_time - self.delay)
    }

    /// Campiona tutte le auto note, in ordine di `car_id`.
    pub fn sample_all(&self, render_time: f64) -> Vec<(u32, CarStateData)> {
        let target = render_time - self.delay;
        let mut out: Vec<(u32, CarStateData)> = self
            .buffers
            .iter()
            .filter_map(|(&id, buf)| sample_buffer(buf, target).map(|s| (id, s)))
            .collect();
        out.sort_by_key(|(id, _)| *id);
        out
    }

    /// Istante dell'ultimo campione ricevuto.
    pub fn latest_recv_time(&self) -> f64 {
        self.latest_recv
    }

    /// Numero di auto tracciate.
    pub fn tracked_cars(&self) -> usize {
        self.buffers.len()
    }

    /// Numero di campioni in buffer per un'auto.
    pub fn buffered(&self, car_id: u32) -> usize {
        self.buffers.get(&car_id).map_or(0, VecDeque::len)
    }

    /// Istante dell'ultimo campione di un'auto, se presente.
    pub fn last_recv(&self, car_id: u32) -> Option<f64> {
        self.buffers.get(&car_id)?.back().map(|s| s.t)
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn lerp_angle(a: f32, b: f32, t: f32) -> f32 {
    let mut d = b - a;
    while d > std::f32::consts::PI {
        d -= std::f32::consts::TAU;
    }
    while d < -std::f32::consts::PI {
        d += std::f32::consts::TAU;
    }
    a + d * t
}

fn sample_buffer(buf: &VecDeque<Snapshot>, target: f64) -> Option<CarStateData> {
    let first = buf.front()?;
    if buf.len() == 1 {
        return Some(first.state);
    }

    if target <= first.t {
        return Some(first.state);
    }

    let last = buf.back()?;
    if target >= last.t {
        let dt = (target - last.t).min(MAX_EXTRAPOLATE) as f32;
        let mut out = last.state;
        out.pos[0] += last.state.vel[0] * dt;
        out.pos[1] += last.state.vel[1] * dt;
        out.pos[2] += last.state.vel[2] * dt;
        return Some(out);
    }

    let mut i = 0;
    while i + 1 < buf.len() && buf[i + 1].t < target {
        i += 1;
    }
    let a = &buf[i];
    let b = &buf[i + 1];
    let span = b.t - a.t;
    let u = if span > 1e-9 {
        ((target - a.t) / span) as f32
    } else {
        0.0
    };

    let mut out = a.state;
    out.car_id = a.state.car_id;
    for k in 0..3 {
        out.pos[k] = lerp(a.state.pos[k], b.state.pos[k], u);
        out.rot[k] = lerp_angle(a.state.rot[k], b.state.rot[k], u);
        out.vel[k] = lerp(a.state.vel[k], b.state.vel[k], u);
    }
    out.speed = lerp(a.state.speed, b.state.speed, u);
    out.rpm = lerp(a.state.rpm, b.state.rpm, u);
    out.throttle = lerp(a.state.throttle, b.state.throttle, u);
    out.brake = lerp(a.state.brake, b.state.brake, u);
    out.steering = lerp(a.state.steering, b.state.steering, u);
    out.gear = if u < 0.5 { a.state.gear } else { b.state.gear };
    Some(out)
}

/// Ruolo del nodo nel canale di stato auto (port di `CarStateSync::Role`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SyncRole {
    #[default]
    None,
    Host,
    Client,
}

/// Canale di stato auto su UDP (port di `CarStateSync`).
#[derive(Debug)]
pub struct CarStateSync {
    role: SyncRole,
    socket: Option<UdpSocket>,
    port: u16,
    peer: Option<SocketAddr>,
    peers: Vec<SocketAddr>,
    seq: u16,
    send_hz: f32,
    last_send: f64,
    interp: RemoteCarInterpolator,
}

/// Client registrati lato host (port di `kMaxClients`).
pub const MAX_HOST_PEERS: usize = 32;
/// Oltre questo ritardo un campione remoto è considerato stantio.
pub const STALE_AFTER: f64 = 1.5;

impl Default for CarStateSync {
    fn default() -> Self {
        Self {
            role: SyncRole::None,
            socket: None,
            port: DEFAULT_SYNC_PORT,
            peer: None,
            peers: Vec::new(),
            seq: 0,
            send_hz: DEFAULT_HZ,
            last_send: f64::NEG_INFINITY,
            interp: RemoteCarInterpolator::new(),
        }
    }
}

impl CarStateSync {
    /// Crea lo stato senza aprire socket (utile per i test e per l'ingest
    /// offline).
    pub fn new() -> Self {
        Self::default()
    }

    /// Apre il socket in modalità host e va in ascolto su `port`
    /// (port di `startHost`).
    pub fn start_host(&mut self, port: u16) -> io::Result<()> {
        self.stop();
        let socket = open_socket(port, true)?;
        self.role = SyncRole::Host;
        self.port = port;
        self.socket = Some(socket);
        self.seq = 0;
        Ok(())
    }

    /// Apre il socket in modalità client e fissa il peer host
    /// (port di `startClient`).
    pub fn start_client(&mut self, host: &str, port: u16) -> io::Result<()> {
        self.stop();
        let socket = open_socket(0, false)?;
        let ip = match host.parse::<IpAddr>() {
            Ok(ip) => ip,
            Err(_) => (host, port)
                .to_socket_addrs()?
                .next()
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "host non risolvibile"))?
                .ip(),
        };
        self.role = SyncRole::Client;
        self.port = port;
        self.peer = Some(SocketAddr::new(ip, port));
        self.socket = Some(socket);
        Ok(())
    }

    /// Chiude il socket e dimentica i remoti (port di `stop`).
    pub fn stop(&mut self) {
        self.socket = None;
        self.role = SyncRole::None;
        self.peer = None;
        self.peers.clear();
        self.interp.clear();
    }

    /// Ruolo corrente.
    pub fn role(&self) -> SyncRole {
        self.role
    }

    /// `true` quando un ruolo è attivo.
    pub fn active(&self) -> bool {
        self.role != SyncRole::None
    }

    /// Imposta la frequenza di invio, limitata a `5..=60` Hz (port di
    /// `setSendHz`).
    pub fn set_send_hz(&mut self, hz: f32) {
        self.send_hz = hz.clamp(5.0, 60.0);
    }

    /// Frequenza di invio corrente.
    pub fn send_hz(&self) -> f32 {
        self.send_hz
    }

    /// Indirizzo locale del socket, se aperto.
    pub fn local_addr(&self) -> Option<SocketAddr> {
        self.socket.as_ref().and_then(|s| s.local_addr().ok())
    }

    /// Peer host (modalità client).
    pub fn peer(&self) -> Option<SocketAddr> {
        self.peer
    }

    /// Client registrati (modalità host).
    pub fn peers(&self) -> &[SocketAddr] {
        &self.peers
    }

    /// Interpolatore dei remoti.
    pub fn interpolator(&self) -> &RemoteCarInterpolator {
        &self.interp
    }

    /// Serializza e (se il socket è aperto) spedisce lo stato.
    ///
    /// Il ritmo è limitato a [`Self::send_hz`]; ritorna `true` quando il
    /// pacchetto è stato costruito/spedito (port di `publish`).
    pub fn publish(&mut self, sim_time: f64, cars: &[CarStatePacked], now: f64) -> bool {
        if !self.active() {
            return false;
        }
        if now - self.last_send < 1.0 / self.send_hz as f64 {
            return false;
        }
        self.last_send = now;
        self.seq = self.seq.wrapping_add(1);
        let packet = CarStatePacket::new(self.seq, sim_time, cars.to_vec());
        let bytes = packet.encode();

        if let Some(socket) = &self.socket {
            match self.role {
                SyncRole::Host => {
                    for peer in &self.peers {
                        let _ = socket.send_to(&bytes, peer);
                    }
                    let bcast = SocketAddr::from(([255, 255, 255, 255], self.port));
                    let _ = socket.send_to(&bytes, bcast);
                }
                SyncRole::Client => {
                    if let Some(peer) = self.peer {
                        let _ = socket.send_to(&bytes, peer);
                    }
                }
                SyncRole::None => {}
            }
        }
        true
    }

    /// Riceve e integra tutti i pacchetti disponibili; ritorna le auto
    /// aggiornate (port di `poll`).
    pub fn poll(&mut self, now: f64) -> usize {
        let mut received: Vec<(Vec<u8>, SocketAddr)> = Vec::new();
        if let Some(socket) = &self.socket {
            let mut buf = [0u8; MAX_PACKET_SIZE];
            loop {
                match socket.recv_from(&mut buf) {
                    Ok((n, from)) => received.push((buf[..n].to_vec(), from)),
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                    Err(_) => break,
                }
            }
        }
        let mut updated = 0;
        for (bytes, from) in received {
            if let Ok(packet) = CarStatePacket::decode(&bytes) {
                updated += self.ingest(&packet, from, now);
            }
        }
        updated
    }

    /// Integra un pacchetto già decodificato; ritorna il numero di auto
    /// inserite nell'interpolatore.
    pub fn ingest(&mut self, packet: &CarStatePacket, from: SocketAddr, now: f64) -> usize {
        if self.role == SyncRole::Host
            && !self.peers.contains(&from)
            && self.peers.len() < MAX_HOST_PEERS
        {
            self.peers.push(from);
        }
        let mut n = 0;
        for car in &packet.cars {
            let data = CarStateData {
                car_id: car.car_id,
                pos: car.pos,
                rot: [0.0, car.heading, 0.0],
                speed: car.speed,
                gear: car.gear as i32,
                throttle: car.throttle,
                brake: car.brake,
                steering: car.steer,
                ..Default::default()
            };
            if self.interp.push(car.car_id, data, now) {
                n += 1;
            }
        }
        n
    }

    /// Campiona un'auto remota, scartando i dati più vecchi di
    /// [`STALE_AFTER`] (port di `sample`).
    pub fn sample(&self, car_id: u32, render_time: f64) -> Option<CarStateData> {
        let last = self.interp.last_recv(car_id)?;
        if render_time - last > STALE_AFTER {
            return None;
        }
        self.interp.sample(car_id, render_time)
    }
}

fn open_socket(bind_port: u16, broadcast: bool) -> io::Result<UdpSocket> {
    let socket = UdpSocket::bind(SocketAddr::from(([0, 0, 0, 0], bind_port)))?;
    socket.set_nonblocking(true)?;
    if broadcast {
        socket.set_broadcast(true)?;
    }
    Ok(socket)
}

/// Configurazione del canale di stato auto.
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct CarStateSyncConfig {
    /// Interruttore generale: a `false` non viene aperto alcun socket.
    pub enabled: bool,
    /// `true` per fare da host, `false` per connettersi a un host.
    pub host: bool,
    /// Porta del canale.
    pub port: u16,
    /// Indirizzo dell'host (solo in modalità client), es. `"127.0.0.1"`.
    pub peer_host: String,
    /// Frequenza di invio in Hz (limitata a `5..=60`).
    pub send_hz: f32,
}

impl Default for CarStateSyncConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            host: true,
            port: DEFAULT_SYNC_PORT,
            peer_host: "127.0.0.1".to_string(),
            send_hz: DEFAULT_HZ,
        }
    }
}

/// Auto locali da spedire, riempite dall'applicazione.
#[derive(Resource, Clone, Debug, Default)]
pub struct LocalCarStates {
    /// Tempo di simulazione associato allo stato.
    pub sim_time: f64,
    /// Stato delle auto locali.
    pub cars: Vec<CarStatePacked>,
}

/// Stato runtime del canale (socket + interpolatore + errore).
#[derive(Resource, Default)]
pub struct CarStateSyncState {
    pub sync: CarStateSync,
    pub error: Option<String>,
}

/// Plugin che apre il canale e lo fa girare a ogni frame.
///
/// Non fa nulla finché [`CarStateSyncConfig::enabled`] resta `false`.
pub struct CarStateSyncPlugin;

impl Plugin for CarStateSyncPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CarStateSyncConfig>()
            .init_resource::<LocalCarStates>()
            .init_resource::<CarStateSyncState>()
            .add_systems(
                Update,
                (car_sync_connect, car_sync_poll, car_sync_publish).chain(),
            );
    }
}

fn car_sync_connect(config: Res<CarStateSyncConfig>, mut state: ResMut<CarStateSyncState>) {
    if !config.enabled || state.sync.active() {
        return;
    }
    let result = if config.host {
        state.sync.start_host(config.port)
    } else {
        state
            .sync
            .start_client(config.peer_host.as_str(), config.port)
    };
    match result {
        Ok(()) => {
            state.sync.set_send_hz(config.send_hz);
            state.error = None;
        }
        Err(e) => state.error = Some(format!("CarStateSync: {e}")),
    }
}

fn car_sync_poll(time: Res<Time>, mut state: ResMut<CarStateSyncState>) {
    state.sync.poll(time.elapsed_secs() as f64);
}

fn car_sync_publish(
    config: Res<CarStateSyncConfig>,
    time: Res<Time>,
    local: Res<LocalCarStates>,
    mut state: ResMut<CarStateSyncState>,
) {
    if !config.enabled {
        return;
    }
    state
        .sync
        .publish(local.sim_time, &local.cars, time.elapsed_secs() as f64);
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

    fn f32(&mut self) -> Result<f32, String> {
        Ok(f32::from_le_bytes(self.take()?))
    }

    fn f64(&mut self) -> Result<f64, String> {
        Ok(f64::from_le_bytes(self.take()?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;
    use std::time::Duration;

    fn packed(id: u32, x: f32) -> CarStatePacked {
        CarStatePacked {
            car_id: id,
            pos: [x, 1.0, 2.0],
            heading: 0.5,
            speed: 42.0,
            steer: -0.1,
            throttle: 0.9,
            brake: 0.0,
            gear: 4,
            flags: CAR_FLAG_ACTIVE,
        }
    }

    fn data(id: u32, t_x: f32, yaw: f32) -> CarStateData {
        CarStateData {
            car_id: id,
            pos: [t_x, 0.0, 0.0],
            rot: [0.0, yaw, 0.0],
            speed: 30.0,
            gear: 3,
            ..Default::default()
        }
    }

    #[test]
    fn packed_is_38_bytes() {
        assert_eq!(CAR_STATE_SIZE, 38);
        let mut out = Vec::new();
        packed(1, 0.0).encode(&mut out);
        assert_eq!(out.len(), CAR_STATE_SIZE);
    }

    #[test]
    fn packet_round_trips_all_cars() {
        let packet = CarStatePacket::new(7, 12.5, vec![packed(1, 0.0), packed(2, 9.0)]);
        let bytes = packet.encode();
        assert_eq!(bytes.len(), PACKET_HEADER_SIZE + 2 * CAR_STATE_SIZE);
        let decoded = CarStatePacket::decode(&bytes).expect("decode");
        assert_eq!(decoded, packet);
    }

    #[test]
    fn packet_header_carries_magic_seq_and_count() {
        let bytes = CarStatePacket::new(3, 1.0, vec![packed(1, 0.0)]).encode();
        assert_eq!(&bytes[0..4], &CAR_STATE_MAGIC.to_le_bytes());
        assert_eq!(CAR_STATE_MAGIC.to_be_bytes(), *b"KSCS");
        assert_eq!(&bytes[4..6], &3u16.to_le_bytes());
        assert_eq!(&bytes[6..8], &1u16.to_le_bytes());
    }

    #[test]
    fn packet_truncates_to_max_cars() {
        let cars = (0..32).map(|i| packed(i, i as f32)).collect();
        let packet = CarStatePacket::new(0, 0.0, cars);
        assert_eq!(packet.cars.len(), MAX_CARS_PER_PACKET);
        assert_eq!(packet.encode().len(), MAX_PACKET_SIZE);
    }

    #[test]
    fn packet_rejects_bad_magic_count_and_truncation() {
        let mut bytes = CarStatePacket::new(1, 0.0, vec![packed(1, 0.0)]).encode();
        bytes[0] = 0;
        assert!(CarStatePacket::decode(&bytes).is_err());

        let mut bytes = CarStatePacket::new(1, 0.0, vec![packed(1, 0.0)]).encode();
        bytes[6] = 99; // count = 99 > MAX
        assert!(CarStatePacket::decode(&bytes).is_err());

        let bytes = CarStatePacket::new(1, 0.0, vec![packed(1, 0.0)]).encode();
        assert!(CarStatePacket::decode(&bytes[..bytes.len() - 1]).is_err());
        assert!(CarStatePacket::decode(&[]).is_err());
    }

    #[test]
    fn from_sim_clamps_gear_and_sets_flags() {
        let car =
            CarStatePacked::from_sim(5, [1.0, 2.0, 3.0], 0.0, 10.0, 0.0, 1.0, 0.0, 42, true, true);
        assert_eq!(car.gear, 8);
        assert!(car.active() && car.is_player());

        let neg = CarStatePacked::from_sim(5, [0.0; 3], 0.0, 0.0, 0.0, 0.0, 1.0, -3, false, false);
        assert_eq!(neg.gear, 0);
        assert!(!neg.active() && !neg.is_player());
    }

    #[test]
    fn send_hz_is_clamped() {
        let mut sync = CarStateSync::new();
        sync.set_send_hz(1.0);
        assert_eq!(sync.send_hz(), 5.0);
        sync.set_send_hz(120.0);
        assert_eq!(sync.send_hz(), 60.0);
    }

    #[test]
    fn publish_rate_is_limited() {
        let mut sync = CarStateSync::new();
        sync.start_host(0).unwrap();
        let cars = [packed(1, 0.0)];
        assert!(sync.publish(0.0, &cars, 0.0));
        assert!(!sync.publish(0.0, &cars, 0.02));
        assert!(sync.publish(0.0, &cars, 0.06));
        assert!(!sync.publish(0.0, &cars, 0.10));
    }

    #[test]
    fn inactive_sync_does_not_publish() {
        let mut sync = CarStateSync::new();
        assert!(!sync.publish(0.0, &[packed(1, 0.0)], 1.0));
    }

    #[test]
    fn ingest_tracks_cars_and_role_peers() {
        let mut sync = CarStateSync::new();
        sync.start_host(0).unwrap();
        let from: SocketAddr = "127.0.0.1:5000".parse().unwrap();
        let packet = CarStatePacket::new(1, 1.0, vec![packed(4, 3.0)]);
        assert_eq!(sync.ingest(&packet, from, 0.0), 1);
        assert_eq!(sync.peers(), &[from]);
        assert_eq!(sync.interpolator().tracked_cars(), 1);
        assert!(sync.sample(4, 0.0).is_some());
    }

    #[test]
    fn sample_goes_stale_after_timeout() {
        let mut sync = CarStateSync::new();
        sync.start_host(0).unwrap();
        let from: SocketAddr = "127.0.0.1:5000".parse().unwrap();
        sync.ingest(
            &CarStatePacket::new(1, 0.0, vec![packed(1, 0.0)]),
            from,
            0.0,
        );
        assert!(sync.sample(1, 1.0).is_some());
        assert!(sync.sample(1, 2.0).is_none());
    }

    #[test]
    fn interpolator_linear_between_samples() {
        let mut interp = RemoteCarInterpolator::new();
        interp.set_delay(0.0);
        interp.push(1, data(1, 0.0, 0.0), 0.0);
        interp.push(1, data(1, 10.0, 0.0), 0.1);
        let mid = interp.sample(1, 0.05).unwrap();
        assert!((mid.pos[0] - 5.0).abs() < 1e-4);
    }

    #[test]
    fn interpolator_clamps_before_first_and_handles_single() {
        let mut interp = RemoteCarInterpolator::new();
        interp.set_delay(0.0);
        interp.push(1, data(1, 7.0, 0.0), 1.0);
        assert_eq!(interp.sample(1, 0.5).unwrap().pos[0], 7.0);

        interp.push(1, data(1, 9.0, 0.0), 1.1);
        assert_eq!(interp.sample(1, 0.5).unwrap().pos[0], 7.0);
    }

    #[test]
    fn interpolator_extrapolates_after_last_and_clamps() {
        let mut interp = RemoteCarInterpolator::new();
        interp.set_delay(0.0);
        let mut state = data(1, 0.0, 0.0);
        state.vel = [10.0, 0.0, 0.0];
        interp.push(1, data(1, 0.0, 0.0), 0.0);
        interp.push(1, state, 0.1);
        // 40 ms past the last snapshot -> 0.4 m along velocity.
        let out = interp.sample(1, 0.14).unwrap();
        assert!((out.pos[0] - 0.4).abs() < 1e-4);
        // Far past the last snapshot: clamped to MAX_EXTRAPOLATE (80 ms).
        let far = interp.sample(1, 1.0).unwrap();
        assert!((far.pos[0] - 0.8).abs() < 1e-4);
    }

    #[test]
    fn interpolator_wraps_angle_across_pi() {
        let mut interp = RemoteCarInterpolator::new();
        interp.set_delay(0.0);
        interp.push(1, data(1, 0.0, 3.0), 0.0);
        interp.push(1, data(1, 0.0, -3.0), 0.1);
        let mid = interp.sample(1, 0.05).unwrap().rot[1];
        // Crossing +pi must go through ~pi, not through 0.
        assert!(mid.abs() > 3.0);
    }

    #[test]
    fn interpolator_rejects_out_of_order_and_caps_ring() {
        let mut interp = RemoteCarInterpolator::new();
        assert!(interp.push(1, data(1, 0.0, 0.0), 1.0));
        assert!(!interp.push(1, data(1, 1.0, 0.0), 0.5));
        for i in 0..INTERPOLATOR_BUFFER_SIZE + 5 {
            interp.push(1, data(1, i as f32, 0.0), 2.0 + i as f64);
        }
        assert_eq!(interp.buffered(1), INTERPOLATOR_BUFFER_SIZE);
    }

    #[test]
    fn interpolator_gear_snaps_and_csv_sorts() {
        let mut interp = RemoteCarInterpolator::new();
        interp.set_delay(0.0);
        let mut a = data(2, 0.0, 0.0);
        a.gear = 2;
        let mut b = data(2, 10.0, 0.0);
        b.gear = 4;
        interp.push(2, a, 0.0);
        interp.push(2, b, 0.1);
        assert_eq!(interp.sample(2, 0.01).unwrap().gear, 2);
        assert_eq!(interp.sample(2, 0.09).unwrap().gear, 4);

        interp.push(1, data(1, 0.0, 0.0), 0.0);
        let all = interp.sample_all(0.0);
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].0, 1);
        assert_eq!(all[1].0, 2);
    }

    #[test]
    fn remove_and_clear_drop_tracked_cars() {
        let mut interp = RemoteCarInterpolator::new();
        interp.push(1, data(1, 0.0, 0.0), 0.0);
        interp.push(2, data(2, 0.0, 0.0), 0.0);
        interp.remove(1);
        assert_eq!(interp.tracked_cars(), 1);
        interp.clear();
        assert_eq!(interp.tracked_cars(), 0);
        assert_eq!(interp.latest_recv_time(), 0.0);
    }

    #[test]
    fn udp_loopback_between_client_and_host() {
        let mut host = CarStateSync::new();
        host.start_host(0).unwrap();
        let host_addr = host.local_addr().unwrap();

        let mut client = CarStateSync::new();
        client.start_client("127.0.0.1", host_addr.port()).unwrap();

        let cars = [packed(9, 12.0)];
        assert!(client.publish(2.5, &cars, 1.0));

        let mut learned = 0;
        for _ in 0..200 {
            learned += host.poll(1.0);
            if learned > 0 {
                break;
            }
            sleep(Duration::from_millis(5));
        }
        assert!(learned > 0, "host non ha ricevuto il pacchetto del client");
        assert_eq!(host.peers().len(), 1);

        let sample = host.sample(9, 1.0).expect("campione remoto");
        assert_eq!(sample.car_id, 9);
        assert!((sample.pos[0] - 12.0).abs() < 1e-3);

        // Host -> client: ora che il peer è noto l'host può spedire.
        let host_cars = [packed(1, 0.0), packed(2, 4.0)];
        assert!(host.publish(2.5, &host_cars, 5.0));
        let mut got = 0;
        for _ in 0..200 {
            got += client.poll(5.0);
            if got >= 2 {
                break;
            }
            sleep(Duration::from_millis(5));
        }
        assert!(got >= 2, "client non ha ricevuto lo stato dell'host");
        assert!(client.sample(2, 5.0).is_some());
    }
}
