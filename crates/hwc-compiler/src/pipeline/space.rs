//! Single-space build orchestration for the v0.3.0 pipeline.
//!
//! Given a `space` declaration, this constructs the [`HardwareSpace`]: it
//! resolves the stackup layers from the profile, lowers fabrication constraints
//! (via [`profile`]), injects dielectric substrate slabs, registers nets, and
//! then dispatches to the primitive population modules ([`pours`], [`contacts`],
//! [`devices`], [`routes`]).

use compact_str::CompactString;
use hwc_engine::HardwareSpace;
use hwc_parser::ast::Expression;
use hwc_parser::SpaceDecl;
use hwc_types::NetId;
use rustc_hash::FxHashMap;

use crate::eval::MemoryEmitter;
use crate::pipeline::contacts;
use crate::pipeline::devices;
use crate::pipeline::error::PipelineError;
use crate::pipeline::profile;
use crate::pipeline::pours;
use crate::pipeline::routes;
use crate::symbol_table::SymbolTable;

/// Build a single [`HardwareSpace`] from its declaration and the comptime emitter output.
pub fn build_space(
    space_decl: &SpaceDecl,
    width_nm: i64,
    height_nm: i64,
    symbol_table: &SymbolTable,
    base_material_registry: &hwc_engine::MaterialRegistry,
    mem: &MemoryEmitter,
    net_id_to_name: &FxHashMap<NetId, CompactString>,
) -> Result<HardwareSpace, PipelineError> {
    let space_name = space_decl.name.name.clone();

    let params = hwc_engine::space::HardwareSpaceParams {
        name: space_name.clone(),
        dimensions: hwc_engine::Dimensions {
            width_nm,
            height_nm,
            depth_nm: 10_000,
        },
        substrate_material_id: 0,
        material_registry: base_material_registry.clone(),
        view: hwc_engine::space::SpaceView::Horizontal,
        manufacturing_grid_nm: 10,
        technology_strategy: hwc_types::Technology::Asic,
    };

    let mut hw_space = hwc_engine::HardwareSpace::new(params);

    // 2. Resolve stackup layers from profile
    if let Some(prof_ident) = &space_decl.profile {
        if let Ok(prof_decl) = symbol_table.get_profile(prof_ident.as_str()) {
            let mut current_z = 0i64;
            for sec in &prof_decl.sections {
                if sec.section_type == "stackup" {
                    for (layer_name, expr) in &sec.fields {
                        // Mandate 4: No silent defaults. Every field must be explicitly declared.
                        let mut mat_name: Option<CompactString> = None;
                        let mut thickness_nm: Option<i64> = None;
                        let mut routable = true;
                        let mut is_device_layer = false;

                        if let Expression::StructInstance { fields, .. } = expr {
                            for fi in fields {
                                let fexpr = match &fi.value {
                                    Some(e) => e,
                                    None => continue,
                                };
                                match fi.name.as_str() {
                                    "material" => {
                                        if let Expression::StringLiteral { value, .. } = fexpr {
                                            mat_name = Some(value.as_str().into());
                                        } else if let Expression::Variable { name, .. } = fexpr {
                                            mat_name = Some(name.clone());
                                        } else {
                                            return Err(PipelineError {
                                                message: format!(
                                                    "FATAL: Stackup layer '{}' field 'material' must be a string or identifier, found '{:?}'",
                                                    layer_name, fexpr
                                                ),
                                            });
                                        }
                                    }
                                    "thickness" => {
                                        if let Expression::Measurement { value, unit, .. } = fexpr {
                                            match unit.to_nanometers(*value) {
                                                Ok(nm) => thickness_nm = Some(nm as i64),
                                                Err(_) => {
                                                    return Err(PipelineError {
                                                        message: format!(
                                                            "FATAL: Stackup layer '{}' field 'thickness' unit could not be converted to nanometers",
                                                            layer_name
                                                        ),
                                                    });
                                                }
                                            }
                                        } else {
                                            return Err(PipelineError {
                                                message: format!(
                                                    "FATAL: Stackup layer '{}' field 'thickness' must be a measurement (e.g. 200nm, 0nm), found '{:?}'",
                                                    layer_name, fexpr
                                                ),
                                            });
                                        }
                                    }
                                    "routable" => {
                                        routable = match fexpr {
                                            Expression::BooleanLiteral { value, .. } => *value,
                                            Expression::Literal { value, .. } => *value != 0,
                                            Expression::Variable { name, .. } if name == "true" => true,
                                            Expression::Variable { name, .. } if name == "false" => false,
                                            other => {
                                                return Err(PipelineError {
                                                    message: format!(
                                                        "FATAL: Stackup layer '{}' field 'routable' must be a boolean (true/false), found '{:?}'",
                                                        layer_name, other
                                                    ),
                                                });
                                            }
                                        };
                                    }
                                    "device_layer" | "is_device_layer" => {
                                        is_device_layer = match fexpr {
                                            Expression::BooleanLiteral { value, .. } => *value,
                                            Expression::Literal { value, .. } => *value != 0,
                                            Expression::Variable { name, .. } if name == "true" => true,
                                            Expression::Variable { name, .. } if name == "false" => false,
                                            other => {
                                                return Err(PipelineError {
                                                    message: format!(
                                                        "FATAL: Stackup layer '{}' field '{}' must be a boolean (true/false), found '{:?}'",
                                                        layer_name, fi.name, other
                                                    ),
                                                });
                                            }
                                        };
                                    }
                                    other => {
                                        return Err(PipelineError {
                                            message: format!(
                                                "FATAL: Stackup layer '{}' contains unknown field '{}'. Valid fields: material, thickness, routable, device_layer",
                                                layer_name, other
                                            ),
                                        });
                                    }
                                }
                            }
                        }

                        // Mandate 4: Fail fast on missing required fields — no silent defaults.
                        let mat_name = mat_name.ok_or_else(|| PipelineError {
                            message: format!(
                                "FATAL: Stackup layer '{}' is missing required field 'material'. \
                                 Every layer must declare its material explicitly (e.g. material: \"Copper\").",
                                layer_name
                            ),
                        })?;
                        let thickness_nm = thickness_nm.ok_or_else(|| PipelineError {
                            message: format!(
                                "FATAL: Stackup layer '{}' is missing required field 'thickness'. \
                                 Every layer must declare its thickness explicitly (e.g. thickness: 200nm). \
                                 Use thickness: 0nm for zero-thickness mask layers.",
                                layer_name
                            ),
                        })?;

                        let is_mask = thickness_nm == 0;
                        // Mandate 4: Material must be registered — no silent fallback to Conductor.
                        let category = hw_space
                            .material_registry
                            .get_category_by_name(&mat_name)
                            .ok_or_else(|| PipelineError {
                                message: format!(
                                    "FATAL: Stackup layer '{}' declares material '{}' which is not registered \
                                     in the material registry. Check your profile's 'materials' section.",
                                    layer_name, mat_name
                                ),
                            })?;
                        let kind = hwc_engine::stackup::LayerKind::from_material_category(category);
                        let z_bottom = current_z;
                        let z_top = current_z + thickness_nm;
                        current_z = z_top;

                        hw_space.stackup_layers.push(
                            hwc_engine::space::StackupLayer::new(
                                layer_name.clone(),
                                z_bottom,
                                z_top,
                                thickness_nm,
                                mat_name,
                                routable,
                                is_mask,
                                kind,
                            )
                            .with_device_layer(is_device_layer),
                        );
                    }
                }
            }
        }
    }

    // 3. Resolve fabrication constraints from the profile
    hw_space.fabrication_constraints = profile::build_fabrication_constraints(space_decl, symbol_table);

    // Require explicit profile stackup (no fallbacks)
    if hw_space.stackup_layers.is_empty() {
        return Err(PipelineError {
            message: format!(
                "Space '{}' requires a valid profile with a 'stackup' section",
                space_name
            ),
        });
    }

    // Update space dimensions depth with true total stackup thickness
    let total_depth_nm = hw_space
        .stackup_layers
        .iter()
        .map(|l| l.z_top)
        .max()
        .unwrap_or(0);
    hw_space.dimensions.depth_nm = total_depth_nm;

    // Inject dielectric substrate slabs (die boundary) into EntityGraph for 2D/3D CAD & DXF
    for st in &hw_space.stackup_layers {
        if !st.is_mask {
            let mat_id = hw_space.material_registry.get_id(&st.material_name).unwrap_or(0);
            if hw_space.material_registry.is_insulator(mat_id) {
                let die_bbox = hwc_engine::BoundingBox::new(
                    hwc_engine::Point3D::new(0, 0, st.z_bottom),
                    hwc_engine::Point3D::new(width_nm, height_nm, st.z_top),
                );
                let substrate_slab = hwc_engine::geometry_router::substrate_types::SubstrateLayer::new(
                    mat_id,
                    hwc_engine::netlist::NetId::UNCONNECTED,
                    die_bbox,
                    hwc_physics::connectivity::SubstrateLayerType::Substrate,
                );
                hw_space.entity_graph.substrate_layers.push(substrate_slab);
            }
        }
    }

    // Register nets in hw_space.netlist
    let default_route_mat_id = hw_space
        .stackup_layers
        .iter()
        .find(|l| l.is_routable)
        .and_then(|l| hw_space.material_registry.get_id(&l.material_name))
        .unwrap_or(0);

    for net_decl in &space_decl.nets {
        let net_name: CompactString = net_decl.name.as_str().into();
        let mut net_mat_id = default_route_mat_id;
        let mut net_width_nm = 300i64;

        if let Some(mat_expr) = net_decl.get_property("material") {
            let mat_name = match mat_expr {
                Expression::StringLiteral { value, .. } => Some(value.as_str()),
                Expression::Variable { name, .. } => Some(name.as_str()),
                _ => None,
            };
            if let Some(name) = mat_name {
                if let Some(id) = hw_space.material_registry.get_id(name) {
                    net_mat_id = id;
                }
            }
        }

        if let Some(w_expr) = net_decl.get_property("width") {
            if let Expression::Measurement { value, unit, .. } = w_expr {
                if let Ok(nm) = unit.to_nanometers(*value) {
                    net_width_nm = nm as i64;
                }
            }
        }

        hw_space.netlist.add_net(net_name.clone(), net_width_nm, net_mat_id);
        
        // v0.3.0 FIX: Extract classification robustly (handles "ground", ground, "signal", signal)
        let classification_str = if let Some(expr) = net_decl.get_property("classification") {
            match expr {
                Expression::StringLiteral { value, .. } => Some(value.as_str().trim_matches('"').to_string()),
                Expression::Variable { name, .. } => Some(name.as_str().trim_matches('"').to_string()),
                _ => None,
            }
        } else {
            net_decl.classification().map(|s| s.to_string())
        };

        let classification = if let Some(class_str) = classification_str {
            match class_str.to_ascii_lowercase().as_str() {
                "power" => hwc_engine::space::NetClassification::Power,
                "ground" => hwc_engine::space::NetClassification::Ground,
                "signal" => hwc_engine::space::NetClassification::Signal,
                "highvoltage" | "high_voltage" => hwc_engine::space::NetClassification::HighVoltage,
                _ => hwc_engine::space::NetClassification::Unclassified,
            }
        } else {
            hwc_engine::space::NetClassification::Unclassified
        };
        
        let mut net_props = hwc_engine::space::NetElectricalProperties::new(classification);
        
        if let Some(potential_expr) = net_decl.potential() {
            if let Some(v) = eval_expr_to_si(potential_expr, symbol_table) {
                net_props.potential_v = Some(v);
            }
        }
        
        if let Some(current_expr) = net_decl.get_property("current") {
            if let Some(a) = eval_expr_to_si(current_expr, symbol_table) {
                net_props.current_ma = Some(a * 1000.0);
            }
        }
        
        hw_space.net_electrical_properties.insert(net_name.clone(), net_props);
        hw_space.net_classifications.insert(net_name.clone(), classification);
    }

    // 4-6. Populate emitted primitives into the EntityGraph / netlist
    pours::populate_pours(
        &mut hw_space,
        mem,
        net_id_to_name,
        space_decl.profile.as_ref().map(|p| p.as_str()).unwrap_or("None"),
    )?;
    contacts::populate_contacts(&mut hw_space, space_decl, mem, net_id_to_name, symbol_table)?;
    devices::populate_devices(&mut hw_space, mem, net_id_to_name, symbol_table)?;
    routes::populate_routes(&mut hw_space, mem)?;

    Ok(hw_space)
}

fn eval_expr_to_si(
    expr: &Expression,
    symbol_table: &SymbolTable,
) -> Option<f64> {
    let unit_registry = hwc_types::UnitRegistry::standard();
    match expr {
        Expression::Measurement { value, unit, .. } => {
            let sym = unit.to_symbol();
            unit_registry.to_base_si(*value, &sym).or_else(|| {
                unit.base_si_multiplier().map(|mul| *value * mul)
            })
        }
        Expression::Literal { value, .. } => Some(*value as f64),
        Expression::FloatLiteral { value, .. } => Some(*value),
        Expression::Variable { .. } => {
            // Constants are evaluated in the Comptime Engine (v0.3.0); the
            // symbol table no longer stores constant declarations, so a bare
            // variable reference cannot be resolved here.
            None
        }
        Expression::Binary { left, operator, right, .. } => {
            let l = eval_expr_to_si(left, symbol_table)?;
            let r = eval_expr_to_si(right, symbol_table)?;
            match operator {
                hwc_parser::BinaryOperator::Add => Some(l + r),
                hwc_parser::BinaryOperator::Subtract => Some(l - r),
                hwc_parser::BinaryOperator::Multiply => Some(l * r),
                hwc_parser::BinaryOperator::Divide => {
                    if r.abs() > 1e-15 {
                        Some(l / r)
                    } else {
                        None
                    }
                }
                _ => None,
            }
        }
        Expression::Unary { operator, operand, .. } => {
            let v = eval_expr_to_si(operand, symbol_table)?;
            match operator {
                hwc_parser::UnaryOperator::Negate => Some(-v),
                hwc_parser::UnaryOperator::Plus => Some(v),
                _ => None,
            }
        }
        Expression::Grouped { expression, .. } => {
            eval_expr_to_si(expression, symbol_table)
        }
        _ => None,
    }
}
