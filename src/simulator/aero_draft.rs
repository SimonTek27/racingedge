//! Scia/effetto risucchio quando si segue un'auto davanti.
//!
//! Port di `src/engine/physics/AeroDraft.h` di ksengine.

use bevy::math::Vec3;

/// Variazione di drag e downforce dovuta alla scia (port di `DraftState`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DraftState {
    /// Frazione di drag rimossa, `0..=0.32`.
    pub drag_reduction: f32,
    /// Frazione di downforce persa, `0..=0.28`.
    pub downforce_loss: f32,
}

/// Larghezza predefinita dell'auto che genera la scia, in metri.
pub const DEFAULT_LEADER_WIDTH: f32 = 1.9;

/// Scia con larghezza predefinita (port di `computeDraft`).
///
/// `ego` e `leader` sono posizioni nel mondo; `ego_fwd` è la direzione di
/// marcia dell'auto inseguitrice. Restituisce una [`DraftState`] nulla quando
/// non c'è scia.
pub fn compute_draft(ego: Vec3, ego_fwd: Vec3, leader: Vec3) -> DraftState {
    compute_draft_with_width(ego, ego_fwd, leader, DEFAULT_LEADER_WIDTH)
}

/// Scia con larghezza esplicita dell'auto davanti (port di `computeDraft`).
pub fn compute_draft_with_width(
    ego: Vec3,
    ego_fwd: Vec3,
    leader: Vec3,
    leader_width: f32,
) -> DraftState {
    let mut state = DraftState::default();

    let d = ego - leader;
    let dist = d.length();
    if !(0.5..=60.0).contains(&dist) {
        return state;
    }

    let fwd = ego_fwd.normalize_or_zero();
    let along = d.dot(fwd);
    if along > 0.0 {
        // Non siamo dietro l'auto davanti.
        return state;
    }

    let behind = -along;
    let lat = d - fwd * along;
    let lateral = lat.length();
    let wake_width = leader_width * (0.6 + behind * 0.06);
    if lateral > wake_width {
        return state;
    }

    let proximity = 1.0 - behind / 60.0;
    let center_factor = 1.0 - (lateral / wake_width) * 0.7;
    state.drag_reduction = (0.32 * proximity * center_factor).clamp(0.0, 0.32);
    state.downforce_loss = (0.28 * proximity * center_factor).clamp(0.0, 0.28);
    state
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_draft_when_far_away() {
        let ego = Vec3::new(0.0, 0.0, 0.0);
        let leader = Vec3::new(0.0, 0.0, 100.0);
        let s = compute_draft(ego, Vec3::Z, leader);
        assert_eq!(s, DraftState::default());
    }

    #[test]
    fn no_draft_when_ahead_of_leader() {
        let ego = Vec3::new(0.0, 0.0, 0.0);
        let leader = Vec3::new(0.0, 0.0, -10.0);
        let s = compute_draft(ego, Vec3::Z, leader);
        assert_eq!(s, DraftState::default());
    }

    #[test]
    fn draft_directly_behind_and_close() {
        // Leader 10 m ahead along +Z; ego facing +Z.
        let ego = Vec3::new(0.0, 0.0, 0.0);
        let leader = Vec3::new(0.0, 0.0, 10.0);
        let s = compute_draft(ego, Vec3::Z, leader);
        assert!(s.drag_reduction > 0.0);
        assert!(s.drag_reduction <= 0.32);
        assert!(s.downforce_loss > 0.0);
        assert!(s.downforce_loss <= 0.28);
    }

    #[test]
    fn closer_gives_more_drag_reduction() {
        let ego = Vec3::ZERO;
        let near = compute_draft(ego, Vec3::Z, Vec3::new(0.0, 0.0, 5.0));
        let far = compute_draft(ego, Vec3::Z, Vec3::new(0.0, 0.0, 30.0));
        assert!(near.drag_reduction > far.drag_reduction);
    }

    #[test]
    fn no_draft_when_laterally_offset() {
        let ego = Vec3::ZERO;
        let leader = Vec3::new(20.0, 0.0, 10.0);
        let s = compute_draft(ego, Vec3::Z, leader);
        assert_eq!(s, DraftState::default());
    }

    #[test]
    fn reductions_are_capped() {
        let ego = Vec3::new(0.0, 0.0, 0.0);
        let leader = Vec3::new(0.0, 0.0, 0.6);
        let s = compute_draft(ego, Vec3::Z, leader);
        assert!(s.drag_reduction <= 0.32);
        assert!(s.downforce_loss <= 0.28);
    }
}
