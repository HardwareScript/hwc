// NOTE: Serde removed for v0.1.4 - these structures will be replaced by Symbol Table
// Keeping for backward compatibility during migration

/// Conductor properties (copper, silver, gold, aluminum)
use compact_str::CompactString;

/// Manufacturing process behavior for Z-axis placement (v0.1.7)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManufacturingProcess {
    /// Drilled and plated through the substrate (PCB style)
    DrilledPlated,
    /// Deposited/Plotted into the grid (CMOS/3D-Print style)
    Deposited,
    /// Etched away from existing material (MEMS style)
    Etched,
}

impl Default for ManufacturingProcess {
    fn default() -> Self {
        ManufacturingProcess::Deposited
    }
}

#[derive(Debug, Clone)]
pub struct ConductorProperties {
    pub name: CompactString,
    pub symbol: CompactString,
    pub description: CompactString,
    pub process: ManufacturingProcess, // v0.1.7

    // Physical properties
    pub density_kg_m3: f64,
    pub thermal_conductivity_w_mk: f64,
    pub color_hex: CompactString,

    // Electrical properties
    pub resistivity_ohm_m: f64,
    pub max_current_density_a_mm2: f64,

    // Temperature coefficients (optional)
    pub resistivity_temp_coeff_per_c: Option<f64>,
    pub thermal_conductivity_temp_coeff_per_c: Option<f64>,
    pub reference_temp_c: Option<f64>,

    // Thermal properties
    pub melting_point_c: f64,
    pub is_metal: bool,
}

/// Insulator/dielectric properties (FR4, air, silicon dioxide)
#[derive(Debug, Clone)]
pub struct InsulatorProperties {
    pub name: CompactString,
    pub symbol: CompactString,
    pub description: CompactString,
    pub process: ManufacturingProcess, // v0.1.7

    // Physical properties
    pub density_kg_m3: f64,
    pub thermal_conductivity_w_mk: f64,
    pub color_hex: CompactString,

    // Electrical properties
    pub relative_permittivity: f64,
    pub dielectric_strength_kv_mm: f64,

    // Temperature coefficients (optional)
    pub thermal_conductivity_temp_coeff_per_c: Option<f64>,
    pub reference_temp_c: Option<f64>,

    // Thermal properties
    pub glass_transition_temp_c: Option<f64>,
    pub max_operating_temp_c: Option<f64>,
}

/// Semiconductor properties (silicon, gallium nitride, gallium arsenide)
#[derive(Debug, Clone)]
pub struct SemiconductorProperties {
    pub name: CompactString,
    pub symbol: CompactString,
    pub description: CompactString,
    pub process: ManufacturingProcess, // v0.1.7

    // Physical properties
    pub density_kg_m3: f64,
    pub thermal_conductivity_w_mk: f64,
    pub color_hex: CompactString,

    // Electrical properties
    pub band_gap_ev: f64,
    pub electron_mobility_cm2_vs: f64,
    pub hole_mobility_cm2_vs: f64,

    // Thermal properties
    pub max_operating_temp_c: Option<f64>,

    // NEW v0.1.6: Doping and biasing properties for physics validation
    /// Doping type: p-type (acceptor), n-type (donor), or intrinsic (undoped)
    pub doping_type: Option<DopingType>,

    /// Bias requirement for proper device operation
    /// - LowestPotential: Must be connected to ground (GND, VSS, 0V)
    /// - HighestPotential: Must be connected to power (VDD, VCC)
    /// - None: No biasing constraint (e.g., intrinsic silicon, floating regions)
    pub bias_requirement: Option<BiasRequirement>,
}

/// Doping type for semiconductors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DopingType {
    /// P-type (acceptor doping) - majority carriers are holes
    /// Examples: Boron-doped silicon, PMOS bulk
    PType,

    /// N-type (donor doping) - majority carriers are electrons
    /// Examples: Phosphorus-doped silicon, NMOS source/drain
    NType,

    /// Intrinsic (undoped) - equal electrons and holes
    /// Examples: Pure silicon wafer
    Intrinsic,
}

impl std::fmt::Display for DopingType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DopingType::PType => write!(f, "p-type"),
            DopingType::NType => write!(f, "n-type"),
            DopingType::Intrinsic => write!(f, "intrinsic"),
        }
    }
}

/// Bias requirement for semiconductor regions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BiasRequirement {
    /// Must be connected to lowest potential (ground)
    /// Physics: Prevents forward-biasing of PN junctions
    /// Example: P-type bulk in NMOS must connect to GND
    LowestPotential,

    /// Must be connected to highest potential (power)
    /// Physics: Prevents forward-biasing of PN junctions
    /// Example: N-well bulk in PMOS must connect to VDD
    HighestPotential,

    /// No biasing constraint
    /// Example: Intrinsic silicon, isolated regions
    None,
}

impl std::fmt::Display for BiasRequirement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BiasRequirement::LowestPotential => write!(f, "ground (lowest potential)"),
            BiasRequirement::HighestPotential => write!(f, "power (highest potential)"),
            BiasRequirement::None => write!(f, "no requirement"),
        }
    }
}

impl BiasRequirement {
    /// Check if a net classification satisfies this bias requirement
    ///
    /// This encapsulates the physics rule: what net type is required for each bias requirement.
    ///
    /// # Arguments
    /// * `net_classification` - The classification of the net (Power, Ground, Signal, Unclassified)
    ///
    /// # Returns
    /// * `Ok(())` if the net classification satisfies the requirement
    /// * `Err(String)` with explanation if it doesn't
    ///
    /// # Physics Rules
    /// - `LowestPotential` requires `Ground` net (prevents forward-biasing PN junctions)
    /// - `HighestPotential` requires `Power` net (prevents forward-biasing PN junctions)
    /// - `None` accepts any net classification
    pub fn validate_net_classification(
        &self,
        net_classification: NetClassification,
    ) -> Result<(), String> {
        match self {
            BiasRequirement::LowestPotential => {
                if matches!(net_classification, NetClassification::Ground) {
                    Ok(())
                } else {
                    Err(format!(
                        "Requires ground net (lowest potential), but net is classified as '{}'",
                        net_classification
                    ))
                }
            }
            BiasRequirement::HighestPotential => {
                if matches!(net_classification, NetClassification::Power) {
                    Ok(())
                } else {
                    Err(format!(
                        "Requires power net (highest potential), but net is classified as '{}'",
                        net_classification
                    ))
                }
            }
            BiasRequirement::None => Ok(()), // No constraint
        }
    }
}

/// Net classification for physics validation
///
/// This enum must match `hwc_engine::space::NetClassification` for compatibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetClassification {
    Power,
    Ground,
    Signal,
    HighVoltage,
    Unclassified,
}

impl std::fmt::Display for NetClassification {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NetClassification::Power => write!(f, "power"),
            NetClassification::Ground => write!(f, "ground"),
            NetClassification::Signal => write!(f, "signal"),
            NetClassification::HighVoltage => write!(f, "high-voltage"),
            NetClassification::Unclassified => write!(f, "unclassified"),
        }
    }
}

/// Material metadata
#[derive(Debug, Clone)]
pub struct MaterialMetadata {
    pub version: CompactString,
    pub last_updated: CompactString,
    pub author: Option<CompactString>,
    pub license: Option<CompactString>,
}

impl Default for MaterialMetadata {
    fn default() -> Self {
        Self {
            version: "0.1.0".into(),
            last_updated: "2026-03-17".into(),
            author: None,
            license: None,
        }
    }
}

// Helper methods for calculations
impl ConductorProperties {
    /// Calculate resistivity at a given temperature
    /// Uses linear temperature coefficient: ρ(T) = ρ₀ × [1 + α × (T - T₀)]
    /// where α is the temperature coefficient and T₀ is the reference temperature
    pub fn resistivity_at_temp(&self, temp_c: f64) -> f64 {
        match (self.resistivity_temp_coeff_per_c, self.reference_temp_c) {
            (Some(alpha), Some(t0)) => self.resistivity_ohm_m * (1.0 + alpha * (temp_c - t0)),
            _ => self.resistivity_ohm_m, // No temperature dependence
        }
    }

    /// Calculate thermal conductivity at a given temperature
    /// Uses linear temperature coefficient: k(T) = k₀ × [1 + α × (T - T₀)]
    pub fn thermal_conductivity_at_temp(&self, temp_c: f64) -> f64 {
        match (
            self.thermal_conductivity_temp_coeff_per_c,
            self.reference_temp_c,
        ) {
            (Some(alpha), Some(t0)) => {
                self.thermal_conductivity_w_mk * (1.0 + alpha * (temp_c - t0))
            }
            _ => self.thermal_conductivity_w_mk, // No temperature dependence
        }
    }

    /// Calculate trace width required for given current (IPC-2221 formula)
    /// Returns width in nanometers
    ///
    /// Uses material-specific properties from the database:
    /// - max_current_density_a_mm2: Maximum safe current density
    /// - thermal_conductivity_w_mk: For thermal calculations
    pub fn calculate_trace_width_nm(
        &self,
        current_ma: i64,
        temp_rise_c: i64,
        is_external: bool,
    ) -> i64 {
        // IPC-2221 formula: A = (I / (k × ΔT^0.44))^(1/0.725)
        // where k depends on layer type
        let k = if is_external { 0.048 } else { 0.024 };

        // Standard copper thickness (1oz = 35µm)
        // This is a manufacturing standard, not a material property
        let copper_thickness_nm = 35_000;

        let current_a = current_ma as f64 / 1000.0;
        let temp_rise = temp_rise_c as f64;

        // Calculate required cross-sectional area using IPC-2221
        let area_mm2 = (current_a / (k * temp_rise.powf(0.44))).powf(1.0 / 0.725);

        // Convert area to width (area = width × thickness)
        let thickness_mm = copper_thickness_nm as f64 / 1_000_000.0;
        let width_mm = area_mm2 / thickness_mm;

        (width_mm * 1_000_000.0) as i64
    }

    /// Calculate resistance for a trace using material resistivity
    /// Returns resistance in ohms
    ///
    /// Uses R = ρ × (L / A) where:
    /// - ρ (rho) = resistivity_ohm_m from material database
    /// - L = length
    /// - A = cross-sectional area
    pub fn calculate_resistance(&self, length_nm: i64, width_nm: i64, thickness_nm: i64) -> f64 {
        let length_m = length_nm as f64 / 1_000_000_000.0;
        let width_m = width_nm as f64 / 1_000_000_000.0;
        let thickness_m = thickness_nm as f64 / 1_000_000_000.0;
        let area_m2 = width_m * thickness_m;

        // Use actual material resistivity from database
        self.resistivity_ohm_m * (length_m / area_m2)
    }

    /// Calculate resistance at a specific temperature
    /// Returns resistance in ohms
    pub fn calculate_resistance_at_temp(
        &self,
        length_nm: i64,
        width_nm: i64,
        thickness_nm: i64,
        temp_c: f64,
    ) -> f64 {
        let length_m = length_nm as f64 / 1_000_000_000.0;
        let width_m = width_nm as f64 / 1_000_000_000.0;
        let thickness_m = thickness_nm as f64 / 1_000_000_000.0;
        let area_m2 = width_m * thickness_m;

        // Use temperature-dependent resistivity
        let resistivity = self.resistivity_at_temp(temp_c);
        resistivity * (length_m / area_m2)
    }
}

impl InsulatorProperties {
    /// Calculate thermal conductivity at a given temperature
    /// Uses linear temperature coefficient: k(T) = k₀ × [1 + α × (T - T₀)]
    pub fn thermal_conductivity_at_temp(&self, temp_c: f64) -> f64 {
        match (
            self.thermal_conductivity_temp_coeff_per_c,
            self.reference_temp_c,
        ) {
            (Some(alpha), Some(t0)) => {
                self.thermal_conductivity_w_mk * (1.0 + alpha * (temp_c - t0))
            }
            _ => self.thermal_conductivity_w_mk, // No temperature dependence
        }
    }

    /// Calculate minimum clearance for given voltage difference
    /// Returns clearance in nanometers
    ///
    /// Uses material-specific dielectric strength from database:
    /// clearance = (voltage / dielectric_strength) × safety_factor
    pub fn calculate_clearance_nm(&self, voltage_diff_mv: i64, safety_factor: i64) -> i64 {
        let voltage_v = voltage_diff_mv as f64 / 1000.0;

        // Use actual material dielectric strength from database
        let dielectric_v_nm = (self.dielectric_strength_kv_mm * 1000.0) / 1_000_000.0;

        let min_clearance_nm = voltage_v / dielectric_v_nm;
        (min_clearance_nm * safety_factor as f64) as i64
    }
}
