use crate::constraint_manager::ConstraintRulebook;
use crate::geometry::BoundingBox;
use crate::space::HardwareSpace;
use rustc_hash::FxHashSet;
use super::types::DrcViolation;

/// Validate clearances between all nets using R*-tree spatial queries.
pub fn validate_clearances(
    space: &HardwareSpace,
    constraints: &ConstraintRulebook,
) -> Result<Vec<DrcViolation>, String> {
    let clearance_nm = constraints
        .fabrication
        .as_ref()
        .map(|fab| fab.min_trace_spacing_nm)
        .ok_or_else(|| {
            "[DRC] FATAL: No fabrication constraints loaded. \
             Add a 'profile:' clause to your space to enable DRC."
                .to_string()
        })?;

    let mut violations = Vec::new();
    let mut checked: FxHashSet<(u32, u32)> = FxHashSet::default();

    // BUG DETECTION: Check for out-of-bounds entities
    let (space_x, space_y, space_z) = (
        space.dimensions.width_nm as i64,
        space.dimensions.height_nm as i64, 
        space.dimensions.depth_nm as i64
    );
    
    eprintln!("[DRC] Space dimensions: {}x{}x{} nm", space_x, space_y, space_z);
    
    let mut oob_count = 0;
    for entity in space.entity_graph.spatial().iter() {
        let is_oob = entity.start.x < 0 || entity.start.x > space_x
            || entity.start.y < 0 || entity.start.y > space_y
            || entity.start.z < 0 || entity.start.z > space_z
            || entity.end.x < 0 || entity.end.x > space_x
            || entity.end.y < 0 || entity.end.y > space_y
            || entity.end.z < 0 || entity.end.z > space_z;
            
        if is_oob {
            oob_count += 1;
            if oob_count <= 5 {  // Only show first 5
                eprintln!("[DRC BUG] OUT-OF-BOUNDS entity #{}:", oob_count);
                eprintln!("  net_id={}, start=({},{},{}), end=({},{},{})",
                    entity.net_id,
                    entity.start.x, entity.start.y, entity.start.z,
                    entity.end.x, entity.end.y, entity.end.z);
            }
        }
    }
    
    if oob_count > 0 {
        eprintln!("[DRC BUG] Found {} out-of-bounds entities total!", oob_count);
    }

    // v0.1.8: Simplified Category A - Geometric Clearance Check
    // Iterate over all entities in the spatial index (pours, routes, components)
    for entity in space.entity_graph.spatial().iter() {
        if entity.net_id == 0 {
            continue; // Skip unconnected geometry for clearance (keepouts handle this)
        }

        // 1. Retrieve physical 3D bounding box (AABB)
        let bbox = BoundingBox::new(entity.start, entity.end);

        // 2. Inflate box in X and Y by the min_spacing rule
        let inflated = bbox.inflate_xy(clearance_nm);

        // 3. Query the R*-tree spatial index with this inflated envelope
        let candidates = space.entity_graph.spatial().query_bbox(&inflated);

        for cand in candidates {
            // 5. If any returned element belongs to a different NetId, flag a violation
            if cand.net_id == 0 || cand.net_id == entity.net_id {
                continue;
            }

            let (net_a, net_b) = if entity.net_id < cand.net_id {
                (entity.net_id as u32, cand.net_id as u32)
            } else {
                (cand.net_id as u32, entity.net_id as u32)
            };
            let pair_key = (net_a, net_b);
            if !checked.insert(pair_key) {
                continue;
            }

            let cand_bbox = BoundingBox::new(cand.start, cand.end);
            let dist = bbox.distance_to(&cand_bbox);
            
            if dist < clearance_nm {
                violations.push(DrcViolation::ClearanceViolation {
                    net_a: format!("net_{}", net_a).into(),
                    net_b: format!("net_{}", net_b).into(),
                    actual_nm: dist,
                    required_nm: clearance_nm,
                    location: entity.start,
                });
            }
        }
    }

    Ok(violations)
}
