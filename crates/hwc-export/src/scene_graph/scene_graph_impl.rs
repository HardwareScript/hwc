//! Main SceneGraph implementation

use super::exporters::export_glb;
use super::materials::{add_materials_from_symbol_table, SceneGraphError};
use super::mesh_generation::{create_box_mesh, create_component_box};
use super::procedural::create_to220_meshes;
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
    pub fn add_box_mesh(&mut self, name: &str, params: BoxParams, material_name: &str, view: SpaceView) {
        self.meshes
            .push(create_box_mesh(name, params, material_name, view, FaceCulling::none()));
    }

    /// Add copper traces from analytic routes (v0.1.7)
    /// NOTE: In v0.1.7, traces are now realized into substrate layers for proper manifold
    /// merging and punch-through. This function is kept for backward compatibility but
    /// is now a no-op to prevent duplication.
    pub fn add_traces(&mut self, _space: &HardwareSpace) {
        // Traces are now handled by add_substrate() via voxel-realized layers.
    }

    /// Add components from HardwareSpace
    pub fn add_components_from_space(
        &mut self,
        space: &HardwareSpace,
        symbol_table: &SymbolTable,
    ) -> Result<(), SceneGraphError> {
        // Build a map of component names to their types from the netlist
        let mut name_to_type = FxHashMap::default();
        for comp_id in 0..space.netlist.component_count() {
            let comp_id = hwc_engine::ComponentId::new(comp_id as u32);
            if let Some(component) = space.netlist.get_component(comp_id) {
                name_to_type.insert(component.name.clone(), component.component_type.clone());
            }
        }

        for component_meta in space.voxel_grid.get_component_metadata() {
            let min_x_mm = component_meta.bbox.min.x as f64 / 1_000_000.0;
            let min_y_mm = component_meta.bbox.min.y as f64 / 1_000_000.0;
            let min_z_mm = component_meta.bbox.min.z as f64 / 1_000_000.0;
            let max_x_mm = component_meta.bbox.max.x as f64 / 1_000_000.0;
            let max_y_mm = component_meta.bbox.max.y as f64 / 1_000_000.0;
            let max_z_mm = component_meta.bbox.max.z as f64 / 1_000_000.0;

            let width_mm = max_x_mm - min_x_mm;
            let height_mm = max_y_mm - min_y_mm;
            let depth_mm = max_z_mm - min_z_mm;

            let center_x_mm = (min_x_mm + max_x_mm) / 2.0;
            let center_y_mm = (min_y_mm + max_y_mm) / 2.0;
            let center_z_mm = (min_z_mm + max_z_mm) / 2.0;

            let material_name = space
                .material_registry
                .get_name(component_meta.material)
                .unwrap_or("Component");

            // Limitation 6: Check for procedural component types
            let component_type = name_to_type.get(&component_meta.name);
            
            if let Some(comp_type) = component_type {
                if comp_type.as_str() == "TO220" {
                    let mut meshes = create_to220_meshes(
                        &component_meta.name,
                        (center_x_mm, center_y_mm, center_z_mm),
                        0.0, // Rotation is already baked into bbox for now
                        space.view,
                    );
                    self.meshes.append(&mut meshes);
                    continue;
                }
            }

            let mesh = create_component_box(
                &component_meta.name,
                (center_x_mm, center_y_mm, center_z_mm),
                (width_mm, height_mm, depth_mm),
                material_name,
                space.view,
            )?;
            self.meshes.push(mesh);
        }

        for comp_id in 0..space.netlist.component_count() {
            let comp_id = hwc_engine::ComponentId::new(comp_id as u32);
            if let Some(component) = space.netlist.get_component(comp_id) {
                if component.component_type.starts_with("Pour(") || component.component_type.starts_with("Contact(") {
                    continue;
                }

                let component_def = symbol_table
                    .get_component(&component.component_type)
                    .map_err(|_| SceneGraphError::ComponentNotFound {
                        component: component.component_type.to_string(),
                    })?;

                if let Some(render_block) = &component_def.render {
                    if let Some(_asset_path) = &render_block.asset {
                        let pos_x_mm = component.position_nm.0 as f64 / 1_000_000.0;
                        let pos_y_mm = component.position_nm.1 as f64 / 1_000_000.0;
                        let pos_z_mm = component.position_nm.2 as f64 / 1_000_000.0;

                        let material_name = component_def
                            .metadata
                            .as_ref()
                            .and_then(|m| m.value.clone())
                            .unwrap_or_else(|| "Component".into());

                        let mesh = create_component_box(
                            &component.name,
                            (pos_x_mm, pos_y_mm, pos_z_mm),
                            (5.0, 3.0, 2.0),
                            &material_name,
                            space.view,
                        )?;
                        self.meshes.push(mesh);
                    }
                }
            }
        }
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
