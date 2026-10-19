//! Shared-memory publisher for external telemetry apps (port of ksengine's
//! shared-memory adapter; see `docs/KSENGINE_PORT_BACKLOG.md` for the source
//! map).
//!
//! Third-party telemetry tools (live timers, dashboards…) open three named
//! shared-memory pages and read a fixed binary layout:
//!
//! * Windows: `Local\acpmf_physics`, `Local\acpmf_graphics`, `Local\acpmf_static`
//! * POSIX:   `/acpmf_physics`,    `/acpmf_graphics`,    `/acpmf_static`
//!
//! The mapping names and the 592-byte floor are part of that external wire
//! format and are kept verbatim so those tools keep working. Every mapping is
//! created at least that big, so readers of the 592-byte prefix stay
//! compatible, while larger pages (our static page) keep their full size.
//!
//! The publisher always keeps an in-process mirror, so behaviour is identical
//! (and testable) even when no OS mapping can be created.
//!
//! ```
//! use racingedge::net::shared_memory::{LiveInput, SharedMemoryPublisher};
//!
//! let mut publisher = SharedMemoryPublisher::new();
//! publisher.publish(&LiveInput {
//!     throttle: 0.8,
//!     gear: 3,
//!     ..LiveInput::default()
//! });
//! assert_eq!(publisher.physics().gas, 0.8);
//! assert_eq!(publisher.physics().gear, 4);
//! ```

/// Every `acpmf_*` mapping is created at least this big (`0x250`), the floor the
/// external tools expect.
pub const SHM_MAPPING_BYTES: usize = 0x250;

/// The mapping size for a page: the fixed 592-byte floor, or the page size when
/// the page is larger.
#[must_use]
pub fn shm_mapping_size(page_bytes: usize) -> usize {
    if page_bytes < SHM_MAPPING_BYTES {
        SHM_MAPPING_BYTES
    } else {
        page_bytes
    }
}

#[cfg(windows)]
mod platform {
    use core::ffi::c_void;

    type Handle = *mut c_void;

    const PAGE_READWRITE: u32 = 0x04;
    const FILE_MAP_ALL_ACCESS: u32 = 0x000F_001F;
    const INVALID_HANDLE_VALUE: Handle = -1isize as Handle;

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateFileMappingW(
            h_file: Handle,
            attrs: *mut c_void,
            protect: u32,
            max_high: u32,
            max_low: u32,
            name: *const u16,
        ) -> Handle;
        fn MapViewOfFile(
            map: Handle,
            access: u32,
            off_high: u32,
            off_low: u32,
            bytes: usize,
        ) -> *mut c_void;
        fn UnmapViewOfFile(base: *const c_void) -> i32;
        fn CloseHandle(handle: Handle) -> i32;
    }

    /// A live file mapping (`Local\<base_name>`).
    pub struct Mapping {
        base: *mut c_void,
        handle: Handle,
    }

    impl Mapping {
        pub fn create(base_name: &str, bytes: usize) -> Option<Mapping> {
            let name = format!("Local\\{base_name}");
            let wide: Vec<u16> = name.encode_utf16().chain(core::iter::once(0)).collect();
            let handle = unsafe {
                CreateFileMappingW(
                    INVALID_HANDLE_VALUE,
                    core::ptr::null_mut(),
                    PAGE_READWRITE,
                    0,
                    bytes as u32,
                    wide.as_ptr(),
                )
            };
            if handle.is_null() {
                return None;
            }
            let base = unsafe { MapViewOfFile(handle, FILE_MAP_ALL_ACCESS, 0, 0, bytes) };
            if base.is_null() {
                unsafe {
                    CloseHandle(handle);
                }
                return None;
            }
            // SAFETY: `base` points at `bytes` of freshly mapped writable memory.
            unsafe {
                core::ptr::write_bytes(base as *mut u8, 0, bytes);
            }
            Some(Mapping { base, handle })
        }

        pub fn as_ptr(&self) -> *mut u8 {
            self.base as *mut u8
        }
    }

    impl Drop for Mapping {
        fn drop(&mut self) {
            // SAFETY: base/handle come from CreateFileMappingW + MapViewOfFile.
            unsafe {
                UnmapViewOfFile(self.base as *const c_void);
                CloseHandle(self.handle);
            }
        }
    }
}

#[cfg(unix)]
mod platform {
    use core::ffi::c_void;

    const O_RDWR: i32 = 0o2;
    const O_CREAT: i32 = 0o100;
    const PROT_READ: i32 = 0x1;
    const PROT_WRITE: i32 = 0x2;
    const MAP_SHARED: i32 = 0x01;

    extern "C" {
        fn shm_open(name: *const core::ffi::c_char, oflag: i32, mode: u32) -> i32;
        fn ftruncate(fd: i32, len: i64) -> i32;
        fn mmap(
            addr: *mut c_void,
            len: usize,
            prot: i32,
            flags: i32,
            fd: i32,
            offset: i64,
        ) -> *mut c_void;
        fn munmap(addr: *mut c_void, len: usize) -> i32;
        fn close(fd: i32) -> i32;
    }

    /// A live POSIX shared-memory segment (`/<base_name>`).
    pub struct Mapping {
        base: *mut c_void,
        len: usize,
        fd: i32,
    }

    impl Mapping {
        pub fn create(base_name: &str, bytes: usize) -> Option<Mapping> {
            let name = std::ffi::CString::new(format!("/{base_name}")).ok()?;
            let fd = unsafe { shm_open(name.as_ptr(), O_CREAT | O_RDWR, 0o666) };
            if fd < 0 {
                return None;
            }
            if unsafe { ftruncate(fd, bytes as i64) } != 0 {
                unsafe {
                    close(fd);
                }
                return None;
            }
            let base = unsafe {
                mmap(
                    core::ptr::null_mut(),
                    bytes,
                    PROT_READ | PROT_WRITE,
                    MAP_SHARED,
                    fd,
                    0,
                )
            };
            if base as isize == -1 {
                unsafe {
                    close(fd);
                }
                return None;
            }
            // SAFETY: `base` points at `bytes` of freshly mapped writable memory.
            unsafe {
                core::ptr::write_bytes(base as *mut u8, 0, bytes);
            }
            Some(Mapping {
                base,
                len: bytes,
                fd,
            })
        }

        pub fn as_ptr(&self) -> *mut u8 {
            self.base as *mut u8
        }
    }

    impl Drop for Mapping {
        fn drop(&mut self) {
            // SAFETY: base/len/fd come from shm_open + mmap.
            unsafe {
                munmap(self.base, self.len);
                close(self.fd);
            }
        }
    }
}

#[cfg(not(any(windows, unix)))]
mod platform {
    /// No mapping backend on this platform: the publisher degrades to its soft
    /// in-process mirror.
    pub struct Mapping;

    impl Mapping {
        pub fn create(_base_name: &str, _bytes: usize) -> Option<Mapping> {
            None
        }

        pub fn as_ptr(&self) -> *mut u8 {
            core::ptr::null_mut()
        }
    }
}

/// `SPageFilePhysics` — live, per-frame physics data.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct PhysicsPage {
    pub packet_id: i32,
    pub gas: f32,
    pub brake: f32,
    pub fuel: f32,
    pub gear: i32,
    pub rpms: i32,
    pub steer_angle: f32,
    pub speed_kmh: f32,
    pub velocity: [f32; 3],
    pub acc_g: [f32; 3],
    pub wheel_slip: [f32; 4],
    pub wheel_load: [f32; 4],
    pub wheels_pressure: [f32; 4],
    pub wheel_angular_speed: [f32; 4],
    pub tyre_wear: [f32; 4],
    pub tyre_dirty_level: [f32; 4],
    pub tyre_core_temperature: [f32; 4],
    pub camber_rad: [f32; 4],
    pub suspension_travel: [f32; 4],
    pub drs: f32,
    pub tc: f32,
    pub heading: f32,
    pub pitch: f32,
    pub roll: f32,
    pub cg_height: f32,
    pub car_damage: [f32; 5],
    pub number_of_tyres_out: i32,
    pub pit_limiter_on: i32,
    pub abs: f32,
    pub kers_charge: f32,
    pub kers_input: f32,
    pub auto_shifter_on: i32,
    pub ride_height: [f32; 2],
    pub turbo_boost: f32,
    pub ballast: f32,
    pub air_density: f32,
    pub air_temp: f32,
    pub road_temp: f32,
    pub local_angular_vel: [f32; 3],
    pub final_ff: f32,
    pub performance_meter: f32,
    pub engine_brake: i32,
    pub ers_recovery_level: i32,
    pub ers_power_level: i32,
    pub ers_heat_charging: i32,
    pub ers_is_charging: i32,
    pub kers_current_kj: f32,
    pub drs_available: i32,
    pub drs_enabled: i32,
    pub brake_temp: [f32; 4],
    pub clutch: f32,
    pub tyre_temp_i: [f32; 4],
    pub tyre_temp_m: [f32; 4],
    pub tyre_temp_o: [f32; 4],
    pub is_ai_controlled: i32,
    pub tyre_contact_point: [[f32; 3]; 4],
    pub tyre_contact_normal: [[f32; 3]; 4],
    pub tyre_contact_heading: [[f32; 3]; 4],
    pub brake_bias: f32,
    pub local_velocity: [f32; 3],
}

impl Default for PhysicsPage {
    fn default() -> Self {
        // SAFETY: every field is a plain integer/float; the all-zero bit pattern
        // is a valid value for all of them.
        let mut page: Self = unsafe { core::mem::zeroed() };
        page.air_density = 1.225;
        page.air_temp = 25.0;
        page.road_temp = 30.0;
        page.brake_bias = 0.55;
        page
    }
}

/// `SPageFileGraphics` — session/position/timing data.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct GraphicsPage {
    pub packet_id: i32,
    pub status: i32,
    pub session: i32,
    pub current_time: [u16; 15],
    pub last_time: [u16; 15],
    pub best_time: [u16; 15],
    pub split: [u16; 15],
    pub completed_laps: i32,
    pub position: i32,
    pub i_current_time: i32,
    pub i_last_time: i32,
    pub i_best_time: i32,
    pub session_time_left: f32,
    pub distance_traveled: f32,
    pub is_in_pit: i32,
    pub current_sector_index: i32,
    pub last_sector_time: i32,
    pub number_of_laps: i32,
    pub tyre_compound: [u16; 33],
    pub replay_time_multiplier: f32,
    pub normalized_car_position: f32,
    pub car_coordinates: [f32; 3],
    pub penalty_time: f32,
    pub flag: i32,
    pub ideal_line_on: i32,
    pub is_in_pit_lane: i32,
    pub surface_grip: f32,
    pub mandatory_pit_done: i32,
    pub wind_speed: f32,
    pub wind_direction: f32,
}

impl Default for GraphicsPage {
    fn default() -> Self {
        // SAFETY: every field is a plain integer/float; zero is valid.
        let mut page: Self = unsafe { core::mem::zeroed() };
        page.replay_time_multiplier = 1.0;
        page.surface_grip = 1.0;
        page
    }
}

/// `SPageFileStatic` — identity/config data, written once.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct StaticPage {
    pub sm_version: [u16; 15],
    pub app_version: [u16; 15],
    pub number_of_sessions: i32,
    pub num_cars: i32,
    pub car_model: [u16; 33],
    pub track: [u16; 33],
    pub player_name: [u16; 33],
    pub player_surname: [u16; 33],
    pub player_nick: [u16; 33],
    pub sector_count: i32,
    pub max_torque: f32,
    pub max_power: f32,
    pub max_rpm: i32,
    pub max_fuel: f32,
    pub suspension_max_travel: [f32; 4],
    pub tyre_radius: [f32; 4],
    pub max_turbo_boost: f32,
    pub deprecated_1: f32,
    pub deprecated_2: f32,
    pub penalties_enabled: i32,
    pub aid_fuel_rate: f32,
    pub aid_tire_rate: f32,
    pub aid_mechanical_damage: f32,
    pub aid_allow_tyre_blankets: i32,
    pub aid_stability: f32,
    pub aid_auto_clutch: i32,
    pub aid_auto_blip: i32,
    pub has_drs: i32,
    pub has_ers: i32,
    pub has_kers: i32,
    pub kers_max_j: f32,
    pub engine_brake_settings_count: i32,
    pub ers_power_controller_count: i32,
    pub track_spline_length: f32,
    pub track_configuration: [u16; 33],
    pub ers_max_j: f32,
    pub is_timed_race: i32,
    pub has_extra_lap: i32,
    pub car_id: [u16; 33],
    pub neutral_gear_is_coast_on: i32,
}

impl Default for StaticPage {
    fn default() -> Self {
        // SAFETY: every field is a plain integer/float; zero is valid.
        let mut page: Self = unsafe { core::mem::zeroed() };
        page.num_cars = 1;
        page.sector_count = 3;
        page.aid_fuel_rate = 1.0;
        page.aid_tire_rate = 1.0;
        page.aid_mechanical_damage = 1.0;
        page
    }
}

/// Live snapshot fed to [`SharedMemoryPublisher::publish`].
#[derive(Debug, Clone)]
pub struct LiveInput {
    pub throttle: f32,
    pub brake: f32,
    pub clutch: f32,
    pub steer: f32,
    pub speed_ms: f32,
    pub rpm: f32,
    pub gear: i32,
    pub fuel: f32,
    pub velocity: [f32; 3],
    pub acc_g: [f32; 3],
    pub local_angular_vel: [f32; 3],
    pub heading: f32,
    pub pitch: f32,
    pub roll: f32,
    pub wheel_slip: [f32; 4],
    pub wheel_load: [f32; 4],
    pub tyre_temp: [f32; 4],
    pub tyre_wear: [f32; 4],
    pub tyre_pressure: [f32; 4],
    pub brake_temp: [f32; 4],
    pub suspension_travel: [f32; 4],
    pub camber: [f32; 4],
    pub final_ff: f32,
    pub surface_grip: f32,
    pub air_temp: f32,
    pub road_temp: f32,
    pub air_density: f32,
    pub car_x: f32,
    pub car_y: f32,
    pub car_z: f32,
    pub normalized_spline: f32,
    pub distance_traveled: f32,
    pub completed_laps: i32,
    pub position: i32,
    pub current_sector: i32,
    pub i_current_time_ms: i32,
    pub i_last_time_ms: i32,
    pub i_best_time_ms: i32,
    pub last_sector_time_ms: i32,
    pub session_type: i32,
    pub status: i32,
    pub in_pit: bool,
    pub pit_limiter: bool,
    pub flag: i32,
    pub brake_bias: f32,
    pub car_model: String,
    pub track_name: String,
    pub player_name: String,
    pub tyre_compound: String,
    pub max_rpm: i32,
    pub max_fuel: f32,
    pub sector_count: i32,
    pub total_laps: i32,
    pub session_time_left: f32,
    pub track_spline_length: f32,
    pub car_damage: [f32; 5],
    pub damage_overall: f32,
    pub engine_health: f32,
    pub power_mult: f32,
    pub drag_mult: f32,
    pub downforce_mult: f32,
    pub damage_warning: i32,
    pub engine_seized: bool,
}

impl Default for LiveInput {
    fn default() -> Self {
        Self {
            throttle: 0.0,
            brake: 0.0,
            clutch: 0.0,
            steer: 0.0,
            speed_ms: 0.0,
            rpm: 0.0,
            gear: 1,
            fuel: 50.0,
            velocity: [0.0; 3],
            acc_g: [0.0; 3],
            local_angular_vel: [0.0; 3],
            heading: 0.0,
            pitch: 0.0,
            roll: 0.0,
            wheel_slip: [0.0; 4],
            wheel_load: [3500.0; 4],
            tyre_temp: [80.0; 4],
            tyre_wear: [0.0; 4],
            tyre_pressure: [2.2, 2.2, 2.0, 2.0],
            brake_temp: [200.0; 4],
            suspension_travel: [0.0; 4],
            camber: [0.0; 4],
            final_ff: 0.0,
            surface_grip: 1.0,
            air_temp: 25.0,
            road_temp: 30.0,
            air_density: 1.225,
            car_x: 0.0,
            car_y: 0.0,
            car_z: 0.0,
            normalized_spline: 0.0,
            distance_traveled: 0.0,
            completed_laps: 0,
            position: 1,
            current_sector: 0,
            i_current_time_ms: 0,
            i_last_time_ms: 0,
            i_best_time_ms: 0,
            last_sector_time_ms: 0,
            session_type: 2,
            status: 2,
            in_pit: false,
            pit_limiter: false,
            flag: 0,
            brake_bias: 0.55,
            car_model: String::new(),
            track_name: String::new(),
            player_name: String::new(),
            tyre_compound: "D".to_string(),
            max_rpm: 8500,
            max_fuel: 100.0,
            sector_count: 3,
            total_laps: 0,
            session_time_left: 0.0,
            track_spline_length: 0.0,
            car_damage: [0.0; 5],
            damage_overall: 0.0,
            engine_health: 1.0,
            power_mult: 1.0,
            drag_mult: 1.0,
            downforce_mult: 1.0,
            damage_warning: 0,
            engine_seized: false,
        }
    }
}

/// Maps a `RaceFlag` code to the external flag code the format uses.
///
/// The format has no green, red or white-and-black code, so those map to
/// `FLAG_NONE`; the double yellow, the slippery-surface flag and the safety car
/// all map to `FLAG_YELLOW`; the meatball maps to the penalty flag (closest
/// match).
#[must_use]
pub fn race_flag_to_page_flag(race_flag: i32) -> i32 {
    match race_flag {
        2 => 2,  // Yellow           -> FLAG_YELLOW
        3 => 1,  // Blue             -> FLAG_BLUE
        4 => 4,  // White            -> FLAG_WHITE
        5 => 3,  // Black            -> FLAG_BLACK
        6 => 5,  // Checkered        -> FLAG_CHECKERED
        7 => 6,  // Meatball         -> FLAG_PENALTY
        8 => 2,  // SafetyCar        -> FLAG_YELLOW
        9 => 2,  // DoubleYellow     -> FLAG_YELLOW
        10 => 2, // YellowRedStripes -> FLAG_YELLOW
        11 => 0, // WhiteAndBlack    -> FLAG_NONE
        12 => 0, // Red              -> FLAG_NONE
        _ => 0,  // None/Green       -> FLAG_NONE
    }
}

/// Writes a `M:SS.mmm` time string into a fixed wide buffer, or `--` for a
/// non-positive time (matching the external format's formatting).
pub fn format_telemetry_time(dst: &mut [u16], ms: i32) {
    if dst.len() < 3 {
        return;
    }
    if ms <= 0 {
        dst[0] = u16::from(b'-');
        dst[1] = u16::from(b'-');
        dst[2] = 0;
        return;
    }
    let minutes = ms / 60_000;
    let seconds = (ms / 1_000) % 60;
    let frac = ms % 1_000;
    write_wide(dst, &format!("{minutes}:{seconds:02}.{frac:03}"));
}

/// Copies a UTF-8 string into a fixed wide buffer, truncating and NUL-terminating.
fn write_wide(dst: &mut [u16], src: &str) {
    let bytes = src.as_bytes();
    let mut i = 0;
    while i + 1 < dst.len() && i < bytes.len() {
        dst[i] = u16::from(bytes[i]);
        i += 1;
    }
    if !dst.is_empty() {
        dst[i.min(dst.len() - 1)] = 0;
    }
}

/// Fills the physics page from a live snapshot (port of `fillPhysics`).
pub fn fill_physics(page: &mut PhysicsPage, input: &LiveInput) {
    page.packet_id += 1;
    page.gas = input.throttle;
    page.brake = input.brake;
    page.clutch = input.clutch;
    page.fuel = input.fuel;
    page.gear = if input.gear < 0 {
        0
    } else if input.gear == 0 {
        1
    } else {
        input.gear + 1
    };
    page.rpms = input.rpm as i32;
    page.steer_angle = input.steer;
    page.speed_kmh = input.speed_ms * 3.6;
    for i in 0..3 {
        page.velocity[i] = input.velocity[i];
        page.acc_g[i] = input.acc_g[i];
        page.local_angular_vel[i] = input.local_angular_vel[i];
        page.local_velocity[i] = input.velocity[i];
    }
    for i in 0..4 {
        page.wheel_slip[i] = input.wheel_slip[i];
        page.wheel_load[i] = input.wheel_load[i];
        page.wheels_pressure[i] = input.tyre_pressure[i];
        page.tyre_core_temperature[i] = input.tyre_temp[i];
        page.tyre_temp_i[i] = input.tyre_temp[i];
        page.tyre_temp_m[i] = input.tyre_temp[i];
        page.tyre_temp_o[i] = input.tyre_temp[i];
        page.tyre_wear[i] = input.tyre_wear[i];
        page.brake_temp[i] = input.brake_temp[i];
        page.suspension_travel[i] = input.suspension_travel[i];
        page.camber_rad[i] = input.camber[i];
    }
    page.heading = input.heading;
    page.pitch = input.pitch;
    page.roll = input.roll;
    page.final_ff = input.final_ff;
    page.air_temp = input.air_temp;
    page.road_temp = input.road_temp;
    page.air_density = input.air_density;
    page.pit_limiter_on = i32::from(input.pit_limiter);
    page.brake_bias = input.brake_bias;
    page.car_damage.copy_from_slice(&input.car_damage);
}

/// Fills the graphics page from a live snapshot (port of `fillGraphics`).
pub fn fill_graphics(page: &mut GraphicsPage, input: &LiveInput) {
    page.packet_id += 1;
    page.status = input.status;
    page.session = input.session_type;
    page.completed_laps = input.completed_laps;
    page.position = input.position;
    page.i_current_time = input.i_current_time_ms;
    page.i_last_time = input.i_last_time_ms;
    page.i_best_time = input.i_best_time_ms;
    page.last_sector_time = input.last_sector_time_ms;
    page.current_sector_index = input.current_sector;
    page.number_of_laps = input.total_laps;
    page.session_time_left = input.session_time_left;
    page.is_in_pit = i32::from(input.in_pit);
    page.is_in_pit_lane = i32::from(input.in_pit);
    page.surface_grip = input.surface_grip;
    page.normalized_car_position = input.normalized_spline;
    page.distance_traveled = input.distance_traveled;
    page.car_coordinates = [input.car_x, input.car_y, input.car_z];
    page.flag = input.flag;
    format_telemetry_time(&mut page.current_time, input.i_current_time_ms);
    format_telemetry_time(&mut page.last_time, input.i_last_time_ms);
    format_telemetry_time(&mut page.best_time, input.i_best_time_ms);
    write_wide(&mut page.tyre_compound, &input.tyre_compound);
}

/// Fills the static page from a live snapshot (port of `fillStatic`).
pub fn fill_static(page: &mut StaticPage, input: &LiveInput) {
    write_wide(&mut page.sm_version, "1.7");
    write_wide(&mut page.app_version, "ksim");
    write_wide(&mut page.car_model, &input.car_model);
    write_wide(&mut page.track, &input.track_name);
    write_wide(
        &mut page.player_name,
        if input.player_name.is_empty() {
            "Driver"
        } else {
            &input.player_name
        },
    );
    write_wide(&mut page.car_id, &input.car_model);
    page.num_cars = 1;
    page.sector_count = input.sector_count;
    page.max_rpm = input.max_rpm;
    page.max_fuel = input.max_fuel;
    page.track_spline_length = input.track_spline_length;
}

/// Publishes the three pages to named shared memory, with a soft in-process
/// mirror when no OS mapping can be created.
pub struct SharedMemoryPublisher {
    ok: bool,
    mapped: bool,
    static_filled: bool,
    physics: PhysicsPage,
    graphics: GraphicsPage,
    static_page: StaticPage,
    phys_map: Option<platform::Mapping>,
    gfx_map: Option<platform::Mapping>,
    stat_map: Option<platform::Mapping>,
}

impl SharedMemoryPublisher {
    /// Creates a closed publisher (call [`open`](Self::open) to map the pages).
    #[must_use]
    pub fn new() -> Self {
        Self {
            ok: false,
            mapped: false,
            static_filled: false,
            physics: PhysicsPage::default(),
            graphics: GraphicsPage::default(),
            static_page: StaticPage::default(),
            phys_map: None,
            gfx_map: None,
            stat_map: None,
        }
    }

    /// Opens the three pages. Returns `true` when the publisher is usable; a
    /// failed mapping still leaves a working soft in-process mirror.
    pub fn open(&mut self) -> bool {
        self.open_named("acpmf_physics", "acpmf_graphics", "acpmf_static")
    }

    /// Opens pages under explicit base names (used by tests and alternate
    /// namespaces).
    pub fn open_named(&mut self, physics: &str, graphics: &str, static_: &str) -> bool {
        if self.ok {
            return true;
        }
        let phys_bytes = shm_mapping_size(core::mem::size_of::<PhysicsPage>());
        let gfx_bytes = shm_mapping_size(core::mem::size_of::<GraphicsPage>());
        let stat_bytes = shm_mapping_size(core::mem::size_of::<StaticPage>());

        self.phys_map = platform::Mapping::create(physics, phys_bytes);
        self.gfx_map = platform::Mapping::create(graphics, gfx_bytes);
        self.stat_map = platform::Mapping::create(static_, stat_bytes);

        if self.phys_map.is_some() && self.gfx_map.is_some() && self.stat_map.is_some() {
            self.mapped = true;
        } else {
            self.phys_map = None;
            self.gfx_map = None;
            self.stat_map = None;
            self.mapped = false;
        }
        self.ok = true;
        true
    }

    /// Closes the mapping (if any); the mirror stays readable until reopened.
    pub fn close(&mut self) {
        self.phys_map = None;
        self.gfx_map = None;
        self.stat_map = None;
        self.ok = false;
        self.mapped = false;
        self.static_filled = false;
    }

    /// Writes the live snapshot to the pages (mirror always, mapping when open).
    pub fn publish(&mut self, input: &LiveInput) {
        if !self.ok {
            self.use_soft();
        }
        fill_physics(&mut self.physics, input);
        fill_graphics(&mut self.graphics, input);
        if self.static_filled {
            self.static_page.max_rpm = input.max_rpm;
            self.static_page.max_fuel = input.max_fuel;
            self.static_page.track_spline_length = input.track_spline_length;
            self.static_page.sector_count = input.sector_count;
        } else {
            fill_static(&mut self.static_page, input);
            self.static_filled = true;
        }
        self.flush();
    }

    /// Marks the static page as needing a full rewrite on the next publish.
    pub fn invalidate_static(&mut self) {
        self.static_filled = false;
    }

    /// Whether the publisher is usable (mapped or soft mirror).
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.ok
    }

    /// Whether an OS-backed shared memory mapping is active.
    #[must_use]
    pub fn is_mapped(&self) -> bool {
        self.mapped
    }

    /// The in-process mirror of the physics page.
    #[must_use]
    pub fn physics(&self) -> &PhysicsPage {
        &self.physics
    }

    /// The in-process mirror of the graphics page.
    #[must_use]
    pub fn graphics(&self) -> &GraphicsPage {
        &self.graphics
    }

    /// The in-process mirror of the static page.
    #[must_use]
    pub fn static_page(&self) -> &StaticPage {
        &self.static_page
    }

    /// The last published physics packet id.
    #[must_use]
    pub fn physics_packet_id(&self) -> i32 {
        self.physics.packet_id
    }

    /// Reads the physics page straight back from the OS mapping (tests).
    #[must_use]
    pub fn read_mapped_physics(&self) -> Option<PhysicsPage> {
        let map = self.phys_map.as_ref()?;
        let mut page = PhysicsPage::default();
        // SAFETY: the mapping holds at least size_of::<PhysicsPage>() bytes and
        // `page` is a valid, aligned destination of that size.
        unsafe {
            core::ptr::copy_nonoverlapping(
                map.as_ptr(),
                core::ptr::addr_of_mut!(page).cast::<u8>(),
                core::mem::size_of::<PhysicsPage>(),
            );
        }
        Some(page)
    }

    fn use_soft(&mut self) {
        self.ok = true;
        self.mapped = false;
    }

    fn flush(&mut self) {
        if let Some(map) = &self.phys_map {
            write_page(&self.physics, map.as_ptr());
        }
        if let Some(map) = &self.gfx_map {
            write_page(&self.graphics, map.as_ptr());
        }
        if let Some(map) = &self.stat_map {
            write_page(&self.static_page, map.as_ptr());
        }
    }
}

impl Default for SharedMemoryPublisher {
    fn default() -> Self {
        Self::new()
    }
}

/// Copies a page struct into a mapped view.
fn write_page<T>(page: &T, dst: *mut u8) {
    // SAFETY: `dst` points at a mapping of at least size_of::<T>() bytes.
    unsafe {
        core::ptr::copy_nonoverlapping(
            core::ptr::addr_of!(*page).cast::<u8>(),
            dst,
            core::mem::size_of::<T>(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::{offset_of, size_of};
    use std::sync::atomic::{AtomicU32, Ordering};

    #[test]
    fn mapping_size_uses_592_byte_floor() {
        assert_eq!(SHM_MAPPING_BYTES, 0x250);
        assert_eq!(shm_mapping_size(0), 592);
        assert_eq!(shm_mapping_size(388), 592);
        assert_eq!(shm_mapping_size(592), 592);
        assert_eq!(shm_mapping_size(676), 676);
    }

    #[test]
    fn page_sizes_match_ac_layout() {
        assert_eq!(size_of::<PhysicsPage>(), 580);
        assert_eq!(size_of::<GraphicsPage>(), 296);
        assert_eq!(size_of::<StaticPage>(), 676);
    }

    #[test]
    fn key_field_offsets_are_locked() {
        assert_eq!(offset_of!(PhysicsPage, brake_bias), 564);
        assert_eq!(offset_of!(PhysicsPage, tyre_contact_point), 420);
        assert_eq!(offset_of!(GraphicsPage, completed_laps), 132);
        assert_eq!(offset_of!(GraphicsPage, tyre_compound), 176);
        assert_eq!(offset_of!(StaticPage, track), 134);
        assert_eq!(offset_of!(StaticPage, track_configuration), 524);
    }

    #[test]
    fn page_defaults_match_ac() {
        let physics = PhysicsPage::default();
        assert_eq!(physics.air_density, 1.225);
        assert_eq!(physics.air_temp, 25.0);
        assert_eq!(physics.road_temp, 30.0);
        assert_eq!(physics.brake_bias, 0.55);
        assert_eq!(GraphicsPage::default().replay_time_multiplier, 1.0);
        assert_eq!(GraphicsPage::default().surface_grip, 1.0);
        assert_eq!(StaticPage::default().num_cars, 1);
        assert_eq!(StaticPage::default().sector_count, 3);
    }

    #[test]
    fn input_defaults_match_ac() {
        let input = LiveInput::default();
        assert_eq!(input.gear, 1);
        assert_eq!(input.fuel, 50.0);
        assert_eq!(input.wheel_load, [3500.0; 4]);
        assert_eq!(input.tyre_temp, [80.0; 4]);
        assert_eq!(input.tyre_pressure, [2.2, 2.2, 2.0, 2.0]);
        assert_eq!(input.tyre_compound, "D");
        assert_eq!(input.max_rpm, 8500);
        assert_eq!(input.session_type, 2);
    }

    #[test]
    fn fill_physics_maps_gear_and_speed() {
        let mut page = PhysicsPage::default();
        fill_physics(
            &mut page,
            &LiveInput {
                throttle: 0.75,
                speed_ms: 25.0,
                gear: 3,
                rpm: 6400.0,
                ..Default::default()
            },
        );
        assert_eq!(page.packet_id, 1);
        assert_eq!(page.gas, 0.75);
        assert_eq!(page.gear, 4);
        assert_eq!(page.rpms, 6400);
        assert_eq!(page.speed_kmh, 90.0);

        fill_physics(
            &mut page,
            &LiveInput {
                gear: 0,
                ..Default::default()
            },
        );
        assert_eq!(page.gear, 1);
        fill_physics(
            &mut page,
            &LiveInput {
                gear: -1,
                ..Default::default()
            },
        );
        assert_eq!(page.gear, 0);
        assert_eq!(page.packet_id, 3);
    }

    #[test]
    fn fill_graphics_formats_times_and_compound() {
        let mut page = GraphicsPage::default();
        fill_graphics(
            &mut page,
            &LiveInput {
                i_current_time_ms: 92_413,
                i_best_time_ms: 60_000,
                completed_laps: 4,
                tyre_compound: "SUPER".to_string(),
                ..Default::default()
            },
        );
        assert_eq!(page.packet_id, 1);
        assert_eq!(page.i_current_time, 92_413);
        assert_eq!(page.completed_laps, 4);
        assert_eq!(wide_to_string(&page.current_time), "1:32.413");
        assert_eq!(wide_to_string(&page.best_time), "1:00.000");
        assert_eq!(wide_to_string(&page.tyre_compound), "SUPER");
    }

    #[test]
    fn fill_static_sets_identity_and_defaults_driver() {
        let mut page = StaticPage::default();
        fill_static(
            &mut page,
            &LiveInput {
                car_model: "ks_gt3".to_string(),
                track_name: "spa".to_string(),
                max_rpm: 9000,
                max_fuel: 120.0,
                track_spline_length: 7004.0,
                ..Default::default()
            },
        );
        assert_eq!(wide_to_string(&page.sm_version), "1.7");
        assert_eq!(wide_to_string(&page.app_version), "ksim");
        assert_eq!(wide_to_string(&page.car_model), "ks_gt3");
        assert_eq!(wide_to_string(&page.track), "spa");
        assert_eq!(wide_to_string(&page.player_name), "Driver");
        assert_eq!(wide_to_string(&page.car_id), "ks_gt3");
        assert_eq!(page.num_cars, 1);
        assert_eq!(page.max_rpm, 9000);
        assert_eq!(page.track_spline_length, 7004.0);
    }

    #[test]
    fn format_time_handles_invalid_and_pads() {
        let mut buf = [1u16; 15];
        format_telemetry_time(&mut buf, 0);
        assert_eq!(wide_to_string(&buf), "--");
        format_telemetry_time(&mut buf, -5);
        assert_eq!(wide_to_string(&buf), "--");
        format_telemetry_time(&mut buf, 5_900);
        assert_eq!(wide_to_string(&buf), "0:05.900");
        format_telemetry_time(&mut buf, 3_725_001);
        assert_eq!(wide_to_string(&buf), "62:05.001");
    }

    #[test]
    fn race_flag_mapping_table() {
        assert_eq!(race_flag_to_page_flag(0), 0);
        assert_eq!(race_flag_to_page_flag(1), 0);
        assert_eq!(race_flag_to_page_flag(2), 2);
        assert_eq!(race_flag_to_page_flag(3), 1);
        assert_eq!(race_flag_to_page_flag(4), 4);
        assert_eq!(race_flag_to_page_flag(5), 3);
        assert_eq!(race_flag_to_page_flag(6), 5);
        assert_eq!(race_flag_to_page_flag(7), 6);
        assert_eq!(race_flag_to_page_flag(8), 2);
        assert_eq!(race_flag_to_page_flag(9), 2);
        assert_eq!(race_flag_to_page_flag(10), 2);
        assert_eq!(race_flag_to_page_flag(11), 0);
        assert_eq!(race_flag_to_page_flag(12), 0);
    }

    #[test]
    fn publish_without_open_uses_soft_mirror() {
        let mut publisher = SharedMemoryPublisher::new();
        publisher.publish(&LiveInput {
            throttle: 0.5,
            gear: 2,
            ..Default::default()
        });
        assert!(publisher.is_open());
        assert!(!publisher.is_mapped());
        assert_eq!(publisher.physics().gas, 0.5);
        assert_eq!(publisher.physics().gear, 3);
        assert_eq!(publisher.physics_packet_id(), 1);
    }

    #[test]
    fn static_page_refreshes_only_dynamic_fields() {
        let mut publisher = SharedMemoryPublisher::new();
        publisher.publish(&LiveInput {
            max_rpm: 8000,
            car_model: "a".to_string(),
            ..Default::default()
        });
        publisher.publish(&LiveInput {
            max_rpm: 9500,
            car_model: "b".to_string(),
            ..Default::default()
        });
        assert_eq!(publisher.static_page().max_rpm, 9500);
        assert_eq!(wide_to_string(&publisher.static_page().car_model), "a");

        publisher.invalidate_static();
        publisher.publish(&LiveInput {
            car_model: "c".to_string(),
            ..Default::default()
        });
        assert_eq!(wide_to_string(&publisher.static_page().car_model), "c");
    }

    #[test]
    fn mapped_publisher_round_trips_physics_page() {
        let mut publisher = SharedMemoryPublisher::new();
        assert!(publisher.open_named(
            &unique_name("physics"),
            &unique_name("graphics"),
            &unique_name("static"),
        ));
        assert!(publisher.is_mapped());

        publisher.publish(&LiveInput {
            throttle: 0.42,
            brake: 0.17,
            speed_ms: 60.0,
            gear: 5,
            i_best_time_ms: 88_000,
            ..Default::default()
        });

        let read = publisher.read_mapped_physics().expect("mapped read");
        assert_eq!(read.gas, 0.42);
        assert_eq!(read.brake, 0.17);
        assert_eq!(read.speed_kmh, 216.0);
        assert_eq!(read.gear, 6);
        assert_eq!(publisher.physics_packet_id(), 1);

        publisher.close();
        assert!(!publisher.is_mapped());
        assert!(publisher.read_mapped_physics().is_none());
    }

    fn unique_name(tag: &str) -> String {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        format!("acpmf_test_{tag}_{}_{id}", std::process::id())
    }

    fn wide_to_string(buf: &[u16]) -> String {
        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        buf[..end].iter().map(|&c| c as u8 as char).collect()
    }
}
