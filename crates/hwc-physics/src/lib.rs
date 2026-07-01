use compact_str::CompactString;

pub mod clearance;
pub mod connectivity;
pub mod electrical;
pub mod electromagnetic;
pub mod error_mapping;
pub mod geometry;
pub mod metadata_tracker;
pub mod parasitic;
pub mod pivb;
pub mod property_extraction;
pub mod spatial_index;
pub mod thermal;
pub mod timing;

pub use clearance::{ClearanceAnalyzer, ClearanceViolation};
pub use connectivity::{ConnectivityChecker, ConnectivityViolation};
pub use electrical::{ElectricalAnalysis, ElectricalAnalyzer, ElectricalViolation};
pub use electromagnetic::{EMAnalysis, EMAnalyzer, EMViolation};
pub use error_mapping::{
    clearance_to_error, connectivity_to_error, electrical_to_error, em_to_error,
    pivb_to_error, thermal_to_error, PhysicsError,
};
pub use geometry::{BoundingBox, Direction, Point3D, TraceSegment};
pub use metadata_tracker::{MetadataChangeFlags, MetadataTracker};
pub use parasitic::{ParasiticExtractionParams, ParasiticExtractor, ParasiticValues};
pub use pivb::{
    ConnectivityGraph, ConnectivityResult, ContactPlacement, FragmentationReport,
    FragmentedIsland, PivbSolver, PlanarIsland, VerticalBridge,
};
pub use property_extraction::{
    extract_dielectric_strength, extract_relative_permittivity, extract_resistivity,
    extract_thermal_conductivity, PropertyError,
};
pub use spatial_index::{DynamicSpatialIndex, IndexedSegment, SpatialEntitySource};
pub use thermal::{ThermalAnalysis, ThermalAnalyzer, ThermalViolation};
pub use timing::{TimingAnalyzer, TimingConstraint, TimingResult, TimingViolation};

/// Bridge rule for multi-material continuity (v0.1.7)
#[derive(Debug, Clone)]
pub struct BridgeRule {
    pub from_material: CompactString,
    pub to_material: CompactString,
    pub interface_material: CompactString,
    pub fill_material: CompactString,
}

/// Vector-first route segment metadata for continuity checking.
#[derive(Debug, Clone)]
pub struct RouteSegmentMetadata {
    pub net: u32,
    pub net_name: Option<CompactString>,
    pub material: u8,
    pub bbox: BoundingBox,
}

/// Comprehensive physics validation report.
///
/// Contains all violations from all physics analyzers.
#[derive(Debug, Clone)]
pub struct PhysicsReport {
    pub electrical_violations: Vec<ElectricalViolation>,
    pub thermal_violations: Vec<ThermalViolation>,
    pub em_violations: Vec<EMViolation>,
    pub clearance_violations: Vec<ClearanceViolation>,
    pub connectivity_violations: Vec<ConnectivityViolation>,
    pub pivb_results: Vec<ConnectivityResult>,
}

impl PhysicsReport {
    pub fn new() -> Self {
        Self {
            electrical_violations: Vec::new(),
            thermal_violations: Vec::new(),
            em_violations: Vec::new(),
            clearance_violations: Vec::new(),
            connectivity_violations: Vec::new(),
            pivb_results: Vec::new(),
        }
    }

    pub fn is_valid(&self) -> bool {
        self.electrical_violations.is_empty()
            && self.thermal_violations.is_empty()
            && self.em_violations.is_empty()
            && self.clearance_violations.is_empty()
            && self.connectivity_violations.is_empty()
            && self.pivb_results.iter().all(|r| r.is_pass())
    }

    pub fn total_violations(&self) -> usize {
        self.electrical_violations.len()
            + self.thermal_violations.len()
            + self.em_violations.len()
            + self.clearance_violations.len()
            + self.connectivity_violations.len()
            + self.pivb_results.iter().filter(|r| r.is_fail()).count()
    }

    /// Convert all violations to error codes with messages
    pub fn to_errors(&self) -> Vec<PhysicsError> {
        let mut errors = Vec::new();

        for violation in &self.electrical_violations {
            errors.push(electrical_to_error(violation));
        }

        for violation in &self.thermal_violations {
            errors.push(thermal_to_error(violation));
        }

        for violation in &self.em_violations {
            errors.push(em_to_error(violation));
        }

        for violation in &self.clearance_violations {
            errors.push(clearance_to_error(violation));
        }

        for violation in &self.connectivity_violations {
            errors.push(connectivity_to_error(violation));
        }

        for result in &self.pivb_results {
            if let ConnectivityResult::Fail(report) = result {
                errors.push(pivb_to_error(report));
            }
        }

        errors
    }

    pub fn format_report(&self) -> CompactString {
        let mut output = String::new();

        if self.is_valid() {
            output.push_str("✓ Design passes all physics checks\n");
            return output.into();
        }

        output.push_str(&format!(
            "✗ {} physics violation(s) found:\n\n",
            self.total_violations()
        ));

        // Electrical violations
        if !self.electrical_violations.is_empty() {
            output.push_str(&format!(
                "⚡ Electrical Violations ({}):\n",
                self.electrical_violations.len()
            ));
            for (i, violation) in self.electrical_violations.iter().enumerate() {
                output.push_str(&format!("  {}. ", i + 1));
                match violation {
                    ElectricalViolation::VoltageDrop {
                        net,
                        actual_mv,
                        max_mv,
                    } => {
                        output.push_str(&format!(
                            "Voltage Drop: Net '{}' has {:.1}mV drop, exceeds max {:.1}mV\n",
                            net, actual_mv, max_mv
                        ));
                    }
                    ElectricalViolation::Resistance {
                        net,
                        actual_ohm,
                        max_ohm,
                    } => {
                        output.push_str(&format!(
                            "Resistance: Net '{}' has {:.3}Ω, exceeds max {:.3}Ω\n",
                            net, actual_ohm, max_ohm
                        ));
                    }
                    ElectricalViolation::Ampacity {
                        net,
                        current_ma,
                        required_width_nm,
                        actual_width_nm,
                    } => {
                        output.push_str(&format!(
                            "Ampacity: Net '{}' requires {}µm trace width for {}mA current, actual: {}µm\n",
                            net,
                            required_width_nm / 1000,
                            current_ma,
                            actual_width_nm / 1000
                        ));
                    }
                }
            }
            output.push('\n');
        }

        // Thermal violations
        if !self.thermal_violations.is_empty() {
            output.push_str(&format!(
                "🔥 Thermal Violations ({}):\n",
                self.thermal_violations.len()
            ));
            for (i, violation) in self.thermal_violations.iter().enumerate() {
                output.push_str(&format!("  {}. ", i + 1));
                match violation {
                    ThermalViolation::TemperatureRise {
                        net,
                        actual_rise_c,
                        max_rise_c,
                    } => {
                        output.push_str(&format!(
                            "Temperature Rise: Net '{}' rises {:.1}°C, exceeds max {:.1}°C\n",
                            net, actual_rise_c, max_rise_c
                        ));
                    }
                    ThermalViolation::MaxTemperature {
                        net,
                        actual_temp_c,
                        max_temp_c,
                    } => {
                        output.push_str(&format!(
                            "Max Temperature: Net '{}' reaches {:.1}°C, exceeds max {:.1}°C\n",
                            net, actual_temp_c, max_temp_c
                        ));
                    }
                    ThermalViolation::ThermalClustering {
                        nets,
                        combined_power_mw,
                        distance_nm,
                    } => {
                        output.push_str(&format!(
                            "Thermal Clustering: {} nets within {}mm dissipating {:.1}mW\n",
                            nets.join(", "),
                            distance_nm / 1_000_000,
                            combined_power_mw
                        ));
                    }
                }
            }
            output.push('\n');
        }

        // Electromagnetic violations
        if !self.em_violations.is_empty() {
            output.push_str(&format!(
                "📡 Electromagnetic Violations ({}):\n",
                self.em_violations.len()
            ));
            for (i, violation) in self.em_violations.iter().enumerate() {
                output.push_str(&format!("  {}. {:?}\n", i + 1, violation));
            }
            output.push('\n');
        }

        // Clearance violations
        if !self.clearance_violations.is_empty() {
            output.push_str(&format!(
                "⚠️  Clearance Violations ({}):\n",
                self.clearance_violations.len()
            ));
            for (i, violation) in self.clearance_violations.iter().enumerate() {
                output.push_str(&format!("  {}. {:?}\n", i + 1, violation));
            }
            output.push('\n');
        }

        // Connectivity violations
        if !self.connectivity_violations.is_empty() {
            output.push_str(&format!(
                "🔌 Connectivity Violations ({}):\n",
                self.connectivity_violations.len()
            ));
            for (i, violation) in self.connectivity_violations.iter().enumerate() {
                output.push_str(&format!("  {}. ", i + 1));
                match violation {
                    ConnectivityViolation::DisconnectedNet {
                        net_name,
                        pour_a,
                        pour_b,
                        reason,
                        smart_hint,
                    } => {
                        output.push_str(&format!(
                            "Disconnected Net: Net '{}' has no physical path between '{}' and '{}'\n",
                            net_name, pour_a, pour_b
                        ));
                        output.push_str(&format!("     Reason: {}\n", reason));
                        if let Some(hint) = smart_hint {
                            output.push_str(&format!("     Hint: {}\n", hint));
                        }
                    }
                    ConnectivityViolation::MaterialInterpenetration {
                        net_name,
                        pour_a,
                        pour_b,
                        material_a,
                        material_b,
                        overlap_location,
                    } => {
                        output.push_str(&format!(
                            "Material Interpenetration: Pour '{}' (material: {}) overlaps with pour '{}' (material: {}) on net '{}'\n",
                            pour_a, material_a, pour_b, material_b, net_name
                        ));
                        output.push_str(&format!("     Location: {}\n", overlap_location));
                        output.push_str(
                            "     Different materials cannot occupy the same physical space.\n",
                        );
                        output.push_str("     Hint: Adjust boundaries so pours touch at edges but do not overlap.\n");
                    }
                }
            }
            output.push('\n');
        }

        // PIVB Connectivity results (Layer 3)
        let failures: Vec<&FragmentationReport> = self.pivb_results.iter()
            .filter_map(|r| if let ConnectivityResult::Fail(f) = r { Some(f) } else { None })
            .collect();

        if !failures.is_empty() {
            output.push_str(&format!(
                "⚡ Physical Connectivity Violations ({}):\n",
                failures.len()
            ));
            for (i, report) in failures.iter().enumerate() {
                output.push_str(&format!("  {}. ", i + 1));
                output.push_str(&format!(
                    "Physical Disconnection: Net '{}' has {} disconnected islands\n",
                    report.net_name, report.component_count
                ));
                for island in &report.islands {
                    output.push_str(&format!(
                        "     Island group {} at z:{}-{} ({} nodes)\n",
                        island.group_index,
                        island.bbox.min.z / 1_000_000,
                        island.bbox.max.z / 1_000_000,
                        island.island_count
                    ));
                }
                output.push_str(&format!("     Fix: {}\n", report.suggested_fix));
            }
            output.push('\n');
        }

        output.push_str("💡 Auto-Fix Suggestions:\n");

        // Generate specific suggestions based on violations
        let mut suggestions_added = false;

        for violation in &self.electrical_violations {
            match violation {
                ElectricalViolation::VoltageDrop { net, .. } => {
                    output.push_str(&format!(
                        "  - Widen trace '{}' or insert buffer to reduce voltage drop\n",
                        net
                    ));
                    suggestions_added = true;
                }
                ElectricalViolation::Ampacity {
                    net,
                    required_width_nm,
                    ..
                } => {
                    output.push_str(&format!(
                        "  - Widen trace '{}' to {}µm (IPC-2221 requirement)\n",
                        net,
                        required_width_nm / 1000
                    ));
                    suggestions_added = true;
                }
                _ => {}
            }
        }

        for violation in &self.thermal_violations {
            match violation {
                ThermalViolation::TemperatureRise { net, .. }
                | ThermalViolation::MaxTemperature { net, .. } => {
                    output.push_str(&format!(
                        "  - Add thermal vias near '{}' for heat dissipation\n",
                        net
                    ));
                    suggestions_added = true;
                }
                ThermalViolation::ThermalClustering { .. } => {
                    output.push_str("  - Increase spacing between high-power traces\n");
                    suggestions_added = true;
                }
            }
        }

        if !suggestions_added {
            output.push_str("  - Review trace widths for high-current nets\n");
            output.push_str("  - Increase clearances for high-voltage nets\n");
            output.push_str("  - Add thermal vias for heat dissipation\n");
            output.push_str("  - Adjust trace geometry for impedance control\n");
        }

        output.into()
    }
}

impl Default for PhysicsReport {
    fn default() -> Self {
        Self::new()
    }
}

/// Physics validation engine for Hardware Script designs.
///
/// This is the unified entry point for Layer 4 (Physics IR) validation.
/// It receives the Hardware IR and Symbol Table from Layer 2 (Compiler)
/// and validates electrical, thermal, electromagnetic, and clearance requirements.
///
/// # Architecture
///
/// The PhysicsEngine integrates with the compiler pipeline:
/// ```text
/// Layer 2: Logical IR (hwc-compiler) — Two-Pass Compilation
///          ↓ (produces HardwareIR + SymbolTable)
/// Layer 3: Physical IR (hwc-engine) — Routing + EntityGraph
///          ↓ (produces routed board)
/// Layer 4: Physics IR (hwc-physics) — Validation ← PhysicsEngine
///          ↓ (validates against physics constraints)
/// Layer 5: Manufacturing (hwc-export) — File Generation
/// ```
///
/// # Usage
///
/// Create a PhysicsEngine and validate a design against physics constraints.
/// The engine checks thermal, electrical, electromagnetic, and clearance requirements.
#[derive(Default)]
pub struct PhysicsEngine {
    pub thermal: ThermalAnalyzer,
    pub electrical: ElectricalAnalyzer,
    pub em: EMAnalyzer,
    pub clearance: ClearanceAnalyzer,
    pub metadata_tracker: MetadataTracker,
}

impl PhysicsEngine {
    pub fn new() -> Self {
        Self {
            thermal: ThermalAnalyzer::new(),
            electrical: ElectricalAnalyzer::new(),
            em: EMAnalyzer::new(),
            clearance: ClearanceAnalyzer::new(),
            metadata_tracker: MetadataTracker::new(),
        }
    }

    /// Check if metadata changed and determine which physics passes need re-validation
    ///
    /// This should be called at the start of every compile to detect changes in:
    /// - Material properties (resistivity, dielectric strength, thermal conductivity)
    /// - Profile constraints (thermal, electrical, clearance)
    /// - Manufacturing constraints (copper thickness, IPC constants)
    /// - Stackup/layer configuration
    ///
    /// # Performance
    ///
    /// Target: < 1 microsecond for hash computation and comparison
    ///
    /// # Returns
    ///
    /// Flags indicating which physics passes need re-validation
    pub fn check_metadata_changed<M, P, F, S>(
        &mut self,
        materials: &M,
        profile: &P,
        manufacturing: &F,
        stackup: &S,
    ) -> MetadataChangeFlags
    where
        M: std::hash::Hash,
        P: std::hash::Hash,
        F: std::hash::Hash,
        S: std::hash::Hash,
    {
        self.metadata_tracker
            .check_metadata_changed(materials, profile, manufacturing, stackup)
    }

    /// Force re-validation of all physics passes
    ///
    /// Resets metadata tracker, causing next check to report all metadata changed.
    /// Use this when you want to force a full re-validation regardless of changes.
    pub fn force_revalidation(&mut self) {
        self.metadata_tracker.force_revalidation();
    }

    /// Validate a complete design against all physics constraints.
    ///
    /// This is the unified entry point for physics validation (Layer 4).
    /// It runs all 4 physics analyzers sequentially and collects violations.
    ///
    /// # Arguments
    /// * `symbol_table` - Symbol Table containing material and profile definitions
    /// * `board_data` - Optional board data with routed traces (None for stub mode)
    ///
    /// # Returns
    /// Comprehensive physics report with all violations
    ///
    /// # Note
    /// This is a stub implementation. Full validation requires:
    /// - Hardware IR with routed traces
    /// - Board structure from Layer 3 (hwc-engine)
    /// - Net voltage and current information
    pub fn validate_design<T>(
        &self,
        _symbol_table: &T,
        _board_data: Option<&BoardData>,
    ) -> PhysicsReport
    where
        T: electrical::SymbolTableTrait
            + thermal::SymbolTableTrait
            + electromagnetic::SymbolTableTrait
            + clearance::SymbolTableTrait,
    {
        // For now, return empty report
        // This will be enhanced when we have actual board data structures from Layer 3
        PhysicsReport::new()
    }

    /// Validate a complete design against all physics constraints in parallel.
    ///
    /// This runs all 4 physics analyzers in parallel using Rayon for improved performance.
    /// All analyzers have read-only access to the board data, ensuring thread safety.
    /// Results are collected deterministically.
    ///
    /// # Arguments
    /// * `symbol_table` - Symbol Table containing material and profile definitions
    /// * `board_data` - Optional board data with routed traces (None for stub mode)
    ///
    /// # Returns
    /// Comprehensive physics report with all violations
    ///
    /// # Performance
    /// Expected ~4× speedup on multi-core systems compared to sequential validation.
    ///
    /// # Note
    /// This is a stub implementation. Full validation requires:
    /// - Hardware IR with routed traces
    /// - Board structure from Layer 3 (hwc-engine)
    /// - Net voltage and current information
    pub fn validate_design_parallel<T>(
        &self,
        _symbol_table: &T,
        _board_data: Option<&BoardData>,
    ) -> PhysicsReport
    where
        T: electrical::SymbolTableTrait
            + thermal::SymbolTableTrait
            + electromagnetic::SymbolTableTrait
            + clearance::SymbolTableTrait
            + Sync,
    {
        use rayon::prelude::*;

        // Type alias to reduce complexity
        type AnalyzerFn<T> = fn(&PhysicsEngine, &T, Option<&BoardData>) -> PhysicsReport;

        // Create a vector of analyzer functions
        // Each returns a PhysicsReport with violations from that analyzer
        let analyzers: Vec<AnalyzerFn<T>> = vec![
            PhysicsEngine::run_electrical_analysis,
            PhysicsEngine::run_thermal_analysis,
            PhysicsEngine::run_em_analysis,
            PhysicsEngine::run_clearance_analysis,
        ];

        // Run all analyzers in parallel (read-only access to self and symbol_table)
        let reports: Vec<PhysicsReport> = analyzers
            .par_iter()
            .map(|analyzer| analyzer(self, _symbol_table, _board_data))
            .collect();

        // Merge all results into a single report
        let mut report = PhysicsReport::new();
        for partial_report in reports {
            report
                .electrical_violations
                .extend(partial_report.electrical_violations);
            report
                .thermal_violations
                .extend(partial_report.thermal_violations);
            report.em_violations.extend(partial_report.em_violations);
            report
                .clearance_violations
                .extend(partial_report.clearance_violations);
        }

        report
    }

    // Individual analyzer runners (read-only access)
    fn run_electrical_analysis<T>(
        &self,
        _symbol_table: &T,
        _board_data: Option<&BoardData>,
    ) -> PhysicsReport
    where
        T: electrical::SymbolTableTrait,
    {
        // For now, return empty report
        // This will be enhanced when we have actual board data structures
        PhysicsReport::new()
    }

    fn run_thermal_analysis<T>(
        &self,
        _symbol_table: &T,
        _board_data: Option<&BoardData>,
    ) -> PhysicsReport
    where
        T: thermal::SymbolTableTrait,
    {
        PhysicsReport::new()
    }

    fn run_em_analysis<T>(
        &self,
        _symbol_table: &T,
        _board_data: Option<&BoardData>,
    ) -> PhysicsReport
    where
        T: electromagnetic::SymbolTableTrait,
    {
        PhysicsReport::new()
    }

    fn run_clearance_analysis<T>(
        &self,
        _symbol_table: &T,
        _board_data: Option<&BoardData>,
    ) -> PhysicsReport
    where
        T: clearance::SymbolTableTrait,
    {
        PhysicsReport::new()
    }
}

/// Board data structure for physics validation.
///
/// This will be populated by Layer 3 (hwc-engine) with actual routed trace geometry.
/// For now, it's a placeholder for future integration.
#[derive(Debug, Clone)]
pub struct BoardData {
    pub nets: Vec<NetData>,
    pub profile_name: CompactString,
}

/// Net data for physics validation.
///
/// Contains all information needed to validate a single net against physics constraints.
#[derive(Debug, Clone)]
pub struct NetData {
    pub name: CompactString,
    pub voltage_mv: Option<i64>,
    pub current_ma: Option<i64>,
    pub traces: Vec<TraceData>,
}

/// Trace segment data for physics validation.
///
/// Represents a single routed trace segment with geometry and material information.
#[derive(Debug, Clone)]
pub struct TraceData {
    pub length_nm: i64,
    pub width_nm: i64,
    pub thickness_nm: i64,
    pub material_name: CompactString,
    pub layer: u8,
}
