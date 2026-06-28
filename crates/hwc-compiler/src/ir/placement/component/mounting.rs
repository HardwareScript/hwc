use super::super::super::conversions::evaluate_expression_to_nm;
use super::super::super::errors::IrError;
use super::super::super::stackup_manager::StackupManager;
use super::super::helpers::parse_rectangle_dimensions;
use crate::SymbolTable;
use hwc_engine::{geometry::Point3D, HardwareSpace};

pub struct MountingResult {
    pub position: Point3D,
    pub body_min_z: i64,
    pub body_max_z: i64,
    pub mount_side: hwc_parser::MountingSide,
    pub _standoff_nm: i64,
    pub _component_height_nm: i64,
}

pub fn resolve_mounting_and_elevation(
    space: &HardwareSpace,
    component: &hwc_parser::ComponentPlacement,
    symbol_table: &SymbolTable,
    stackup_manager: &StackupManager,
    mut position: Point3D,
    origin: hwc_parser::OriginPoint,
) -> Result<MountingResult, IrError> {
    // v0.1.7: Component Mounting Abstraction
    let mount_side = component.mount.unwrap_or(hwc_parser::MountingSide::Top);

    // Default position.z to the board surface if no elevation is specified
    if component.elevation.is_none() {
        position.z = stackup_manager.board_surface_z(mount_side);
    }

    // v0.1.7: Resolve elevation from 'on layer:' or 'on z:' prepositional syntax
    if let Some(elevation) = &component.elevation {
        let z_user_nm = stackup_manager.resolve_elevation(elevation, symbol_table)?;
        let final_z = crate::ir::conversions::apply_z_origin_physical(
            z_user_nm,
            origin.z,
            space.dimensions.depth_nm,
        );
        position.z = final_z;
    }

    // v0.1.7: Component Mounting Abstraction - Calculate body bounds
    let component_height_nm =
        if let Ok(component_def) = symbol_table.get_component(component.component_type.as_str()) {
            component_def
                .layout
                .as_ref()
                .and_then(|l| l.shape.as_ref())
                .and_then(|s| parse_rectangle_dimensions(s, symbol_table))
                .map(|(_, _, d)| d)
                .ok_or_else(|| IrError::MissingAsicConstraint {
                    message: format!(
                        "Component '{}' has no explicit height in its layout definition.",
                        component.component_type
                    ),
                    hint: "Add 'shape: Rectangle(w, h, d)' to the component's layout definition.".into(),
                })?
        } else {
            return Err(IrError::MissingAsicConstraint {
                message: format!(
                    "Component type '{}' is not declared in the symbol table.",
                    component.component_type
                ),
                hint: "Declare the component type with a layout definition before using it.".into(),
            });
        };

    // v0.1.7: Resolve standoff height (default to 0 if omitted)
    let standoff_nm = match &component.standoff {
        Some(expr) => {
            evaluate_expression_to_nm(expr, symbol_table).map_err(|e| IrError::CoordinateResolutionFailed {
                coordinate_str: "standoff height".into(),
                reason: e.to_string(),
            })?
        }
        None => {
            // Fallback to component definition's standoff
            if let Ok(comp_def) = symbol_table.get_component(component.component_type.as_str()) {
                comp_def
                    .layout
                    .as_ref()
                    .and_then(|l| l.standoff.as_ref())
                    .map(|expr| evaluate_expression_to_nm(expr, symbol_table))
                    .transpose()
                    .map_err(|e| IrError::CoordinateResolutionFailed {
                        coordinate_str: "component definition standoff".into(),
                        reason: e.to_string(),
                    })?
                    .unwrap_or(0) // Default to 0 (no secret compiler offsets!)
            } else {
                0
            }
        }
    };

    let (body_min_z, body_max_z) = match mount_side {
        hwc_parser::MountingSide::Top => (
            position.z + standoff_nm,
            position.z + component_height_nm + standoff_nm,
        ),
        hwc_parser::MountingSide::Bottom => (
            position.z - component_height_nm - standoff_nm,
            position.z - standoff_nm,
        ),
        hwc_parser::MountingSide::Embedded => (
            position.z - component_height_nm / 2,
            position.z + component_height_nm / 2,
        ),
    };

    Ok(MountingResult {
        position,
        body_min_z,
        body_max_z,
        mount_side,
        _standoff_nm: standoff_nm,
        _component_height_nm: component_height_nm,
    })
}

pub fn handle_snap_to_surface(space: &HardwareSpace, position: &mut Point3D) {
    let mut highest_z_nm = 0;

    // Check substrate layers for highest surface
    for layer in &space.entity_graph.substrate_layers {
        if position.x >= layer.bbox.min.x
            && position.x <= layer.bbox.max.x
            && position.y >= layer.bbox.min.y
            && position.y <= layer.bbox.max.y
        {
            highest_z_nm = highest_z_nm.max(layer.bbox.max.z);
        }
    }

    // Check substrate bounding box (global fallback)
    if let Some(bbox) = &space.substrate_bbox {
        if position.x >= bbox.min.x
            && position.x <= bbox.max.x
            && position.y >= bbox.min.y
            && position.y <= bbox.max.y
        {
            highest_z_nm = highest_z_nm.max(bbox.max.z);
        }
    }

    position.z = highest_z_nm;
}
