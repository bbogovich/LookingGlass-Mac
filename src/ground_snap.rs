//! Visual ground contact for Alice on slopes and ledge edges (macOS fork QoL fix).
//!
//! Collision uses an upright 30 x 30 box with the origin at the feet (docs/MOVEMENT.md).
//! On a slope the box rests on its downhill edge or corner, and on a ledge it can rest
//! on the edge while its centre is over lower ground. The model is drawn at the box
//! centre, so Alice appeared to hover. This module lowers only the *drawn* position to
//! the ground directly beneath the centre. Collision, movement, triggers, saves and
//! routes still use `Player::feet` unchanged.

use crate::collision::World;
use crate::movement::Player;
use macroquad::prelude::*;

/// Largest visual drop. An axis-aligned 15-unit half-width box on a walkable slope
/// floats at most 15 * (|nx| + |ny|) / nz ≈ 21 units at 45° diagonal; beyond this the
/// gap is a real drop (a pit or a stair below), where hovering is the lesser evil.
const MAX_DROP: f32 = 24.;
/// Ground steeper than this (normal z below) is not something Alice could stand on.
const MIN_WALKABLE_Z: f32 = 0.5;
/// Ray start above the feet, so a feet origin resting exactly on a surface still hits.
const PROBE_LIFT: f32 = 2.;
/// Exponential smoothing rate (1/s); about 50 ms to settle, so steps don't pop.
const RATE: f32 = 20.;
/// Treat larger jumps of the feet between frames as a teleport and settle instantly.
const TELEPORT: f32 = 64.;

#[derive(Default)]
pub struct GroundSnap {
    offset: f32,
    last_feet: Option<Vec3>,
}

impl GroundSnap {
    /// The feet position to draw Alice at this frame.
    pub fn feet(&mut self, world: &World, player: &Player) -> Vec3 {
        let feet = player.feet;
        let target = if player.grounded && !player.swimming && !player.steam {
            gap_below(world, feet)
        } else {
            0.
        };
        let teleported = self
            .last_feet
            .map_or(true, |last| last.distance(feet) > TELEPORT);
        self.last_feet = Some(feet);
        self.offset = if teleported {
            target
        } else {
            smooth(self.offset, target, get_frame_time())
        };
        feet - Vec3::Z * self.offset
    }
}

/// Distance from the feet down to walkable ground under the box centre, if within reach.
fn gap_below(world: &World, feet: Vec3) -> f32 {
    let start = feet + Vec3::Z * PROBE_LIFT;
    let end = feet - Vec3::Z * MAX_DROP;
    let hit = world.sweep(start, end, Vec3::ZERO);
    if hit.start_solid || hit.fraction >= 1. || hit.normal.z < MIN_WALKABLE_Z {
        return 0.;
    }
    ((MAX_DROP + PROBE_LIFT) * hit.fraction - PROBE_LIFT).clamp(0., MAX_DROP)
}

fn smooth(current: f32, target: f32, dt: f32) -> f32 {
    let dt = if dt.is_finite() {
        dt.clamp(0., 0.1)
    } else {
        0.
    };
    current + (target - current) * (1. - (-RATE * dt).exp())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoothing_converges_without_overshoot() {
        let mut offset = 0.;
        for _ in 0..30 {
            let next = smooth(offset, 10., 1. / 60.);
            assert!(next >= offset && next <= 10.);
            offset = next;
        }
        assert!((offset - 10.).abs() < 0.1, "{offset}");
    }

    #[test]
    fn smoothing_ignores_bad_frame_times() {
        assert_eq!(smooth(3., 10., f32::NAN), 3.);
        assert_eq!(smooth(3., 10., -1.), 3.);
        assert!(smooth(3., 10., 5.) <= 10.);
    }
}
