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

/// Format a NetName for display
pub fn format_net_name(net: &hwc_parser::NetName) -> CompactString {
    if let Some(ref index) = net.index {
        // Try to evaluate the index to get a concrete value
        match index.evaluate_const() {
            Ok(hwc_parser::Value::Number(n)) => format!("{}[{}]", net.base, n).into(),
            _ => format!("{}[...]", net.base).into(),
        }
    } else {
        net.base.clone()
    }
}

/// Print warning about identity collision across iterations
pub fn print_identity_collision_warning(net_name: &str, iterations: &[usize], variable: &str) {
    eprintln!("\n⚠️  IDENTITY COLLISION WARNING");
    eprintln!("   Net: {}", net_name);
    eprintln!(
        "   Loop variable '{}' produced the same net in {} iterations:",
        variable,
        iterations.len()
    );
    eprintln!("   Iterations: {:?}", iterations);
    eprintln!("\n   Common causes:");
    eprintln!("   - Integer division truncation: i/2 where i=0,1 both → 0");
    eprintln!("   - Modulo operations: i%3 repeats every 3 iterations");
    eprintln!("   - Intentional net sharing (this may be correct!)");
    eprintln!("\n   If intentional: This warning is informational only.");
    eprintln!("   If accidental: Check your index expression for truncation.\n");
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
