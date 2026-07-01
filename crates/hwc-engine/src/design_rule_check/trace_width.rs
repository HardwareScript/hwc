use crate::constraint_manager::ConstraintRulebook;
use crate::space::HardwareSpace;
use super::types::DrcViolation;

/// Validate trace widths for all nets using analytic geometry.
pub fn validate_trace_widths(
    space: &HardwareSpace,
    constraints: &ConstraintRulebook,
) -> Result<Vec<DrcViolation>, String> {
    let mut violations = Vec::new();

    // Get required trace width from fabrication constraints — fail-fast if missing
    let required_width_nm = constraints
        .fabrication
        .as_ref()
        .map(|fab| fab.min_trace_width_nm)
        .ok_or_else(|| {
            "[DRC] FATAL: No fabrication constraints loaded. \
             Add a 'profile:' clause to your space to enable DRC."
                .to_string()
        })?;

    // v0.1.8: Perform analytic trace width checks using the EntityGraph.
    // In a vector-first system, trace width is a property of the analytic segment itself.
    for (net_id, segments) in space.entity_graph.get_all_routes() {
        for segment in segments {
            if segment.width_nm < required_width_nm {
                violations.push(DrcViolation::TraceWidthViolation {
                    net: format!("net_{}", net_id.raw()).into(),
                    actual_nm: segment.width_nm,
                    required_nm: required_width_nm,
                    location: segment.start,
                });
            }
        }
    }

    Ok(violations)
}