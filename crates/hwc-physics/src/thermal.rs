/// Thermal analysis results
#[derive(Debug, Clone)]
pub struct ThermalAnalysis {
    pub temperature_rise_c: f64,
    pub is_safe: bool,
    pub max_safe_temp_c: f64,
}

/// Thermal violation types
#[derive(Debug, Clone)]
pub enum ThermalViolation {
    TemperatureRise {
        net: CompactString,
        actual_rise_c: f64,
        max_rise_c: f64,
    },
    MaxTemperature {
        net: CompactString,
        actual_temp_c: f64,
        max_temp_c: f64,
    },
    ThermalClustering {
        nets: Vec<CompactString>,
        combined_power_mw: f64,
        distance_nm: i64,
    },
}

use compact_str::CompactString;
use hwc_parser::MaterialDefinition;

/// Profile constraints for thermal validation
#[derive(Debug, Clone)]
pub struct ProfileConstraints {
    pub max_temp_rise_c: Option<f64>,
    pub max_operating_temp_c: Option<f64>,
    pub ambient_temp_c: f64,
    pub clustering_threshold_nm: i64,
}

impl Default for ProfileConstraints {
    fn default() -> Self {
        Self {
            max_temp_rise_c: Some(20.0),        // Default 20°C max rise
            max_operating_temp_c: Some(85.0),   // Default 85°C max temp
            ambient_temp_c: 25.0,               // Default 25°C ambient
            clustering_threshold_nm: 5_000_000, // Default 5mm clustering threshold
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

#[derive(Default)]
pub struct ThermalAnalyzer {
    // Thermal simulation state
}

/// Parameters for thermal analysis
pub struct ThermalAnalysisParams {
    pub power_mw: f64,
    pub length_nm: i64,
    pub width_nm: i64,
    pub thickness_nm: i64,
    pub thermal_conductivity_w_mk: f64,
    pub ambient_temp_c: f64,
    pub max_operating_temp_c: f64,
}

impl ThermalAnalyzer {
    pub fn new() -> Self {
        Self {}
    }

    /// Calculate temperature rise using simplified thermal model with Symbol Table
    ///
    /// # Arguments
    /// * `power_mw` - Power dissipation in milliwatts
    /// * `length_nm` - Trace length in nanometers
    /// * `width_nm` - Trace width in nanometers
    /// * `thickness_nm` - Trace thickness in nanometers
    /// * `material_name` - Material name to look up in Symbol Table
    /// * `symbol_table` - Symbol Table containing material definitions
    ///
    /// # Returns
    /// Temperature rise in °C
    ///
    /// # Model
    /// Simplified 1D heat transfer: ΔT = P / (k × A)
    /// where A is the surface area for heat dissipation
    pub fn calculate_temperature_rise_with_symbol_table(
        &self,
        power_mw: f64,
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

        // Extract thermal conductivity using property extraction helper
        let thermal_conductivity = crate::extract_thermal_conductivity(material_def)?;

        // Calculate temperature rise (UNCHANGED)
        let power_w = power_mw / 1000.0;
        let length_m = length_nm as f64 / 1_000_000_000.0;
        let width_m = width_nm as f64 / 1_000_000_000.0;
        let thickness_m = thickness_nm as f64 / 1_000_000_000.0;

        let top_bottom_area = 2.0 * length_m * width_m;
        let side_area = 2.0 * length_m * thickness_m + 2.0 * width_m * thickness_m;
        let total_area = top_bottom_area + side_area;

        let thermal_resistance = 1.0 / (thermal_conductivity * total_area);
        Ok(power_w * thermal_resistance)
    }

    /// Calculate temperature rise using simplified thermal model
    ///
    /// # Arguments
    /// * `power_mw` - Power dissipation in milliwatts
    /// * `length_nm` - Trace length in nanometers
    /// * `width_nm` - Trace width in nanometers
    /// * `thickness_nm` - Trace thickness in nanometers
    /// * `thermal_conductivity_w_mk` - Material thermal conductivity (W/m·K)
    ///
    /// # Returns
    /// Temperature rise in °C
    ///
    /// # Model
    /// Simplified 1D heat transfer: ΔT = P / (k × A)
    /// where A is the surface area for heat dissipation
    pub fn calculate_temperature_rise(
        &self,
        power_mw: f64,
        length_nm: i64,
        width_nm: i64,
        thickness_nm: i64,
        thermal_conductivity_w_mk: f64,
    ) -> f64 {
        // Convert power to watts
        let power_w = power_mw / 1000.0;

        // Convert dimensions to meters
        let length_m = length_nm as f64 / 1_000_000_000.0;
        let width_m = width_nm as f64 / 1_000_000_000.0;
        let thickness_m = thickness_nm as f64 / 1_000_000_000.0;

        // Calculate surface area (top + bottom + sides)
        let top_bottom_area = 2.0 * length_m * width_m;
        let side_area = 2.0 * length_m * thickness_m + 2.0 * width_m * thickness_m;
        let total_area = top_bottom_area + side_area;

        // Simplified thermal resistance model
        // ΔT = P × R_thermal, where R_thermal = 1 / (k × A)
        let thermal_resistance = 1.0 / (thermal_conductivity_w_mk * total_area);
        power_w * thermal_resistance
    }

    /// Detect thermal clustering between multiple traces
    ///
    /// # Arguments
    /// * `traces` - List of (power_mw, position) tuples
    /// * `clustering_threshold_nm` - Distance threshold for clustering detection
    ///
    /// # Returns
    /// List of thermal clustering violations
    pub fn detect_thermal_clustering(
        &self,
        traces: &[(String, f64, i64)], // (net_name, power_mw, position_nm)
        clustering_threshold_nm: i64,
    ) -> Vec<ThermalViolation> {
        let mut violations = Vec::new();

        // Check all pairs of traces
        for i in 0..traces.len() {
            for j in (i + 1)..traces.len() {
                let (name_a, power_a, pos_a) = &traces[i];
                let (name_b, power_b, pos_b) = &traces[j];

                let distance = (pos_a - pos_b).abs();

                // If traces are close and both have significant power
                if distance < clustering_threshold_nm && *power_a > 100.0 && *power_b > 100.0 {
                    violations.push(ThermalViolation::ThermalClustering {
                        nets: vec![name_a.clone().into(), name_b.clone().into()],
                        combined_power_mw: power_a + power_b,
                        distance_nm: distance,
                    });
                }
            }
        }

        violations
    }

    /// Validate maximum temperature
    ///
    /// # Arguments
    /// * `ambient_temp_c` - Ambient temperature in °C
    /// * `temperature_rise_c` - Temperature rise from power dissipation
    /// * `max_operating_temp_c` - Maximum safe operating temperature
    ///
    /// # Returns
    /// Ok if temperature is safe, Err with violation otherwise
    pub fn validate_max_temperature(
        &self,
        net_name: &str,
        ambient_temp_c: f64,
        temperature_rise_c: f64,
        max_operating_temp_c: f64,
    ) -> Result<(), ThermalViolation> {
        let actual_temp_c = ambient_temp_c + temperature_rise_c;

        if actual_temp_c > max_operating_temp_c {
            Err(ThermalViolation::MaxTemperature {
                net: net_name.into(),
                actual_temp_c,
                max_temp_c: max_operating_temp_c,
            })
        } else {
            Ok(())
        }
    }

    /// Validate temperature rise against profile constraints
    ///
    /// # Arguments
    /// * `net_name` - Name of the net
    /// * `actual_rise_c` - Actual temperature rise
    /// * `constraints` - Profile constraints
    ///
    /// # Returns
    /// Ok if within limit, Err with violation otherwise
    pub fn validate_temperature_rise_with_constraints(
        &self,
        net_name: &str,
        actual_rise_c: f64,
        constraints: &ProfileConstraints,
    ) -> Result<(), ThermalViolation> {
        if let Some(max_rise_c) = constraints.max_temp_rise_c {
            if actual_rise_c > max_rise_c {
                return Err(ThermalViolation::TemperatureRise {
                    net: net_name.into(),
                    actual_rise_c,
                    max_rise_c,
                });
            }
        }
        Ok(())
    }

    /// Validate maximum temperature against profile constraints
    ///
    /// # Arguments
    /// * `net_name` - Name of the net
    /// * `temperature_rise_c` - Temperature rise from power dissipation
    /// * `constraints` - Profile constraints
    ///
    /// # Returns
    /// Ok if temperature is safe, Err with violation otherwise
    pub fn validate_max_temperature_with_constraints(
        &self,
        net_name: &str,
        temperature_rise_c: f64,
        constraints: &ProfileConstraints,
    ) -> Result<(), ThermalViolation> {
        let actual_temp_c = constraints.ambient_temp_c + temperature_rise_c;

        if let Some(max_temp_c) = constraints.max_operating_temp_c {
            if actual_temp_c > max_temp_c {
                return Err(ThermalViolation::MaxTemperature {
                    net: net_name.into(),
                    actual_temp_c,
                    max_temp_c,
                });
            }
        }
        Ok(())
    }

    /// Generate auto-fix suggestion for temperature rise violation
    ///
    /// # Arguments
    /// * `violation` - Temperature rise violation
    /// * `trace_width_nm` - Current trace width
    ///
    /// # Returns
    /// Human-readable suggestion for fixing the violation
    pub fn suggest_temperature_fix(
        &self,
        violation: &ThermalViolation,
        trace_width_nm: i64,
    ) -> CompactString {
        match violation {
            ThermalViolation::TemperatureRise {
                net,
                actual_rise_c,
                max_rise_c,
            } => {
                let ratio = actual_rise_c / max_rise_c;
                let suggested_width_nm = (trace_width_nm as f64 * ratio.sqrt()).ceil() as i64;
                let suggested_width_um = suggested_width_nm / 1000;

                format!(
                    "Net '{}': Temperature rise {:.1}°C exceeds limit {:.1}°C\n\
                     💡 Auto-fix suggestions:\n\
                     1. Widen trace from {}µm to {}µm (increases heat dissipation area)\n\
                     2. Add thermal vias to inner ground/power planes (improves heat spreading)\n\
                     3. Increase copper thickness (70µm instead of 35µm)\n\
                     4. Add thermal relief pads to reduce thermal resistance\n\
                     5. Increase spacing from other high-power traces",
                    net,
                    actual_rise_c,
                    max_rise_c,
                    trace_width_nm / 1000,
                    suggested_width_um
                )
                .into()
            }
            ThermalViolation::MaxTemperature {
                net,
                actual_temp_c,
                max_temp_c,
            } => format!(
                "Net '{}': Operating temperature {:.1}°C exceeds limit {:.1}°C\n\
                     💡 Auto-fix suggestions:\n\
                     1. Add thermal vias near high-power components\n\
                     2. Use heat sinks or thermal pads on hot components\n\
                     3. Increase board copper weight (2oz instead of 1oz)\n\
                     4. Add cooling fans or forced air circulation\n\
                     5. Reduce ambient temperature or improve ventilation",
                net, actual_temp_c, max_temp_c
            )
            .into(),
            ThermalViolation::ThermalClustering {
                nets,
                combined_power_mw,
                distance_nm,
            } => format!(
                "Thermal clustering detected: {} nets within {}mm dissipating {:.1}mW\n\
                     💡 Auto-fix suggestions:\n\
                     1. Increase spacing between high-power traces (>5mm recommended)\n\
                     2. Route high-power traces on opposite sides of board\n\
                     3. Add thermal vias between clustered traces\n\
                     4. Use thicker copper or wider traces to spread heat\n\
                     5. Add ground plane for heat spreading",
                nets.join(", "),
                distance_nm / 1_000_000,
                combined_power_mw
            )
            .into(),
        }
    }

    /// Perform complete thermal analysis on a trace
    ///
    /// # Arguments
    /// * `params` - Thermal analysis parameters
    ///
    /// # Returns
    /// Complete thermal analysis results
    pub fn analyze_trace_thermal(&self, params: ThermalAnalysisParams) -> ThermalAnalysis {
        let temperature_rise_c = self.calculate_temperature_rise(
            params.power_mw,
            params.length_nm,
            params.width_nm,
            params.thickness_nm,
            params.thermal_conductivity_w_mk,
        );

        let actual_temp_c = params.ambient_temp_c + temperature_rise_c;
        let is_safe = actual_temp_c <= params.max_operating_temp_c;

        ThermalAnalysis {
            temperature_rise_c,
            is_safe,
            max_safe_temp_c: params.max_operating_temp_c,
        }
    }
}
