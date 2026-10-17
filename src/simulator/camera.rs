//! Telecamere: cockpit, chase e libera.
//!
//! Port di `src/simulator/CameraController.{h,cpp}` di ksengine.

use bevy::math::{Mat4, Vec3, Vec4};

/// Matrice di vista right-handed (equivalente a `Mat4::look_at_rh`).
fn look_at_rh(eye: Vec3, center: Vec3, up: Vec3) -> Mat4 {
    let f = (center - eye).normalize_or_zero();
    let s = f.cross(up).normalize_or_zero();
    let u = s.cross(f);
    Mat4::from_cols(
        Vec4::new(s.x, u.x, -f.x, 0.0),
        Vec4::new(s.y, u.y, -f.y, 0.0),
        Vec4::new(s.z, u.z, -f.z, 0.0),
        Vec4::new(-s.dot(eye), -u.dot(eye), f.dot(eye), 1.0),
    )
}

/// Proiezione prospettica right-handed, profondità `0..=1` (equivalente a
/// `Mat4::perspective_rh`).
fn perspective_rh(fov_y_radians: f32, aspect_ratio: f32, z_near: f32, z_far: f32) -> Mat4 {
    let f = 1.0 / (fov_y_radians * 0.5).tan();
    Mat4::from_cols(
        Vec4::new(f / aspect_ratio, 0.0, 0.0, 0.0),
        Vec4::new(0.0, f, 0.0, 0.0),
        Vec4::new(0.0, 0.0, z_far / (z_near - z_far), -1.0),
        Vec4::new(0.0, 0.0, (z_near * z_far) / (z_near - z_far), 0.0),
    )
}

/// Modalità della telecamera (port di `CameraController::Mode`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CameraMode {
    /// Dall'abitacolo.
    Cockpit,
    /// Inseguimento.
    #[default]
    Chase,
    /// Libera.
    Free,
}

/// Controller della telecamera (port di `CameraController`).
#[derive(Clone, Debug)]
pub struct CameraController {
    mode: CameraMode,

    view_matrix: Mat4,
    projection_matrix: Mat4,

    position: Vec3,
    forward: Vec3,
    up: Vec3,

    aspect_ratio: f32,
    fov: f32,
    near_plane: f32,
    far_plane: f32,

    chase_distance: f32,
    chase_height: f32,
    chase_lag: f32,
    chase_current_pos: Vec3,
    chase_initialized: bool,

    free_yaw: f32,
    free_pitch: f32,
    free_speed: f32,

    cockpit_offset: Vec3,
    cockpit_look_target: Vec3,

    head_bob_phase: f32,
    head_bob_amplitude: f32,
}

impl Default for CameraController {
    fn default() -> Self {
        Self::new()
    }
}

impl CameraController {
    /// Una telecamera con i valori predefiniti (port del costruttore).
    pub fn new() -> Self {
        Self {
            mode: CameraMode::Chase,
            view_matrix: Mat4::IDENTITY,
            projection_matrix: Mat4::IDENTITY,
            position: Vec3::ZERO,
            forward: Vec3::NEG_Z,
            up: Vec3::Y,
            aspect_ratio: 16.0 / 9.0,
            fov: 60.0,
            near_plane: 0.1,
            far_plane: 10000.0,
            chase_distance: 6.0,
            chase_height: 2.5,
            chase_lag: 5.0,
            chase_current_pos: Vec3::ZERO,
            chase_initialized: false,
            free_yaw: 0.0,
            free_pitch: 0.0,
            free_speed: 20.0,
            cockpit_offset: Vec3::new(0.0, 1.2, -0.3),
            cockpit_look_target: Vec3::new(0.0, 1.0, 10.0),
            head_bob_phase: 0.0,
            head_bob_amplitude: 0.003,
        }
    }

    /// Imposta la modalità.
    pub fn set_mode(&mut self, mode: CameraMode) {
        self.mode = mode;
    }

    /// La modalità corrente.
    pub fn mode(&self) -> CameraMode {
        self.mode
    }

    /// Aggiorna la telecamera (port di `update`).
    pub fn update(&mut self, dt: f32, car_body_matrix: Mat4, speed_kmh: f32) {
        match self.mode {
            CameraMode::Cockpit => self.update_cockpit(car_body_matrix, speed_kmh),
            CameraMode::Chase => self.update_chase(car_body_matrix, speed_kmh, dt),
            CameraMode::Free => self.update_free(dt),
        }
        self.update_projection();
    }

    /// La matrice di vista.
    pub fn view_matrix(&self) -> Mat4 {
        self.view_matrix
    }

    /// La matrice di proiezione.
    pub fn projection_matrix(&self) -> Mat4 {
        self.projection_matrix
    }

    /// La posizione.
    pub fn position(&self) -> Vec3 {
        self.position
    }

    /// La direzione di vista.
    pub fn forward(&self) -> Vec3 {
        self.forward
    }

    /// Imposta la posizione.
    pub fn set_position(&mut self, pos: Vec3) {
        self.position = pos;
    }

    /// Imposta l'aspect ratio.
    pub fn set_aspect_ratio(&mut self, aspect_ratio: f32) {
        self.aspect_ratio = aspect_ratio;
    }

    /// Imposta il FOV, in gradi.
    pub fn set_fov(&mut self, fov: f32) {
        self.fov = fov;
    }

    /// Imposta il piano vicino.
    pub fn set_near_plane(&mut self, near_plane: f32) {
        self.near_plane = near_plane;
    }

    /// Imposta il piano lontano.
    pub fn set_far_plane(&mut self, far_plane: f32) {
        self.far_plane = far_plane;
    }

    /// Il piano vicino.
    pub fn near_plane(&self) -> f32 {
        self.near_plane
    }

    /// Il piano lontano.
    pub fn far_plane(&self) -> f32 {
        self.far_plane
    }

    /// L'aspect ratio.
    pub fn aspect_ratio(&self) -> f32 {
        self.aspect_ratio
    }

    /// Imposta la distanza di inseguimento.
    pub fn set_chase_distance(&mut self, distance: f32) {
        self.chase_distance = distance;
    }

    /// Imposta l'altezza di inseguimento.
    pub fn set_chase_height(&mut self, height: f32) {
        self.chase_height = height;
    }

    /// Imposta il ritardo di inseguimento.
    pub fn set_chase_lag(&mut self, lag: f32) {
        self.chase_lag = lag;
    }

    /// Muove la telecamera libera (port di `moveFree`).
    pub fn move_free(&mut self, delta: Vec3) {
        let right = self.forward.cross(self.up).normalize_or_zero();
        self.position += self.forward * delta.z + right * delta.x + self.up * delta.y;
    }

    /// Ruota la telecamera libera, in gradi (port di `rotateFree`).
    pub fn rotate_free(&mut self, yaw_delta: f32, pitch_delta: f32) {
        self.free_yaw += yaw_delta;
        self.free_pitch += pitch_delta;
        self.free_pitch = self.free_pitch.clamp(-89.0, 89.0);

        let yaw_rad = self.free_yaw.to_radians();
        let pitch_rad = self.free_pitch.to_radians();

        self.forward = Vec3::new(
            pitch_rad.cos() * yaw_rad.sin(),
            pitch_rad.sin(),
            pitch_rad.cos() * yaw_rad.cos(),
        )
        .normalize_or_zero();
    }

    /// Imposta la velocità di movimento della telecamera libera.
    pub fn set_free_speed(&mut self, speed: f32) {
        self.free_speed = speed;
    }

    /// La velocità di movimento della telecamera libera.
    pub fn free_speed(&self) -> f32 {
        self.free_speed
    }

    /// Imposta l'offset dell'abitacolo.
    pub fn set_cockpit_offset(&mut self, offset: Vec3) {
        self.cockpit_offset = offset;
    }

    /// Imposta il punto di mira dell'abitacolo.
    pub fn set_cockpit_look_target(&mut self, target: Vec3) {
        self.cockpit_look_target = target;
    }

    fn update_cockpit(&mut self, car_matrix: Mat4, speed_kmh: f32) {
        let mut local_pos = self.cockpit_offset;

        let bob_speed = speed_kmh / 3.6;
        self.head_bob_phase += bob_speed * 0.05;
        let bob = self.head_bob_phase.sin() * self.head_bob_amplitude * bob_speed;
        local_pos += Vec3::new(0.0, bob, 0.0);

        self.position = car_matrix.transform_point3(local_pos);

        let world_target = car_matrix.transform_point3(self.cockpit_look_target);

        self.forward = (world_target - self.position).normalize_or_zero();
        self.up = Vec3::Y;

        self.view_matrix = look_at_rh(self.position, self.position + self.forward, self.up);
    }

    fn update_chase(&mut self, car_matrix: Mat4, _speed_kmh: f32, dt: f32) {
        let car_pos = car_matrix.transform_point3(Vec3::ZERO);
        let car_forward = (car_matrix.transform_point3(Vec3::Z) - car_pos).normalize_or_zero();
        let car_up = (car_matrix.transform_point3(Vec3::Y) - car_pos).normalize_or_zero();

        let target_pos = car_pos - car_forward * self.chase_distance + car_up * self.chase_height;

        if !self.chase_initialized {
            self.chase_current_pos = target_pos;
            self.chase_initialized = true;
        }

        let lag_factor = 1.0 - (-self.chase_lag * dt).exp();
        self.chase_current_pos += (target_pos - self.chase_current_pos) * lag_factor;

        self.position = self.chase_current_pos;

        let look_target = car_pos + car_up * 1.0;
        self.forward = (look_target - self.position).normalize_or_zero();
        self.up = Vec3::Y;

        self.view_matrix = look_at_rh(self.position, look_target, self.up);
    }

    fn update_free(&mut self, _dt: f32) {
        self.view_matrix = look_at_rh(self.position, self.position + self.forward, self.up);
    }

    fn update_projection(&mut self) {
        self.projection_matrix = perspective_rh(
            self.fov.to_radians(),
            self.aspect_ratio,
            self.near_plane,
            self.far_plane,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::Quat;

    fn translate(x: f32, y: f32, z: f32) -> Mat4 {
        Mat4::from_translation(Vec3::new(x, y, z))
    }

    #[test]
    fn default_is_chase() {
        assert_eq!(CameraController::new().mode(), CameraMode::Chase);
    }

    #[test]
    fn chase_camera_sits_behind_the_car() {
        let mut cam = CameraController::new();
        // Car at origin facing +Z; camera should be at -Z (behind), raised.
        cam.update(0.016, translate(0.0, 0.0, 0.0), 0.0);
        let pos = cam.position();
        assert!(pos.z < 0.0, "pos = {pos:?}");
        assert!(pos.y > 0.0, "pos = {pos:?}");
        assert!(pos.is_finite());
    }

    #[test]
    fn cockpit_camera_uses_offset() {
        let mut cam = CameraController::new();
        cam.set_mode(CameraMode::Cockpit);
        cam.update(0.016, translate(10.0, 0.0, 5.0), 0.0);
        let pos = cam.position();
        // Offset (0, 1.2, -0.3) applied to a car translated to (10, 0, 5).
        assert!((pos.x - 10.0).abs() < 1e-5);
        assert!((pos.y - 1.2).abs() < 1e-5);
        assert!((pos.z - 4.7).abs() < 1e-5);
    }

    #[test]
    fn cockpit_camera_follows_car_rotation() {
        let mut cam = CameraController::new();
        cam.set_mode(CameraMode::Cockpit);
        let rotated = Mat4::from_rotation_translation(
            Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
            Vec3::ZERO,
        );
        cam.update(0.016, rotated, 0.0);
        // Looking target (0,1,10) rotated +90° about Y points along +X.
        assert!(cam.forward().x > 0.9, "forward = {:?}", cam.forward());
    }

    #[test]
    fn rotate_free_clamps_pitch() {
        let mut cam = CameraController::new();
        cam.set_mode(CameraMode::Free);
        cam.rotate_free(0.0, 1000.0);
        // Forward must stay finite and upward-ish, not flip over.
        assert!(cam.forward().is_finite());
        assert!(cam.forward().y <= 1.0 + 1e-5);
    }

    #[test]
    fn rotate_free_yaw_then_move() {
        let mut cam = CameraController::new();
        cam.set_mode(CameraMode::Free);
        cam.rotate_free(0.0, 0.0);
        cam.set_position(Vec3::ZERO);
        cam.move_free(Vec3::new(0.0, 0.0, 5.0));
        assert!(cam.position().is_finite());
        assert!(cam.position().length() > 0.0);
    }

    #[test]
    fn projection_uses_configured_planes() {
        let mut cam = CameraController::new();
        cam.set_near_plane(0.5);
        cam.set_far_plane(500.0);
        cam.set_aspect_ratio(2.0);
        cam.set_fov(90.0);
        cam.update(0.016, Mat4::IDENTITY, 0.0);
        assert_eq!(cam.near_plane(), 0.5);
        assert_eq!(cam.far_plane(), 500.0);
        assert_eq!(cam.aspect_ratio(), 2.0);
        assert!(cam.projection_matrix().is_finite());
    }

    #[test]
    fn chase_position_is_finite_after_many_steps() {
        let mut cam = CameraController::new();
        for i in 0..100 {
            let z = i as f32 * 0.5;
            cam.update(0.016, translate(0.0, 0.0, z), 100.0);
        }
        assert!(cam.position().is_finite());
        assert!(cam.view_matrix().is_finite());
    }
}
