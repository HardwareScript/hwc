/// Electromagnetic analysis results
use compact_str::CompactString;

#[derive(Debug, Clone)]
pub struct EMAnalysis {
    pub impedance_ohm: f64,
    pub is_controlled: bool,
    pub target_impedance_ohm: Option<f64>,
}

/// Electromagnetic violation types
#[derive(Debug, Clone)]
pub enum EMViolation {
    ImpedanceMismatch {
        net: CompactString,
        actual_ohm: f64,
        target_ohm: f64,
        tolerance_percent: f64,
    },
    Crosstalk {
        net_a: CompactString,
        net_b: CompactString,
        crosstalk_coefficient: f64,
        max_coefficient: f64,
    },
}

use hwc_parser::MaterialDefinition;

/// Trait for accessing material definitions from Symbol Table.
///
/// This trait enables dependency inversion - the physics crate doesn't need
/// to depend on hwc-compiler, but can accept any type that implements this trait.
pub trait SymbolTableTrait {
    fn get_material(&self, name: &str) -> Result<&MaterialDefinition, String>;
}

#[derive(Default)]
pub struct EMAnalyzer {
    // Electromagnetic simulation state
}

impl EMAnalyzer {
    pub fn new() -> Self {
        Self {}
    }

    /// Calculate microstrip impedance using simplified formula with Symbol Table
    ///
    /// # Arguments
    /// * `trace_width_nm` - Trace width in nanometers
    /// * `trace_thickness_nm` - Trace thickness in nanometers
    /// * `dielectric_height_nm` - Height above ground plane in nanometers
    /// * `dielectric_material_name` - Dielectric material name to look up in Symbol Table
    /// * `symbol_table` - Symbol Table containing material definitions
    ///
    /// # Returns
    /// Characteristic impedance in ohms
    ///
    /// # Formula
    /// Z₀ ≈ 87/√(εr+1.41) × ln(5.98h/(0.8w+t))
    /// This is a simplified microstrip formula suitable for most PCB designs
    pub fn calculate_microstrip_impedance_with_symbol_table(
        &self,
        trace_width_nm: i64,
        trace_thickness_nm: i64,
        dielectric_height_nm: i64,
        dielectric_material_name: &str,
        symbol_table: &dyn SymbolTableTrait,
    ) -> Result<f64, crate::PropertyError> {
        // Load material from Symbol Table
        let material_def = symbol_table
            .get_material(dielectric_material_name)
            .map_err(|e| crate::PropertyError::MissingProperty {
                material: dielectric_material_name.to_string().into(),
                property: format!("material lookup failed: {}", e),
            })?;

        // Extract relative permittivity using property extraction helper
        let relative_permittivity = crate::extract_relative_permittivity(material_def)?;

        // Calculate impedance (UNCHANGED)
        let w = trace_width_nm as f64 / 1_000_000.0;
        let t = trace_thickness_nm as f64 / 1_000_000.0;
        let h = dielectric_height_nm as f64 / 1_000_000.0;
        let er = relative_permittivity;

        Ok(87.0 / (er + 1.41).sqrt() * (5.98 * h / (0.8 * w + t)).ln())
    }

    /// Calculate microstrip impedance using simplified formula
    ///
    /// # Arguments
    /// * `trace_width_nm` - Trace width in nanometers
    /// * `trace_thickness_nm` - Trace thickness in nanometers
    /// * `dielectric_height_nm` - Height above ground plane in nanometers
    /// * `relative_permittivity` - Dielectric constant (εr)
    ///
    /// # Returns
    /// Characteristic impedance in ohms
    ///
    /// # Formula
    /// Z₀ ≈ 87/√(εr+1.41) × ln(5.98h/(0.8w+t))
    /// This is a simplified microstrip formula suitable for most PCB designs
    pub fn calculate_microstrip_impedance(
        &self,
        trace_width_nm: i64,
        trace_thickness_nm: i64,
        dielectric_height_nm: i64,
        relative_permittivity: f64,
    ) -> f64 {
        // Convert to millimeters for calculation
        let w = trace_width_nm as f64 / 1_000_000.0;
        let t = trace_thickness_nm as f64 / 1_000_000.0;
        let h = dielectric_height_nm as f64 / 1_000_000.0;
        let er = relative_permittivity;

        // Microstrip impedance formula
        // Z0 ≈ 87 / sqrt(εr + 1.41) * ln(5.98 * h / (0.8 * w + t))
        87.0 / (er + 1.41).sqrt() * (5.98 * h / (0.8 * w + t)).ln()
    }

    /// Validate impedance matching within tolerance
    ///
    /// # Arguments
    /// * `net_name` - Name of the net
    /// * `actual_impedance_ohm` - Calculated impedance
    /// * `target_impedance_ohm` - Target impedance (e.g., 50Ω, 90Ω)
    /// * `tolerance_percent` - Acceptable tolerance (typically 10%)
    ///
    /// # Returns
    /// Ok if within tolerance, Err with violation otherwise
    pub fn validate_impedance_matching(
        &self,
        net_name: &str,
        actual_impedance_ohm: f64,
        target_impedance_ohm: f64,
        tolerance_percent: f64,
    ) -> Result<(), EMViolation> {
        let tolerance = target_impedance_ohm * (tolerance_percent / 100.0);
        let min_impedance = target_impedance_ohm - tolerance;
        let max_impedance = target_impedance_ohm + tolerance;

        if actual_impedance_ohm < min_impedance || actual_impedance_ohm > max_impedance {
            Err(EMViolation::ImpedanceMismatch {
                net: net_name.to_string().into(),
                actual_ohm: actual_impedance_ohm,
                target_ohm: target_impedance_ohm,
                tolerance_percent,
            })
        } else {
            Ok(())
        }
    }

    /// Calculate crosstalk coefficient between parallel traces
    ///
    /// # Arguments
    /// * `spacing_nm` - Distance between traces in nanometers
    /// * `trace_width_nm` - Width of traces in nanometers
    /// * `parallel_length_nm` - Length of parallel run in nanometers
    ///
    /// # Returns
    /// Crosstalk coefficient (0.0 = no crosstalk, 1.0 = maximum crosstalk)
    ///
    /// # Model
    /// Simplified near-end crosstalk model:
    /// - Crosstalk increases with parallel length
    /// - Crosstalk decreases with spacing
    /// - Normalized to 0-1 range for easy threshold checking
    pub fn calculate_crosstalk_coefficient(
        &self,
        spacing_nm: i64,
        trace_width_nm: i64,
        parallel_length_nm: i64,
    ) -> f64 {
        // Convert to millimeters
        let spacing_mm = spacing_nm as f64 / 1_000_000.0;
        let width_mm = trace_width_nm as f64 / 1_000_000.0;
        let length_mm = parallel_length_nm as f64 / 1_000_000.0;

        // Simplified crosstalk model
        // Crosstalk ∝ (length / spacing) × (width / spacing)
        // Normalized to 0-1 range
        let spacing_ratio = width_mm / spacing_mm;
        let length_factor = (length_mm / 10.0).min(1.0); // Normalize to 10mm reference

        let coefficient = spacing_ratio * length_factor;

        // Clamp to 0-1 range
        coefficient.clamp(0.0, 1.0)
    }

    /// Validate crosstalk between traces
    ///
    /// # Arguments
    /// * `net_a` - First net name
    /// * `net_b` - Second net name
    /// * `crosstalk_coefficient` - Calculated crosstalk coefficient
    /// * `max_coefficient` - Maximum acceptable coefficient (typically 0.1-0.2)
    ///
    /// # Returns
    /// Ok if acceptable, Err with violation otherwise
    pub fn validate_crosstalk(
        &self,
        net_a: &str,
        net_b: &str,
        crosstalk_coefficient: f64,
        max_coefficient: f64,
    ) -> Result<(), EMViolation> {
        if crosstalk_coefficient > max_coefficient {
            Err(EMViolation::Crosstalk {
                net_a: net_a.to_string().into(),
                net_b: net_b.into(),
                crosstalk_coefficient,
                max_coefficient,
            })
        } else {
            Ok(())
        }
    }

    /// Perform complete electromagnetic analysis
    ///
    /// # Arguments
    /// * `trace_width_nm` - Trace width
    /// * `trace_thickness_nm` - Trace thickness
    /// * `dielectric_height_nm` - Height above ground plane
    /// * `relative_permittivity` - Dielectric constant
    /// * `target_impedance_ohm` - Optional target impedance for controlled traces
    ///
    /// # Returns
    /// Complete electromagnetic analysis results
    pub fn analyze_trace(
        &self,
        trace_width_nm: i64,
        trace_thickness_nm: i64,
        dielectric_height_nm: i64,
        relative_permittivity: f64,
        target_impedance_ohm: Option<f64>,
    ) -> EMAnalysis {
        let impedance_ohm = self.calculate_microstrip_impedance(
            trace_width_nm,
            trace_thickness_nm,
            dielectric_height_nm,
            relative_permittivity,
        );

        let is_controlled = if let Some(target) = target_impedance_ohm {
            // Check if within 10% tolerance
            let tolerance = target * 0.1;
            (impedance_ohm - target).abs() <= tolerance
        } else {
            false
        };

        EMAnalysis {
            impedance_ohm,
            is_controlled,
            target_impedance_ohm,
        }
    }

    /// Detect parallel overlap between two nets using geometric segment intersection.
    ///
    /// Projects traces onto the X-Y plane as line segments and computes the total
    /// physical overlap length where parallel segments from each net are collinear.
    ///
    /// # Arguments
    /// * `net_a_coords` - Physical (x, y, z) coordinates in nanometers for net A
    /// * `net_b_coords` - Physical (x, y, z) coordinates in nanometers for net B
    ///
    /// # Returns
    /// Total parallel overlap length in nanometers
    ///
    /// # Algorithm
    /// 1. Build segments from consecutive points (projected onto X-Y, Z ignored)
    /// 2. For each pair of segments (one from each net):
    ///    - If both horizontal and on the same Y: compute X-range overlap
    ///    - If both vertical and on the same X: compute Y-range overlap
    /// 3. Sum all overlap lengths
    pub fn detect_parallel_overlap(
        &self,
        net_a_coords: &[(i64, i64, i64)],
        net_b_coords: &[(i64, i64, i64)],
    ) -> i64 {
        fn build_segments(coords: &[(i64, i64, i64)]) -> Vec<((i64, i64), (i64, i64))> {
            coords
                .windows(2)
                .map(|w| {
                    let a = (w[0].0, w[0].1);
                    let b = (w[1].0, w[1].1);
                    if a <= b { (a, b) } else { (b, a) }
                })
                .collect()
        }

        let segs_a = build_segments(net_a_coords);
        let segs_b = build_segments(net_b_coords);

        let mut total_overlap = 0i64;

        for &(a_start, a_end) in &segs_a {
            for &(b_start, b_end) in &segs_b {
                // Both horizontal on the same Y
                if a_start.1 == a_end.1 && b_start.1 == b_end.1 && a_start.1 == b_start.1 {
                    let lo = a_start.0.max(b_start.0);
                    let hi = a_end.0.min(b_end.0);
                    if lo < hi {
                        total_overlap += hi - lo;
                    }
                }
                // Both vertical on the same X
                else if a_start.0 == a_end.0 && b_start.0 == b_end.0 && a_start.0 == b_start.0
                {
                    let lo = a_start.1.max(b_start.1);
                    let hi = a_end.1.min(b_end.1);
                    if lo < hi {
                        total_overlap += hi - lo;
                    }
                }
            }
        }

        total_overlap
    }

    /// Validate crosstalk risk between two nets based on parallel overlap.
    ///
    /// This is the high-level API for Task B2 crosstalk detection.
    ///
    /// # Arguments
    /// * `net_a` - First net name
    /// * `net_b` - Second net name
    /// * `net_a_coords` - Physical (x, y, z) coordinates in nanometers for net A
    /// * `net_b_coords` - Physical (x, y, z) coordinates in nanometers for net B
    /// * `max_parallel_overlap_nm` - Maximum acceptable parallel overlap (e.g., 10mm = 10_000_000nm)
    ///
    /// # Returns
    /// Ok if overlap is acceptable, Err with violation otherwise
    pub fn validate_crosstalk_overlap(
        &self,
        net_a: &str,
        net_b: &str,
        net_a_coords: &[(i64, i64, i64)],
        net_b_coords: &[(i64, i64, i64)],
        max_parallel_overlap_nm: i64,
    ) -> Result<(), EMViolation> {
        let overlap_nm = self.detect_parallel_overlap(net_a_coords, net_b_coords);

        if overlap_nm > max_parallel_overlap_nm {
            // Convert to crosstalk coefficient for violation reporting
            let crosstalk_coefficient = (overlap_nm as f64) / (max_parallel_overlap_nm as f64);
            Err(EMViolation::Crosstalk {
                net_a: net_a.to_string().into(),
                net_b: net_b.into(),
                crosstalk_coefficient,
                max_coefficient: 1.0,
            })
        } else {
            Ok(())
        }
    }

    /// Legacy method for backwards compatibility
    #[deprecated(note = "Use calculate_microstrip_impedance instead")]
    pub fn analyze_impedance(
        &self,
        trace_width_mm: f64,
        trace_thickness_mm: f64,
        dielectric_height_mm: f64,
        relative_permittivity: f64,
    ) -> f64 {
        // Z0 ≈ 87 / sqrt(εr + 1.41) * ln(5.98 * h / (0.8 * w + t))
        let er = relative_permittivity;
        let h = dielectric_height_mm;
        let w = trace_width_mm;
        let t = trace_thickness_mm;

        87.0 / (er + 1.41).sqrt() * (5.98 * h / (0.8 * w + t)).ln()
    }
}
