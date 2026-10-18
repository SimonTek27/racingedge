//! Multi-car manager + AI traffic (port of ksengine `MultiCarManager.*`,
//! `AIController.*` and `MultiCarManager_TrafficUpdate.inl`).
//!
//! The upstream `MultiCarManager` owns a `VehicleSimulator` per car; the full
//! vehicle simulator is an application-level concern in RacingEdge, so this
//! port keeps the *bookkeeping* (entries, grid, traffic list, collision
//! detection) and drives AI cars through a small kinematic stand-in
//! ([`CarKinematics`]) that stands in for `VehicleSimulator::updatePhysics`.
//! [`AiController`] itself is a faithful port of the upstream spline
//! follower, including the speed-scheduled lookahead, curvature feedforward
//! and derivative damping used to keep the heading loop stable.

use bevy::math::Vec3;

use crate::formats::ai_spline::{read_spline, AiSpline};

fn wrap_pi(mut a: f32) -> f32 {
    while a > std::f32::consts::PI {
        a -= std::f32::consts::TAU;
    }
    while a < -std::f32::consts::PI {
        a += std::f32::consts::TAU;
    }
    a
}

/// Un'auto "altra" vista dall'AI per il sorpasso (port di `AiTrafficCar`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AiTrafficCar {
    pub id: i32,
    pub x: f32,
    pub z: f32,
    pub heading: f32,
    pub speed: f32,
    pub along_track: f32,
}

/// Stato in ingresso a un passo AI (posizione e stato del veicolo).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AiInput {
    pub position: Vec3,
    pub heading: f32,
    pub speed: f32,
    pub gear: i32,
}

/// Uscita di controllo calcolata dall'AI.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AiOutput {
    pub throttle: f32,
    pub brake: f32,
    pub steering: f32,
    pub gear: i32,
}

/// Inseguitore di linea AI (port di `ks::sim::AIController`).
#[derive(Clone, Debug, Default)]
pub struct AiController {
    spline: AiSpline,
    cumulative: Vec<f32>,
    ready: bool,
    current_idx: usize,
    lap_count: u32,
    prev_progress: f32,
    traveled_since_lap: f32,
    has_progress: bool,

    speed_factor: f32,
    aggression: f32,
    lookahead_base: f32,
    lookahead_per_speed: f32,
    lookahead_min: f32,
    lookahead_max: f32,

    lateral_offset: f32,
    lateral_target: f32,
    overtake_enabled: bool,
    overtake_side: f32,

    err_lp: f32,
    err_lp_prev: f32,
    d_err_lp: f32,
    err_init: bool,

    output: AiOutput,
}

impl AiController {
    /// Crea un controllore non pronto (nessuna linea caricata).
    pub fn new() -> Self {
        Self {
            speed_factor: 1.0,
            aggression: 0.5,
            lookahead_base: 8.0,
            lookahead_per_speed: 1.2,
            lookahead_min: 12.0,
            lookahead_max: 40.0,
            overtake_enabled: true,
            overtake_side: 1.0,
            output: AiOutput {
                gear: 1,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    /// Carica la linea AI da una [`AiSpline`] già letta (port di `loadSpline`).
    pub fn load_from_spline(&mut self, spline: AiSpline) -> bool {
        if !spline.is_valid() {
            return false;
        }
        let mut cumulative = vec![0.0f32; spline.points.len()];
        for i in 1..spline.points.len() {
            cumulative[i] = cumulative[i - 1]
                + spline.points[i - 1]
                    .position
                    .distance_to(spline.points[i].position);
        }

        self.spline = spline;
        self.cumulative = cumulative;
        self.ready = true;
        self.current_idx = 0;
        self.lap_count = 0;
        self.prev_progress = 0.0;
        self.traveled_since_lap = 0.0;
        self.has_progress = false;
        self.lateral_offset = 0.0;
        self.lateral_target = 0.0;
        true
    }

    /// Carica la linea AI da `<trackDirectory>/ai/fast_lane.ai` (port di
    /// `loadSpline`); tenta il companion `.txt` se il binario non è valido.
    pub fn load_spline_file(&mut self, track_directory: &str) -> bool {
        let path = format!("{track_directory}/ai/fast_lane.ai");
        let mut spline = read_spline(&path);
        if !spline.is_valid() {
            spline = read_spline(format!("{path}.txt"));
        }
        self.load_from_spline(spline)
    }

    /// `true` una volta che una linea valida è stata caricata.
    pub fn is_ready(&self) -> bool {
        self.ready
    }

    pub fn speed_factor(&self) -> f32 {
        self.speed_factor
    }

    pub fn set_speed_factor(&mut self, factor: f32) {
        self.speed_factor = factor;
    }

    pub fn aggression(&self) -> f32 {
        self.aggression
    }

    pub fn set_aggression(&mut self, aggression: f32) {
        self.aggression = aggression;
    }

    pub fn overtake_enabled(&self) -> bool {
        self.overtake_enabled
    }

    pub fn set_overtake_enabled(&mut self, enabled: bool) {
        self.overtake_enabled = enabled;
    }

    /// Ultimo throttle calcolato.
    pub fn throttle(&self) -> f32 {
        self.output.throttle
    }

    /// Ultimo freno calcolato.
    pub fn brake(&self) -> f32 {
        self.output.brake
    }

    /// Ultimo sterzo calcolato.
    pub fn steering(&self) -> f32 {
        self.output.steering
    }

    /// Marcia obiettivo calcolata.
    pub fn target_gear(&self) -> i32 {
        self.output.gear
    }

    /// Indice del punto più vicino all'ultimo aggiornamento.
    pub fn nearest_index(&self) -> usize {
        self.current_idx
    }

    /// Offset laterale corrente dalla linea (m).
    pub fn lateral_offset(&self) -> f32 {
        self.lateral_offset
    }

    /// Giri completati.
    pub fn lap_count(&self) -> u32 {
        self.lap_count
    }

    /// Distanza cumulata all'indice più vicino (m).
    pub fn progress_distance(&self) -> f32 {
        self.cumulative
            .get(self.current_idx)
            .copied()
            .unwrap_or(0.0)
    }

    /// Distanza totale della linea (m).
    pub fn spline_length(&self) -> f32 {
        self.spline.total_distance
    }

    /// Passo senza traffico (port di `update` a 5 argomenti).
    pub fn update(&mut self, input: AiInput, dt: f32) {
        self.update_with_traffic(input, dt, &[], -1);
    }

    /// Passo con lista traffico per il sorpasso (port di `update` a 7 argomenti).
    pub fn update_with_traffic(
        &mut self,
        input: AiInput,
        dt: f32,
        traffic: &[AiTrafficCar],
        self_id: i32,
    ) {
        if !self.ready || self.spline.points.is_empty() {
            self.output.throttle = 0.0;
            self.output.brake = 1.0;
            self.output.steering = 0.0;
            return;
        }

        let position = input.position;
        let nearest = self.find_nearest_point(position);
        self.current_idx = nearest;
        self.detect_lap(nearest);

        // Lookahead programmato in base alla velocità: un valore fisso di 30 m
        // aggiunge >2 s di ritardo di fase e fa divergere l'anello di heading.
        let lookahead_dist = (self.lookahead_base + self.lookahead_per_speed * input.speed)
            .clamp(self.lookahead_min, self.lookahead_max);
        let lookahead = self.find_lookahead_point(nearest, lookahead_dist);
        let target_point = self.spline.points[lookahead];
        let mut target_pos = Vec3::new(
            target_point.position.x,
            target_point.position.y,
            target_point.position.z,
        );

        let mut target_speed = target_point.speed * self.speed_factor;
        let curvature = target_point.curvature.abs();
        if curvature > 0.01 {
            target_speed *= 1.0 - curvature * self.aggression * 0.5;
        }

        target_speed = self.evaluate_traffic(
            position,
            input.speed,
            nearest,
            traffic,
            self_id,
            target_speed,
        );

        let lat_rate = 2.5 * dt;
        if self.lateral_offset < self.lateral_target {
            self.lateral_offset = self.lateral_target.min(self.lateral_offset + lat_rate);
        } else {
            self.lateral_offset = self.lateral_target.max(self.lateral_offset - lat_rate);
        }
        if self.lateral_offset.abs() > 1e-4 {
            let (tx, tz) = self.tangent_at(lookahead);
            target_pos.x += -tz * self.lateral_offset;
            target_pos.z += tx * self.lateral_offset;
        }

        let mut steering = self.calculate_steering(position, input.heading, target_pos);

        // Curvature feedforward: il solo feedback di heading deve prima
        // costruire un errore cross-track, che oscillava in un ciclo limite
        // (~12 s). Normalizzato su Ld così il termine è la curvatura firma.
        let w = wrap_pi(self.tangent_heading(lookahead) - self.tangent_heading(nearest));
        let ff = -(w / lookahead_dist) * (input.speed * input.speed) * 0.09;
        steering = (steering + ff).clamp(-1.0, 1.0);

        // Smorzamento derivativo: doppio passa-basso (err, poi dErr) per tenere
        // la quantizzazione dell'indice più vicino fuori dalla derivata.
        {
            let dx = target_pos.x - position.x;
            let dz = target_pos.z - position.z;
            let err = wrap_pi(dx.atan2(dz) - input.heading);
            let ad = (dt / 0.25).clamp(0.0, 1.0);
            self.err_lp += (err - self.err_lp) * ad;
            if !self.err_init {
                self.err_lp_prev = self.err_lp;
                self.err_init = true;
            }
            let d_err = (self.err_lp - self.err_lp_prev) / dt.max(1e-4);
            self.err_lp_prev = self.err_lp;
            self.d_err_lp += (d_err - self.d_err_lp) * (dt / 0.30).clamp(0.0, 1.0);
            steering = (steering - self.d_err_lp.clamp(-0.4, 0.4)).clamp(-1.0, 1.0);
        }

        self.output.steering = steering;
        self.output.throttle = Self::calculate_throttle(input.speed, target_speed);
        self.output.brake = Self::calculate_brake(input.speed, target_speed);
        self.output.gear = Self::calculate_gear(input.speed);
    }

    fn detect_lap(&mut self, nearest_idx: usize) {
        if !self.spline.closed || self.spline.total_distance <= 0.0 {
            return;
        }
        let total = self.spline.total_distance;
        let prog = self.cumulative[nearest_idx];

        if self.has_progress {
            let crossed_line = self.prev_progress >= total * 0.5 && prog <= total * 0.25;
            if crossed_line {
                if self.traveled_since_lap >= total * 0.5 {
                    self.lap_count += 1;
                }
                self.traveled_since_lap = 0.0;
            } else if prog > self.prev_progress {
                self.traveled_since_lap += prog - self.prev_progress;
            }
        }
        self.prev_progress = prog;
        self.has_progress = true;
    }

    fn tangent_at(&self, idx: usize) -> (f32, f32) {
        let n = self.spline.points.len();
        if n < 2 {
            return (0.0, 1.0);
        }
        let i0 = idx.min(n - 1);
        let i1 = (i0 + 1).min(n - 1);
        let a = self.spline.points[i0].position;
        let b = self.spline.points[i1].position;
        let mut tx = b.x - a.x;
        let mut tz = b.z - a.z;
        let len = (tx * tx + tz * tz).sqrt();
        if len > 1e-4 {
            tx /= len;
            tz /= len;
        } else {
            tx = 0.0;
            tz = 1.0;
        }
        (tx, tz)
    }

    fn tangent_heading(&self, idx: usize) -> f32 {
        let pts = &self.spline.points;
        if pts.is_empty() {
            return 0.0;
        }
        let i = idx % pts.len();
        let j = (i + 1) % pts.len();
        (pts[j].position.x - pts[i].position.x).atan2(pts[j].position.z - pts[i].position.z)
    }

    fn evaluate_traffic(
        &mut self,
        car_pos: Vec3,
        speed: f32,
        nearest_idx: usize,
        traffic: &[AiTrafficCar],
        self_id: i32,
        mut target_speed: f32,
    ) -> f32 {
        if !self.overtake_enabled || traffic.is_empty() {
            self.lateral_target = 0.0;
            return target_speed;
        }
        let (tx, tz) = self.tangent_at(nearest_idx);
        let (nx, nz) = (-tz, tx);
        let mut closest_ahead = 1e9f32;
        let mut side_bias = 0.0f32;
        let mut blocked = false;
        for o in traffic {
            if o.id == self_id {
                continue;
            }
            let dx = o.x - car_pos.x;
            let dz = o.z - car_pos.z;
            let along = dx * tx + dz * tz;
            let lat = dx * nx + dz * nz;
            if along > 2.0 && along < 45.0 && lat.abs() < 6.0 {
                if along < closest_ahead {
                    closest_ahead = along;
                    side_bias = lat;
                }
                if along < 25.0 && speed - o.speed > -5.0 {
                    blocked = true;
                }
                if along < 12.0 && o.speed < speed {
                    target_speed = target_speed.min(o.speed * 0.95);
                }
            }
        }
        if blocked && closest_ahead < 35.0 {
            if side_bias.abs() > 0.5 {
                self.overtake_side = if side_bias > 0.0 { -1.0 } else { 1.0 };
            }
            self.lateral_target = self.overtake_side * (2.0 + 1.5 * self.aggression);
            if self.aggression > 0.4 && closest_ahead > 8.0 {
                target_speed *= 1.0 + 0.08 * self.aggression;
            }
        } else {
            self.lateral_target = 0.0;
        }
        target_speed
    }

    fn find_nearest_point(&self, pos: Vec3) -> usize {
        let mut best_idx = 0;
        let mut best_dist = f32::MAX;
        for (i, p) in self.spline.points.iter().enumerate() {
            let dx = p.position.x - pos.x;
            let dy = p.position.y - pos.y;
            let dz = p.position.z - pos.z;
            let d = dx * dx + dy * dy + dz * dz;
            if d < best_dist {
                best_dist = d;
                best_idx = i;
            }
        }
        best_idx
    }

    fn find_lookahead_point(&self, nearest_idx: usize, dist: f32) -> usize {
        if self.spline.points.is_empty() {
            return 0;
        }
        let target = self.cumulative[nearest_idx] + dist;
        for (i, d) in self.cumulative.iter().enumerate().skip(nearest_idx) {
            if *d >= target {
                return i;
            }
        }
        self.spline.points.len() - 1
    }

    fn calculate_steering(&self, car_pos: Vec3, car_heading: f32, target_pos: Vec3) -> f32 {
        let dx = target_pos.x - car_pos.x;
        let dz = target_pos.z - car_pos.z;
        let err = wrap_pi(dx.atan2(dz) - car_heading);
        // Negato: la risposta di imbardata del VehicleSimulator è opposta
        // all'input di sterzo, quindi chiudere l'anello su err richiede il
        // segno invertito.
        -(err * 0.6).clamp(-1.0, 1.0)
    }

    fn calculate_throttle(current_speed: f32, target_speed: f32) -> f32 {
        let err = target_speed - current_speed;
        if err > 2.0 {
            1.0
        } else if err > 0.5 {
            0.45
        } else if err > -0.5 {
            0.1
        } else {
            0.0
        }
    }

    fn calculate_brake(current_speed: f32, target_speed: f32) -> f32 {
        let err = current_speed - target_speed;
        if err <= 1.0 {
            0.0
        } else {
            (err * 0.1).clamp(0.0, 1.0)
        }
    }

    fn calculate_gear(speed: f32) -> i32 {
        if speed < 10.0 {
            1
        } else if speed < 20.0 {
            2
        } else if speed < 30.0 {
            3
        } else if speed < 40.0 {
            4
        } else if speed < 50.0 {
            5
        } else {
            6
        }
    }
}

/// Modello cinematico minimo che sostituisce `VehicleSimulator::updatePhysics`
/// per le auto AI del manager. Non è la fisica di gara: è solo una piantina
/// stabile che permette all'anello AI di chiudersi e di essere testato.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CarKinematics {
    pub max_accel: f32,
    pub max_brake: f32,
    pub drag: f32,
    pub steer_rate: f32,
}

impl Default for CarKinematics {
    fn default() -> Self {
        Self {
            max_accel: 8.0,
            max_brake: 14.0,
            drag: 0.02,
            steer_rate: 0.9,
        }
    }
}

/// Posizione/assetto di un'auto gestita dal manager.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CarPose {
    pub position: Vec3,
    pub heading: f32,
    pub speed: f32,
}

/// Una vettura registrata nel manager (port di `CarEntry` senza il
/// `VehicleSimulator`, che resta all'applicazione).
#[derive(Clone, Debug, Default)]
pub struct CarEntry {
    pub id: i32,
    pub client_index: Option<i32>,
    pub car_name: String,
    pub driver_name: String,
    pub is_player: bool,
    pub is_active: bool,
    pub externally_driven: bool,
    pub ai: Option<AiController>,
    pub kinematics: CarKinematics,
    pub pose: CarPose,
    pub color: [f32; 3],
    pub throttle: f32,
    pub brake: f32,
    pub steering: f32,
    pub gear: i32,
}

impl CarEntry {
    fn new(id: i32, car_name: String, driver_name: String, is_player: bool) -> Self {
        Self {
            id,
            is_player,
            is_active: true,
            car_name,
            driver_name,
            color: [1.0, 1.0, 1.0],
            gear: 1,
            kinematics: CarKinematics::default(),
            ..Default::default()
        }
    }

    fn integrate(&mut self, dt: f32) {
        let accel = self.throttle * self.kinematics.max_accel
            - self.brake * self.kinematics.max_brake
            - self.kinematics.drag * self.pose.speed;
        self.pose.speed = (self.pose.speed + accel * dt).max(0.0);

        let grip = (self.pose.speed / (self.pose.speed + 5.0)).clamp(0.0, 1.0);
        self.pose.heading =
            wrap_pi(self.pose.heading - self.steering * self.kinematics.steer_rate * grip * dt);

        let forward = Vec3::new(self.pose.heading.sin(), 0.0, self.pose.heading.cos());
        self.pose.position += forward * self.pose.speed * dt;
    }
}

/// Slot della griglia di partenza (port di `GridCar`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GridCar {
    pub driver_name: String,
    pub car_name: String,
    pub grid_slot: i32,
}

/// Gestore multi-auto (port di `ks::sim::MultiCarManager`).
#[derive(Clone, Debug, Default)]
pub struct MultiCarManager {
    cars: Vec<CarEntry>,
    next_car_id: i32,
    player_car_id: i32,
    ai_speed_factor: f32,
    ai_spline_path: String,
    grid_spline: AiSpline,
    collision_enabled: bool,
    last_collisions: Vec<(i32, i32)>,
}

impl MultiCarManager {
    /// Crea un manager vuoto con speed factor AI 1.0 e collisioni attive.
    pub fn new() -> Self {
        Self {
            next_car_id: 0,
            player_car_id: -1,
            ai_speed_factor: 1.0,
            collision_enabled: true,
            ..Default::default()
        }
    }

    /// Registra un'auto e ritorna il suo id (port di `addCar`).
    pub fn add_car(
        &mut self,
        car_name: &str,
        driver_name: &str,
        start_position: Vec3,
        is_player: bool,
    ) -> i32 {
        let id = self.next_car_id;
        self.next_car_id += 1;

        let mut entry = CarEntry::new(id, car_name.to_string(), driver_name.to_string(), is_player);
        entry.pose = CarPose {
            position: start_position,
            heading: 0.0,
            speed: 0.0,
        };

        if !is_player {
            let mut ai = AiController::new();
            ai.set_speed_factor(self.ai_speed_factor);
            if self.grid_spline.is_valid() {
                ai.load_from_spline(self.grid_spline.clone());
            }
            entry.ai = Some(ai);
        } else {
            self.player_car_id = id;
        }

        self.cars.push(entry);
        id
    }

    /// Rimuove un'auto per id.
    pub fn remove_car(&mut self, car_id: i32) {
        if let Some(pos) = self.cars.iter().position(|c| c.id == car_id) {
            self.cars.remove(pos);
        }
    }

    /// Rimuove tutte le auto e azzera il giocatore.
    pub fn clear_all_cars(&mut self) {
        self.cars.clear();
        self.player_car_id = -1;
    }

    /// Imposta lo speed factor di tutte le AI (port di `setAISpeedFactor`).
    pub fn set_ai_speed_factor(&mut self, factor: f32) {
        self.ai_speed_factor = factor;
        for car in &mut self.cars {
            if let Some(ai) = &mut car.ai {
                ai.set_speed_factor(factor);
            }
        }
    }

    pub fn ai_speed_factor(&self) -> f32 {
        self.ai_speed_factor
    }

    /// Marca un'auto come guidata esternamente (client di rete/giocatore).
    pub fn set_car_externally_driven(&mut self, car_id: i32) -> bool {
        match self.cars.iter_mut().find(|c| c.id == car_id) {
            Some(car) => {
                car.ai = None;
                car.externally_driven = true;
                true
            }
            None => false,
        }
    }

    /// Assegna un client index a un'auto esternamente guidata.
    pub fn set_car_client_index(&mut self, car_id: i32, client_index: i32) -> bool {
        if !self.set_car_externally_driven(car_id) {
            return false;
        }
        if let Some(car) = self.cars.iter_mut().find(|c| c.id == car_id) {
            car.client_index = Some(client_index);
        }
        true
    }

    pub fn car_count(&self) -> usize {
        self.cars.len()
    }

    pub fn cars(&self) -> &[CarEntry] {
        &self.cars
    }

    pub fn get_car(&self, id: i32) -> Option<&CarEntry> {
        self.cars.iter().find(|c| c.id == id)
    }

    pub fn get_car_mut(&mut self, id: i32) -> Option<&mut CarEntry> {
        self.cars.iter_mut().find(|c| c.id == id)
    }

    pub fn get_car_by_client_index(&self, client_index: i32) -> Option<&CarEntry> {
        self.cars
            .iter()
            .find(|c| c.client_index == Some(client_index))
    }

    pub fn player_car(&self) -> Option<&CarEntry> {
        self.get_car(self.player_car_id)
    }

    pub fn player_car_id(&self) -> i32 {
        self.player_car_id
    }

    /// Collisioni rilevate durante l'ultimo [`Self::update`].
    pub fn last_collisions(&self) -> &[(i32, i32)] {
        &self.last_collisions
    }

    pub fn collision_enabled(&self) -> bool {
        self.collision_enabled
    }

    pub fn set_collision_enabled(&mut self, enabled: bool) {
        self.collision_enabled = enabled;
    }

    /// Carica la linea AI di griglia da una [`AiSpline`] e la propaga alle AI.
    pub fn load_ai_spline(&mut self, spline: AiSpline) {
        if !spline.is_valid() {
            return;
        }
        for car in &mut self.cars {
            if let Some(ai) = &mut car.ai {
                ai.load_from_spline(spline.clone());
            }
        }
        self.grid_spline = spline;
    }

    /// Carica `<trackDirectory>/ai/fast_lane.ai` (port di `loadAiSpline`).
    pub fn load_ai_spline_from_dir(&mut self, track_directory: &str) -> bool {
        self.ai_spline_path = track_directory.to_string();
        let path = format!("{track_directory}/ai/fast_lane.ai");
        let mut spline = read_spline(&path);
        if !spline.is_valid() {
            spline = read_spline(format!("{path}.txt"));
        }
        if !spline.is_valid() {
            return false;
        }
        self.load_ai_spline(spline);
        true
    }

    pub fn ai_spline_path(&self) -> &str {
        &self.ai_spline_path
    }

    /// Posizione e heading dello slot di griglia (port di `gridPose`).
    /// Ritorna `None` senza una linea valida.
    pub fn grid_pose(&self, slot: i32, spacing: f32) -> Option<(Vec3, f32)> {
        let pts = &self.grid_spline.points;
        let total = self.grid_spline.total_distance;
        if slot < 0 || !self.grid_spline.is_valid() || total <= 0.0 || pts.len() < 2 {
            return None;
        }

        let back = (slot / 2) as f32 * spacing;
        let d = if self.grid_spline.closed {
            let m = (total - back) % total;
            if m < 0.0 {
                m + total
            } else {
                m
            }
        } else {
            back.min(total * 0.999)
        };

        let hi = pts
            .partition_point(|p| p.distance < d)
            .min(pts.len() - 1)
            .max(1);
        let lo = hi - 1;
        let seg_len = pts[hi].distance - pts[lo].distance;
        let t = if seg_len > 1e-4 {
            ((d - pts[lo].distance) / seg_len).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let a = pts[lo].position;
        let b = pts[hi].position;
        let mut pos = Vec3::new(
            a.x + (b.x - a.x) * t,
            a.y + (b.y - a.y) * t,
            a.z + (b.z - a.z) * t,
        );

        let dx = b.x - a.x;
        let dz = b.z - a.z;
        let heading = if dx.abs() + dz.abs() > 1e-5 {
            dx.atan2(dz)
        } else {
            0.0
        };
        let lateral = if slot % 2 == 0 { 1.5 } else { -1.5 };
        pos.x += -heading.cos() * lateral;
        pos.z += heading.sin() * lateral;

        Some((pos, heading))
    }

    /// Schiera una griglia da una lista di auto (port di `spawnGrid`).
    /// Senza linea valida non schiera nulla.
    pub fn spawn_grid_field(&mut self, field: &[GridCar], spacing: f32) -> Vec<i32> {
        let mut ids = Vec::new();
        for gc in field {
            let Some((pos, heading)) = self.grid_pose(gc.grid_slot, spacing) else {
                break;
            };
            let id = self.add_car(&gc.car_name, &gc.driver_name, pos, false);
            if let Some(car) = self.get_car_mut(id) {
                car.pose.position = pos;
                car.pose.heading = heading;
            }
            ids.push(id);
        }
        ids
    }

    /// Schiera `count` auto con nomi `{driver_prefix}{i+1}` (port di `spawnGrid`).
    pub fn spawn_grid(
        &mut self,
        count: i32,
        car_name: &str,
        driver_prefix: &str,
        spacing: f32,
    ) -> Vec<i32> {
        let field: Vec<GridCar> = (0..count.max(0))
            .map(|i| GridCar {
                driver_name: format!("{driver_prefix}{}", i + 1),
                car_name: car_name.to_string(),
                grid_slot: i,
            })
            .collect();
        self.spawn_grid_field(&field, spacing)
    }

    /// Un passo: raccoglie il traffico, fa girare le AI, integra le pose e
    /// rileva le collisioni (port di `update`).
    pub fn update(&mut self, dt: f32) {
        let traffic: Vec<AiTrafficCar> = self
            .cars
            .iter()
            .filter(|c| c.is_active)
            .map(|c| AiTrafficCar {
                id: c.id,
                x: c.pose.position.x,
                z: c.pose.position.z,
                heading: c.pose.heading,
                speed: c.pose.speed,
                along_track: 0.0,
            })
            .collect();

        for car in &mut self.cars {
            if !car.is_active {
                continue;
            }

            if !car.is_player && !car.externally_driven {
                if let Some(ai) = &mut car.ai {
                    if ai.is_ready() {
                        let input = AiInput {
                            position: car.pose.position,
                            heading: car.pose.heading,
                            speed: car.pose.speed,
                            gear: car.gear,
                        };
                        ai.update_with_traffic(input, dt, &traffic, car.id);
                        car.throttle = ai.throttle();
                        car.brake = ai.brake();
                        car.steering = ai.steering();
                        car.gear = ai.target_gear();
                    }
                }
            }

            if !car.externally_driven {
                car.integrate(dt);
            }
        }

        self.check_collisions();
    }

    fn check_collisions(&mut self) {
        self.last_collisions.clear();
        if !self.collision_enabled {
            return;
        }
        for i in 0..self.cars.len() {
            if !self.cars[i].is_active {
                continue;
            }
            for j in (i + 1)..self.cars.len() {
                if !self.cars[j].is_active {
                    continue;
                }
                let dist = self.cars[i]
                    .pose
                    .position
                    .distance(self.cars[j].pose.position);
                if dist < 2.0 && dist > 0.01 {
                    self.last_collisions
                        .push((self.cars[i].id, self.cars[j].id));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::ai_spline::{AiSplinePoint, AiVec3};

    fn straight_spline(n: usize, step: f32, speed: f32) -> AiSpline {
        let mut spline = AiSpline {
            points: (0..n)
                .map(|i| AiSplinePoint {
                    position: AiVec3::new(0.0, 0.0, i as f32 * step),
                    curvature: 0.0,
                    speed,
                    distance: i as f32 * step,
                    lap: 0,
                })
                .collect(),
            total_distance: (n - 1) as f32 * step,
            closed: false,
        };
        spline.total_distance = (n - 1) as f32 * step;
        spline
    }

    fn ring_spline(n: usize, radius: f32) -> AiSpline {
        let mut points: Vec<AiSplinePoint> = Vec::with_capacity(n);
        let mut cum = 0.0f32;
        for i in 0..n {
            let a = std::f32::consts::TAU * i as f32 / n as f32;
            let pos = AiVec3::new(radius * a.sin(), 0.0, radius * a.cos());
            if i > 0 {
                cum += points[i - 1].position.distance_to(pos);
            }
            points.push(AiSplinePoint {
                position: pos,
                curvature: 1.0 / radius,
                speed: 40.0,
                distance: cum,
                lap: 0,
            });
        }
        AiSpline {
            total_distance: cum,
            closed: true,
            points,
        }
    }

    #[test]
    fn empty_controller_is_not_ready_and_brakes() {
        let mut ai = AiController::new();
        assert!(!ai.is_ready());
        ai.update(AiInput::default(), 0.016);
        assert_eq!(ai.throttle(), 0.0);
        assert_eq!(ai.brake(), 1.0);
        assert_eq!(ai.steering(), 0.0);
    }

    #[test]
    fn load_from_spline_marks_ready_and_computes_cumulative() {
        let mut ai = AiController::new();
        assert!(ai.load_from_spline(straight_spline(11, 10.0, 30.0)));
        assert!(ai.is_ready());
        assert_eq!(ai.spline_length(), 100.0);
        assert!(!ai.load_from_spline(AiSpline::default()));
    }

    #[test]
    fn on_line_straight_follows_at_target_speed() {
        let mut ai = AiController::new();
        ai.load_from_spline(straight_spline(11, 10.0, 30.0));
        // Car on the line at origin, stationary: full throttle to reach 30.
        ai.update(
            AiInput {
                position: Vec3::new(0.0, 0.0, 0.0),
                heading: 0.0,
                speed: 0.0,
                gear: 1,
            },
            0.016,
        );
        assert_eq!(ai.throttle(), 1.0);
        assert_eq!(ai.brake(), 0.0);
        assert!(ai.steering().abs() < 1e-3);
        assert_eq!(ai.nearest_index(), 0);
    }

    #[test]
    fn steering_sign_corrects_lateral_offset() {
        let mut ai = AiController::new();
        ai.load_from_spline(straight_spline(21, 5.0, 30.0));
        // Car sits to the +x side of a +z line; target ahead is at x = 0.
        ai.update(
            AiInput {
                position: Vec3::new(3.0, 0.0, 0.0),
                heading: 0.0,
                speed: 20.0,
                gear: 2,
            },
            0.016,
        );
        // err = atan2(target.x - car.x, ...) is negative (target towards -x),
        // steering = -clamp(err*0.6) is therefore positive.
        assert!(ai.steering() > 0.0);
    }

    #[test]
    fn brakes_when_faster_than_target() {
        let mut ai = AiController::new();
        ai.load_from_spline(straight_spline(11, 10.0, 20.0));
        ai.update(
            AiInput {
                position: Vec3::new(0.0, 0.0, 0.0),
                heading: 0.0,
                speed: 60.0,
                gear: 5,
            },
            0.016,
        );
        assert!(ai.brake() > 0.0);
        assert_eq!(ai.throttle(), 0.0);
    }

    #[test]
    fn gear_map_matches_speed_bands() {
        let mut ai = AiController::new();
        ai.load_from_spline(straight_spline(11, 10.0, 30.0));
        let gear_at = |ai: &mut AiController, speed: f32| {
            ai.update(
                AiInput {
                    position: Vec3::ZERO,
                    heading: 0.0,
                    speed,
                    gear: 1,
                },
                0.016,
            );
            ai.target_gear()
        };
        assert_eq!(gear_at(&mut ai, 5.0), 1);
        assert_eq!(gear_at(&mut ai, 25.0), 3);
        assert_eq!(gear_at(&mut ai, 45.0), 5);
        assert_eq!(gear_at(&mut ai, 60.0), 6);
    }

    #[test]
    fn lap_counted_only_after_covering_half_the_track() {
        let ring = ring_spline(8, 50.0);
        let total = ring.total_distance;
        let positions: Vec<Vec3> = ring
            .points
            .iter()
            .map(|p| Vec3::new(p.position.x, p.position.y, p.position.z))
            .collect();
        let pos = |i: usize| positions[i];
        let mut ai = AiController::new();
        ai.load_from_spline(ring);
        assert_eq!(ai.lap_count(), 0);

        // Start at index 0 (progress 0).
        ai.update(
            AiInput {
                position: pos(0),
                heading: 0.0,
                speed: 30.0,
                gear: 3,
            },
            0.016,
        );
        // Jump to index 6 => progress 0.75 * total (travelled > half).
        ai.update(
            AiInput {
                position: pos(6),
                heading: 0.0,
                speed: 30.0,
                gear: 3,
            },
            0.016,
        );
        assert_eq!(ai.lap_count(), 0);
        // Back across the line => lap completed.
        ai.update(
            AiInput {
                position: pos(0),
                heading: 0.0,
                speed: 30.0,
                gear: 3,
            },
            0.016,
        );
        assert_eq!(ai.lap_count(), 1);
        assert!(total > 0.0);
    }

    #[test]
    fn short_hop_over_line_does_not_count_a_lap() {
        let ring = ring_spline(8, 50.0);
        let positions: Vec<Vec3> = ring
            .points
            .iter()
            .map(|p| Vec3::new(p.position.x, p.position.y, p.position.z))
            .collect();
        let pos = |i: usize| positions[i];
        let mut ai = AiController::new();
        ai.load_from_spline(ring);
        // Spawn just behind the line (index 7) then cross to index 0.
        ai.update(
            AiInput {
                position: pos(7),
                heading: 0.0,
                speed: 30.0,
                gear: 3,
            },
            0.016,
        );
        ai.update(
            AiInput {
                position: pos(0),
                heading: 0.0,
                speed: 30.0,
                gear: 3,
            },
            0.016,
        );
        assert_eq!(ai.lap_count(), 0);
    }

    #[test]
    fn traffic_ahead_triggers_overtake_and_slows() {
        let mut ai = AiController::new();
        ai.load_from_spline(straight_spline(31, 5.0, 40.0));
        let traffic = [AiTrafficCar {
            id: 2,
            x: 0.0,
            z: 10.0,
            heading: 0.0,
            speed: 10.0,
            along_track: 10.0,
        }];
        ai.update_with_traffic(
            AiInput {
                position: Vec3::ZERO,
                heading: 0.0,
                speed: 30.0,
                gear: 3,
            },
            0.05,
            &traffic,
            1,
        );
        // Blocked: lateral target set, offset ramps up.
        assert!(ai.lateral_offset() > 0.0);
        // Blocked car caps the target speed and lifts throttle/brakes.
        assert!(ai.throttle() < 1.0);
    }

    #[test]
    fn overtake_disabled_keeps_offset_zero() {
        let mut ai = AiController::new();
        ai.load_from_spline(straight_spline(31, 5.0, 40.0));
        ai.set_overtake_enabled(false);
        let traffic = [AiTrafficCar {
            id: 2,
            x: 0.0,
            z: 10.0,
            heading: 0.0,
            speed: 10.0,
            along_track: 10.0,
        }];
        ai.update_with_traffic(
            AiInput {
                position: Vec3::ZERO,
                heading: 0.0,
                speed: 30.0,
                gear: 3,
            },
            0.05,
            &traffic,
            1,
        );
        assert_eq!(ai.lateral_offset(), 0.0);
    }

    #[test]
    fn add_remove_and_player_tracking() {
        let mut mgr = MultiCarManager::new();
        let player = mgr.add_car("r500", "You", Vec3::ZERO, true);
        let bot = mgr.add_car("r500", "Bot", Vec3::new(0.0, 0.0, 5.0), false);
        assert_eq!(mgr.car_count(), 2);
        assert_eq!(mgr.player_car_id(), player);
        assert_eq!(mgr.player_car().unwrap().driver_name, "You");
        assert!(mgr.get_car(bot).unwrap().ai.is_some());
        assert!(mgr.get_car(player).unwrap().ai.is_none());

        mgr.remove_car(bot);
        assert_eq!(mgr.car_count(), 1);
        mgr.clear_all_cars();
        assert_eq!(mgr.car_count(), 0);
        assert_eq!(mgr.player_car_id(), -1);
    }

    #[test]
    fn client_index_marks_car_externally_driven() {
        let mut mgr = MultiCarManager::new();
        mgr.load_ai_spline(straight_spline(11, 10.0, 30.0));
        let id = mgr.add_car("r500", "Remote", Vec3::ZERO, false);
        assert!(mgr.get_car(id).unwrap().ai.is_some());
        assert!(mgr.set_car_client_index(id, 7));
        let car = mgr.get_car(id).unwrap();
        assert!(car.externally_driven);
        assert!(car.ai.is_none());
        assert_eq!(mgr.get_car_by_client_index(7).unwrap().id, id);
    }

    #[test]
    fn grid_pose_is_two_wide_and_spaced() {
        let mut mgr = MultiCarManager::new();
        mgr.load_ai_spline(straight_spline(11, 10.0, 30.0));

        let (p0, h0) = mgr.grid_pose(0, 8.0).unwrap();
        let (p1, _) = mgr.grid_pose(1, 8.0).unwrap();
        let (p2, _) = mgr.grid_pose(2, 8.0).unwrap();

        assert!((h0).abs() < 1e-4);
        // Front row staggered left/right around the line.
        assert!((p0.x - -1.5).abs() < 1e-3 && p0.z.abs() < 1e-3);
        assert!((p1.x - 1.5).abs() < 1e-3 && p1.z.abs() < 1e-3);
        // Second row sits one spacing back.
        assert!((p2.z - 8.0).abs() < 1e-3);
    }

    #[test]
    fn grid_pose_needs_a_spline() {
        let mgr = MultiCarManager::new();
        assert!(mgr.grid_pose(0, 8.0).is_none());
        assert!(mgr.grid_pose(-1, 8.0).is_none());
    }

    #[test]
    fn spawn_grid_creates_numbered_drivers() {
        let mut mgr = MultiCarManager::new();
        mgr.load_ai_spline(straight_spline(21, 10.0, 30.0));
        let ids = mgr.spawn_grid(4, "r500", "Bot ", 8.0);
        assert_eq!(ids.len(), 4);
        assert_eq!(mgr.car_count(), 4);
        assert_eq!(mgr.get_car(ids[0]).unwrap().driver_name, "Bot 1");
        assert_eq!(mgr.get_car(ids[3]).unwrap().driver_name, "Bot 4");
        // All spawned cars are AI.
        assert!(mgr.cars().iter().all(|c| c.ai.is_some()));
    }

    #[test]
    fn spawn_grid_without_spline_spawns_nothing() {
        let mut mgr = MultiCarManager::new();
        assert!(mgr.spawn_grid(4, "r500", "Bot ", 8.0).is_empty());
        assert_eq!(mgr.car_count(), 0);
    }

    #[test]
    fn collisions_reported_within_two_metres() {
        let mut mgr = MultiCarManager::new();
        mgr.load_ai_spline(straight_spline(11, 10.0, 30.0));
        let a = mgr.add_car("r500", "A", Vec3::new(0.0, 0.0, 0.0), false);
        let b = mgr.add_car("r500", "B", Vec3::new(1.0, 0.0, 0.0), false);
        mgr.update(0.0);
        assert_eq!(mgr.last_collisions(), &[(a, b)]);

        mgr.set_collision_enabled(false);
        mgr.update(0.0);
        assert!(mgr.last_collisions().is_empty());
    }

    #[test]
    fn set_ai_speed_factor_propagates_to_cars() {
        let mut mgr = MultiCarManager::new();
        mgr.load_ai_spline(straight_spline(11, 10.0, 30.0));
        let id = mgr.add_car("r500", "Bot", Vec3::ZERO, false);
        mgr.set_ai_speed_factor(0.85);
        assert_eq!(mgr.ai_speed_factor(), 0.85);
        assert_eq!(
            mgr.get_car(id).unwrap().ai.as_ref().unwrap().speed_factor(),
            0.85
        );
    }

    #[test]
    fn externally_driven_car_is_not_integrated() {
        let mut mgr = MultiCarManager::new();
        mgr.load_ai_spline(straight_spline(31, 5.0, 40.0));
        let id = mgr.add_car("r500", "Remote", Vec3::ZERO, false);
        mgr.set_car_externally_driven(id);
        mgr.update(0.5);
        // No AI, no integration: pose untouched at the start position.
        assert_eq!(mgr.get_car(id).unwrap().pose.position, Vec3::ZERO);
    }
}
