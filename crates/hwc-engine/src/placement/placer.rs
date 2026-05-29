//! Main component placer implementation.

use crate::geometry::Point3D;
use crate::netlist::ComponentId;
use crate::space::VoxelSize;
use crate::voxel::MaterialId;
use crate::voxel_grid::VoxelGrid;

use super::collision::check_collision;
use super::component_definition::{
    load_component_definition, BakedComponent, ComponentDefinition, Footprint,
};
use super::error::PlacementError;
use super::geometry::{calculate_global_bounding_box, transform_pin_position};
use super::substrate::{
    place_cylinder_substrate, place_substrate, place_substrate_layers, place_substrate_with_cutouts,
};
use super::types::{DiagnosticReporter, PlacementParams, SymbolTableTrait};
use crate::geometry::BoundingBox;

/// Parameters for substrate placement with cutouts
pub struct SubstratePlacementParams<'a> {
    pub grid: &'a mut VoxelGrid,
    pub voxel_size: &'a VoxelSize,
    pub material_id: MaterialId,
    pub start: Point3D,
    pub end: Point3D,
    pub net_id: u32,
    pub cutouts: &'a [BoundingBox],
}

/// Component placement engine.
///
/// Handles transforming component definitions into physical voxel space
/// with rotation, collision detection, and pin position calculation.
pub struct ComponentPlacer {
    // Future: Component library/database
}

impl ComponentPlacer {
    /// Create a new component placer.
    pub fn new() -> Self {
        Self {}
    }

    /// Place a substrate layer in the voxel grid.
    ///
    /// # Arguments
    /// * `grid` - Voxel grid to place substrate in
    /// * `voxel_size` - Size of each voxel in nanometers
    /// * `material_id` - Substrate material ID from MaterialRegistry
    /// * `start` - Starting position in nanometers
    /// * `end` - Ending position in nanometers
    ///
    /// # Returns
    /// Ok if successful, error if invalid region
    pub fn place_substrate(
        &self,
        grid: &mut VoxelGrid,
        voxel_size: &VoxelSize,
        material_id: MaterialId,
        start: Point3D,
        end: Point3D,
        net_id: u32,
    ) -> Result<(), PlacementError> {
        place_substrate(grid, voxel_size, material_id, start, end, net_id)
    }

    /// Place a cylindrical substrate layer (v0.1.6).
    pub fn place_cylinder_substrate(
        &self,
        grid: &mut VoxelGrid,
        material_id: MaterialId,
        start: Point3D,
        end: Point3D,
        net_id: u32,
        diameter: i64,
    ) -> Result<(), PlacementError> {
        place_cylinder_substrate(grid, material_id, start, end, net_id, diameter)
    }

    /// Place a substrate layer with cutouts (mounting holes, edge cuts, etc.).
    ///
    /// # Arguments
    /// * `params` - Substrate placement parameters including grid, voxel_size, material_id,
    ///   start, end, net_id, and cutouts
    ///
    /// # Returns
    /// Ok if successful, error if invalid region
    pub fn place_substrate_with_cutouts(
        &self,
        params: SubstratePlacementParams,
    ) -> Result<(), PlacementError> {
        place_substrate_with_cutouts(
            params.grid,
            params.voxel_size,
            params.material_id,
            params.start,
            params.end,
            params.net_id,
            params.cutouts,
        )
    }

    /// Place multiple substrate layers.
    ///
    /// Useful for multi-layer PCBs or silicon chips with multiple dielectric layers.
    pub fn place_substrate_layers(
        &self,
        grid: &mut VoxelGrid,
        voxel_size: &VoxelSize,
        layers: &[(MaterialId, Point3D, Point3D)],
    ) -> Result<(), PlacementError> {
        place_substrate_layers(grid, voxel_size, layers)
    }

    /// Place a component in the voxel grid.
    ///
    /// # Arguments
    /// * `params` - Placement parameters (grid, voxel_size, arena, symbol_table, name, type, position, rotation)
    ///
    /// # Returns
    /// Component ID if successful, error if collision detected
    ///
    /// # Performance (v0.1.6 Semantic Baking)
    /// This function now uses pre-baked component definitions (pure integers) when available,
    /// eliminating repeated string parsing in placement loops.
    pub fn place_component<S: SymbolTableTrait, R: DiagnosticReporter>(
        &self,
        params: PlacementParams<S, R>,
    ) -> Result<ComponentId, PlacementError> {
        let PlacementParams {
            grid,
            voxel_size,
            arena,
            symbol_table,
            material_registry,
            name,
            component_type,
            position,
            rotation_deg,
            merge_waiver,
            collector,
        } = params;

        // Phase 4.1: Load component definition from Symbol Table
        // SEMANTIC BAKING: Try to use cached baked component first (fast path)
        let definition = if let Some(baked) = symbol_table.get_baked_component(&component_type) {
            // Fast path: Use pre-baked integers (no parsing!)
            convert_baked_to_definition(baked)
        } else {
            // Slow path: Parse on-demand (fallback for unbaked components)
            load_component_definition(&component_type, symbol_table)?
        };

        // Phase 4.2: Transform local coordinates to global
        let global_bbox = calculate_global_bounding_box(&definition, position, rotation_deg);

        let (width, height, depth) = match definition.footprint {
            Footprint::Rectangle {
                width_nm,
                height_nm,
                depth_nm,
            } => (width_nm, height_nm, depth_nm),
        };

        // Phase 4.3: Check for collisions
        // v0.1.7: Unified Merge Waiver (Silicon Law)
        // - merge: true (All) -> Waives everything
        // - merge: [pins] (Specific) -> Waives only if collision is in a listed pin region
        if merge_waiver != hwc_parser::MergeWaiver::None && merge_waiver != hwc_parser::MergeWaiver::All {
            if let Some((voxel_x, voxel_y, voxel_z)) =
                check_collision(grid, voxel_size, &global_bbox)?
            {
                // SURGICAL WAIVER: Check if the collision point is inside a waived pin region
                let mut waived = false;
                if let hwc_parser::MergeWaiver::Specific(waived_pins) = &merge_waiver {
                    // Convert voxel back to nanometers for geometric check
                    let collision_pt = VoxelGrid::voxel_to_nm(voxel_x, voxel_y, voxel_z, voxel_size);
                    
                    // Check if collision point is inside any of the waived pin footprints
                    for pin_def in &definition.pins {
                        if waived_pins.contains(&pin_def.name) {
                            let global_pin_pos = transform_pin_position(
                                pin_def.local_offset,
                                position,
                                (width, height, depth),
                                rotation_deg,
                            );
                            
                            // Estimate pin bounding box (PadShape)
                            // Simple approximation: check if collision point is near pin center
                            // Future: use exact PadShape footprint
                            let dist_sq = (collision_pt.x - global_pin_pos.x).pow(2) 
                                        + (collision_pt.y - global_pin_pos.y).pow(2);
                            
                            if dist_sq < 500_000 * 500_000 { // 500nm radius (generous for transistor pins)
                                waived = true;
                                break;
                            }
                        }
                    }
                }

                if waived {
                    let collision_nm = VoxelGrid::voxel_to_nm(voxel_x, voxel_y, voxel_z, voxel_size);
                    let msg = format!(
                        "Component '{}' allowed to overlap at ({:.3}, {:.3}, {:.3})mm", 
                        name, 
                        collision_nm.x as f64 / 1_000_000.0,
                        collision_nm.y as f64 / 1_000_000.0,
                        collision_nm.z as f64 / 1_000_000.0
                    );
                    if let Some(reporter) = collector {
                        reporter.report_waiver(&msg);
                    } else {
                        println!("⚠️ Waiver applied: {}", msg);
                    }
                } else {
                    // Convert voxel coordinates back to physical coordinates for error message
                    let collision_nm = VoxelGrid::voxel_to_nm(voxel_x, voxel_y, voxel_z, voxel_size);
                    let collision_mm = Point3D::new(
                        collision_nm.x / 1_000_000,
                        collision_nm.y / 1_000_000,
                        collision_nm.z / 1_000_000,
                    );

                    return Err(PlacementError::Collision {
                        component: name,
                        position: collision_mm,
                    });
                }
            }
        } else if merge_waiver == hwc_parser::MergeWaiver::None {
            // Default behavior: Check all collisions
            if let Some((voxel_x, voxel_y, voxel_z)) =
                check_collision(grid, voxel_size, &global_bbox)?
            {
                let collision_nm = VoxelGrid::voxel_to_nm(voxel_x, voxel_y, voxel_z, voxel_size);
                let collision_mm = Point3D::new(
                    collision_nm.x / 1_000_000,
                    collision_nm.y / 1_000_000,
                    collision_nm.z / 1_000_000,
                );

                return Err(PlacementError::Collision {
                    component: name,
                    position: collision_mm,
                });
            }
        }
        // If merge_waiver == All, we skip collision entirely.

        // Phase 4.4: Add component to arena
        let component_id = arena.add_component(
            name.clone(),
            component_type.clone(),
            (position.x, position.y, position.z),
        );

        // Phase 4.5: Add pins to arena with transformed positions
        for pin_def in &definition.pins {
            let global_pin_pos = transform_pin_position(
                pin_def.local_offset,
                position,
                (width, height, depth),
                rotation_deg,
            );

            arena.add_pin(
                component_id,
                pin_def.name.clone(),
                (
                    global_pin_pos.x - position.x,
                    global_pin_pos.y - position.y,
                    global_pin_pos.z - position.z,
                ),
                Some(pin_def.pad_shape.clone()),
            );
        }

        // Phase 4.6: Register component as sparse metadata (GOD-TIER ARCHITECTURE)
        // Get or register the component's material dynamically
        let material_id = material_registry.get_or_register(&definition.material_name);

        // Instead of filling voxels (Density Bomb), register component as sparse metadata
        // The router/DRC will check this metadata when needed via get_material()
        register_component_metadata(
            grid,
            &definition,
            position,
            rotation_deg,
            material_id,
            &name,
        )?;

        Ok(component_id)
    }
}

impl Default for ComponentPlacer {
    fn default() -> Self {
        Self::new()
    }
}

/// Convert a BakedComponent (pre-parsed integers) back to ComponentDefinition.
///
/// SEMANTIC BAKING: This is the fast path - no string parsing, just struct conversion.
/// The BakedComponent was already parsed during registration, so this is pure O(1) work.
fn convert_baked_to_definition(baked: &BakedComponent) -> ComponentDefinition {
    ComponentDefinition {
        name: baked.name.clone(),
        footprint: Footprint::Rectangle {
            width_nm: baked.width_nm,
            height_nm: baked.height_nm,
            depth_nm: baked.depth_nm,
        },
        pins: baked.pins.clone(),
        material_name: baked.material_name.clone(),
    }
}

/// Register component as sparse metadata instead of filling voxels.
///
/// GOD-TIER SPARSE ARCHITECTURE:
/// - Placement is O(1): Just push metadata to a vector
/// - Memory is O(components), not O(voxels)
/// - Router sees components via get_material() lookup
/// - Same pattern as SubstrateLayer (proven to work)
///
/// For PCBs: This registers the component body (plastic/ceramic housing)
/// For Silicon: This registers the transistor structure (Polysilicon, N-doped, etc.)
fn register_component_metadata(
    grid: &mut VoxelGrid,
    definition: &ComponentDefinition,
    position: Point3D,
    rotation_deg: f64,
    material_id: crate::voxel::MaterialId,
    name: &str,
) -> Result<(), PlacementError> {
    let bbox = calculate_global_bounding_box(definition, position, rotation_deg);

    // Register component as sparse metadata (no voxel filling!)
    use smallvec::SmallVec;
    grid.add_component_metadata(
        bbox,
        material_id,
        name.into(),
        definition.name.clone(), // Use definition name as component_type
        SmallVec::new(),
    );

    Ok(())
}
