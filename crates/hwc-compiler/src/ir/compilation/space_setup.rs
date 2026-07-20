use crate::ir::errors::IrError;
use crate::SymbolTable;

/// Create the hardware space and validate ASIC constraints.
pub fn create_space(
    space_def: &hwc_parser::SpaceDefinition,
    symbol_table: &SymbolTable,
) -> Result<hwc_engine::HardwareSpace, IrError> {
    let space = crate::ir::space_builder::create_hardware_space(space_def, symbol_table)?;
    crate::ir::space_builder::validate_asic_constraints(space_def, symbol_table)?;
    Ok(space)
}

/// Resolve profile and extract solder mask thickness (library-driven, not hardcoded).
pub fn resolve_solder_mask_thickness(
    space_def: &hwc_parser::SpaceDefinition,
    symbol_table: &SymbolTable,
) -> Result<(Option<hwc_parser::ProfileDefinition>, i64), IrError> {
    let profile = space_def
        .profile
        .as_ref()
        .and_then(|p| symbol_table.get_profile(p.as_str()).ok())
        .cloned();

    let solder_mask_thickness_nm = profile
        .as_ref()
        .and_then(|p| p.manufacturing.as_ref())
        .and_then(|m| m.solder_mask_thickness.as_ref())
        .map(|t| crate::ir::conversions::measurement_to_nm(t, symbol_table))
        .transpose()
        .map_err(|e| IrError::InvalidRouteExpression {
            expression: "solder_mask_thickness".into(),
            reason: e.to_string(),
        })?
        .ok_or_else(|| IrError::MissingAsicConstraint {
            message: "PDK missing required 'manufacturing.solder_mask_thickness' constraint."
                .into(),
            hint: "Add 'manufacturing: { solder_mask_thickness: <value> }' to your profile.".into(),
        })?;

    Ok((profile, solder_mask_thickness_nm))
}

/// Create the stackup manager.
pub fn create_stackup_and_materials(
    profile: Option<&hwc_parser::ProfileDefinition>,
    symbol_table: &SymbolTable,
    resolution_nm: i64,
    origin_z: hwc_parser::OriginZ,
    solder_mask_thickness_nm: i64,
) -> Result<crate::ir::stackup_manager::StackupManager, IrError> {
    let stackup_manager = crate::ir::stackup_manager::StackupManager::new(
        profile.and_then(|prof| prof.stackup.as_ref()),
        symbol_table,
        resolution_nm,
        origin_z,
        solder_mask_thickness_nm,
    )
    .unwrap_or_else(|_| {
        crate::ir::stackup_manager::StackupManager::new(
            None,
            symbol_table,
            resolution_nm,
            origin_z,
            solder_mask_thickness_nm,
        )
        .expect("Failed to create fallback StackupManager")
    });

    Ok(stackup_manager)
}

/// Write stackup layer thicknesses into MaterialRegistry.
pub fn populate_material_registry(
    space: &mut hwc_engine::HardwareSpace,
    profile: Option<&hwc_parser::ProfileDefinition>,
    symbol_table: &SymbolTable,
) {
    if let Some(stackup) = profile.and_then(|p| p.stackup.as_ref()) {
        for layer in &stackup.layers {
            if let Ok(thickness_nm) =
                crate::ir::conversions::evaluate_expression_to_nm(&layer.thickness, symbol_table)
            {
                if let Some(mat_id) = space.material_registry.get_id(&layer.material) {
                    let existing = space.material_registry.get_physical_props(mat_id);
                    space.material_registry.set_physical_props(
                        mat_id,
                        existing.map(|p| p.resistivity_ohm_m).unwrap_or(0.0),
                        existing.map(|p| p.thermal_conductivity_w_mk).unwrap_or(0.0),
                        thickness_nm,
                        existing.and_then(|p| p.max_current_density_a_mm2),
                    );
                }
            }
        }
    }
}

/// Create the universal evaluation context.
pub fn build_eval_context(symbol_table: &SymbolTable) -> hwc_parser::EvaluationContext {
    crate::constraint_solver::ConstraintSolver::build_eval_context(symbol_table)
}

/// Generate solder mask layers if the profile specifies them.
pub fn generate_solder_mask(
    space: &mut hwc_engine::HardwareSpace,
    solder_mask_thickness_nm: i64,
    stackup_manager: &crate::ir::stackup_manager::StackupManager,
) -> Result<(), IrError> {
    if solder_mask_thickness_nm == 0 {
        return Ok(());
    }

    let width_nm = space.dimensions.width_nm;
    let height_nm = space.dimensions.height_nm;
    let stackup_height_nm = stackup_manager.board_thickness_nm();

    let has_solder_mask = space.entity_graph.get_substrate_layers().iter().any(|l| {
        l.layer_type == hwc_engine::geometry_router::substrate_types::SubstrateLayerType::SolderMask
    });

    if has_solder_mask {
        return Ok(());
    }

    let mask_material_id = space
        .material_registry
        .get_id("SolderMask")
        .ok_or_else(|| IrError::UndeclaredMaterial {
            material: "SolderMask".into(),
        })?;

    let top_mask_bbox = hwc_engine::geometry::BoundingBox::new(
        hwc_engine::geometry::Point3D::new(0, 0, stackup_height_nm),
        hwc_engine::geometry::Point3D::new(
            width_nm,
            height_nm,
            stackup_height_nm + solder_mask_thickness_nm,
        ),
    );
    space.entity_graph.add_substrate_layer(
        mask_material_id,
        0,
        top_mask_bbox,
        hwc_engine::geometry_router::substrate_types::SubstrateLayerType::SolderMask,
    );

    let bottom_mask_bbox = hwc_engine::geometry::BoundingBox::new(
        hwc_engine::geometry::Point3D::new(0, 0, -solder_mask_thickness_nm),
        hwc_engine::geometry::Point3D::new(width_nm, height_nm, 0),
    );
    space.entity_graph.add_substrate_layer(
        mask_material_id,
        0,
        bottom_mask_bbox,
        hwc_engine::geometry_router::substrate_types::SubstrateLayerType::SolderMask,
    );

    Ok(())
}
