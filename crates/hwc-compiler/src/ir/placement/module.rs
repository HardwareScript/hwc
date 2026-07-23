use super::super::conversions::{coordinate_to_point, CoordinateContext};
use super::super::errors::IrError;
use super::context::PlacementContext;
use super::helpers::extract_placements_from_layout_statements;
use compact_str::CompactString;
use hwc_engine::{HardwareSpace, Point3D};
use rustc_hash::FxHashMap;

pub fn place_module_instance(
    space: &mut HardwareSpace,
    module_placement: &hwc_parser::ComponentPlacement,
    layouts: &[hwc_parser::ModuleLayoutBlock],
    bbox_tracker: &mut crate::bounding_box_tracker::BoundingBoxTracker,
    ctx: &PlacementContext,
) -> Result<(), IrError> {
    use crate::module_flattener::flatten_module;

    let module_def = ctx
        .symbol_table
        .get_module(module_placement.component_type.as_str())
        .map_err(|e| IrError::PlacementConstraint {
            message: format!("Module not found: {}", e),
            component: module_placement.component_type.to_string(),
        })?;

    println!("🔧 Flattening module: {}", module_placement.component_type);

    let flattened = flatten_module(module_def).map_err(|e| IrError::PlacementConstraint {
        message: format!("Module flattening failed: {}", e),
        component: module_placement.component_type.to_string(),
    })?;

    println!("   ├─ Flattened {} components", flattened.components.len());
    println!("   └─ Flattened {} routes", flattened.routes.len());

    let _inherited_z_test = ctx
        .stackup_manager
        .resolve_elevation_top(
            &hwc_parser::Elevation::Physical {
                start: hwc_parser::Expression::Measurement {
                    value: 0.0,
                    unit: hwc_parser::Unit::Millimeter,
                    span: hwc_parser::Span::new(0, 0),
                },
                end: None,
            },
            ctx.symbol_table,
            ctx.eval_context,
        )
        .unwrap_or(0);

    let instance_name =
        module_placement
            .name
            .as_ref()
            .ok_or_else(|| IrError::PlacementConstraint {
                message: "Module instantiation requires a name".into(),
                component: module_placement.component_type.to_string(),
            })?;

    let intrinsic_layout = &module_def.intrinsic_layout;
    let layout_block = layouts
        .iter()
        .find(|layout| layout.module_instance.as_str() == instance_name.as_str());

    let placements_source: Option<Vec<_>> = if let Some(intr) = intrinsic_layout {
        println!(
            "   ├─ Using intrinsic layout ({} statements) from module definition",
            intr.len()
        );
        Some(extract_placements_from_layout_statements(intr))
    } else if let Some(ext) = layout_block {
        println!(
            "   ├─ Found external layout block with {} mappings",
            ext.statements.len()
        );
        Some(extract_placements_from_layout_statements(&ext.statements))
    } else {
        None
    };

    if let Some(placements) = placements_source {
        for module_comp in &flattened.components {
            let comp_internal_name =
                module_comp
                    .name
                    .as_ref()
                    .ok_or_else(|| IrError::PlacementConstraint {
                        message: "Module component must have a name for layout mapping".into(),
                        component: instance_name.to_string().into(),
                    })?;

            let mapping = placements
                .iter()
                .find(|p| p.component_name == *comp_internal_name)
                .ok_or_else(|| IrError::PlacementConstraint {
                    message: format!(
                        "No layout mapping found for component '{}' in module '{}'",
                        comp_internal_name, instance_name
                    ),
                    component: instance_name.to_string().into(),
                })?;

            let coord_ctx = CoordinateContext {
                origin: ctx.origin,
                space_dimensions: &space.dimensions,
                symbol_table: ctx.symbol_table,
                eval_context: ctx.eval_context,
                bbox_tracker: Some(bbox_tracker),
                stackup_manager: ctx.stackup_manager,
                profile: ctx.profile,
            };
            let z_expr = match &mapping.position {
                hwc_parser::Coordinate::Positional { z, .. }
                | hwc_parser::Coordinate::Declarative { z, .. } => z,
                hwc_parser::Coordinate::Relative(_) => &hwc_parser::Expression::Literal {
                    value: 0,
                    span: hwc_parser::Span::new(0, 0),
                },
            };
            let resolved_z = ctx
                .stackup_manager
                .resolve_z_expression(z_expr, ctx.symbol_table, ctx.eval_context)
                .unwrap_or(0);
            let mut position = coordinate_to_point(&mapping.position, &coord_ctx).map_err(|e| {
                IrError::CoordinateResolutionFailed {
                    coordinate_str: format!("module component '{}' position", comp_internal_name),
                    reason: e,
                }
            })?;
            position.z = resolved_z;

            let comp_name = format!("{}.{}", instance_name, comp_internal_name);
            println!("   ├─ Placing {} at position", comp_name);

            // Add component to netlist arena (v0.1.8 replacement for ComponentPlacer)
            let component_id = space.netlist.add_component(
                comp_name.clone().into(),
                module_comp.component_type.clone(),
                (position.x, position.y, position.z),
            );

            // Register pins in netlist arena
            if let Ok(component_def) = ctx
                .symbol_table
                .get_component(module_comp.component_type.as_str())
            {
                for pin_name in &component_def.pins {
                    space
                        .netlist
                        .add_pin(component_id, pin_name.clone(), (0, 0, 0), None);
                }
            }
        }
    } else {
        println!("   ⚠️  No layout (intrinsic or external) - using automatic offset placement (may cause collisions)");

        let coord_ctx = CoordinateContext {
            origin: ctx.origin,
            space_dimensions: &space.dimensions,
            symbol_table: ctx.symbol_table,
            eval_context: ctx.eval_context,
            bbox_tracker: Some(bbox_tracker),
            stackup_manager: ctx.stackup_manager,
            profile: ctx.profile,
        };
        let base_position = coordinate_to_point(
            module_placement.position.as_ref().ok_or_else(|| {
                IrError::CoordinateResolutionFailed {
                    coordinate_str: format!("module '{}' base position", instance_name),
                    reason: "Module has no explicit position".into(),
                }
            })?,
            &coord_ctx,
        )
        .map_err(|e| IrError::CoordinateResolutionFailed {
            coordinate_str: format!("module '{}' base position", instance_name),
            reason: e,
        })?;

        for (idx, module_comp) in flattened.components.iter().enumerate() {
            let comp_name = if let Some(ref name) = module_comp.name {
                format!("{}.{}", instance_name, name)
            } else {
                format!("{}.comp_{}", instance_name, idx)
            };

            println!("   ├─ Placing component: {}", comp_name);

            let offset_x = (idx as i64) * 1_000_000;
            let position =
                Point3D::new(base_position.x + offset_x, base_position.y, base_position.z);

            // Add component to netlist arena (v0.1.8 replacement for ComponentPlacer)
            let component_id = space.netlist.add_component(
                comp_name.clone().into(),
                module_comp.component_type.clone(),
                (position.x, position.y, position.z),
            );

            // Register pins in netlist arena
            if let Ok(component_def) = ctx
                .symbol_table
                .get_component(module_comp.component_type.as_str())
            {
                for pin_name in &component_def.pins {
                    space
                        .netlist
                        .add_pin(component_id, pin_name.clone(), (0, 0, 0), None);
                }
            }
        }
    }

    println!(
        "✅ Module {} successfully flattened and placed",
        instance_name
    );

    let mut promoted_pin_positions: FxHashMap<CompactString, (i64, i64, i64)> =
        FxHashMap::default();

    let module_pin_names: Vec<CompactString> =
        module_def.pins.iter().map(|p| p.name.clone()).collect();

    for route in &flattened.routes {
        let from_is_mod = module_pin_names.contains(&route.from.component)
            || (route.from.component.is_empty() && module_pin_names.contains(&route.from.pin));
        let to_is_mod = module_pin_names.contains(&route.to.component)
            || (route.to.component.is_empty() && module_pin_names.contains(&route.to.pin));

        if from_is_mod {
            let mod_pin = if route.from.component.is_empty() {
                route.from.pin.clone()
            } else {
                route.from.component.clone()
            };
            let internal_name = format!("{}.{}", instance_name, route.to.component);
            if let Some(internal_id) = space.netlist.get_component_by_name(&internal_name) {
                for pid in space.netlist.get_component_pins(internal_id) {
                    if let Some(pin_data) = space.netlist.get_pin(pid) {
                        if pin_data.name == route.to.pin {
                            if let Some(pos) = space.netlist.get_pin_position(pid) {
                                promoted_pin_positions.insert(mod_pin, pos);
                                break;
                            }
                        }
                    }
                }
            }
        }

        if to_is_mod {
            let mod_pin = if route.to.component.is_empty() {
                route.to.pin.clone()
            } else {
                route.to.component.clone()
            };
            let internal_name = format!("{}.{}", instance_name, route.from.component);
            if let Some(internal_id) = space.netlist.get_component_by_name(&internal_name) {
                for pid in space.netlist.get_component_pins(internal_id) {
                    if let Some(pin_data) = space.netlist.get_pin(pid) {
                        if pin_data.name == route.from.pin {
                            if let Some(pos) = space.netlist.get_pin_position(pid) {
                                promoted_pin_positions.insert(mod_pin, pos);
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    if !promoted_pin_positions.is_empty() {
        println!(
            "   ├─ Promoted {} macro pins to physical anchor positions",
            promoted_pin_positions.len()
        );
    }

    {
        use crate::symbol_table::expand_pin_declarations;

        let coord_ctx = CoordinateContext {
            origin: ctx.origin,
            space_dimensions: &space.dimensions,
            symbol_table: ctx.symbol_table,
            eval_context: ctx.eval_context,
            bbox_tracker: Some(bbox_tracker),
            stackup_manager: ctx.stackup_manager,
            profile: ctx.profile,
        };
        let virtual_position = coordinate_to_point(
            module_placement.position.as_ref().ok_or_else(|| {
                IrError::CoordinateResolutionFailed {
                    coordinate_str: format!("module '{}' virtual position", instance_name),
                    reason: "Module has no explicit position".into(),
                }
            })?,
            &coord_ctx,
        )
        .map_err(|e| IrError::CoordinateResolutionFailed {
            coordinate_str: format!("module '{}' virtual position", instance_name),
            reason: e,
        })?;

        let module_component_id = space.netlist.add_component(
            instance_name.to_string(),
            module_placement.component_type.to_string().into(),
            (virtual_position.x, virtual_position.y, virtual_position.z),
        );

        let expanded_pins = expand_pin_declarations(&module_def.pins);
        for pin_name in &expanded_pins {
            let pin_pos = promoted_pin_positions.get(pin_name).copied().unwrap_or((
                virtual_position.x,
                virtual_position.y,
                virtual_position.z,
            ));
            space
                .netlist
                .add_pin(module_component_id, pin_name.clone(), pin_pos, None);
        }

        println!(
            "   ├─ Registered virtual module component '{}' with {} interface pin(s) ({} promoted to physical anchors)",
            instance_name,
            expanded_pins.len(),
            promoted_pin_positions.len()
        );
    }

    if !flattened.routes.is_empty() {
        println!("   ├─ Processing {} module routes", flattened.routes.len());

        let module_pins: Vec<CompactString> =
            module_def.pins.iter().map(|p| p.name.clone()).collect();

        for module_route in &flattened.routes {
            let from_is_module_pin = module_pins.contains(&module_route.from.component)
                || (module_route.from.component.is_empty()
                    && module_pins.contains(&module_route.from.pin));
            let to_is_module_pin = module_pins.contains(&module_route.to.component)
                || (module_route.to.component.is_empty()
                    && module_pins.contains(&module_route.to.pin));

            if from_is_module_pin || to_is_module_pin {
                println!(
                    "   ├─ Skipping interface route: {}.{} → {}.{} (connects to module pins)",
                    module_route.from.component,
                    module_route.from.pin,
                    module_route.to.component,
                    module_route.to.pin
                );
                continue;
            }

            let from_component = format!("{}.{}", instance_name, module_route.from.component);
            let to_component = format!("{}.{}", instance_name, module_route.to.component);

            println!(
                "   ├─ Route: {}.{} → {}.{}",
                from_component, module_route.from.pin, to_component, module_route.to.pin
            );

            let space_route = hwc_parser::Route {
                from: hwc_parser::RouteEndpointSpec::ComponentPin {
                    component_name: from_component.clone().into(),
                    component_index: module_route.from.component_index.clone().and_then(|idx| {
                        match idx {
                            hwc_parser::ArrayIndex::Literal(n) => {
                                Some(hwc_parser::Expression::Literal {
                                    value: n as i64,
                                    span: module_route.from.span,
                                })
                            }
                            _ => None,
                        }
                    }),
                    pin_name: module_route.from.pin.clone(),
                    pin_index: module_route
                        .from
                        .pin_index
                        .clone()
                        .and_then(|idx| match idx {
                            hwc_parser::ArrayIndex::Literal(n) => {
                                Some(hwc_parser::Expression::Literal {
                                    value: n as i64,
                                    span: module_route.from.span,
                                })
                            }
                            _ => None,
                        }),
                    span: module_route.from.span,
                },
                to: hwc_parser::RouteEndpointSpec::ComponentPin {
                    component_name: to_component.clone().into(),
                    component_index: module_route.to.component_index.clone().and_then(|idx| {
                        match idx {
                            hwc_parser::ArrayIndex::Literal(n) => {
                                Some(hwc_parser::Expression::Literal {
                                    value: n as i64,
                                    span: module_route.to.span,
                                })
                            }
                            _ => None,
                        }
                    }),
                    pin_name: module_route.to.pin.clone(),
                    pin_index: module_route.to.pin_index.clone().and_then(|idx| match idx {
                        hwc_parser::ArrayIndex::Literal(n) => {
                            Some(hwc_parser::Expression::Literal {
                                value: n as i64,
                                span: module_route.to.span,
                            })
                        }
                        _ => None,
                    }),
                    span: module_route.to.span,
                },
                width: None,
                layer: None,
                strategy: None,
                pattern: None,
                strategy_params: vec![],
                path: None,
                signal_group: None,
                bridge: None,
                exit_escape: None,
                enter_escape: None,
                current_limit_ac: None,
                intent: None,
                escape_stub: None, // v0.1.9: Use profile default
                span: module_route.span,
            };

            super::super::routing::route_trace(
                space,
                &space_route,
                ctx.origin,
                ctx.symbol_table,
                ctx.eval_context,
                ctx.stackup_manager,
                ctx.profile,
            )
            .map_err(|_e| IrError::NoPathFound {
                net: format!("{}.{}", instance_name, module_route.from.pin).into(),
                from_pin: format!("{}.{}", from_component, module_route.from.pin).into(),
                to_pin: format!("{}.{}", to_component, module_route.to.pin).into(),
            })?;
        }

        println!("   └─ All module routes processed");
    }

    Ok(())
}
