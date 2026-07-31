use crate::ir::errors::IrError;

/// Resolve `routing.min_segment_length` from the PDK profile.
pub fn require_min_segment_length_nm(
    profile: Option<&hwc_parser::ProfileDefinition>,
) -> Result<i64, IrError> {
    let measurement = profile
        .and_then(|p| p.routing.as_ref())
        .and_then(|r| r.min_segment_length.as_ref())
        .ok_or_else(|| IrError::MissingRoutingHeuristics {
            field: "min_segment_length".into(),
            hint: "Add 'min_segment_length: <value>' to the profile's 'routing:' block \
                   (e.g. min_segment_length: 180nm for ASIC, 0.2mm for PCB)."
                .into(),
        })?;

    let pm = measurement
        .to_picometers_i64()
        .ok_or_else(|| IrError::MissingRoutingHeuristics {
            field: "min_segment_length".into(),
            hint: "routing.min_segment_length must be a distance unit (nm, um, mm, ...).".into(),
        })?;
    Ok(pm / 1000)
}

/// Collapse a Manhattan path into line segments.
///
/// Drops collinear / same-axis backtracking middle points, and drops turns whose
/// distance from the current segment start is below `min_seg_len_nm`.
///
/// **IMPORTANT:** Diagonal segments (non-Manhattan moves like miters) are NEVER filtered
/// by min_seg_len_nm, as they represent intentional geometric features.
pub fn manhattan_path_to_segments(
    path: &[hwc_engine::Point3D],
    min_seg_len_nm: i64,
) -> Vec<hwc_engine::LineSegment> {
    eprintln!("[MANHATTAN_TO_SEGMENTS] Input path has {} waypoints, min_seg_len_nm={}", path.len(), min_seg_len_nm);
    for (idx, p) in path.iter().enumerate() {
        eprintln!("[MANHATTAN_TO_SEGMENTS]   waypoint[{}]: {:?}", idx, p);
    }
    
    if path.len() < 2 {
        return Vec::new();
    }

    let min_seg_len_sq = min_seg_len_nm.saturating_mul(min_seg_len_nm);
    eprintln!("[MANHATTAN_TO_SEGMENTS] min_seg_len_sq = {}", min_seg_len_sq);
    let mut segments = Vec::new();
    let mut start = path[0];

    for i in 1..path.len() - 1 {
        let p1 = path[i - 1];
        let p2 = path[i];
        let p3 = path[i + 1];

        let d1x = p2.x - p1.x;
        let d1y = p2.y - p1.y;
        let d1z = p2.z - p1.z;

        let d2x = p3.x - p2.x;
        let d2y = p3.y - p2.y;
        let d2z = p3.z - p2.z;

        let is_collinear = (d1x == 0 && d2x == 0 && d1y == 0 && d2y == 0 && d1z == 0 && d2z == 0)
            || (d1x == 0
                && d2x == 0
                && d1z == 0
                && d2z == 0
                && d1y.signum() == d2y.signum()
                && d1y != 0)
            || (d1y == 0
                && d2y == 0
                && d1z == 0
                && d2z == 0
                && d1x.signum() == d2x.signum()
                && d1x != 0)
            || (d1x == 0
                && d2x == 0
                && d1z == 0
                && d2z == 0
                && d1y != 0
                && d2y != 0
                && ((p1.y < p2.y && p2.y > p3.y) || (p1.y > p2.y && p2.y < p3.y)))
            || (d1y == 0
                && d2y == 0
                && d1z == 0
                && d2z == 0
                && d1x != 0
                && d2x != 0
                && ((p1.x < p2.x && p2.x > p3.x) || (p1.x > p2.x && p2.x < p3.x)));

        let seg_len_sq =
            (p2.x - start.x).pow(2) + (p2.y - start.y).pow(2) + (p2.z - start.z).pow(2);
        let is_short = seg_len_sq < min_seg_len_sq;

        // Check if the segment from start -> p2 is diagonal (non-Manhattan)
        // Diagonal segments are intentional geometric features (like miters) and should never be filtered
        let dx = (p2.x - start.x).abs();
        let dy = (p2.y - start.y).abs();
        let dz = (p2.z - start.z).abs();
        let is_diagonal = (dx > 0 && dy > 0) || (dx > 0 && dz > 0) || (dy > 0 && dz > 0);

        eprintln!("[MANHATTAN_TO_SEGMENTS] waypoint[{}] at {:?}:", i, p2);
        eprintln!("[MANHATTAN_TO_SEGMENTS]   is_collinear={}, is_short={} (seg_len_sq={} vs min={}), is_diagonal={}", 
                 is_collinear, is_short, seg_len_sq, min_seg_len_sq, is_diagonal);

        // Emit segment if:
        // 1. Not collinear AND (not short OR is diagonal)
        // 2. Not a duplicate point
        if !is_collinear && (!is_short || is_diagonal) && start != p2 {
            eprintln!("[MANHATTAN_TO_SEGMENTS]   → Emitting segment: {:?} -> {:?}", start, p2);
            segments.push(hwc_engine::LineSegment::new(start, p2));
            start = p2;
        } else {
            eprintln!("[MANHATTAN_TO_SEGMENTS]   → Skipping this waypoint");
        }
    }

    let last = path[path.len() - 1];
    if start != last {
        eprintln!("[MANHATTAN_TO_SEGMENTS] Emitting final segment: {:?} -> {:?}", start, last);
        segments.push(hwc_engine::LineSegment::new(start, last));
    }

    eprintln!("[MANHATTAN_TO_SEGMENTS] Final output: {} segments", segments.len());
    for (idx, seg) in segments.iter().enumerate() {
        eprintln!("[MANHATTAN_TO_SEGMENTS]   segment[{}]: {:?} -> {:?}", idx, seg.start, seg.end);
    }

    segments
}

/// Check if a route needs automatic routing (v0.1.7).
pub fn needs_automatic_routing(route: &hwc_parser::Route) -> bool {
    route.path.is_none()
}
