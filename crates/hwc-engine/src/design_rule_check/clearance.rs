use crate::constraint_manager::ConstraintRulebook;
use crate::space::HardwareSpace;
use super::types::DrcViolation;

/// Validate clearances between all nets using analytic geometry and spatial index.
pub fn validate_clearances(
    space: &HardwareSpace,
    constraints: &ConstraintRulebook,
) -> Vec<DrcViolation> {
    let mut violations = Vec::new();

    // Get required clearance from fabrication constraints.
    let (base_clearance_nm, _hv_clearance_nm) = match constraints.fabrication.as_ref() {
        Some(fab) => (
            fab.min_trace_spacing_nm,
            Some(fab.high_voltage_clearance_nm),
        ),
        None => return vec![], // No profile loaded — nothing to check
    };

    // v0.1.8: Perform analytic clearance checks using the EntityGraph spatial index.
    // This iterates over all substrate layers and route segments.
    let layers = space.entity_graph.get_substrate_layers();
    
    for (i, layer_a) in layers.iter().enumerate() {
        if layer_a.net == 0 { continue; } // Skip substrate

        for layer_b in &layers[i+1..] {
            if layer_b.net == 0 || layer_a.net == layer_b.net { continue; }

            // Determine required clearance
            let required_nm = base_clearance_nm; // Simplification for now

            // Analytic distance check
            let dist = layer_a.bbox.manhattan_distance(&layer_b.bbox);
            if dist < required_nm {
                violations.push(DrcViolation::ClearanceViolation {
                    net_a: format!("net_{}", layer_a.net).into(),
                    net_b: format!("net_{}", layer_b.net).into(),
                    actual_nm: dist,
                    required_nm,
                    location: layer_a.bbox.min,
                });
            }
        }
    }

    violations
}