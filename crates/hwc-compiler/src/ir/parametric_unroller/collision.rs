//! Collision detection and warning system for net identity conflicts
//!
//! Tracks when the same net name is generated multiple times, either:
//! - Across iterations (identity collision: i/2 where i=0,1 both → 0)
//! - Within same iteration (multiple objects using same net)

use compact_str::CompactString;

/// Warning about identity collision
pub struct CollisionWarning {
    pub iteration: usize,
    pub net_name: CompactString,
    pub object_type: CompactString,
    pub object_name: CompactString,
}

/// Print warnings about same-iteration collisions
pub fn print_same_iteration_collision_warnings(warnings: &[CollisionWarning]) {
    eprintln!("\n⚠️  SAME-ITERATION NET COLLISION");
    eprintln!("   Multiple objects in the same loop iteration reference the same net:");
    for warning in warnings {
        eprintln!(
            "   - Iteration {}: {} '{}' uses net '{}'",
            warning.iteration, warning.object_type, warning.object_name, warning.net_name
        );
    }
    eprintln!("\n   This is usually intentional (connecting multiple objects to the same net).");
    eprintln!("   If accidental: Check your net naming logic.\n");
}
