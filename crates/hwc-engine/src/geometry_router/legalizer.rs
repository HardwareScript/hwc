use crate::geometry::{BoundingBox, Point3D, TraceSegment};
use crate::geometry_router::spatial_index::{DynamicSpatialIndex, IndexedSegment};

use crate::netlist::NetId;
use rustc_hash::FxHashMap;

/// A detected clearance violation between two traces.
#[derive(Clone, Debug)]
pub struct ClearanceViolation {
    pub violator_id: usize,
    pub victim_id: usize,
    pub violator_net: NetId,
    pub victim_net: NetId,
    pub overlap_bbox: BoundingBox,
    pub required_shift_nm: i64,
}

/// A localized legalization window — a small bounding box around a collision
/// where trace vectors are adjusted without triggering global re-routing.
#[derive(Clone, Debug)]
pub struct LegalizationWindow {
    pub bbox: BoundingBox,
    pub segment_ids: Vec<usize>,
    pub source_violation: ClearanceViolation,
    pub max_displacement_nm: i64,
}

/// A QP variable representing a trace segment inside a legalization window.
#[derive(Clone, Debug)]
pub struct QpVariable {
    pub segment_id: usize,
    pub original_x: i64,
    pub original_y: i64,
    pub optimized_x: i64,
    pub optimized_y: i64,
}

/// The Localized Legalization Engine.
pub struct Legalizer {
    pub max_nudge_nm: i64,
    pub min_clearance_nm: i64,
    pub window_margin_nm: i64,
}

impl Legalizer {
    pub fn new(min_clearance_nm: i64) -> Self {
        Self {
            max_nudge_nm: min_clearance_nm * 2,
            min_clearance_nm,
            window_margin_nm: min_clearance_nm,
        }
    }

    /// Compute the bounding box that encloses two overlapping segments' physical extents.
    #[inline]
    fn segment_overlap_bbox(seg_a: &TraceSegment, seg_b: &TraceSegment) -> BoundingBox {
        let bbox_a = seg_a.bounding_box();
        let bbox_b = seg_b.bounding_box();
        let min_x = bbox_a.min.x.max(bbox_b.min.x);
        let min_y = bbox_a.min.y.max(bbox_b.min.y);
        let max_x = bbox_a.max.x.min(bbox_b.max.x);
        let max_y = bbox_a.max.y.min(bbox_b.max.y);
        let z = seg_a.start.z.min(seg_b.start.z);
        BoundingBox {
            min: Point3D::new(min_x, min_y, z),
            max: Point3D::new(max_x, max_y, z),
        }
    }

    /// Compute the required shift to separate two overlapping segments.
    ///
    /// **v0.2.3: Asymmetric Constraint Formulation**
    /// Returns (shift_for_A, shift_for_B) based on frozen status:
    /// - Both mutable: symmetric nudge (shift/2 for each)
    /// - A frozen, B mutable: (0, full_shift) - B moves entirely
    /// - A mutable, B frozen: (full_shift, 0) - A moves entirely
    /// - Both frozen: (0, 0) - skip legalization, let DRC validate
    #[inline]
    fn required_shift_asymmetric(
        seg_a: &TraceSegment,
        seg_b: &TraceSegment,
        min_clearance: i64,
    ) -> (i64, i64) {
        let bbox_a = seg_a.bounding_box();
        let bbox_b = seg_b.bounding_box();

        let overlap_x = (bbox_a.max.x.min(bbox_b.max.x) - bbox_a.min.x.max(bbox_b.min.x)).max(0);
        let overlap_y = (bbox_a.max.y.min(bbox_b.max.y) - bbox_a.min.y.max(bbox_b.min.y)).max(0);

        let total_shift = if overlap_x > 0 && overlap_y > 0 {
            overlap_x.min(overlap_y) + min_clearance
        } else if overlap_x > 0 {
            overlap_x + min_clearance
        } else if overlap_y > 0 {
            overlap_y + min_clearance
        } else {
            let gap_x = if bbox_a.max.x <= bbox_b.min.x {
                bbox_b.min.x - bbox_a.max.x
            } else {
                bbox_a.min.x - bbox_b.max.x
            };
            let gap_y = if bbox_a.max.y <= bbox_b.min.y {
                bbox_b.min.y - bbox_a.max.y
            } else {
                bbox_a.min.y - bbox_b.max.y
            };
            let gap = gap_x.max(gap_y);
            if gap < min_clearance {
                min_clearance - gap
            } else {
                0
            }
        };

        // **v0.2.3: Asymmetric distribution based on frozen status**
        match (seg_a.is_frozen, seg_b.is_frozen) {
            (false, false) => {
                // Both mutable: symmetric nudge
                let half_shift = total_shift / 2;
                (half_shift, total_shift - half_shift)
            }
            (true, false) => {
                // A is frozen (child route): B takes full shift
                (0, total_shift)
            }
            (false, true) => {
                // B is frozen (child route): A takes full shift
                (total_shift, 0)
            }
            (true, true) => {
                // Both frozen (child routes from same/neighboring cells)
                // Skip legalization - let DRC validate placement spacing
                (0, 0)
            }
        }
    }

    pub fn detect_violations(
        &self,
        segments: &[TraceSegment],
        net_ids: &[NetId],
        spatial_index: &DynamicSpatialIndex,
    ) -> Vec<ClearanceViolation> {
        let mut violations = Vec::new();
        let mut seen_pairs = rustc_hash::FxHashSet::default();

        for (idx, seg) in segments.iter().enumerate() {
            let seg_net_id = net_ids.get(idx).map(|n| n.raw() as usize).unwrap_or(0);
            let half_w = seg.width_nm / 2;
            let query_bbox = BoundingBox {
                min: Point3D::new(
                    seg.start.x.min(seg.end.x) - half_w - self.min_clearance_nm,
                    seg.start.y.min(seg.end.y) - half_w - self.min_clearance_nm,
                    seg.start.z.min(seg.end.z),
                ),
                max: Point3D::new(
                    seg.start.x.max(seg.end.x) + half_w + self.min_clearance_nm,
                    seg.start.y.max(seg.end.y) + half_w + self.min_clearance_nm,
                    seg.start.z.max(seg.end.z),
                ),
            };

            let nearby = spatial_index.query_bbox(&query_bbox);
            for neighbor in nearby {
                if neighbor.segment_id <= idx {
                    continue;
                }

                let pair_key = (idx.min(neighbor.segment_id), idx.max(neighbor.segment_id));
                if !seen_pairs.insert(pair_key) {
                    continue;
                }

                let neighbor_seg = &segments[neighbor.segment_id];

                // **Same-Net Clearance Exemption**
                // Minimum clearance between elements of the SAME net is 0nm (touching, merging,
                // and T-junction taps are physically legal). min_clearance_nm strictly applies
                // to different nets (seg_net_id != neighbor_net_id).
                if neighbor.net_id.raw() as usize == seg_net_id {
                    continue;
                }

                // **v0.2.3: Skip frozen-frozen pairs** - these are pre-verified child cells
                if seg.is_frozen && neighbor_seg.is_frozen {
                    continue;
                }

                let (shift_a, shift_b) =
                    Self::required_shift_asymmetric(seg, neighbor_seg, self.min_clearance_nm);

                // Skip if no shift needed or both frozen
                if shift_a == 0 && shift_b == 0 {
                    continue;
                }

                let overlap = Self::segment_overlap_bbox(seg, neighbor_seg);

                violations.push(ClearanceViolation {
                    violator_id: idx,
                    victim_id: neighbor.segment_id,
                    violator_net: NetId(seg_net_id as u32),
                    victim_net: NetId::new(neighbor.net_id.raw()),
                    overlap_bbox: overlap,
                    required_shift_nm: shift_a.max(shift_b),
                });
            }
        }

        violations
    }


    pub fn create_window(
        &self,
        violation: &ClearanceViolation,
        segments: &[TraceSegment],
    ) -> LegalizationWindow {
        let mut bbox = violation.overlap_bbox;
        bbox = bbox.expand(self.window_margin_nm);

        if let Some(violator) = segments.get(violation.violator_id) {
            bbox = bbox.union(&violator.bounding_box());
        }
        if let Some(victim) = segments.get(violation.victim_id) {
            bbox = bbox.union(&victim.bounding_box());
        }
        bbox = bbox.expand(self.window_margin_nm);

        let mut segment_ids = Vec::new();
        for (idx, seg) in segments.iter().enumerate() {
            let seg_bbox = seg.bounding_box();
            if bbox_overlaps_2d(&bbox, &seg_bbox) {
                segment_ids.push(idx);
            }
        }

        LegalizationWindow {
            bbox,
            segment_ids,
            source_violation: violation.clone(),
            max_displacement_nm: self.max_nudge_nm,
        }
    }

    pub fn compute_nudge(
        &self,
        violation: &ClearanceViolation,
        segments: &[TraceSegment],
    ) -> (i64, i64) {
        let violator = match segments.get(violation.violator_id) {
            Some(s) => s,
            None => return (0, 0),
        };
        let victim = match segments.get(violation.victim_id) {
            Some(s) => s,
            None => return (0, 0),
        };

        // **v0.2.3: If violator is frozen, don't compute nudge for it**
        if violator.is_frozen {
            return (0, 0);
        }

        let violator_cx = (violator.start.x + violator.end.x) / 2;
        let violator_cy = (violator.start.y + violator.end.y) / 2;
        let victim_cx = (victim.start.x + victim.end.x) / 2;
        let victim_cy = (victim.start.y + victim.end.y) / 2;

        let dir_x = victim_cx - violator_cx;
        let dir_y = victim_cy - violator_cy;

        let shift = violation.required_shift_nm;

        if violator.is_horizontal() {
            if dir_y >= 0 {
                (0, -shift)
            } else {
                (0, shift)
            }
        } else if violator.is_vertical() {
            if dir_x >= 0 {
                (-shift, 0)
            } else {
                (shift, 0)
            }
        } else {
            let dist_sq = dir_x * dir_x + dir_y * dir_y;
            if dist_sq == 0 {
                (shift, 0)
            } else {
                let dist = approx_sqrt(dist_sq);
                if dist == 0 {
                    (shift, 0)
                } else {
                    let nx = -dir_y;
                    let ny = dir_x;
                    let n_len = approx_sqrt(nx * nx + ny * ny);
                    if n_len == 0 {
                        (shift, 0)
                    } else {
                        let scale = shift * 1_000_000 / n_len;
                        ((nx * scale) / 1_000_000, (ny * scale) / 1_000_000)
                    }
                }
            }
        }
    }

    /// Apply rigid-body nudges (uniform dx, dy per segment).
    ///
    /// Used by the flat `legalize()` path where elbow continuity is not tracked.
    pub fn apply_nudges(
        &self,
        segments: &[TraceSegment],
        _window: &LegalizationWindow,
        displacements: &[(usize, i64, i64)],
    ) -> Vec<TraceSegment> {
        let disp_map: FxHashMap<usize, (i64, i64)> = displacements
            .iter()
            .map(|&(id, dx, dy)| (id, (dx, dy)))
            .collect();

        segments
            .iter()
            .enumerate()
            .map(|(idx, seg)| {
                if let Some(&(dx, dy)) = disp_map.get(&idx) {
                    TraceSegment {
                        start: Point3D::new(seg.start.x + dx, seg.start.y + dy, seg.start.z),
                        end: Point3D::new(seg.end.x + dx, seg.end.y + dy, seg.end.z),
                        width_nm: seg.width_nm,
                        material_id: seg.material_id,
                        is_frozen: seg.is_frozen, // Preserve frozen status
                    }
                } else {
                    seg.clone()
                }
            })
            .collect()
    }

    /// **Flaw 2 & Fatal Bug 4 Fix: Manhattan Orthogonal Elbow Continuity Propagation**
    ///
    /// Given a set of raw rigid-body displacements `(seg_idx, dx, dy)`, propagate
    /// endpoint deltas to connected segments so that shared corner joints (elbows)
    /// do not snap open or slant into off-grid diagonal angles.
    ///
    /// Rules for Manhattan Orthogonality:
    /// - Nudging a horizontal segment in Y (perpendicular) adjusts the connected vertical segment's length.
    /// - Nudging a horizontal segment in X (parallel) shifts the ENTIRE connected vertical segment in X (both endpoints).
    /// - Nudging a vertical segment in X (perpendicular) adjusts the connected horizontal segment's length.
    /// - Nudging a vertical segment in Y (parallel) shifts the ENTIRE connected horizontal segment in Y (both endpoints).
    fn propagate_elbow_continuity(
        segments: &[TraceSegment],
        raw_displacements: &[(usize, i64, i64)],
    ) -> Vec<(usize, i64, i64, i64, i64)> {
        let mut start_dx = vec![0i64; segments.len()];
        let mut start_dy = vec![0i64; segments.len()];
        let mut end_dx = vec![0i64; segments.len()];
        let mut end_dy = vec![0i64; segments.len()];
        let mut has_delta = vec![false; segments.len()];

        let mut queue = Vec::new();

        for &(idx, dx, dy) in raw_displacements {
            if idx >= segments.len() || (dx == 0 && dy == 0) {
                continue;
            }
            start_dx[idx] += dx;
            start_dy[idx] += dy;
            end_dx[idx] += dx;
            end_dy[idx] += dy;
            has_delta[idx] = true;
            queue.push((idx, dx, dy));
        }

        while let Some((i, dx, dy)) = queue.pop() {
            if i >= segments.len() {
                continue;
            }
            let seg_i = &segments[i];
            let old_start_i = seg_i.start;
            let old_end_i = seg_i.end;
            let i_is_horiz = seg_i.is_horizontal();
            let i_is_vert = seg_i.is_vertical();

            for (j, seg_j) in segments.iter().enumerate() {
                if j == i || seg_j.is_frozen {
                    continue;
                }

                let j_touches_start = seg_j.start == old_start_i || seg_j.end == old_start_i;
                let j_touches_end = seg_j.start == old_end_i || seg_j.end == old_end_i;

                if !j_touches_start && !j_touches_end {
                    continue;
                }

                let j_at_start = seg_j.start == old_start_i || seg_j.start == old_end_i;
                let j_at_end = seg_j.end == old_start_i || seg_j.end == old_end_i;
                let j_is_horiz = seg_j.is_horizontal();
                let j_is_vert = seg_j.is_vertical();

                let mut added_dx_start = 0i64;
                let mut added_dy_start = 0i64;
                let mut added_dx_end = 0i64;
                let mut added_dy_end = 0i64;

                if i_is_horiz {
                    // Perpendicular Y shift -> adjusts vertical j's length at joint
                    if dy != 0 {
                        if j_at_start { added_dy_start += dy; }
                        if j_at_end { added_dy_end += dy; }
                    }
                    // Parallel X shift -> shifts entire vertical j along X to preserve orthogonality
                    if dx != 0 {
                        if j_is_vert {
                            added_dx_start += dx;
                            added_dx_end += dx;
                        } else {
                            if j_at_start { added_dx_start += dx; }
                            if j_at_end { added_dx_end += dx; }
                        }
                    }
                } else if i_is_vert {
                    // Perpendicular X shift -> adjusts horizontal j's length at joint
                    if dx != 0 {
                        if j_at_start { added_dx_start += dx; }
                        if j_at_end { added_dx_end += dx; }
                    }
                    // Parallel Y shift -> shifts entire horizontal j along Y to preserve orthogonality
                    if dy != 0 {
                        if j_is_horiz {
                            added_dy_start += dy;
                            added_dy_end += dy;
                        } else {
                            if j_at_start { added_dy_start += dy; }
                            if j_at_end { added_dy_end += dy; }
                        }
                    }
                } else {
                    // Non-orthogonal fallback
                    if j_at_start {
                        added_dx_start += dx;
                        added_dy_start += dy;
                    }
                    if j_at_end {
                        added_dx_end += dx;
                        added_dy_end += dy;
                    }
                }

                if added_dx_start != 0 || added_dy_start != 0 || added_dx_end != 0 || added_dy_end != 0 {
                    start_dx[j] += added_dx_start;
                    start_dy[j] += added_dy_start;
                    end_dx[j] += added_dx_end;
                    end_dy[j] += added_dy_end;

                    if !has_delta[j] {
                        has_delta[j] = true;
                        let j_dx = (added_dx_start + added_dx_end) / 2;
                        let j_dy = (added_dy_start + added_dy_end) / 2;
                        if j_dx != 0 || j_dy != 0 {
                            queue.push((j, j_dx, j_dy));
                        }
                    }
                }
            }
        }

        has_delta
            .iter()
            .enumerate()
            .filter(|(_, &has)| has)
            .map(|(idx, _)| (idx, start_dx[idx], start_dy[idx], end_dx[idx], end_dy[idx]))
            .collect()
    }

    /// Apply per-endpoint nudges (Flaw 2 fix: elbow-aware application).
    ///
    /// Each entry is `(seg_idx, dx_start, dy_start, dx_end, dy_end)`.
    /// Frozen segments are never moved.
    fn apply_nudges_with_elbow(
        segments: &[TraceSegment],
        deltas: &[(usize, i64, i64, i64, i64)],
    ) -> Vec<TraceSegment> {
        // Build a lookup: seg_idx → (dx_start, dy_start, dx_end, dy_end)
        let mut delta_map: FxHashMap<usize, (i64, i64, i64, i64)> = FxHashMap::default();
        for &(idx, dxs, dys, dxe, dye) in deltas {
            let entry = delta_map.entry(idx).or_insert((0, 0, 0, 0));
            entry.0 += dxs;
            entry.1 += dys;
            entry.2 += dxe;
            entry.3 += dye;
        }

        segments
            .iter()
            .enumerate()
            .map(|(idx, seg)| {
                if seg.is_frozen {
                    return seg.clone();
                }
                if let Some(&(dxs, dys, dxe, dye)) = delta_map.get(&idx) {
                    TraceSegment {
                        start: Point3D::new(
                            seg.start.x + dxs,
                            seg.start.y + dys,
                            seg.start.z,
                        ),
                        end: Point3D::new(
                            seg.end.x + dxe,
                            seg.end.y + dye,
                            seg.end.z,
                        ),
                        width_nm: seg.width_nm,
                        material_id: seg.material_id,
                        is_frozen: seg.is_frozen,
                    }
                } else {
                    seg.clone()
                }
            })
            .collect()
    }

    pub fn create_qp_variables(
        &self,
        segments: &[TraceSegment],
        window: &LegalizationWindow,
    ) -> Vec<QpVariable> {
        window
            .segment_ids
            .iter()
            .filter_map(|&id| {
                segments.get(id).map(|seg| {
                    let cx = (seg.start.x + seg.end.x) / 2;
                    let cy = (seg.start.y + seg.end.y) / 2;
                    QpVariable {
                        segment_id: id,
                        original_x: cx,
                        original_y: cy,
                        optimized_x: cx,
                        optimized_y: cy,
                    }
                })
            })
            .collect()
    }

    pub fn legalize(
        &self,
        segments: &[TraceSegment],
        net_ids: &[NetId],
        spatial_index: &DynamicSpatialIndex, // Use pre-configured index from caller
        max_iterations: usize,
    ) -> (Vec<TraceSegment>, Vec<NetId>) {
        let mut current = segments.to_vec();
        let current_net_ids = net_ids.to_vec();

        eprintln!(
            "[LEGALIZER DEBUG] Starting legalization with {} segments",
            current.len()
        );
        eprintln!(
            "[LEGALIZER DEBUG] Using caller-provided spatial index (layer-aware: {})",
            spatial_index.layer_z_ranges().is_some()
        );

        for _iter in 0..max_iterations {
            let violations = self.detect_violations(&current, &current_net_ids, spatial_index);
            if violations.is_empty() {
                eprintln!("[LEGALIZER DEBUG] No violations found - legalization complete");
                break;
            }

            eprintln!(
                "[LEGALIZER DEBUG] Found {} violations in iteration {}",
                violations.len(),
                _iter
            );

            let mut windows: Vec<LegalizationWindow> = violations
                .iter()
                .map(|v| self.create_window(v, &current))
                .collect();
            windows = merge_windows(&windows);

            let mut all_displacements: Vec<(usize, i64, i64)> = Vec::new();

            for window in &windows {
                let v = &window.source_violation;
                let (dx, dy) = self.compute_nudge(v, &current);
                if dx != 0 || dy != 0 {
                    all_displacements.push((v.violator_id, dx, dy));
                }
            }

            if all_displacements.is_empty() {
                eprintln!("[LEGALIZER DEBUG] No nudges computed - legalization stalled");
                break;
            }

            let empty_bbox = BoundingBox {
                min: Point3D::new(0, 0, 0),
                max: Point3D::new(0, 0, 0),
            };
            let empty_violation = ClearanceViolation {
                violator_id: 0,
                victim_id: 0,
                violator_net: NetId(0),
                victim_net: NetId(0),
                overlap_bbox: empty_bbox,
                required_shift_nm: 0,
            };
            let empty_window = LegalizationWindow {
                bbox: empty_bbox,
                segment_ids: Vec::new(),
                source_violation: empty_violation,
                max_displacement_nm: 0,
            };
            let window_ref = windows.first().unwrap_or(&empty_window);
            current = self.apply_nudges(&current, window_ref, &all_displacements);

            // Note: Caller must rebuild spatial index with updated segments for next iteration
        }

        (current, current_net_ids)
    }

    /// **v0.2.4: Hierarchical Legalization Engine (CORRECTED)**
    ///
    /// Legalizes parent-level routes while treating child-instance routes as static obstacles.
    ///
    /// # Flaw 1 Fix (Stale Spatial Index):
    /// Takes ownership of the initial `spatial_index`. At the end of every iteration, the
    /// parent region of the index is rebuilt from the updated `all_segments` positions before
    /// the next `detect_violations` call. The frozen child/obstacle segments are kept verbatim.
    ///
    /// # Flaw 2 Fix (Elbow Continuity):
    /// Uses `propagate_elbow_continuity` + `apply_nudges_with_elbow` so that nudging segment i
    /// also stretches the shared corner joint with adjacent segments i-1 and i+1.
    ///
    /// # Arguments
    /// * `parent_segments` - Mutable parent-level routes (can be nudged)
    /// * `parent_net_ids` - Net IDs corresponding to parent segments
    /// * `child_segments` - Immutable child-instance routes (fixed obstacles)
    /// * `child_net_ids` - Net IDs corresponding to child segments
    /// * `spatial_index` - Owned spatial index (will be rebuilt each iteration)
    /// * `max_iterations` - Maximum number of legalization iterations
    ///
    /// # Returns
    /// Legalized parent segments (child segments are unchanged)
    pub fn legalize_hierarchical(
        &self,
        parent_segments: &[TraceSegment],
        parent_net_ids: &[NetId],
        child_segments: &[TraceSegment],
        child_net_ids: &[NetId],
        mut spatial_index: DynamicSpatialIndex, // **Flaw 1 Fix: owned, rebuilt each iteration**
        max_iterations: usize,
    ) -> (Vec<TraceSegment>, Vec<NetId>) {
        // Combine parent and child segments for violation detection.
        // Parent segments come first so indices 0..parent_count refer to parents.
        let parent_count = parent_segments.len();
        let mut all_segments = parent_segments.to_vec();
        all_segments.extend_from_slice(child_segments);

        let mut all_net_ids = parent_net_ids.to_vec();
        all_net_ids.extend_from_slice(child_net_ids);

        eprintln!(
            "[HIERARCHICAL LEGALIZER] Starting legalization: {} parent segments, {} child segments (frozen)",
            parent_count,
            child_segments.len()
        );

        let z_ranges = spatial_index.layer_z_ranges();

        for iter in 0..max_iterations {
            // **Flaw 1 Fix: detect_violations always uses up-to-date positions**
            let violations = self.detect_violations(&all_segments, &all_net_ids, &spatial_index);

            // Filter to only violations involving at least one parent segment
            let parent_violations: Vec<_> = violations
                .into_iter()
                .filter(|v| v.violator_id < parent_count || v.victim_id < parent_count)
                .collect();

            if parent_violations.is_empty() {
                eprintln!(
                    "[HIERARCHICAL LEGALIZER] No parent violations found - legalization complete at iteration {}",
                    iter
                );
                break;
            }

            eprintln!(
                "[HIERARCHICAL LEGALIZER] Iteration {}: {} violations involving parent routes",
                iter,
                parent_violations.len()
            );

            // Collect raw rigid-body nudges for parent segments only.
            let mut raw_displacements: Vec<(usize, i64, i64)> = Vec::new();

            for violation in &parent_violations {
                let (dx, dy) = self.compute_nudge(violation, &all_segments);
                if (dx != 0 || dy != 0) && violation.violator_id < parent_count {
                    // Only nudge parent segments
                    raw_displacements.push((violation.violator_id, dx, dy));
                }
            }

            if raw_displacements.is_empty() {
                eprintln!(
                    "[HIERARCHICAL LEGALIZER] No nudges computed - legalization stalled at iteration {}",
                    iter
                );
                break;
            }

            eprintln!(
                "[HIERARCHICAL LEGALIZER] Applying {} raw nudges (with elbow continuity propagation)",
                raw_displacements.len()
            );

            // **Flaw 2 Fix: Propagate corner elbow continuity before applying**
            // This prevents connected segments from snapping open at shared joints.
            let elbow_deltas =
                Self::propagate_elbow_continuity(&all_segments, &raw_displacements);

            // Apply per-endpoint nudges (preserves elbow joints, skips frozen segments)
            all_segments = Self::apply_nudges_with_elbow(&all_segments, &elbow_deltas);

            // **Flaw 1 & 3 Fix: Rebuild spatial index with preserved layer Z-ranges and stackup thickness.**
            spatial_index = rebuild_spatial_index(&all_segments, &all_net_ids, z_ranges.as_deref());

            eprintln!(
                "[HIERARCHICAL LEGALIZER] Rebuilt spatial index with {} entries after iteration {}",
                spatial_index.len(),
                iter
            );
        }

        // Extract legalized parent segments
        let legalized_parent = all_segments[..parent_count].to_vec();
        let parent_nets = all_net_ids[..parent_count].to_vec();

        (legalized_parent, parent_nets)
    }
}

/// **Spatial index rebuild helper.**
///
/// Constructs a fresh `DynamicSpatialIndex` from the current segment positions while
/// preserving layer Z-ranges and stackup layer thickness to prevent false cross-layer 3D collisions.
pub fn rebuild_spatial_index(
    segments: &[TraceSegment],
    net_ids: &[NetId],
    z_ranges: Option<&[(i64, i64)]>,
) -> DynamicSpatialIndex {
    let mut index = DynamicSpatialIndex::new();
    if let Some(ranges) = z_ranges {
        index.set_layer_z_ranges(ranges);
    }
    for (idx, (seg, net_id)) in segments.iter().zip(net_ids.iter()).enumerate() {
        let layer_z = seg.start.z;
        let z_span = (seg.start.z - seg.end.z).abs();

        let thickness_nm = if z_span > 0 {
            z_span
        } else if let Some(ranges) = z_ranges {
            ranges
                .iter()
                .find(|&&(z_min, z_max)| layer_z >= z_min && layer_z <= z_max)
                .map(|&(z_min, z_max)| (z_max - z_min).max(1))
                .unwrap_or(10)
        } else {
            10
        };

        index.insert(IndexedSegment::new(
            hwc_physics::SpatialEntitySource::RouteSegment {
                net_idx: net_id.raw() as usize,
                seg_idx: idx,
            },
            idx,
            *net_id,
            seg,
            layer_z,
            thickness_nm,
        ));
    }
    index
}

/// Merge overlapping legalization windows to avoid redundant solving.
pub fn merge_windows(windows: &[LegalizationWindow]) -> Vec<LegalizationWindow> {
    if windows.is_empty() {
        return Vec::new();
    }

    let mut merged: Vec<LegalizationWindow> = windows.to_vec();
    let mut changed = true;

    while changed {
        changed = false;
        let mut new_merged = Vec::new();
        let mut used = vec![false; merged.len()];

        for i in 0..merged.len() {
            if used[i] {
                continue;
            }

            let mut current = merged[i].clone();

            for j in (i + 1)..merged.len() {
                if used[j] {
                    continue;
                }

                if bbox_overlaps_2d(&current.bbox, &merged[j].bbox) {
                    current.bbox = current.bbox.union(&merged[j].bbox);
                    let mut all_ids: Vec<usize> = current
                        .segment_ids
                        .iter()
                        .chain(merged[j].segment_ids.iter())
                        .copied()
                        .collect();
                    all_ids.sort_unstable();
                    all_ids.dedup();
                    current.segment_ids = all_ids;
                    current.max_displacement_nm = current
                        .max_displacement_nm
                        .max(merged[j].max_displacement_nm);
                    used[j] = true;
                    changed = true;
                }
            }

            new_merged.push(current);
        }

        merged = new_merged;
    }

    merged
}

/// Check if two bounding boxes overlap (2D, ignoring Z).
#[inline]
pub fn bbox_overlaps_2d(a: &BoundingBox, b: &BoundingBox) -> bool {
    a.min.x < b.max.x && a.max.x > b.min.x && a.min.y < b.max.y && a.max.y > b.min.y
}

/// Approximate integer square root via Newton's method.
#[inline]
fn approx_sqrt(n: i64) -> i64 {
    if n <= 0 {
        return 0;
    }
    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}
