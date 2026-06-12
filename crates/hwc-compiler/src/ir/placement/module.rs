//! Module instance placement functionality.
//!
//! v0.1.7: Modules can now carry intrinsic physical layout (Physical Macros) to prevent pile-up at [0,0,0].
//! Intrinsic layout on the ModuleDefinition takes precedence over external space layout blocks.

use super::super::conversions::{coordinate_to_point, CoordinateContext};
use super::super::errors::IrError;
use super::helpers::extract_placements_from_layout_statements;
use crate::SymbolTable;
use compact_str::CompactString;
use hwc_engine::{ComponentPlacer, HardwareSpace, PlacementParams, Point3D};
use rustc_hash::FxHashMap;

/// Place a module instance by flattening it and placing all internal components.
/// v0.1.7: Now receives stackup_manager so sub-components inherit parent's Z elevations (Z-Context Inheritance).
pub fn place_module_instance(
    space: &mut HardwareSpace,
    module_placement: &hwc_parser::ComponentPlacement,
    origin: hwc_parser::OriginPoint,
    symbol_table: &SymbolTable,
    layouts: &[hwc_parser::ModuleLayoutBlock],
    bbox_tracker: &mut crate::bounding_box_tracker::BoundingBoxTracker,
    eval_context: &hwc_parser::EvaluationContext,
    collector: &hwc_diagnostics::DiagnosticCollector,
    stackup_manager: &super::super::stackup_manager::StackupManager, // Z-Context Inheritance wired: parent's StackupManager now available for resolving semantic Z in module sub-components and intrinsic layouts
    profile: Option<&hwc_parser::ProfileDefinition>,
) -> Result<(), IrError> {
    use crate::module_flattener::flatten_module;

    // Get the module definition
    let module_def = symbol_table
        .get_module(module_placement.component_type.as_str())
        .map_err(|e| IrError::PlacementError(format!("Module not found: {}", e)))?;

    println!("🔧 Flattening module: {}", module_placement.component_type);

    // Flatten the module (evaluate for loops, if conditionals)
    let flattened = flatten_module(module_def)
        .map_err(|e| IrError::PlacementError(format!("Module flattening failed: {}", e)))?;

    println!("   ├─ Flattened {} components", flattened.components.len());
    println!("   └─ Flattened {} routes", flattened.routes.len());

    // Z-Context Inheritance (wired): resolve using parent's stackup so module internals
    // get correct z_nm from the using space's profile (not a default or child's).
    // This makes modules reusable across different stackups/profiles.
    let _inherited_z_test = stackup_manager.resolve_elevation_top(
        &hwc_parser::Elevation::Physical {
            start: hwc_parser::Expression::Measurement {
                value: 0.0,
                unit: hwc_parser::Unit::Millimeter,
                span: hwc_parser::Span::new(0, 0),
            },
            end: None,
        },
        symbol_table,
        space.voxel_size.z_nm,
    ).unwrap_or(0);

    // Get the module instance name
    let instance_name = module_placement
        .name
        .as_ref()
        .ok_or_else(|| IrError::PlacementError("Module instantiation requires a name".into()))?;

    // v0.1.7 Physical Pile fix: Prefer intrinsic layout defined inside the module (self-contained Physical Macro)
    // over external layout block in the using space. This allows reusable modules with relative physical structure.
    let intrinsic_layout = &module_def.intrinsic_layout;
    let layout_block = layouts
        .iter()
        .find(|layout| layout.module_instance.as_str() == instance_name.as_str());

    // Determine placements source (always extracted to flat Vec<ModuleInternalPlacement>)
    let placements_source: Option<Vec<_>> = if let Some(intr) = intrinsic_layout {
        println!("   ├─ Using intrinsic layout ({} statements) from module definition", intr.len());
        Some(extract_placements_from_layout_statements(intr))
    } else if let Some(ext) = layout_block {
        println!("   ├─ Found external layout block with {} mappings", ext.statements.len());
        Some(extract_placements_from_layout_statements(&ext.statements))
    } else {
        None
    };

    if let Some(placements) = placements_source {
        // Use the chosen layout (intrinsic or external) to place sub-components
        for module_comp in &flattened.components {
            let comp_internal_name = module_comp.name.as_ref().ok_or_else(|| {
                IrError::PlacementError(
                    "Module component must have a name for layout mapping".into(),
                )
            })?;

            let mapping = placements
                .iter()
                .find(|p| p.component_name == *comp_internal_name)
                .ok_or_else(|| {
                    IrError::PlacementError(format!(
                        "No layout mapping found for component '{}' in module '{}'",
                        comp_internal_name, instance_name
                    ))
                })?;

            let ctx = CoordinateContext {
                voxel_size: &space.voxel_size,
                grid_size: &space.grid,
                origin,
                space_dimensions: &space.dimensions,
                symbol_table,
                eval_context,
                bbox_tracker: Some(bbox_tracker),
                stackup_manager,
                profile,
            };
            // Z-Context Inheritance (properly wired via stackup_manager.rs helper):
            // Resolve Z using parent's StackupManager so semantic layers (e.g. "l1") in module
            // intrinsic layouts or external module layouts inherit the correct physical z_nm.
            let z_expr = match &mapping.position {
                hwc_parser::Coordinate::Positional { z, .. } | hwc_parser::Coordinate::Declarative { z, .. } => z,
                hwc_parser::Coordinate::Relative(_) => &hwc_parser::Expression::Literal { value: 0, span: hwc_parser::Span::new(0, 0) },
            };
            let resolved_z = stackup_manager.resolve_z_expression(z_expr, symbol_table).unwrap_or(0);
            let mut position = coordinate_to_point(&mapping.position, &ctx);
            position.z = resolved_z;  // inherit from parent's stackup for semantic layers

            let comp_name = format!("{}.{}", instance_name, comp_internal_name);
            println!("   ├─ Placing {} at position", comp_name);

            let placer = ComponentPlacer::new();
            placer
                .place_component(PlacementParams {
                    grid: &mut space.voxel_grid,
                    voxel_size: &space.voxel_size,
                    arena: &mut space.netlist,
                    symbol_table,
                    material_registry: &mut space.material_registry,
                    name: comp_name.into(),
                    component_type: module_comp.component_type.clone(),
                    position,
                    rotation_deg: 0.0,
                    merge_waiver: hwc_parser::MergeWaiver::None,
                    collector: Some(&crate::DiagnosticReporterAdapter(collector)),
                })
                .map_err(|e| {
                    IrError::PlacementError(format!("Failed to place module component: {}", e))
                })?;
        }
    } else {
        println!("   ⚠️  No layout (intrinsic or external) - using automatic offset placement (may cause collisions)");

        let ctx = CoordinateContext {
                voxel_size: &space.voxel_size,
                grid_size: &space.grid,
                origin,
                space_dimensions: &space.dimensions,
                symbol_table,
                eval_context,
                bbox_tracker: Some(bbox_tracker),
                stackup_manager,
                profile,
            };
        let base_position = coordinate_to_point(&module_placement.position, &ctx);

        for (idx, module_comp) in flattened.components.iter().enumerate() {
            let comp_name = if let Some(ref name) = module_comp.name {
                format!("{}.{}", instance_name, name)
            } else {
                format!("{}.comp_{}", instance_name, idx)
            };

            println!("   ├─ Placing component: {}", comp_name);

            let offset_x = (idx as i64) * 1_000_000; // 1mm offset
            let position = Point3D::new(base_position.x + offset_x, base_position.y, base_position.z);

            let placer = ComponentPlacer::new();
            placer
                .place_component(PlacementParams {
                    grid: &mut space.voxel_grid,
                    voxel_size: &space.voxel_size,
                    arena: &mut space.netlist,
                    symbol_table,
                    material_registry: &mut space.material_registry,
                    name: comp_name.into(),
                    component_type: module_comp.component_type.clone(),
                    position,
                    rotation_deg: 0.0,
                    merge_waiver: hwc_parser::MergeWaiver::None,
                    collector: Some(&crate::DiagnosticReporterAdapter(collector)),
                })
                .map_err(|e| {
                    IrError::PlacementError(format!("Failed to place module component: {}", e))
                })?;
        }
    }

    println!(
        "✅ Module {} successfully flattened and placed",
        instance_name
    );

    // === Macro Pin Promotion (v0.1.7 roadmap) ===
    // After placing internal components with their real physical positions (from intrinsic or external layout),
    // trace the module's logical routes to find which internal pin each interface pin is wired to.
    // Promote the virtual module pin to that physical anchor location so the parent space/router
    // can see the true macro pin positions (instead of always (0,0,0)).
    let mut promoted_pin_positions: FxHashMap<CompactString, (i64, i64, i64)> = FxHashMap::default();

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

    // Register the module instance as a virtual component in the netlist.
    //
    // This allows space-level routes like `route MainDSP.Bus_Out[0] to Amp.RF_IN`
    // to resolve `MainDSP` as a component and `Bus_Out[0]` as one of its pins.
    //
    // Array interface pins (e.g., `Bus_Out[64]`) are expanded into individual
    // pin entries: `Bus_Out[0]`, `Bus_Out[1]`, ..., `Bus_Out[63]`.
    {
        use crate::symbol_table::expand_pin_declarations;

        // Determine a nominal position for the virtual module component.
        // Use the module placement position if available, otherwise (0, 0, 0).
        let ctx = CoordinateContext {
            voxel_size: &space.voxel_size,
            grid_size: &space.grid,
            origin,
            space_dimensions: &space.dimensions,
            symbol_table,
            eval_context,
            bbox_tracker: Some(bbox_tracker),
            stackup_manager,
            profile,
        };
        let virtual_position = coordinate_to_point(&module_placement.position, &ctx);

        // Register the module instance as a virtual component.
        let module_component_id = space.netlist.add_component(
            instance_name.to_string(),
            module_placement.component_type.to_string().into(),
            (virtual_position.x, virtual_position.y, virtual_position.z),
        );

        // Expand and register all interface pins.
        // Use promoted physical positions from Macro Pin Promotion (if a connection was traced).
        let expanded_pins = expand_pin_declarations(&module_def.pins);
        for pin_name in &expanded_pins {
            let pin_pos = promoted_pin_positions
                .get(pin_name)
                .copied()
                .unwrap_or((virtual_position.x, virtual_position.y, virtual_position.z));
            space.netlist.add_pin(
                module_component_id,
                pin_name.clone(),
                pin_pos,
                None, // Module interface pins don't have physical pads (pads come from internal)
            );
        }

        println!(
            "   ├─ Registered virtual module component '{}' with {} interface pin(s) ({} promoted to physical anchors)",
            instance_name,
            expanded_pins.len(),
            promoted_pin_positions.len()
        );
    }

    // Process module routes and convert them to space routes
    if !flattened.routes.is_empty() {
        println!("   ├─ Processing {} module routes", flattened.routes.len());

        // Get the list of module interface pins
        let module_pins: Vec<CompactString> =
            module_def.pins.iter().map(|p| p.name.clone()).collect();

        for module_route in &flattened.routes {
            // Check if this route references module pins (interface pins)
            // Module pins can appear as:
            // 1. Component name (when used alone): component='In', pin=''
            // 2. Pin name with empty component: component='', pin='In'
            let from_is_module_pin = module_pins.contains(&module_route.from.component)
                || (module_route.from.component.is_empty()
                    && module_pins.contains(&module_route.from.pin));
            let to_is_module_pin = module_pins.contains(&module_route.to.component)
                || (module_route.to.component.is_empty()
                    && module_pins.contains(&module_route.to.pin));

            // Skip routes that connect to module interface pins
            // These would need to be connected from outside the module
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

            // Resolve module pin references to full component.pin paths
            let from_component = format!("{}.{}", instance_name, module_route.from.component);
            let to_component = format!("{}.{}", instance_name, module_route.to.component);

            println!(
                "   ├─ Route: {}.{} → {}.{}",
                from_component, module_route.from.pin, to_component, module_route.to.pin
            );

            // Create a space route from the module route
            let space_route = hwc_parser::Route {
                from: hwc_parser::PinReference {
                    component: from_component.into(),
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
                    pin: module_route.from.pin.clone(),
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
                to: hwc_parser::PinReference {
                    component: to_component.into(),
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
                    pin: module_route.to.pin.clone(),
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
                strategy: None,
                strategy_params: vec![],
                path: None, // Module routes are logical, no waypoints
                signal_group: None,
                bridge: None,
                exit_escape: None,
                enter_escape: None,
                span: module_route.span,
            };

            // Route the trace in the space
            super::super::routing::route_trace(
                space,
                &space_route,
                origin,
                symbol_table,
                eval_context,
                stackup_manager,
                profile,
            )
            .map_err(|e| {
                IrError::RoutingError(format!("Failed to route module connection: {}", e))
            })?;
        }

        println!("   └─ All module routes processed");
    }

    Ok(())
}
