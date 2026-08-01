use crate::geometry::{BoundingBox, Point3D, TraceSegment};
use crate::geometry_router::spatial_index::DynamicSpatialIndex;

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
    #[inline]
    fn required_shift(seg_a: &TraceSegment, seg_b: &TraceSegment, min_clearance: i64) -> i64 {
        let bbox_a = seg_a.bounding_box();
        let bbox_b = seg_b.bounding_box();

        let overlap_x = (bbox_a.max.x.min(bbox_b.max.x) - bbox_a.min.x.max(bbox_b.min.x)).max(0);
        let overlap_y = (bbox_a.max.y.min(bbox_b.max.y) - bbox_a.min.y.max(bbox_b.min.y)).max(0);

        if overlap_x > 0 && overlap_y > 0 {
            overlap_x.max(overlap_y) + min_clearance
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

                let neighbor_seg = TraceSegment {
                    start: neighbor.start,
                    end: neighbor.end,
                    width_nm: neighbor.width_nm,
                    material_id: 0,
                };

                // Skip same-net overlaps (these are legal T-junctions or taps)
                if neighbor.net_id.raw() as usize == seg_net_id {
                    continue;
                }

                let shift = Self::required_shift(seg, &neighbor_seg, self.min_clearance_nm);
                if shift <= 0 {
                    continue;
                }

                let overlap = Self::segment_overlap_bbox(seg, &neighbor_seg);

                violations.push(ClearanceViolation {
                    violator_id: idx,
                    victim_id: neighbor.segment_id,
                    violator_net: NetId(seg_net_id as u32),
                    victim_net: NetId::new(neighbor.net_id.raw()),
                    overlap_bbox: overlap,
                    required_shift_nm: shift,
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
        spatial_index: &DynamicSpatialIndex,  // Use pre-configured index from caller
        max_iterations: usize,
    ) -> (Vec<TraceSegment>, Vec<NetId>) {
        let mut current = segments.to_vec();
        let current_net_ids = net_ids.to_vec();
        
        eprintln!("[LEGALIZER DEBUG] Starting legalization with {} segments", current.len());
        eprintln!("[LEGALIZER DEBUG] Using caller-provided spatial index (layer-aware: {})", 
            spatial_index.layer_z_ranges().is_some());

        for _iter in 0..max_iterations {
            let violations = self.detect_violations(&current, &current_net_ids, spatial_index);
            if violations.is_empty() {
                eprintln!("[LEGALIZER DEBUG] No violations found - legalization complete");
                break;
            }

            eprintln!("[LEGALIZER DEBUG] Found {} violations in iteration {}", violations.len(), _iter);

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
