use crate::ir::errors::IrError;
use crate::SymbolTable;

/// Create the hardware space and validate ASIC constraints.
pub fn create_space(
    space_def: &hwc_parser::SpaceDefinition,
    symbol_table: &SymbolTable,
    eval_context: &hwc_parser::EvaluationContext,
) -> Result<hwc_engine::HardwareSpace, IrError> {
    let space = crate::ir::space_builder::create_hardware_space(space_def, symbol_table, eval_context)?;
    crate::ir::space_builder::validate_asic_constraints(space_def, symbol_table, eval_context)?;
    Ok(space)
}

/// Resolve profile and extract solder mask thickness (library-driven, not hardcoded).
pub fn resolve_solder_mask_thickness(
    space_def: &hwc_parser::SpaceDefinition,
    symbol_table: &SymbolTable,
    eval_context: &hwc_parser::EvaluationContext,
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
        .map(|t| crate::ir::conversions::measurement_to_nm(t, symbol_table, eval_context))
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
    eval_context: &hwc_parser::EvaluationContext,
    resolution_nm: i64,
    origin_z: hwc_parser::OriginZ,
    solder_mask_thickness_nm: i64,
) -> Result<crate::ir::stackup_manager::StackupManager, IrError> {
    let stackup_manager = crate::ir::stackup_manager::StackupManager::new(
        profile.and_then(|prof| prof.stackup.as_ref()),
        symbol_table,
        eval_context,
        resolution_nm,
        origin_z,
        solder_mask_thickness_nm,
    )
    .unwrap_or_else(|_| {
        crate::ir::stackup_manager::StackupManager::new(
            None,
            symbol_table,
            eval_context,
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
    eval_context: &hwc_parser::EvaluationContext,
) {
    if let Some(stackup) = profile.and_then(|p| p.stackup.as_ref()) {
        for layer in &stackup.layers {
            if let Ok(thickness_nm) =
                crate::ir::conversions::evaluate_expression_to_nm(&layer.thickness, symbol_table, eval_context)
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
///
/// ## CLEAN ARCHITECTURE: Strongly-Typed Variable Storage
///
/// This function populates the `EvaluationContext` with strongly-typed `Value` enums,
/// preserving unit information throughout the compilation pipeline:
///
/// 1. **Dimensionless constants** (PI, e, user-defined scalars) → `Value::Number`
/// 2. **PDK physical properties** (edge_clearance, min_width) → `Value::Measurement` with units
/// 3. **Local let bindings** (v0.2.0) → Evaluated and stored as appropriate Value type
///
/// By storing PDK properties as `Value::Measurement` instead of bare `i64` nanometers,
/// we maintain dimensional correctness and enable clean physics-aware expression evaluation.
pub fn build_eval_context(
    symbol_table: &SymbolTable,
    profile: Option<&hwc_parser::ProfileDefinition>,
    space_def: &hwc_parser::SpaceDefinition,
) -> hwc_parser::EvaluationContext {
    let mut eval_context = crate::constraint_solver::ConstraintSolver::build_eval_context(symbol_table);
    
    // Register PDK profile variables as "pdk.*" for use in expressions
    // Store as Value::Measurement to preserve unit information
    if let Some(prof) = profile {
        if let Some(trace) = &prof.trace {
            // Register trace constraints as Measurements (units preserved!)
            if let Some(edge_clearance) = &trace.edge_clearance {
                eval_context.insert(
                    "pdk.edge_clearance".into(),
                    hwc_parser::Value::Measurement {
                        value: edge_clearance.value,
                        unit: edge_clearance.unit.clone(),
                    },
                );
            }
            eval_context.insert(
                "pdk.min_width".into(),
                hwc_parser::Value::Measurement {
                    value: trace.min_width.value,
                    unit: trace.min_width.unit.clone(),
                },
            );
            eval_context.insert(
                "pdk.min_spacing".into(),
                hwc_parser::Value::Measurement {
                    value: trace.min_spacing.value,
                    unit: trace.min_spacing.unit.clone(),
                },
            );
        }
    }
    
    // v0.2.0: Register local let bindings from space block
    // Example: `let edge_pad_w = 150um` becomes available in all subsequent expressions
    for statement in &space_def.statements {
        if let hwc_parser::SpaceTopLevelStatement::Let(let_binding) = statement {
            // Evaluate the expression in the current context
            match let_binding.value.evaluate(&eval_context) {
                Ok(value) => {
                    eval_context.insert(let_binding.name.clone(), value);
                }
                Err(e) => {
                    eprintln!(
                        "[WARN] Failed to evaluate let binding '{}': {}",
                        let_binding.name, e
                    );
                    // Continue compilation - will fail later if this variable is used
                }
            }
        }
    }
    
    eval_context
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
        hwc_engine::NetId::UNCONNECTED,
        top_mask_bbox,
        hwc_engine::geometry_router::substrate_types::SubstrateLayerType::SolderMask,
    );

    let bottom_mask_bbox = hwc_engine::geometry::BoundingBox::new(
        hwc_engine::geometry::Point3D::new(0, 0, -solder_mask_thickness_nm),
        hwc_engine::geometry::Point3D::new(width_nm, height_nm, 0),
    );
    space.entity_graph.add_substrate_layer(
        mask_material_id,
        hwc_engine::NetId::UNCONNECTED,
        bottom_mask_bbox,
        hwc_engine::geometry_router::substrate_types::SubstrateLayerType::SolderMask,
    );

    Ok(())
}
