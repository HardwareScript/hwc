//! Electrical analysis for Hardware Script physics validation.
//!
//! This module provides electrical analysis capabilities including:
//! - Trace resistance calculations
//! - Voltage drop analysis
//! - Power dissipation calculations
//! - Ampacity validation using IPC-2221

use compact_str::CompactString;
use hwc_parser::MaterialDefinition;

/// Profile constraints for physics validation
#[derive(Debug, Clone)]
pub struct ProfileConstraints {
    pub max_voltage_drop_mv: Option<f64>,
    pub max_temp_rise_c: Option<f64>,
    pub max_operating_temp_c: Option<f64>,
    pub ambient_temp_c: f64,
}

impl Default for ProfileConstraints {
    fn default() -> Self {
        Self {
            max_voltage_drop_mv: Some(100.0), // Default 100mV max drop
            max_temp_rise_c: Some(20.0),      // Default 20°C max rise
            max_operating_temp_c: Some(85.0), // Default 85°C max temp
            ambient_temp_c: 25.0,             // Default 25°C ambient
        }
    }
}

/// Trait for accessing material definitions from Symbol Table.
///
/// This trait enables dependency inversion - the physics crate doesn't need
/// to depend on hwc-compiler, but can accept any type that implements this trait.
pub trait SymbolTableTrait {
    fn get_material(&self, name: &str) -> Result<&MaterialDefinition, String>;
    fn get_profile_constraints(&self, profile_name: &str) -> Result<ProfileConstraints, String>;
}

/// Electrical analysis results
#[derive(Debug, Clone)]
pub struct ElectricalAnalysis {
    pub resistance_ohm: f64,
    pub voltage_drop_mv: f64,
    pub power_dissipation_mw: f64,
}

/// Electrical violation types
#[derive(Debug, Clone)]
pub enum ElectricalViolation {
    VoltageDrop {
        net: CompactString,
        actual_mv: f64,
        max_mv: f64,
    },
    Resistance {
        net: CompactString,
        actual_ohm: f64,
        max_ohm: f64,
    },
    Ampacity {
        net: CompactString,
        current_ma: i64,
        required_width_nm: i64,
        actual_width_nm: i64,
    },
}

#[derive(Default)]
pub struct ElectricalAnalyzer {
    // Electrical simulation state
}

impl ElectricalAnalyzer {
    pub fn new() -> Self {
        Self {}
    }

    /// Calculate trace resistance using R = ρ × (L / A) with Symbol Table
    ///
    /// # Arguments
    /// * `length_nm` - Trace length in nanometers
    /// * `width_nm` - Trace width in nanometers
    /// * `thickness_nm` - Trace thickness in nanometers
    /// * `material_name` - Material name to look up in Symbol Table
    /// * `symbol_table` - Symbol Table containing material definitions
    ///
    /// # Returns
    /// Resistance in ohms
    pub fn calculate_trace_resistance_with_symbol_table(
        &self,
        length_nm: i64,
        width_nm: i64,
        thickness_nm: i64,
        material_name: &str,
        symbol_table: &dyn SymbolTableTrait,
    ) -> Result<f64, crate::PropertyError> {
        // Load material from Symbol Table
        let material_def = symbol_table.get_material(material_name).map_err(|e| {
            crate::PropertyError::MissingProperty {
                material: material_name.to_string().into(),
                property: format!("material lookup failed: {}", e),
            }
        })?;

        // Extract resistivity using property extraction helper
        let resistivity = crate::extract_resistivity(material_def)?;

        // Calculate resistance: R = ρ × (L / A) (UNCHANGED)
        let length_m = length_nm as f64 / 1_000_000_000.0;
        let width_m = width_nm as f64 / 1_000_000_000.0;
        let thickness_m = thickness_nm as f64 / 1_000_000_000.0;
        let area_m2 = width_m * thickness_m;

        Ok(resistivity * (length_m / area_m2))
    }

    /// Calculate trace resistance using R = ρ × (L / A)
    ///
    /// # Arguments
    /// * `length_nm` - Trace length in nanometers
    /// * `width_nm` - Trace width in nanometers
    /// * `thickness_nm` - Trace thickness in nanometers
    /// * `resistivity_ohm_m` - Material resistivity in ohm-meters
    ///
    /// # Returns
    /// Resistance in ohms
    pub fn calculate_trace_resistance(
        &self,
        length_nm: i64,
        width_nm: i64,
        thickness_nm: i64,
        resistivity_ohm_m: f64,
    ) -> f64 {
        // Convert nanometers to meters
        let length_m = length_nm as f64 / 1_000_000_000.0;
        let width_m = width_nm as f64 / 1_000_000_000.0;
        let thickness_m = thickness_nm as f64 / 1_000_000_000.0;

        // Calculate cross-sectional area
        let area_m2 = width_m * thickness_m;

        // R = ρ × (L / A)
        resistivity_ohm_m * (length_m / area_m2)
    }

    /// Calculate voltage drop using V = I × R
    ///
    /// # Arguments
    /// * `resistance_ohm` - Trace resistance in ohms
    /// * `current_ma` - Current in milliamps
    ///
    /// # Returns
    /// Voltage drop in millivolts
    pub fn calculate_voltage_drop(&self, resistance_ohm: f64, current_ma: i64) -> f64 {
        let current_a = current_ma as f64 / 1000.0;
        let voltage_drop_v = current_a * resistance_ohm;
        voltage_drop_v * 1000.0 // Convert to millivolts
    }

    /// Calculate power dissipation using P = I² × R
    ///
    /// # Arguments
    /// * `resistance_ohm` - Trace resistance in ohms
    /// * `current_ma` - Current in milliamps
    ///
    /// # Returns
    /// Power dissipation in milliwatts
    pub fn calculate_power_dissipation(&self, resistance_ohm: f64, current_ma: i64) -> f64 {
        let current_a = current_ma as f64 / 1000.0;
        let power_w = current_a * current_a * resistance_ohm;
        power_w * 1000.0 // Convert to milliwatts
    }

    /// Validate voltage drop against profile constraints
    ///
    /// # Arguments
    /// * `net_name` - Name of the net
    /// * `actual_voltage_drop_mv` - Actual voltage drop in millivolts
    /// * `constraints` - Profile constraints
    ///
    /// # Returns
    /// Ok if within limit, Err with violation and auto-fix suggestion otherwise
    pub fn validate_voltage_drop(
        &self,
        net_name: &str,
        actual_voltage_drop_mv: f64,
        constraints: &ProfileConstraints,
    ) -> Result<(), ElectricalViolation> {
        if let Some(max_mv) = constraints.max_voltage_drop_mv {
            if actual_voltage_drop_mv > max_mv {
                return Err(ElectricalViolation::VoltageDrop {
                    net: net_name.to_string().into(),
                    actual_mv: actual_voltage_drop_mv,
                    max_mv,
                });
            }
        }
        Ok(())
    }

    /// Generate auto-fix suggestion for voltage drop violation
    ///
    /// # Arguments
    /// * `violation` - Voltage drop violation
    /// * `trace_length_nm` - Current trace length
    /// * `trace_width_nm` - Current trace width
    ///
    /// # Returns
    /// Human-readable suggestion for fixing the violation
    pub fn suggest_voltage_drop_fix(
        &self,
        violation: &ElectricalViolation,
        trace_length_nm: i64,
        trace_width_nm: i64,
    ) -> CompactString {
        if let ElectricalViolation::VoltageDrop {
            net,
            actual_mv,
            max_mv,
        } = violation
        {
            let ratio = actual_mv / max_mv;
            let suggested_width_nm = (trace_width_nm as f64 * ratio).ceil() as i64;
            let suggested_width_um = suggested_width_nm / 1000;

            format!(
                "Net '{}': Voltage drop {:.1}mV exceeds limit {:.1}mV\n\
                 💡 Auto-fix suggestions:\n\
                 1. Widen trace from {}µm to {}µm (increases cross-section, reduces resistance)\n\
                 2. Insert buffer at midpoint (splits net into two shorter segments)\n\
                 3. Use thicker copper (e.g., 70µm instead of 35µm)\n\
                 4. Reduce trace length by {}mm (optimize routing path)",
                net,
                actual_mv,
                max_mv,
                trace_width_nm / 1000,
                suggested_width_um,
                (trace_length_nm as f64 * 0.1 / 1_000_000.0).ceil()
            )
            .into()
        } else {
            "Invalid violation type".into()
        }
    }

    /// Generate auto-fix suggestion for ampacity violation
    ///
    /// # Arguments
    /// * `violation` - Ampacity violation
    ///
    /// # Returns
    /// Human-readable suggestion for fixing the violation
    pub fn suggest_ampacity_fix(&self, violation: &ElectricalViolation) -> CompactString {
        if let ElectricalViolation::Ampacity {
            net,
            current_ma,
            required_width_nm,
            actual_width_nm,
        } = violation
        {
            format!(
                "Net '{}': Trace width {}µm insufficient for {}mA current\n\
                 💡 Auto-fix suggestions:\n\
                 1. Widen trace to {}µm (IPC-2221 requirement for {}mA)\n\
                 2. Use thicker copper (70µm instead of 35µm reduces required width)\n\
                 3. Split current across multiple parallel traces\n\
                 4. Add thermal vias for heat dissipation",
                net,
                actual_width_nm / 1000,
                current_ma,
                required_width_nm / 1000,
                current_ma
            )
            .into()
        } else {
            "Invalid violation type".into()
        }
    }

    /// Perform complete electrical analysis on a trace with Symbol Table
    ///
    /// # Arguments
    /// * `length_nm` - Trace length in nanometers
    /// * `width_nm` - Trace width in nanometers
    /// * `thickness_nm` - Trace thickness in nanometers
    /// * `current_ma` - Current in milliamps
    /// * `material_name` - Material name to look up in Symbol Table
    /// * `symbol_table` - Symbol Table containing material definitions
    ///
    /// # Returns
    /// Complete electrical analysis results
    pub fn analyze_trace_with_symbol_table(
        &self,
        length_nm: i64,
        width_nm: i64,
        thickness_nm: i64,
        current_ma: i64,
        material_name: &str,
        symbol_table: &dyn SymbolTableTrait,
    ) -> Result<ElectricalAnalysis, crate::PropertyError> {
        let resistance_ohm = self.calculate_trace_resistance_with_symbol_table(
            length_nm,
            width_nm,
            thickness_nm,
            material_name,
            symbol_table,
        )?;

        let voltage_drop_mv = self.calculate_voltage_drop(resistance_ohm, current_ma);
        let power_dissipation_mw = self.calculate_power_dissipation(resistance_ohm, current_ma);

        Ok(ElectricalAnalysis {
            resistance_ohm,
            voltage_drop_mv,
            power_dissipation_mw,
        })
    }

    /// Perform complete electrical analysis on a trace
    ///
    /// # Arguments
    /// * `length_nm` - Trace length in nanometers
    /// * `width_nm` - Trace width in nanometers
    /// * `thickness_nm` - Trace thickness in nanometers
    /// * `current_ma` - Current in milliamps
    /// * `resistivity_ohm_m` - Material resistivity
    ///
    /// # Returns
    /// Complete electrical analysis results
    pub fn analyze_trace(
        &self,
        length_nm: i64,
        width_nm: i64,
        thickness_nm: i64,
        current_ma: i64,
        resistivity_ohm_m: f64,
    ) -> ElectricalAnalysis {
        let resistance_ohm =
            self.calculate_trace_resistance(length_nm, width_nm, thickness_nm, resistivity_ohm_m);
        let voltage_drop_mv = self.calculate_voltage_drop(resistance_ohm, current_ma);
        let power_dissipation_mw = self.calculate_power_dissipation(resistance_ohm, current_ma);

        ElectricalAnalysis {
            resistance_ohm,
            voltage_drop_mv,
            power_dissipation_mw,
        }
    }

    /// Legacy method for backwards compatibility
    #[deprecated(note = "Use calculate_trace_resistance instead")]
    pub fn analyze_resistance(
        &self,
        length_mm: f64,
        cross_section_mm2: f64,
        resistivity: f64,
    ) -> f64 {
        // R = ρ * L / A
        resistivity * (length_mm / 1000.0) / (cross_section_mm2 / 1_000_000.0)
    }
}
