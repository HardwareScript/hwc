//! Core types for placement module.

use compact_str::CompactString;

use crate::geometry::Point3D;
use crate::netlist::NetlistArena;
use crate::space::VoxelSize;
use crate::voxel::MaterialRegistry;
use crate::voxel_grid::VoxelGrid;

/// Trait for accessing component and material definitions.
///
/// This trait uses Dependency Inversion to avoid circular dependencies:
/// - hwc-engine defines what it needs (this trait)
/// - hwc-compiler implements it (SymbolTable)
/// - No direct dependency from engine to compiler
pub trait SymbolTableTrait {
    /// Get a component definition by name (Phase 2.1)
    fn get_component(&self, name: &str) -> Result<&hwc_parser::ComponentDefinition, String>;

    /// Get a material definition by name (Phase 2.2)
    fn get_material(&self, name: &str) -> Result<&hwc_parser::MaterialDefinition, String>;

    /// Resolve a unit symbol to its definition (for custom units)
    ///
    /// This enables proper unit conversion for user-defined and stdlib units.
    /// Returns None if the unit symbol is not found in the symbol table.
    fn resolve_unit_symbol(&self, symbol: &str) -> Option<&hwc_parser::UnitDefinition>;

    /// **CANONICAL UNIT CONVERSION METHOD**
    ///
    /// Convert a measurement to nanometers. This is the SINGLE SOURCE OF TRUTH
    /// for all unit conversions. Every part of the compiler/engine that needs to
    /// convert measurements MUST use this method.
    ///
    /// # Why This Exists
    /// - Eliminates hardcoded unit conversion logic scattered across the codebase
    /// - Supports custom user-defined units automatically
    /// - Makes the system truly extensible - add a unit in stdlib, it works everywhere
    ///
    /// # Returns
    /// Value in nanometers, or error if the unit cannot be resolved or is not a length unit
    fn measurement_to_nm(&self, measurement: &hwc_parser::Measurement) -> Result<i64, String>;

    /// Get a pre-baked component definition (v0.1.6 Semantic Baking).
    ///
    /// PERFORMANCE: Returns a cached BakedComponent with pre-parsed dimensions as integers.
    /// This eliminates repeated string parsing in placement loops.
    ///
    /// Returns None if the component hasn't been baked yet.
    fn get_baked_component(
        &self,
        name: &str,
    ) -> Option<&super::component_definition::BakedComponent>;
}

/// Interface for reporting diagnostics from the engine to the compiler's diagnostic system.
///
/// This avoids a direct dependency on hwc-diagnostics (leaf crate separation).
pub trait DiagnosticReporter {
    fn report_waiver(&self, message: &str);
}

/// Component placement parameters.
pub struct PlacementParams<'a, S, R> {
    pub grid: &'a mut VoxelGrid,
    pub voxel_size: &'a VoxelSize,
    pub arena: &'a mut NetlistArena,
    pub symbol_table: &'a S,
    pub material_registry: &'a mut MaterialRegistry,
    pub name: CompactString,
    pub component_type: CompactString,
    pub position: Point3D,
    pub rotation_deg: f64,
    /// v0.1.7: Unified merge waiver for intentional overlap
    /// Supports global (true) or granular ([pins]) waivers.
    pub merge_waiver: hwc_parser::MergeWaiver,
    /// Optional reporter for waivers and warnings
    pub collector: Option<&'a R>,
}
