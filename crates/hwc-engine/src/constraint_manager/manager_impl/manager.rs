//! Constraint Manager: Main orchestration struct.
//!
//! This module implements the main ConstraintManager struct that coordinates
//! all constraint generation operations.

use super::bounding_box::calculate_module_bounding_box;
use super::constraint_generation::{
    generate_clearance_zone, generate_net_constraints, NetConstraintParams,
};
use super::electrical_analysis;
use super::impedance::calculate_trace_impedance;
use super::layer_assignment::assign_layer_directions;
use super::net_classification::classify_nets;
use super::symbol_table::SymbolTableTrait;
use crate::constraint_manager::types::{
    ConstraintRulebook, FabricationConstraints, LayerDirection,
};
use crate::geometry::BoundingBox;
use crate::netlist::NetlistArena;
use rustc_hash::FxHashMap;

/// Constraint Manager: Main entry point for constraint generation.
///
/// This manager orchestrates the translation of material properties and
/// electrical requirements into geometric constraints before routing begins.
///
/// **Documentation Reference**:
/// - `Docs/v0.1.3/ROUTING-AND-PHYSICS.md` (lines 1-400, constraint translation)
/// - `Docs/v0.1.3/COMPILER-INTERNALS.md` (lines 400-600, Layer 3 Physical IR)
pub struct ConstraintManager {
    /// Coordinate snapping resolution in nanometers
    manufacturing_grid_nm: i64,

    /// Default safety factor for clearance calculations
    safety_factor: i64,

    /// Default temperature rise for trace width calculations (°C)
    default_temp_rise_c: i64,

    /// Default maximum parallel length for crosstalk (nanometers)
    default_max_parallel_nm: i64,
}

impl ConstraintManager {
    /// Create a new constraint manager.
    ///
    /// All parameters must come from the PDK profile.
    pub fn new(
        manufacturing_grid_nm: i64,
        safety_factor: i64,
        default_temp_rise_c: i64,
        default_max_parallel_nm: i64,
    ) -> Self {
        Self {
            manufacturing_grid_nm,
            safety_factor,
            default_temp_rise_c,
            default_max_parallel_nm,
        }
    }

    /// Get the snapping resolution
    pub fn manufacturing_grid_nm(&self) -> i64 {
        self.manufacturing_grid_nm
    }

    /// Get the safety factor
    pub fn safety_factor(&self) -> i64 {
        self.safety_factor
    }

    /// Get the default temperature rise
    pub fn default_temp_rise_c(&self) -> i64 {
        self.default_temp_rise_c
    }

    /// Get the default maximum parallel length
    pub fn default_max_parallel_nm(&self) -> i64 {
        self.default_max_parallel_nm
    }

    /// Generate constraints for a single net.
    ///
    /// Delegates to the constraint_generation module.
    pub fn generate_net_constraints<S: SymbolTableTrait>(
        &self,
        net: &crate::netlist::NetData,
        params: &NetConstraintParams,
        symbol_table: &S,
    ) -> Result<crate::constraint_manager::types::RouteConstraints, String> {
        generate_net_constraints(net, params, symbol_table)
    }

    /// Generate clearance zone for a net.
    pub fn generate_clearance_zone<S: SymbolTableTrait>(
        &self,
        net_id: crate::netlist::NetId,
        voltage_mv: i64,
        material_name: &str,
        symbol_table: &S,
    ) -> Result<crate::constraint_manager::types::ClearanceZone, String> {
        generate_clearance_zone(
            net_id,
            voltage_mv,
            material_name,
            symbol_table,
            self.safety_factor,
        )
    }

    /// Assign layer directions for Manhattan routing.
    ///
    /// Delegates to the layer_assignment module.
    pub fn assign_layer_directions(&self, num_layers: usize) -> FxHashMap<usize, LayerDirection> {
        assign_layer_directions(num_layers)
    }

    /// Generate complete constraint rulebook for routing.
    ///
    /// This is the main entry point that generates all constraints
    /// before routing begins.
    ///
    /// **v0.1.4 Implementation**: Now performs complete constraint generation
    /// for all nets in the netlist, including electrical analysis and clearance zones.
    ///
    /// # Arguments
    /// * `netlist` - Netlist arena with all components, pins, and nets
    /// * `num_layers` - Number of routing layers
    /// * `material_name` - Name of the dielectric material (e.g., "FR4", "Air")
    /// * `symbol_table` - Symbol Table containing material and profile definitions
    /// * `is_external` - True for external layers, false for internal
    ///
    /// # Returns
    /// Complete constraint rulebook for the router, or error if materials not found
    ///
    /// # Errors
    /// Returns error if:
    /// - Material is not defined in Symbol Table
    /// - Material is missing required properties
    /// - Electrical analysis fails for any net
    pub fn generate_constraints<S: SymbolTableTrait>(
        &self,
        netlist: &NetlistArena,
        num_layers: usize,
        material_name: &str,
        symbol_table: &S,
        is_external: bool,
        fabrication_constraints: Option<&FabricationConstraints>,
    ) -> Result<ConstraintRulebook, String> {
        let mut rulebook = ConstraintRulebook::new(self.manufacturing_grid_nm);

        // Assign layer directions for Manhattan routing
        rulebook.layer_directions = self.assign_layer_directions(num_layers);

        // Iterate over all nets and generate constraints
        for net_id in netlist.all_net_ids() {
            let net = netlist
                .get_net(net_id)
                .ok_or_else(|| format!("Net ID {:?} not found in netlist", net_id))?;

            // Perform electrical analysis for this net
            let unit_registry = hwc_types::UnitRegistry::new(vec![]);
            let (voltage_mv, current_ma_opt) =
                electrical_analysis::analyze_net_electrical(net, netlist, None, &unit_registry)?;

            // Generate routing constraints for this net
            let params = NetConstraintParams {
                voltage_mv,
                current_ma: current_ma_opt.unwrap_or(0),
                material_name,
                is_external,
                safety_factor: self.safety_factor,
                default_temp_rise_c: self.default_temp_rise_c,
                default_max_parallel_nm: self.default_max_parallel_nm,
                fabrication_constraints,
            };

            let constraints = self.generate_net_constraints(net, &params, symbol_table)?;

            rulebook.per_net_constraints.insert(net_id, constraints);

            // Generate clearance zone if net has voltage
            if voltage_mv > 0 {
                let clearance_zone =
                    self.generate_clearance_zone(net_id, voltage_mv, material_name, symbol_table)?;

                rulebook.clearance_zones.push(clearance_zone);
            }
        }

        Ok(rulebook)
    }

    /// Calculate trace impedance.
    ///
    /// Delegates to the impedance module.
    pub fn calculate_trace_impedance(
        &self,
        trace_width_nm: i64,
        copper_thickness_nm: i64,
        dielectric_height_nm: i64,
        relative_permittivity: f64,
    ) -> f64 {
        calculate_trace_impedance(
            trace_width_nm,
            copper_thickness_nm,
            dielectric_height_nm,
            relative_permittivity,
        )
    }

    /// Calculate the bounding box for a module instance from its declaration.
    pub fn calculate_module_bounding_box(
        &self,
        module: &hwc_parser::ModuleDecl,
        arena: &hwc_parser::ast::arena::AstArena,
    ) -> BoundingBox {
        calculate_module_bounding_box(module, self.manufacturing_grid_nm, arena)
    }

    /// Classify all nets as internal (within a module) or global (crossing boundaries).
    pub fn classify_nets(
        &self,
        netlist: &NetlistArena,
    ) -> super::net_classification::NetClassificationResult {
        classify_nets(netlist)
    }
}
