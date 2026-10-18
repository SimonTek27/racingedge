//! Deterministic fixed-timestep loop — port of ksengine
//! `SimulationLoop::m_physicsDt` (GPL-3.0).
//! Original: <https://github.com/SimonTek27/ksengine> (`src/simulator/SimulationLoop.h`).
//!
//! The ksengine `SimulationLoop` runs physics at a fixed 1 kHz (`m_physicsDt =
//! 0.001`, an accumulator in `tick()`). This module ports just that timing
//! core: an accumulator that turns variable frame time into a whole number of
//! 1 ms physics steps, so the simulation is deterministic regardless of frame
//! rate. The full C++ loop (rendering, UI, telemetry, networking) is not part
//! of this port; callers drive [`SimulationLoop::tick`] from a Bevy system.
//!
//! Typical usage in a Bevy system:
//!
//! ```ignore
//! use bevy::prelude::*;
//! use racingedge::simulator::r#loop::{PHYSICS_DT, SimulationLoop};
//!
//! fn fixed_physics_step(
//!     time: Res<Time>,
//!     mut loop_: ResMut<SimulationLoop>,
//!     mut query: Query<(&mut Transform, &mut Car)>,
//! ) {
//!     for _ in 0..loop_.tick(time.delta_secs() as f64) {
//!         // one physics step taken — advance game logic here
//!         for (mut transform, mut car) in &mut query {
//!             // ... step vehicle physics by PHYSICS_DT ...
//!         }
//!     }
//! }
//! ```
use bevy::prelude::Resource;

/// Fixed physics step: 1 kHz, exactly as ksengine `SimulationLoop::m_physicsDt`.
pub const PHYSICS_DT: f64 = 0.001;

/// Accumulator‑driven fixed timestep loop.
/// Each call to `tick(dt)` adds `dt` to an internal accumulator and then
/// consumes every whole `PHYSICS_DT` interval it can, returning how many
/// physics steps the caller should run.  The remaining fractional time stays in
/// the accumulator for the next frame, so the simulation always advances by an
/// exact multiple of `PHYSICS_DT` no matter how irregular the frame time is.
///
/// Typical frame pattern:
///
/// ```ignore
/// for _ in 0..loop_.tick(delta) {
///     // step physics, AI, weather, lap timer, etc.
/// }
/// ```
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct SimulationLoop {
    accumulator: f64,
}

impl SimulationLoop {
    /// Create a new `SimulationLoop` starting at rest.
    pub fn new() -> Self {
        Self::default()
    }

    /// Advance the loop by `dt` seconds (typically `Time::delta_secs()`).
    /// Returns the number of fixed `PHYSICS_DT` steps that elapsed; `0` means
    /// the frame did not accumulate a whole step.
    ///
    /// # Example
    ///
    /// ```
    /// use racingedge::simulator::r#loop::SimulationLoop;
    /// let mut loop_ = SimulationLoop::new();
    /// assert_eq!(loop_.tick(0.0005), 0);
    /// assert_eq!(loop_.tick(0.0008), 1); // 0.0005 + 0.0008 = 0.0013 -> one step
    /// assert!((loop_.remaining() - 0.0003).abs() < 1e-12);
    /// ```
    pub fn tick(&mut self, dt: f64) -> u32 {
        self.accumulator += dt;
        let mut steps = 0;
        while self.accumulator >= PHYSICS_DT {
            self.accumulator -= PHYSICS_DT;
            steps += 1;
        }
        steps
    }

    /// The remaining accumulated time (always < `PHYSICS_DT`).
    pub fn remaining(&self) -> f64 {
        self.accumulator
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_less_than_dt_returns_zero() {
        let mut loop_ = SimulationLoop::new();
        assert_eq!(loop_.tick(0.0005), 0);
    }

    #[test]
    fn tick_exactly_dt_returns_one() {
        let mut loop_ = SimulationLoop::new();
        assert_eq!(loop_.tick(0.001), 1);
        assert_eq!(loop_.remaining(), 0.0);
    }

    #[test]
    fn tick_spans_multiple_steps() {
        let mut loop_ = SimulationLoop::new();
        assert_eq!(loop_.tick(0.0005), 0);
        assert_eq!(loop_.tick(0.0015), 2); // 0.0005 + 0.0015 = 0.002 -> two steps
        assert_eq!(loop_.tick(0.001), 1);
        assert_eq!(loop_.tick(0.0025), 2);
    }

    #[test]
    fn remaining_always_less_than_dt() {
        let mut loop_ = SimulationLoop::new();
        // Deterministic sweep of frame deltas up to 10 ms.
        for i in 1..=100 {
            let dt = i as f64 * 0.0001;
            loop_.tick(dt);
            assert!(
                loop_.remaining() < PHYSICS_DT,
                "remaining {} >= {}",
                loop_.remaining(),
                PHYSICS_DT
            );
        }
    }

    #[test]
    fn many_small_ticks_accumulate_to_steps() {
        let mut loop_ = SimulationLoop::new();
        let mut steps = 0;
        // 1000 ticks of 0.5 ms == 0.5 s == 500 physics steps.
        for _ in 0..1000 {
            steps += loop_.tick(0.0005);
        }
        assert_eq!(steps, 500);
    }
}
