//! Stackup layer resolution for child-instance routes.
//!
//! **v0.2.1 FIX**: Child routes often contain vias plus horizontal segments.
//! The routing layer is derived from the first horizontal segment's Z level,
//! falling back to the first via's Z level for via-only routes.
//!
//! **v0.2.2 LAYER LINEAGE**: The resolved layer name is returned alongside the
//! Z-range so the resulting trace carries explicit layer lineage.

use crate::netlist::NetId;
use crate::space::{LineSegment, StackupLayer};
use compact_str::CompactString;

/// Resolved layer information: ((z_bottom, z_top), layer_name).
pub(super) type ResolvedLayer = ((i64, i64), CompactString);

/// Determine the stackup layer for a child route.
///
/// Returns `None` when the route is empty or no stackup layer contains the
/// route's centerline Z.
pub(super) fn resolve_child_route_layer(
    line_segments: &[LineSegment],
    stackup_layers: &[StackupLayer],
    instance_name: &str,
    net_id: NetId,
) -> Option<ResolvedLayer> {
    if line_segments.is_empty() {
        return None;
    }

    // Collect all horizontal segments (where start.z == end.z)
    let horizontal_z_levels: Vec<i64> = line_segments
        .iter()
        .filter(|s| s.start.z == s.end.z)
        .map(|s| s.start.z)
        .collect();

    eprintln!(
        "[ROUTING DB]   Found {} horizontal segments at Z levels: {:?}",
        horizontal_z_levels.len(),
        horizontal_z_levels
    );

    match horizontal_z_levels.first() {
        // If we have horizontal segments, use the first one's Z level
        Some(&centerline_z) => {
            log_stackup_context(stackup_layers, instance_name, net_id, centerline_z);
            find_layer_for_centerline(stackup_layers, centerline_z)
        }
        None => {
            eprintln!("[ROUTING DB]   No horizontal segments found - route is pure vias");
            resolve_via_only_layer(line_segments, stackup_layers)
        }
    }
}

/// Emit diagnostic context about the stackup before performing the lookup.
fn log_stackup_context(
    stackup_layers: &[StackupLayer],
    instance_name: &str,
    net_id: NetId,
    centerline_z: i64,
) {
    eprintln!(
        "[ROUTING DB] Child route for net={:?}, instance='{}': looking up layer at Z={}nm (stackup has {} layers)",
        net_id,
        instance_name,
        centerline_z,
        stackup_layers.len()
    );

    for (idx, layer) in stackup_layers.iter().enumerate() {
        eprintln!(
            "[ROUTING DB]   Layer {}: z_bottom={}, z_top={}, name='{}'",
            idx, layer.z_bottom, layer.z_top, layer.name
        );
    }
}

/// Look up the layer from stackup (single source of truth).
///
/// Uses half-open intervals `[z_bottom, z_top)` for all layers except the
/// topmost, to match `HardwareSpace::find_layer_at_z` semantics and avoid
/// ambiguity at shared layer boundaries (e.g. Z=1250 is metal1.z_bottom,
/// not d1.z_top).
fn find_layer_for_centerline(
    stackup_layers: &[StackupLayer],
    centerline_z: i64,
) -> Option<ResolvedLayer> {
    let layer_count = stackup_layers.len();

    let result = stackup_layers
        .iter()
        .enumerate()
        .find(|(idx, layer)| {
            let is_last = *idx == layer_count - 1;
            let matches = if is_last {
                centerline_z >= layer.z_bottom && centerline_z <= layer.z_top
            } else {
                centerline_z >= layer.z_bottom && centerline_z < layer.z_top
            };
            eprintln!(
                "[ROUTING DB]   Checking layer '{}': z_bottom={}, z_top={}, centerline={}, matches={}",
                layer.name, layer.z_bottom, layer.z_top, centerline_z, matches
            );
            matches
        })
        .map(|(_, layer)| {
            eprintln!(
                "[ROUTING DB]   ✓ Found layer '{}' at Z={}→{}nm for centerline Z={}nm",
                layer.name, layer.z_bottom, layer.z_top, centerline_z
            );
            ((layer.z_bottom, layer.z_top), layer.name.clone())
        });

    if result.is_none() {
        eprintln!(
            "[ROUTING DB]   ✗ No layer found for centerline Z={}nm!",
            centerline_z
        );
        eprintln!(
            "[ROUTING DB]   FATAL: Child route has no matching stackup layer. This should never happen."
        );
    }

    result
}

/// For via-only routes, use the first segment's Z to find a layer.
///
/// Unlike [`find_layer_for_centerline`], this uses fully closed intervals since
/// a via legitimately terminates on a layer boundary.
fn resolve_via_only_layer(
    line_segments: &[LineSegment],
    stackup_layers: &[StackupLayer],
) -> Option<ResolvedLayer> {
    let first_seg = line_segments.first()?;
    let via_z = first_seg.start.z;

    stackup_layers
        .iter()
        .find(|layer| via_z >= layer.z_bottom && via_z <= layer.z_top)
        .map(|layer| {
            eprintln!(
                "[ROUTING DB]   ✓ Via-only route: using layer '{}' at Z={}nm",
                layer.name, via_z
            );
            ((layer.z_bottom, layer.z_top), layer.name.clone())
        })
}
