use crate::geometry::TraceSegment;
use crate::netlist::NetId;
use rustc_hash::FxHashMap;

/// Signal integrity constraints for a trace.
#[derive(Clone, Debug)]
pub struct SignalConstraints {
    pub net_id: NetId,
    /// Target characteristic impedance (ohms). None = no impedance target.
    pub target_impedance_ohms: Option<f64>,
    /// Maximum crosstalk (coupling coefficient). None = no crosstalk limit.
    pub max_crosstalk: Option<f64>,
    /// Maximum parallel run length before spacing must increase (nm).
    pub max_parallel_run_nm: i64,
    /// Minimum spacing for impedance control (nm).
    pub min_spacing_impedance_nm: i64,
    /// Minimum spacing for crosstalk control (nm).
    pub min_spacing_crosstalk_nm: i64,
}

/// A compaction move: a segment shifted by (dx, dy).
#[derive(Clone, Debug)]
pub struct CompactionMove {
    pub segment_id: usize,
    pub dx: i64,
    pub dy: i64,
}

/// The Constraint-Aware Compaction Engine.
///
/// Slides adjacent traces together to minimize layout area,
/// but caps movement at signal integrity limits (impedance,
/// crosstalk, clearance).
pub struct Compactor {
    /// Default minimum clearance (nm)
    pub default_clearance_nm: i64,
}

impl Compactor {
    pub fn new(default_clearance_nm: i64) -> Self {
        Self { default_clearance_nm }
    }

    /// Compute the minimum required spacing between two parallel traces
    /// based on their signal integrity constraints.
    #[inline]
    pub fn min_spacing(
        &self,
        a: &SignalConstraints,
        b: &SignalConstraints,
        parallel_run_nm: i64,
    ) -> i64 {
        let mut spacing = self.default_clearance_nm;

        // Impedance spacing
        if a.target_impedance_ohms.is_some() || b.target_impedance_ohms.is_some() {
            spacing = spacing.max(a.min_spacing_impedance_nm.max(b.min_spacing_impedance_nm));
        }

        // Crosstalk spacing (increases with parallel run length)
        if parallel_run_nm > 0 && a.max_crosstalk.is_some() && b.max_crosstalk.is_some() {
            let crosstalk_spacing = a.min_spacing_crosstalk_nm.max(b.min_spacing_crosstalk_nm);
            let scale = if a.max_parallel_run_nm > 0 {
                (parallel_run_nm as f64 / a.max_parallel_run_nm as f64).min(2.0)
            } else {
                1.0
            };
            let scaled_spacing = (crosstalk_spacing as f64 * scale) as i64;
            spacing = spacing.max(scaled_spacing);
        }

        spacing
    }

    /// Compute parallel run length between two segments.
    /// Returns 0 if they're not parallel or don't overlap.
    #[inline]
    pub fn parallel_run_length(a: &TraceSegment, b: &TraceSegment) -> i64 {
        // Both horizontal (same Y and Z)
        if a.start.y == a.end.y && b.start.y == b.end.y && a.start.y == b.start.y
            && a.start.z == a.end.z && b.start.z == b.end.z && a.start.z == b.start.z
        {
            let a_min_x = a.start.x.min(a.end.x);
            let a_max_x = a.start.x.max(a.end.x);
            let b_min_x = b.start.x.min(b.end.x);
            let b_max_x = b.start.x.max(b.end.x);
            let overlap_start = a_min_x.max(b_min_x);
            let overlap_end = a_max_x.min(b_max_x);
            return (overlap_end - overlap_start).max(0);
        }
        // Both vertical (same X and Z)
        if a.start.x == a.end.x && b.start.x == b.end.x && a.start.x == b.start.x
            && a.start.z == a.end.z && b.start.z == b.end.z && a.start.z == b.start.z
        {
            let a_min_y = a.start.y.min(a.end.y);
            let a_max_y = a.start.y.max(a.end.y);
            let b_min_y = b.start.y.min(b.end.y);
            let b_max_y = b.start.y.max(b.end.y);
            let overlap_start = a_min_y.max(b_min_y);
            let overlap_end = a_max_y.min(b_max_y);
            return (overlap_end - overlap_start).max(0);
        }
        0
    }

    /// Generate compaction moves for a set of segments.
    /// Slides traces together up to their signal integrity limits.
    #[inline]
    pub fn compact(
        &self,
        segments: &[TraceSegment],
        net_ids: &[NetId],
        constraints: &FxHashMap<NetId, SignalConstraints>,
    ) -> Vec<CompactionMove> {
        let mut moves = Vec::new();

        for (i, seg_a) in segments.iter().enumerate() {
            let net_a = net_ids.get(i).copied().unwrap_or(NetId(0));
            for (j, seg_b) in segments.iter().enumerate().skip(i + 1) {
                if seg_a.start.z != seg_b.start.z {
                    continue;
                }

                let run = Self::parallel_run_length(seg_a, seg_b);
                if run == 0 {
                    continue;
                }

                let net_b = net_ids.get(j).copied().unwrap_or(NetId(0));
                let constraints_a = constraints.get(&net_a);
                let constraints_b = constraints.get(&net_b);

                // Compute current spacing (perpendicular distance)
                let current_spacing = if seg_a.start.y == seg_a.end.y && seg_a.start.y == seg_b.start.y
                {
                    // Horizontal traces — spacing is Y difference
                    (seg_a.start.y - seg_b.start.y).abs()
                } else if seg_a.start.x == seg_a.end.x && seg_a.start.x == seg_b.start.x {
                    // Vertical traces — spacing is X difference
                    (seg_a.start.x - seg_b.start.x).abs()
                } else {
                    continue;
                };

                // Compute minimum allowed spacing
                let min_sp = if let (Some(ca), Some(cb)) = (constraints_a, constraints_b) {
                    self.min_spacing(ca, cb, run)
                } else {
                    self.default_clearance_nm
                };

                if current_spacing > min_sp {
                    let shift = (current_spacing - min_sp) / 2;
                    let (dx, dy) = if seg_a.start.y == seg_a.end.y && seg_a.start.y == seg_b.start.y {
                        // Horizontal traces — shift vertically
                        let dy = if seg_a.start.y > seg_b.start.y { -shift } else { shift };
                        (0, dy)
                    } else {
                        // Vertical traces — shift horizontally
                        let dx = if seg_a.start.x > seg_b.start.x { -shift } else { shift };
                        (dx, 0)
                    };
                    moves.push(CompactionMove {
                        segment_id: j,
                        dx,
                        dy,
                    });
                }
            }
        }

        moves
    }

    /// Apply compaction moves to segments.
    #[inline]
    pub fn apply_moves(segments: &[TraceSegment], moves: &[CompactionMove]) -> Vec<TraceSegment> {
        let mut result = segments.to_vec();
        for m in moves {
            if let Some(seg) = result.get_mut(m.segment_id) {
                seg.start.x += m.dx;
                seg.start.y += m.dy;
                seg.end.x += m.dx;
                seg.end.y += m.dy;
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Point3D;

    #[test]
    fn test_parallel_run_length_horizontal() {
        let a = TraceSegment::new(Point3D::new(0, 0, 1), Point3D::new(100, 0, 1), 10, 0);
        let b = TraceSegment::new(Point3D::new(50, 0, 1), Point3D::new(200, 0, 1), 10, 0);
        assert_eq!(Compactor::parallel_run_length(&a, &b), 50);
    }

    #[test]
    fn test_parallel_run_length_vertical() {
        let a = TraceSegment::new(Point3D::new(0, 0, 1), Point3D::new(0, 100, 1), 10, 0);
        let b = TraceSegment::new(Point3D::new(0, 50, 1), Point3D::new(0, 200, 1), 10, 0);
        assert_eq!(Compactor::parallel_run_length(&a, &b), 50);
    }

    #[test]
    fn test_parallel_run_length_not_parallel() {
        let a = TraceSegment::new(Point3D::new(0, 0, 1), Point3D::new(100, 0, 1), 10, 0);
        let b = TraceSegment::new(Point3D::new(0, 0, 1), Point3D::new(0, 100, 1), 10, 0);
        assert_eq!(Compactor::parallel_run_length(&a, &b), 0);
    }

    #[test]
    fn test_apply_moves() {
        let segments = vec![TraceSegment::new(
            Point3D::new(10, 20, 1),
            Point3D::new(100, 20, 1),
            10,
            0,
        )];
        let moves = vec![CompactionMove {
            segment_id: 0,
            dx: 5,
            dy: -3,
        }];
        let result = Compactor::apply_moves(&segments, &moves);
        assert_eq!(result[0].start, Point3D::new(15, 17, 1));
        assert_eq!(result[0].end, Point3D::new(105, 17, 1));
    }

    #[test]
    fn test_min_spacing_default() {
        let compactor = Compactor::new(200);
        let a = SignalConstraints {
            net_id: NetId(1),
            target_impedance_ohms: None,
            max_crosstalk: None,
            max_parallel_run_nm: 1000,
            min_spacing_impedance_nm: 0,
            min_spacing_crosstalk_nm: 0,
        };
        let b = SignalConstraints {
            net_id: NetId(2),
            target_impedance_ohms: None,
            max_crosstalk: None,
            max_parallel_run_nm: 1000,
            min_spacing_impedance_nm: 0,
            min_spacing_crosstalk_nm: 0,
        };
        assert_eq!(compactor.min_spacing(&a, &b, 100), 200);
    }
}
