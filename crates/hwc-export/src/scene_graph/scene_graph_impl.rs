//! Main SceneGraph implementation

use super::exporters::export_glb;
use super::materials::{add_materials_from_symbol_table, SceneGraphError};
use super::mesh_generation::create_box_mesh;
//
//
use super::substrate::add_substrate;
use super::types::{BoxParams, FaceCulling, MaterialNode, MeshNode};
use compact_str::CompactString;
use hwc_compiler::SymbolTable;
use hwc_engine::{HardwareSpace, SpaceView};
use hwc_parser::ProfileDefinition;
use rustc_hash::FxHashMap;

/// Scene graph for 3D visualization
#[derive(Debug)]
pub struct SceneGraph {
    pub materials: FxHashMap<CompactString, MaterialNode>,
    pub meshes: Vec<MeshNode>,
}

impl SceneGraph {
    /// Create a new empty scene graph
    pub fn new() -> Self {
        Self {
            materials: FxHashMap::default(),
            meshes: Vec::new(),
        }
    }

    /// Add materials from Symbol Table
    pub fn add_materials(&mut self, symbol_table: &SymbolTable) -> Result<(), SceneGraphError> {
        add_materials_from_symbol_table(&mut self.materials, symbol_table)
    }

    /// Add substrate mesh (FR4 base) from actual substrate layers
    pub fn add_substrate(&mut self, space: &HardwareSpace, profile: Option<&ProfileDefinition>) {
        add_substrate(&mut self.meshes, space, &self.materials, profile);
    }

    /// Helper: Add a box mesh to the scene
    pub fn add_box_mesh(
        &mut self,
        name: &str,
        params: BoxParams,
        material_name: &str,
        view: SpaceView,
    ) {
        self.meshes.push(create_box_mesh(
            name,
            params,
            material_name,
            view,
            FaceCulling::none(),
        ));
    }

    /// Add copper traces from analytic routes (v0.1.7)
    /// NOTE: In v0.1.7, traces are now realized into substrate layers for proper manifold
    /// merging and punch-through. This function is kept for backward compatibility but
    /// is now a no-op to prevent duplication.
    pub fn add_traces(&mut self, _space: &HardwareSpace) {
        // Traces are now handled by add_substrate() via realized layers.
    }

    /// Add components from HardwareSpace
    pub fn add_components_from_space(
        &mut self,
        space: &HardwareSpace,
        symbol_table: &SymbolTable,
    ) -> Result<(), SceneGraphError> {
        // v0.1.7 Strategy A: Component body rendering is disabled to focus on
        // copper pours, traces, and pads which are rendered via the copper pool
        // union system. Component pins are represented by copper pour pads
        // defined in the hardware script or via internal component pours.
        //
        // Only render components that have explicit render blocks with assets.
        let _ = space;
        let _ = symbol_table;
        Ok(())
    }

    /// Export scene graph to GLB format (glTF binary) with proper per-material meshes
    pub fn export_glb(&self) -> Vec<u8> {
        export_glb(&self.materials, &self.meshes)
    }
}

impl Default for SceneGraph {
    fn default() -> Self {
        Self::new()
    }
}
