/// Clearance validation based on voltage and material properties.
///
/// This module implements Phase 4 of System 4: voltage-based clearance validation
/// using material dielectric strength from the Symbol Table.
///
/// **Documentation References**:
/// - `Docs/v0.1.4/ROUTING-AND-PHYSICS.md` (Translation 1: Dielectric Breakdown to Clearance)
/// - `ROADMAP/v0.1.4/SYSTEM-4-IMPLEMENTATION-PLAN.md` (Phase 4)
///
use crate::property_extraction::extract_dielectric_strength;
use compact_str::CompactString;
use hwc_parser::MaterialDefinition;

/// Trait for Symbol Table access (dependency inversion for testing)
pub trait SymbolTableTrait {
    fn get_material(&self, name: &str) -> Result<&MaterialDefinition, String>;
}

/// Clearance violation types
#[derive(Debug, Clone)]
pub enum ClearanceViolation {
    DielectricBreakdown {
        net_a: CompactString,
        net_b: CompactString,
        voltage_diff_mv: i64,
        actual_clearance_nm: i64,
        required_clearance_nm: i64,
        material: CompactString,
    },
    AltitudeAdjustment {
        net_a: CompactString,
        net_b: CompactString,
        altitude_m: i64,
        base_clearance_nm: i64,
        adjusted_clearance_nm: i64,
    },
}

#[derive(Default)]
pub struct ClearanceAnalyzer {
    // Clearance validation state
}

/// Parameters for clearance validation
pub struct ClearanceValidationParams<'a> {
    pub net_a: &'a str,
    pub net_b: &'a str,
    pub voltage_a_mv: i64,
    pub voltage_b_mv: i64,
    pub actual_clearance_nm: i64,
    pub dielectric_strength_kv_mm: f64,
    pub material_name: &'a str,
}

impl ClearanceAnalyzer {
    pub fn new() -> Self {
        Self {}
    }

    /// Calculate required clearance based on voltage and dielectric strength.
    ///
    /// # Formula
    /// ```text
    /// clearance = (voltage / dielectric_strength) × safety_factor
    /// ```
    ///
    /// # Arguments
    /// * `voltage_diff_mv` - Voltage difference in millivolts
    /// * `dielectric_strength_kv_mm` - Material dielectric strength in kV/mm
    /// * `safety_factor` - Safety multiplier (typically 2.0 for industry standard)
    ///
    /// # Returns
    /// Required clearance in nanometers
    ///
    /// # Example
    /// ```
    /// use hwc_physics::clearance::ClearanceAnalyzer;
    ///
    /// let analyzer = ClearanceAnalyzer::new();
    ///
    /// // 120V through air (3 kV/mm dielectric strength)
    /// let clearance = analyzer.calculate_required_clearance(
    ///     120_000,  // 120V in millivolts
    ///     3.0,      // Air: 3 kV/mm
    ///     2.0       // 2× safety factor
    /// );
    ///
    /// assert_eq!(clearance, 80_000); // 0.08mm required
    /// ```
    pub fn calculate_required_clearance(
        &self,
        voltage_diff_mv: i64,
        dielectric_strength_kv_mm: f64,
        safety_factor: f64,
    ) -> i64 {
        // Convert voltage to volts
        let voltage_v = voltage_diff_mv as f64 / 1000.0;

        // Convert dielectric strength to V/mm
        let dielectric_v_mm = dielectric_strength_kv_mm * 1000.0;

        // Calculate minimum clearance in mm
        let min_clearance_mm = voltage_v / dielectric_v_mm;

        // Apply safety factor
        let required_clearance_mm = min_clearance_mm * safety_factor;

        // Convert to nanometers
        (required_clearance_mm * 1_000_000.0) as i64
    }

    /// Validate clearance between two nets.
    ///
    /// # Arguments
    /// * `params` - Clearance validation parameters
    ///
    /// # Returns
    /// Ok if clearance is sufficient, Err with violation otherwise
    pub fn validate_clearance(
        &self,
        params: ClearanceValidationParams,
    ) -> Result<(), ClearanceViolation> {
        // Calculate voltage difference
        let voltage_diff_mv = (params.voltage_a_mv - params.voltage_b_mv).abs();

        // Calculate required clearance with 2× safety factor
        let required_clearance_nm = self.calculate_required_clearance(
            voltage_diff_mv,
            params.dielectric_strength_kv_mm,
            2.0,
        );

        if params.actual_clearance_nm < required_clearance_nm {
            Err(ClearanceViolation::DielectricBreakdown {
                net_a: params.net_a.to_string().into(),
                net_b: params.net_b.into(),
                voltage_diff_mv,
                actual_clearance_nm: params.actual_clearance_nm,
                required_clearance_nm,
                material: params.material_name.into(),
            })
        } else {
            Ok(())
        }
    }

    /// Adjust clearance for altitude (air thins at altitude).
    ///
    /// # Formula
    /// ```text
    /// adjusted_clearance = base_clearance × (1 + altitude / 10000)
    /// ```
    ///
    /// # Arguments
    /// * `base_clearance_nm` - Base clearance at sea level in nanometers
    /// * `altitude_m` - Altitude in meters
    ///
    /// # Returns
    /// Adjusted clearance in nanometers
    ///
    /// # Example
    /// ```
    /// use hwc_physics::clearance::ClearanceAnalyzer;
    ///
    /// let analyzer = ClearanceAnalyzer::new();
    ///
    /// // 0.08mm clearance at 3000m altitude
    /// let adjusted = analyzer.adjust_clearance_for_altitude(80_000, 3000);
    ///
    /// // Should be ~30% higher at 3000m
    /// assert_eq!(adjusted, 104_000); // 0.104mm
    /// ```
    pub fn adjust_clearance_for_altitude(&self, base_clearance_nm: i64, altitude_m: i64) -> i64 {
        // Air thins at altitude → lower dielectric strength
        // Formula: clearance_adjusted = clearance × (1 + altitude/10000)
        let altitude_factor = 1.0 + (altitude_m as f64 / 10000.0);
        (base_clearance_nm as f64 * altitude_factor) as i64
    }

    /// Validate clearance with altitude adjustment.
    ///
    /// # Arguments
    /// * `net_a` - First net name
    /// * `net_b` - Second net name
    /// * `base_clearance_nm` - Base clearance at sea level
    /// * `altitude_m` - Altitude in meters
    ///
    /// # Returns
    /// Ok if clearance is sufficient, Err with violation otherwise
    pub fn validate_clearance_with_altitude(
        &self,
        net_a: &str,
        net_b: &str,
        base_clearance_nm: i64,
        altitude_m: i64,
    ) -> Result<(), ClearanceViolation> {
        let adjusted_clearance_nm =
            self.adjust_clearance_for_altitude(base_clearance_nm, altitude_m);

        // For now, just return the adjustment info
        // In a real implementation, you'd compare with actual clearance
        if altitude_m > 0 {
            Err(ClearanceViolation::AltitudeAdjustment {
                net_a: net_a.to_string().into(),
                net_b: net_b.into(),
                altitude_m,
                base_clearance_nm,
                adjusted_clearance_nm,
            })
        } else {
            Ok(())
        }
    }

    /// Calculate required clearance using Symbol Table for material properties.
    ///
    /// # Arguments
    /// * `voltage_diff_mv` - Voltage difference in millivolts
    /// * `material_name` - Name of the dielectric material
    /// * `symbol_table` - Symbol Table containing material definitions
    /// * `safety_factor` - Safety multiplier (typically 2.0)
    ///
    /// # Returns
    /// Required clearance in nanometers
    ///
    /// # Example
    /// ```
    /// use hwc_physics::clearance::ClearanceAnalyzer;
    /// use hwc_compiler::SymbolTable;
    /// use hwc_diagnostics::DiagnosticCollector;
    /// use hwc_parser::{Identifier, MaterialCategory, MaterialDefinition, Measurement, Property, PropertyValue, Span, Unit};
    ///
    /// // Create a Symbol Table with Air material
    /// let mut symbol_table = SymbolTable::new();
    /// let collector = DiagnosticCollector::new("", 100);
    /// let air_material = MaterialDefinition {
    ///     name: Identifier { name: "Air".into(), span: Span::new(0, 3) },
    ///     category: MaterialCategory::Insulator,
    ///     symbol: None,
    ///     description: Some("Air dielectric".into()),
    ///     properties: vec![
    ///         Property {
    ///             key: "dielectric_strength".into(),
    ///             value: PropertyValue::Measurement(Measurement {
    ///                 value: 3.0,
    ///                 unit: Unit::Custom("kV/mm".into()),
    ///                 span: Span::new(0, 10),
    ///             }),
    ///             span: Span::new(0, 10),
    ///         },
    ///     ],
    ///     span: Span::new(0, 100),
    /// };
    /// symbol_table.register_material(&collector, air_material);
    ///
    /// let analyzer = ClearanceAnalyzer::new();
    /// let clearance = analyzer.calculate_required_clearance_with_symbol_table(
    ///     120_000,  // 120V
    ///     "Air",
    ///     &symbol_table,
    ///     2.0       // 2× safety factor
    /// ).unwrap();
    ///
    /// assert_eq!(clearance, 80_000); // 0.08mm = 80,000nm
    /// ```
    pub fn calculate_required_clearance_with_symbol_table<T: SymbolTableTrait>(
        &self,
        voltage_diff_mv: i64,
        material_name: &str,
        symbol_table: &T,
        safety_factor: f64,
    ) -> Result<i64, crate::PropertyError> {
        // Get material from Symbol Table
        let material_def = symbol_table.get_material(material_name).map_err(|e| {
            crate::PropertyError::MissingProperty {
                material: material_name.to_string().into(),
                property: format!("material lookup failed: {}", e),
            }
        })?;

        // Extract dielectric strength
        let dielectric_strength_kv_mm = extract_dielectric_strength(material_def)?;

        // Use existing calculation method
        Ok(self.calculate_required_clearance(
            voltage_diff_mv,
            dielectric_strength_kv_mm,
            safety_factor,
        ))
    }

    /// Validate clearance using Symbol Table for material properties.
    ///
    /// # Arguments
    /// * `params` - Clearance validation parameters
    /// * `symbol_table` - Symbol Table containing material definitions
    ///
    /// # Returns
    /// Ok if clearance is sufficient, Err with violation otherwise
    pub fn validate_clearance_with_symbol_table<T: SymbolTableTrait>(
        &self,
        params: &ClearanceValidationParams,
        symbol_table: &T,
    ) -> Result<(), ClearanceViolation> {
        // Calculate voltage difference
        let voltage_diff_mv = (params.voltage_a_mv - params.voltage_b_mv).abs();

        // Calculate required clearance with 2× safety factor
        let required_clearance_nm = self
            .calculate_required_clearance_with_symbol_table(
                voltage_diff_mv,
                params.material_name,
                symbol_table,
                2.0,
            )
            .map_err(|_| ClearanceViolation::DielectricBreakdown {
                net_a: params.net_a.to_string().into(),
                net_b: params.net_b.into(),
                voltage_diff_mv,
                actual_clearance_nm: params.actual_clearance_nm,
                required_clearance_nm: 0,
                material: params.material_name.into(),
            })?;

        if params.actual_clearance_nm < required_clearance_nm {
            Err(ClearanceViolation::DielectricBreakdown {
                net_a: params.net_a.to_string().into(),
                net_b: params.net_b.into(),
                voltage_diff_mv,
                actual_clearance_nm: params.actual_clearance_nm,
                required_clearance_nm,
                material: params.material_name.into(),
            })
        } else {
            Ok(())
        }
    }
}
