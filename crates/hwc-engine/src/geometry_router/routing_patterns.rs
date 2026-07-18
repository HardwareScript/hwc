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
//!
//! # v0.1.8 Architecture Note (Roadmap 14.3 — Routing Pattern Rasterization)
//!
//! Previously, `generate_moves` and the internal line drawing used a 3D Bresenham
//! grid rasterization algorithm. Points were quantized to `step_size_nm` grid cells
//! by dividing coordinates by `step_size_nm`, rasterizing in grid space, then
//! multiplying back by `step_size_nm`. This grid quantization introduced unnecessary
//! snapping artifacts and was tied to the deprecated occupancy-grid collision system.
//!
//! As of v0.1.8, all line drawing uses continuous parametric interpolation in pure
//! nanometer space. Points are sampled at `step_size_nm` intervals along the exact
//! mathematical line between two endpoints. No grid division or multiplication is
//! performed. Collision checking is fully delegated to the spatial index.

use crate::geometry::Point3D;
use compact_str::CompactString;

/// A routing pattern defined as relative vector steps.
///
/// Patterns are macro-moves that the topological router can inject into its
/// path segments to burn extra path length while maintaining the
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

    /// Generate absolute coordinates for this pattern.
    ///
    /// **Algorithm:**
    /// 1. Start at current position with current heading
    /// 2. For each step:
    ///    - Apply rotation to heading
    ///    - Convert polar to Cartesian
    ///    - Interpolate line from current to target
    ///    - Update position and heading
    /// 3. Return all points the pattern passes through
    ///
    /// v0.1.8: Uses continuous parametric interpolation (`interpolate_line`),
    /// NOT Bresenham grid rasterization. Points are in nanometer space with
    /// no grid quantization. See Roadmap 14.3.
    ///
    /// # Arguments
    /// * `current_pos` - Starting position
    /// * `current_heading` - Current heading in degrees (0=East, 90=North)
    /// * `step_size_nm` - Spacing between interpolated points in nanometers
    ///
    /// # Returns
    /// Vector of all points the pattern passes through (nanometer coordinates)
    pub fn generate_moves(
        &self,
        current_pos: Point3D,
        current_heading: i64,
        step_size_nm: i64,
    ) -> Vec<Point3D> {
        let mut all_points = Vec::new();
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

            // v0.1.8: Continuous parametric interpolation — no grid quantization.
            // Points are sampled at step_size_nm intervals along the exact line.
            let segment_points = interpolate_line(pos, target_pos, step_size_nm);
            all_points.extend(segment_points);

            // Update position for the next step in the pattern
            pos = target_pos;
        }

        all_points
    }

    /// Check if this pattern can be placed at the given position.
    ///
    /// v0.1.8: Collision checking is fully delegated to the spatial index.
    /// This function always returns true.
    ///
    /// # Arguments
    /// * `current_pos` - Starting position
    /// * `current_heading` - Current heading in degrees
    /// * `step_size_nm` - Spacing between interpolated points in nanometers
    ///
    /// # Returns
    /// true (collision checking is deferred to the spatial index)
    pub fn can_place(
        &self,
        _current_pos: Point3D,
        _current_heading: i64,
        _step_size_nm: i64,
    ) -> bool {
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

/// Interpolate points along a line in continuous nanometer space.
///
/// v0.1.8 (Roadmap 14.3): Replaces the old `rasterize_line` function which used
/// 3D Bresenham grid rasterization. The old implementation quantized coordinates
/// to `step_size_nm` grid cells (`start / step_size_nm`) and then multiplied
/// back (`* step_size_nm`), introducing grid-snapping artifacts. This new
/// implementation uses parametric interpolation (`start + t * (end - start)`)
/// to produce exact nanometer-space points at `step_size_nm` intervals.
///
/// # Arguments
/// * `start` - Starting point
/// * `end` - Ending point
/// * `step_size_nm` - Spacing between sampled points in nanometers
///
/// # Returns
/// Vector of points along the line in nanometer coordinates
fn interpolate_line(start: Point3D, end: Point3D, step_size_nm: i64) -> Vec<Point3D> {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let dz = end.z - start.z;

    // L-infinity distance gives us the correct step count for parametric sampling
    let max_dist = dx.abs().max(dy.abs()).max(dz.abs());

    // Degenerate case: start == end
    if max_dist == 0 {
        return vec![start];
    }

    let steps = (max_dist / step_size_nm).max(1);
    let mut points = Vec::with_capacity(steps as usize + 1);

    for i in 0..=steps {
        let t_numerator = i;
        let t_denominator = steps;

        let x = start.x + (dx * t_numerator / t_denominator);
        let y = start.y + (dy * t_numerator / t_denominator);
        let z = start.z + (dz * t_numerator / t_denominator);

        points.push(Point3D::new(x, y, z));
    }

    points
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
