use crate::ir::errors::IrError;
use compact_str::CompactString;
use hwc_engine::HardwareSpace;

use super::endpoint_resolution::construct_entity_name;
use super::pin_resolution::get_pin_ids;

/// Register a net for a route and connect the source and target pins.
pub fn register_net_for_route(
    space: &mut HardwareSpace,
    route: &hwc_parser::Route,
    symbol_table: &crate::SymbolTable,
    eval_context: &hwc_parser::EvaluationContext,
    stackup_manager: &crate::ir::stackup_manager::StackupManager,
    profile: Option<&hwc_parser::ProfileDefinition>,
    space_def: Option<&hwc_parser::SpaceDefinition>,
) -> Result<hwc_engine::netlist::NetId, IrError> {
    let (start_pin_id, goal_pin_id) = get_pin_ids(space, route)?;

    let from_name = construct_entity_name(&route.from).unwrap_or_else(|_| "src".into());
    let to_name = construct_entity_name(&route.to).unwrap_or_else(|_| "dst".into());
    let net_name: CompactString = format!("NET_{}_to_{}", from_name, to_name).into();

    let width_nm = if let Some(w_expr) = &route.width {
        crate::ir::conversions::evaluate_expression_to_nm(w_expr, symbol_table, eval_context)
            .map_err(|e| IrError::InvalidRouteExpression {
                expression: "trace width".into(),
                reason: e.to_string(),
            })?
    } else {
        profile.and_then(|p| p.trace.as_ref())
            .map(|t| crate::ir::conversions::measurement_to_nm(&t.min_width, symbol_table, eval_context))
            .transpose()
            .map_err(|e| IrError::InvalidRouteExpression {
                expression: "profile trace width".into(),
                reason: e.to_string(),
            })?
            .ok_or_else(|| IrError::MissingAsicConstraint {
                message: "Route has no explicit width and PDK has no 'trace.min_width' constraint".into(),
                hint: "Add 'width: <value>' to the route, or declare 'trace: min_width: <value>' in the profile.".into(),
            })?
    };

    let start_pin_z = space
        .netlist
        .get_pin_position(start_pin_id)
        .map(|pos| pos.2)
        .unwrap_or(0);
    let copper_id = (|| -> Option<hwc_engine::material::MaterialId> {
        let layer_name = stackup_manager.get_layer_name_at_z(start_pin_z)?;
        let mat_name = profile
            .and_then(|p| p.stackup.as_ref())
            .and_then(|stackup| {
                stackup
                    .layers
                    .iter()
                    .find(|l| l.name.name == layer_name)
                    .map(|l| l.material.clone())
            })?;
        space.material_registry.get_id(&mat_name)
    })()
    .ok_or_else(|| IrError::UndeclaredMaterial {
        material: format!(
            "No material found at Z={}nm (check stackup definition)",
            start_pin_z
        )
        .into(),
    })?;

    let existing_net = if let Some(pin_data) = space.netlist.get_pin(start_pin_id) {
        pin_data.connected_net
    } else {
        None
    };

    let goal_net = if let Some(pin_data) = space.netlist.get_pin(goal_pin_id) {
        pin_data.connected_net
    } else {
        None
    };

    let net_id = match (existing_net, goal_net) {
        (Some(e), Some(g)) if e == g => e,

        (Some(e), Some(g)) => {
            let e_name = space
                .netlist
                .get_net(e)
                .map(|n| n.name.as_str())
                .unwrap_or("unknown");
            let g_name = space
                .netlist
                .get_net(g)
                .map(|n| n.name.as_str())
                .unwrap_or("unknown");

            if !e_name.starts_with("NET_") && !g_name.starts_with("NET_") && e_name != g_name {
                if let Some(space_def) = space_def {
                    if let Some(module_ref) = &space_def.implements_module {
                        if let Ok(module_def) = symbol_table.get_module(module_ref) {
                            let mut found_declaration = false;

                            for stmt in &module_def.statements {
                                if let hwc_parser::ModuleStatement::Route(module_route) = stmt {
                                    let logical_from = module_route.from.pin.as_str();
                                    let logical_to = module_route.to.pin.as_str();

                                    if (e_name == logical_from && g_name == logical_to)
                                        || (e_name == logical_to && g_name == logical_from)
                                    {
                                        found_declaration = true;
                                        break;
                                    }
                                }
                            }

                            if !found_declaration {
                                return Err(IrError::InvalidRouteExpression {
                                    expression: format!("route {} to {}", 
                                        construct_entity_name(&route.from)?,
                                        construct_entity_name(&route.to)?),
                                    reason: format!(
                                        "Route connects nets '{}' and '{}', but module '{}' does not declare connectivity between these nets. \
                                         Add 'route {} to {}' to the module definition, or remove the cross-net route.",
                                        e_name, g_name, module_ref, e_name, g_name
                                    ),
                                });
                            }
                        }
                    } else {
                        return Err(IrError::InvalidRouteExpression {
                            expression: format!("route {} to {}", 
                                construct_entity_name(&route.from)?,
                                construct_entity_name(&route.to)?),
                            reason: format!(
                                "Route would short-circuit nets '{}' and '{}'. \
                                 Cross-net routing is only allowed when implementing a module that declares this connectivity. \
                                 Either add 'implements ModuleName' to the space and declare the route in the module, \
                                 or connect both endpoints to the same net.",
                                e_name, g_name
                            ),
                        });
                    }
                }

                e
            } else {
                let (keep, drop) = if e_name.starts_with("NET_") && !g_name.starts_with("NET_") {
                    (g, e)
                } else {
                    (e, g)
                };

                let drop_name = space
                    .netlist
                    .get_net(drop)
                    .map(|n| n.name.as_str())
                    .unwrap_or("");
                if drop_name.starts_with("NET_") {
                    if let Some(drop_pins) = space.netlist.get_net_pins(drop).map(|p| p.to_vec()) {
                        for p in drop_pins {
                            space.netlist.connect_pin(p, keep);
                        }
                    }
                }
                keep
            }
        }

        (Some(e), None) => e,
        (None, Some(g)) => g,

        (None, None) => space.netlist.add_net(net_name.clone(), width_nm, copper_id),
    };

    space.netlist.connect_pin(start_pin_id, net_id);
    space.netlist.connect_pin(goal_pin_id, net_id);

    // v0.1.9.1 BUG FIX: Do NOT call set_entity_net here!
    //
    // PROBLEM: When pads/planes are placed with explicit `net:` declarations,
    // they already have their net_id assigned in the EntityGraph. Calling
    // set_entity_net() here would OVERWRITE those assignments, causing all
    // pads on different nets to be merged into a single net.
    //
    // EXAMPLE BUG (before fix):
    //   nets: { A1: {...}, B1: {...}, A2: {...}, B2: {...} }  ← 4 nets declared
    //   Pad_A1 net: A1  ← NetId(1)
    //   Pad_B1 net: B1  ← NetId(2)
    //   route Pad_A1 to Pad_B1
    //     → set_entity_net("Pad_A1", "A1")  ← NetId(1)
    //     → set_entity_net("Pad_B1", "A1")  ← NetId(1) WRONG! Was NetId(2)
    //   Pad_A2 net: A2  ← NetId(3)
    //   Pad_B2 net: B2  ← NetId(4)
    //   route Pad_A2 to Pad_B2
    //     → set_entity_net("Pad_A2", "A2")  ← NetId(1) WRONG! Was NetId(3)
    //     → set_entity_net("Pad_B2", "A2")  ← NetId(1) WRONG! Was NetId(4)
    //   Result: All 4 pads end up on NetId(1), causing crosstalk test to fail
    //
    // CORRECT BEHAVIOR:
    // - EntityGraph tracks which net each physical pad belongs to (set during placement)
    // - Netlist tracks which pins are electrically connected (merged during routing)
    // - Routing creates physical traces between pads, but doesn't change which nets they're on
    // - The netlist merger (lines 117-149) already handles net connectivity correctly
    //
    // WHEN TO UPDATE ENTITY NET:
    // Only update entity net assignments when routing between component pins that had NO
    // pre-existing net assignment (i.e., auto-generated NET_foo_to_bar names).
    // Planes/pads with explicit `net:` declarations should keep their original net_id.

    Ok(net_id)
}
