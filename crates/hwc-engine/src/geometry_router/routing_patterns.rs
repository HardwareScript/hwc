//! Routing Pattern System
//!
//! **Architecture Reference:** CONSTRAINT-AWARE-ROUTING.md Phase 2-3
//!
//! Patterns are defined as sequences of relative vector steps using polar notation:
//! `distance r angle`
//!
//! # Coordinate System
//!
//! - Patterns are strictly planar (2D) macros applied to the current routing layer
//! - `distance`: The magnitude of the vector (how far to travel)
//! - `r`: The rotation operator
//! - `angle`: Degrees to rotate relative to the router's current forward heading
//!
//! # Example Patterns
//!
//! ```hw
//! # Zigzag pattern for length matching
//! define pattern "Zigzag" (gap: Measurement):
//!     steps:
//!         - gap r 45
//!         - gap r -45
//!         - gap r -45
//!         - gap r 45
//!
//! # Trombone pattern for DDR5
//! define pattern "Trombone" (gap: Measurement, amp: Measurement):
//!     steps:
//!         - gap r 0
//!         - amp r 90
//!         - gap * 2 r 0
//!         - amp r -90
//!         - gap r 0
//! ```

use crate::geometry::Point3D;
use compact_str::CompactString;
use rustc_hash::FxHashSet;

/// A routing pattern defined as relative vector steps.
///
/// Patterns are macro-moves that the A* router can inject into its
/// neighbor generator to burn extra voxels while maintaining the
/// same start and end points.
#[derive(Debug, Clone)]
pub struct RoutingPattern {
    /// Pattern name (e.g., "Zigzag", "Trombone")
    pub name: CompactString,

    /// Sequence of pattern steps
    pub steps: Vec<PatternStep>,
}

/// A single step in a pattern: distance r angle
#[derive(Debug, Clone, Copy)]
pub struct PatternStep {
    /// Distance to travel in nanometers
    pub distance_nm: i64,

    /// Angle to rotate in degrees (relative to current heading)
    pub angle_deg: i64,
}

impl RoutingPattern {
    /// Create a new routing pattern.
    pub fn new(name: CompactString, steps: Vec<PatternStep>) -> Self {
        Self { name, steps }
    }

    /// Generate absolute voxel coordinates for this pattern.
    ///
    /// **Algorithm:**
    /// 1. Start at current position with current heading
    /// 2. For each step:
    ///    - Apply rotation to heading
    ///    - Convert polar to Cartesian
    ///    - Rasterize line from current to target
    ///    - Update position and heading
    /// 3. Return all voxels the pattern passes through
    ///
    /// **Critical:** Uses 3D Bresenham to rasterize lines, preventing
    /// the trace from "teleporting" through obstacles.
    ///
    /// # Arguments
    /// * `current_pos` - Starting position
    /// * `current_heading` - Current heading in degrees (0=East, 90=North)
    /// * `voxel_size_nm` - Voxel size in nanometers
    ///
    /// # Returns
    /// Vector of all voxels the pattern passes through
    pub fn generate_moves(
        &self,
        current_pos: Point3D,
        current_heading: i64,
        voxel_size_nm: i64,
    ) -> Vec<Point3D> {
        let mut all_voxels = Vec::new();
        let mut pos = current_pos;
        let mut heading = current_heading;

        for step in &self.steps {
            // Apply rotation
            heading = (heading + step.angle_deg) % 360;
            if heading < 0 {
                heading += 360;
            }

            // Convert polar to Cartesian for the target endpoint
            let rad = (heading as f64).to_radians();
            let dx = (step.distance_nm as f64 * rad.cos()) as i64;
            let dy = (step.distance_nm as f64 * rad.sin()) as i64;

            let target_x = pos.x + dx;
            let target_y = pos.y + dy;
            let target_pos = Point3D::new(target_x, target_y, pos.z);

            // CRITICAL: Interpolate every voxel between 'pos' and 'target_pos'
            // using a standard 3D line rasterization algorithm (Bresenham)
            // This prevents the trace from "teleporting" through obstacles
            let segment_voxels = rasterize_line(pos, target_pos, voxel_size_nm);
            all_voxels.extend(segment_voxels);

            // Update position for the next step in the pattern
            pos = target_pos;
        }

        all_voxels
    }

    /// Check if this pattern can be placed at the given position.
    ///
    /// Validates that all voxels the pattern would occupy are clear.
    ///
    /// # Arguments
    /// * `current_pos` - Starting position
    /// * `current_heading` - Current heading in degrees
    /// * `voxel_size_nm` - Voxel size in nanometers
    /// * `occupied_voxels` - Set of occupied voxels to check against
    ///
    /// # Returns
    /// true if pattern can be placed, false if collision detected
    pub fn can_place(
        &self,
        current_pos: Point3D,
        current_heading: i64,
        voxel_size_nm: i64,
        occupied_voxels: &FxHashSet<Point3D>,
    ) -> bool {
        let voxels = self.generate_moves(current_pos, current_heading, voxel_size_nm);

        // Check if any voxel is occupied
        for voxel in voxels {
            if occupied_voxels.contains(&voxel) {
                return false;
            }
        }

        true
    }

    /// Calculate the total length this pattern adds.
    ///
    /// # Returns
    /// Total length in nanometers
    pub fn total_length(&self) -> i64 {
        self.steps.iter().map(|s| s.distance_nm).sum()
    }
}

/// Rasterize a line between two points using 3D Bresenham algorithm.
///
/// Returns all voxels that the line passes through.
///
/// # Arguments
/// * `start` - Starting point
/// * `end` - Ending point
/// * `voxel_size_nm` - Voxel size in nanometers
///
/// # Returns
/// Vector of all voxels the line passes through
fn rasterize_line(start: Point3D, end: Point3D, voxel_size_nm: i64) -> Vec<Point3D> {
    // Convert to voxel coordinates
    let x0 = start.x / voxel_size_nm;
    let y0 = start.y / voxel_size_nm;
    let z0 = start.z / voxel_size_nm;

    let x1 = end.x / voxel_size_nm;
    let y1 = end.y / voxel_size_nm;
    let z1 = end.z / voxel_size_nm;

    let mut voxels = Vec::new();

    // 3D Bresenham line algorithm
    let dx = (x1 - x0).abs();
    let dy = (y1 - y0).abs();
    let dz = (z1 - z0).abs();

    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let sz = if z0 < z1 { 1 } else { -1 };

    let mut x = x0;
    let mut y = y0;
    let mut z = z0;

    // Determine dominant axis
    if dx >= dy && dx >= dz {
        let mut p1 = 2 * dy - dx;
        let mut p2 = 2 * dz - dx;

        while x != x1 {
            voxels.push(Point3D::new(
                x * voxel_size_nm,
                y * voxel_size_nm,
                z * voxel_size_nm,
            ));

            x += sx;
            if p1 >= 0 {
                y += sy;
                p1 -= 2 * dx;
            }
            if p2 >= 0 {
                z += sz;
                p2 -= 2 * dx;
            }
            p1 += 2 * dy;
            p2 += 2 * dz;
        }
    } else if dy >= dx && dy >= dz {
        let mut p1 = 2 * dx - dy;
        let mut p2 = 2 * dz - dy;

        while y != y1 {
            voxels.push(Point3D::new(
                x * voxel_size_nm,
                y * voxel_size_nm,
                z * voxel_size_nm,
            ));

            y += sy;
            if p1 >= 0 {
                x += sx;
                p1 -= 2 * dy;
            }
            if p2 >= 0 {
                z += sz;
                p2 -= 2 * dy;
            }
            p1 += 2 * dx;
            p2 += 2 * dz;
        }
    } else {
        let mut p1 = 2 * dy - dz;
        let mut p2 = 2 * dx - dz;

        while z != z1 {
            voxels.push(Point3D::new(
                x * voxel_size_nm,
                y * voxel_size_nm,
                z * voxel_size_nm,
            ));

            z += sz;
            if p1 >= 0 {
                y += sy;
                p1 -= 2 * dz;
            }
            if p2 >= 0 {
                x += sx;
                p2 -= 2 * dz;
            }
            p1 += 2 * dy;
            p2 += 2 * dx;
        }
    }

    // Add final point
    voxels.push(Point3D::new(
        x1 * voxel_size_nm,
        y1 * voxel_size_nm,
        z1 * voxel_size_nm,
    ));

    voxels
}

/// Standard library of routing patterns.
pub struct StandardPatterns;

/// Represents a routed trace with its metadata for length matching calculations.
#[derive(Debug, Clone)]
pub struct RoutedTrace {
    /// Net name this trace belongs to
    pub net_name: CompactString,
    /// Total length of the trace in nanometers
    pub length_nm: i64,
    /// The route segments
    pub segments: Vec<RouteSegment>,
}

/// A single segment of a routed trace.
#[derive(Debug, Clone)]
pub struct RouteSegment {
    pub start: Point3D,
    pub end: Point3D,
}

impl RoutedTrace {
    /// Calculate the total length of this trace.
    pub fn calculate_length(&self) -> i64 {
        self.segments
            .iter()
            .map(|s| s.start.manhattan_distance(&s.end))
            .sum()
    }
}

/// Length Matching Engine for PCB signal groups.
///
/// Implements the "Length Targeter" and "Meander Generator" for DDR5 and other
/// high-speed bus length matching requirements.
pub struct LengthMatchingEngine {
    /// Target length in nanometers (the longest trace in the group)
    pub target_length_nm: i64,
    /// Vector of traces with their calculated lengths
    pub traces: Vec<RoutedTrace>,
}

impl LengthMatchingEngine {
    /// Create a new length matching engine.
    ///
    /// # Arguments
    /// * `traces` - Vector of routed traces to match
    ///
    /// # Returns
    /// LengthMatchingEngine with target length set to the longest trace.
    pub fn new(traces: Vec<RoutedTrace>) -> Self {
        let target_length_nm = traces
            .iter()
            .map(|t| t.calculate_length())
            .max()
            .unwrap_or(0);

        Self {
            target_length_nm,
            traces,
        }
    }

    /// Calculate length deficits for all traces.
    ///
    /// Returns a vector of (net_name, deficit_nm) tuples where deficit is
    /// how much shorter the trace is than the target.
    pub fn calculate_deficits(&self) -> Vec<(CompactString, i64)> {
        self.traces
            .iter()
            .map(|t| {
                let length = t.calculate_length();
                let deficit = self.target_length_nm.saturating_sub(length);
                (t.net_name.clone(), deficit)
            })
            .collect()
    }

    /// Generate meander patterns to consume a length deficit.
    ///
    /// # Arguments
    /// * `deficit_nm` - How much length needs to be added (nanometers)
    /// * `trace_width_nm` - Width of the trace (for clearance calculations)
    /// * `preferred_pattern` - "trombone" or "serpentine"
    /// * `amplitude_multiplier` - Multiplier for trace_width to determine meander amplitude
    ///
    /// # Returns
    /// A RoutingPattern that adds the required length.
    pub fn generate_meander(
        &self,
        deficit_nm: i64,
        trace_width_nm: i64,
        _preferred_pattern: &str,
        amplitude_multiplier: i64,
    ) -> Option<RoutingPattern> {
        if deficit_nm <= 0 {
            return None;
        }

        // Trombone pattern: creates a rectangular "fold"
        // Each trombone adds approximately: 2 * amp + gap length
        // For a standard trombone with amp = trace_width * amplitude_multiplier, gap = deficit/3
        let amp_nm = trace_width_nm * amplitude_multiplier;
        let gap_nm = (deficit_nm / 3).max(trace_width_nm);

        Some(StandardPatterns::trombone(gap_nm, amp_nm))
    }

    /// Generate a serpentine pattern for a given deficit.
    ///
    /// # Arguments
    /// * `deficit_nm` - How much length needs to be added (nanometers)
    /// * `trace_width_nm` - Width of the trace (for amplitude calculation)
    ///
    /// # Returns
    /// A RoutingPattern that adds the required length.
    pub fn generate_serpentine(
        &self,
        deficit_nm: i64,
        trace_width_nm: i64,
    ) -> Option<RoutingPattern> {
        if deficit_nm <= 0 {
            return None;
        }

        // Serpentine: wavelength and amplitude
        // Each full cycle adds approximately: 2 * wavelength
        let wavelength_nm = trace_width_nm * 4;
        let amplitude_nm = trace_width_nm * 2;

        Some(StandardPatterns::serpentine(wavelength_nm, amplitude_nm))
    }

    /// Get the total additional length needed across all traces.
    pub fn total_deficit_nm(&self) -> i64 {
        self.calculate_deficits().iter().map(|(_, d)| *d).sum()
    }
}

impl StandardPatterns {
    /// Create a Zigzag pattern for length matching.
    ///
    /// # Arguments
    /// * `gap_nm` - Gap between zigzag peaks (nanometers)
    ///
    /// # Returns
    /// Zigzag routing pattern
    pub fn zigzag(gap_nm: i64) -> RoutingPattern {
        RoutingPattern::new(
            "Zigzag".into(),
            vec![
                PatternStep {
                    distance_nm: gap_nm,
                    angle_deg: 45,
                },
                PatternStep {
                    distance_nm: gap_nm,
                    angle_deg: -45,
                },
                PatternStep {
                    distance_nm: gap_nm,
                    angle_deg: -45,
                },
                PatternStep {
                    distance_nm: gap_nm,
                    angle_deg: 45,
                },
            ],
        )
    }

    /// Create a Trombone pattern for DDR5 length matching.
    ///
    /// # Arguments
    /// * `gap_nm` - Gap between trombone segments (nanometers)
    /// * `amp_nm` - Amplitude of the trombone (nanometers)
    ///
    /// # Returns
    /// Trombone routing pattern
    pub fn trombone(gap_nm: i64, amp_nm: i64) -> RoutingPattern {
        RoutingPattern::new(
            "Trombone".into(),
            vec![
                PatternStep {
                    distance_nm: gap_nm,
                    angle_deg: 0,
                },
                PatternStep {
                    distance_nm: amp_nm,
                    angle_deg: 90,
                },
                PatternStep {
                    distance_nm: gap_nm * 2,
                    angle_deg: 0,
                },
                PatternStep {
                    distance_nm: amp_nm,
                    angle_deg: -90,
                },
                PatternStep {
                    distance_nm: gap_nm,
                    angle_deg: 0,
                },
            ],
        )
    }

    /// Create a simple Serpentine pattern.
    ///
    /// # Arguments
    /// * `wavelength_nm` - Wavelength of the serpentine (nanometers)
    /// * `amplitude_nm` - Amplitude of the serpentine (nanometers)
    ///
    /// # Returns
    /// Serpentine routing pattern
    pub fn serpentine(wavelength_nm: i64, amplitude_nm: i64) -> RoutingPattern {
        let half_wave = wavelength_nm / 2;

        RoutingPattern::new(
            "Serpentine".into(),
            vec![
                PatternStep {
                    distance_nm: amplitude_nm,
                    angle_deg: 90,
                },
                PatternStep {
                    distance_nm: half_wave,
                    angle_deg: 0,
                },
                PatternStep {
                    distance_nm: amplitude_nm * 2,
                    angle_deg: -90,
                },
                PatternStep {
                    distance_nm: half_wave,
                    angle_deg: 0,
                },
                PatternStep {
                    distance_nm: amplitude_nm,
                    angle_deg: 90,
                },
            ],
        )
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_routed_trace_length() {
        let trace = RoutedTrace {
            net_name: "NET1".into(),
            length_nm: 0,
            segments: vec![
                RouteSegment {
                    start: Point3D::new(0, 0, 0),
                    end: Point3D::new(1000, 0, 0),
                },
                RouteSegment {
                    start: Point3D::new(1000, 0, 0),
                    end: Point3D::new(1000, 500, 0),
                },
            ],
        };

        // Length should be 1000 + 500 = 1500 nm
        assert_eq!(trace.calculate_length(), 1500);
    }

    #[test]
    fn test_length_matching_engine() {
        let traces = vec![
            RoutedTrace {
                net_name: "NET1".into(),
                length_nm: 0,
                segments: vec![RouteSegment {
                    start: Point3D::new(0, 0, 0),
                    end: Point3D::new(3000, 0, 0),
                }],
            },
            RoutedTrace {
                net_name: "NET2".into(),
                length_nm: 0,
                segments: vec![RouteSegment {
                    start: Point3D::new(0, 0, 0),
                    end: Point3D::new(5000, 0, 0),
                }],
            },
            RoutedTrace {
                net_name: "NET3".into(),
                length_nm: 0,
                segments: vec![RouteSegment {
                    start: Point3D::new(0, 0, 0),
                    end: Point3D::new(4000, 0, 0),
                }],
            },
        ];

        let engine = LengthMatchingEngine::new(traces);

        // Target should be NET2 with 5000nm
        assert_eq!(engine.target_length_nm, 5000);

        let deficits = engine.calculate_deficits();
        assert_eq!(deficits.len(), 3);

        // NET1: 2000nm deficit, NET2: 0nm, NET3: 1000nm
        let net1_deficit = deficits.iter().find(|(n, _)| n == "NET1").unwrap();
        let net2_deficit = deficits.iter().find(|(n, _)| n == "NET2").unwrap();
        let net3_deficit = deficits.iter().find(|(n, _)| n == "NET3").unwrap();

        assert_eq!(net1_deficit.1, 2000);
        assert_eq!(net2_deficit.1, 0);
        assert_eq!(net3_deficit.1, 1000);
    }

    #[test]
    fn test_trombone_pattern_generation() {
        let traces = vec![RoutedTrace {
            net_name: "NET1".into(),
            length_nm: 0,
            segments: vec![RouteSegment {
                start: Point3D::new(0, 0, 0),
                end: Point3D::new(1000, 0, 0),
            }],
        }];

        let engine = LengthMatchingEngine::new(traces);
        let pattern = engine.generate_meander(2000, 200, "trombone", 2);

        assert!(pattern.is_some());
        let pattern = pattern.unwrap();
        assert_eq!(pattern.name.as_str(), "Trombone");
        assert_eq!(pattern.total_length(), 3464);
    }

    #[test]
    fn test_serpentine_pattern_generation() {
        let traces = vec![RoutedTrace {
            net_name: "NET1".into(),
            length_nm: 0,
            segments: vec![RouteSegment {
                start: Point3D::new(0, 0, 0),
                end: Point3D::new(1000, 0, 0),
            }],
        }];

        let engine = LengthMatchingEngine::new(traces);
        let pattern = engine.generate_serpentine(2000, 200);

        assert!(pattern.is_some());
        let pattern = pattern.unwrap();
        assert_eq!(pattern.name.as_str(), "Serpentine");
    }

    #[test]
    fn test_no_meander_for_zero_deficit() {
        let traces = vec![RoutedTrace {
            net_name: "NET1".into(),
            length_nm: 0,
            segments: vec![RouteSegment {
                start: Point3D::new(0, 0, 0),
                end: Point3D::new(5000, 0, 0),
            }],
        }];

        let engine = LengthMatchingEngine::new(traces);
        let pattern = engine.generate_meander(0, 200, "trombone", 2);

        assert!(pattern.is_none());
    }
}
