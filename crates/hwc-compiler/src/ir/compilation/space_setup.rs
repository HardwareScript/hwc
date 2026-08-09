use crate::ir::errors::IrError;
use crate::SymbolTable;

/// Create the hardware space and validate ASIC constraints.
pub fn create_space(
    space_def: &hwc_parser::SpaceDefinition,
    symbol_table: &SymbolTable,
    eval_context: &hwc_parser::EvaluationContext,
    unit_registry: &hwc_types::UnitRegistry,
    arena: &hwc_parser::ast::arena::AstArena,
) -> Result<hwc_engine::HardwareSpace, IrError> {
    let space = crate::ir::space_builder::create_hardware_space(
        space_def,
        symbol_table,
        eval_context,
        unit_registry,
    )?;
    crate::ir::space_builder::validate_asic_constraints(
        space_def,
        symbol_table,
        eval_context,
        arena,
    )?;
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
///
/// v0.2.0: NO FALLBACK. If the stackup fails to resolve, compilation fails
/// with a clear error. The old code created an empty StackupManager which
/// silently produced incorrect routing geometry.
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
    )?;

    if stackup_manager.layer_count() == 0 {
        return Err(IrError::MissingAsicConstraint {
            message: "No stackup layers defined in profile".into(),
            hint: "Add a 'stackup:' block to your profile with at least one layer definition.\n\nExample:\n  stackup:\n    active: material: Silicon, thickness: 400nm\n    metal1: material: Aluminum, thickness: 400nm".into(),
        });
    }

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
            if let Ok(thickness_nm) = crate::ir::conversions::evaluate_expression_to_nm(
                &layer.thickness,
                symbol_table,
                eval_context,
            ) {
                if let Some(mat_id) = space.material_registry.get_id(&layer.material) {
                    // Update thickness in material properties (preserving all other properties)
                    let mut props = space
                        .material_registry
                        .get_physical_props(mat_id)
                        .cloned()
                        .unwrap_or_else(hwc_engine::material::MaterialPhysicalProps::new);

                    props.set("thickness", thickness_nm as f64);

                    space.material_registry.set_physical_props(mat_id, props);
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
) -> Result<hwc_parser::EvaluationContext, IrError> {
    let mut eval_context =
        crate::constraint_solver::ConstraintSolver::build_eval_context(symbol_table);

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
    // v0.2.1: DIMENSIONAL TYPE SAFETY - Fail hard on unit mismatches
    for statement in &space_def.statements {
        if let hwc_parser::SpaceTopLevelStatement::Let(let_binding) = statement {
            // Evaluate the expression in the current context
            match let_binding.value.evaluate(&eval_context) {
                Ok(value) => {
                    eval_context.insert(let_binding.name.clone(), value);
                }
                Err(e) => {
                    // Convert evaluation error to dimensional unit mismatch if it's a unit conversion error
                    let error_msg = e.to_string();
                    if error_msg.contains("Cannot convert") || error_msg.contains("unit") {
                        return Err(IrError::DimensionalUnitMismatch {
                            expression: format!(
                                "let {} = {:?}",
                                let_binding.name, let_binding.value
                            ),
                            operation: "evaluate".to_string(),
                            detail: format!("Expression evaluation failed: {}", error_msg),
                        });
                    } else {
                        return Err(IrError::InvalidExpression(format!(
                            "Failed to evaluate let binding '{}': {}",
                            let_binding.name, e
                        )));
                    }
                }
            }
        }
    }

    // v0.2.1: Register immutable constant bindings from space block
    // Example: `const PI: 3.14159` becomes available in all subsequent expressions
    // Constants can shadow prelude constants in the same scope
    for statement in &space_def.statements {
        if let hwc_parser::SpaceTopLevelStatement::Const(const_binding) = statement {
            // Evaluate the expression in the current context
            match const_binding.value.evaluate(&eval_context) {
                Ok(value) => {
                    eval_context.insert(const_binding.name.clone(), value);
                }
                Err(e) => {
                    let error_msg = e.to_string();
                    if error_msg.contains("Cannot convert") || error_msg.contains("unit") {
                        return Err(IrError::DimensionalUnitMismatch {
                            expression: format!(
                                "const {} = {:?}",
                                const_binding.name, const_binding.value
                            ),
                            operation: "evaluate".to_string(),
                            detail: format!("Expression evaluation failed: {}", error_msg),
                        });
                    } else {
                        return Err(IrError::InvalidExpression(format!(
                            "Failed to evaluate const binding '{}': {}",
                            const_binding.name, e
                        )));
                    }
                }
            }
        }
    }

    Ok(eval_context)
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
