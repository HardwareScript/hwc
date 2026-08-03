use crate::ir::errors::IrError;
use crate::ir::placement_item::PlacementItem;
use crate::SymbolTable;

/// Phase 0: Try to load routes from lockfile. Returns true if routes were loaded.
pub fn try_load_lockfile(
    space: &mut hwc_engine::HardwareSpace,
    lockfile_path: Option<&std::path::Path>,
    force_reroute: bool,
) -> Result<bool, IrError> {
    let Some(path) = lockfile_path else {
        return Ok(false);
    };
    if force_reroute {
        eprintln!("[LOCK] --force-reroute: Skipping lockfile load, will compute all routes fresh");
        return Ok(false);
    }

    let current_fingerprint = hwc_engine::geometry_router::compute_fingerprint_from_space(space);

    match hwc_engine::geometry_router::load_lockfile(path) {
        Ok(loaded) => {
            if !hwc_engine::geometry_router::is_valid(&loaded, &current_fingerprint) {
                eprintln!(
                    "[LOCK] Lockfile fingerprint mismatch for '{}'. Will compute routes fresh.",
                    space.name
                );
                return Ok(false);
            }
            match hwc_engine::geometry_router::lockfile_to_traces(
                &loaded,
                &space.netlist,
                &space.stackup_layers,
                &space.material_registry,
            ) {
                Ok(cached_traces) => {
                    for trace in cached_traces {
                        let trace_segments: Vec<hwc_engine::geometry::TraceSegment> = trace
                            .segments
                            .iter()
                            .map(|line_seg| {
                                hwc_engine::geometry::TraceSegment::new(
                                    line_seg.start,
                                    line_seg.end,
                                    trace.cross_section.width_nm,
                                    trace.material,
                                )
                            })
                            .collect();

                        space
                            .entity_graph
                            .register_trace_segments(trace.net_id, trace_segments);

                        // v0.2.0: Register cached routes in routing database (single source of truth)
                        let from_entity = format!("lockfile_{}_start", trace.net_name);
                        let to_entity = format!("lockfile_{}_end", trace.net_name);
                        space.routing_database.register_parent_route(
                            trace,
                            from_entity.into(),
                            to_entity.into(),
                        );
                    }

                    eprintln!(
                        "[LOCK] Valid lockfile loaded for '{}'. Skipping all routing (manual + auto).",
                        space.name
                    );
                    Ok(true)
                }
                Err(e) => {
                    eprintln!(
                        "[LOCK] Lockfile load failed: {}. Will compute routes fresh.",
                        e
                    );
                    Ok(false)
                }
            }
        }
        Err(_) => Ok(false),
    }
}

/// Collect route net policies and return them mapped by NetId.
pub fn collect_route_net_policies(
    space: &hwc_engine::HardwareSpace,
    space_def: &hwc_parser::SpaceDefinition,
    symbol_table: &SymbolTable,
    eval_context: &hwc_parser::EvaluationContext,
) -> rustc_hash::FxHashMap<hwc_engine::netlist::NetId, hwc_engine::RoutingPattern> {
    let mut route_net_policies = rustc_hash::FxHashMap::default();

    for policy in &space_def.route_net_policies() {
        if let Some(ref pattern_inst) = policy.pattern {
            match crate::ir::routing::instantiate_pattern(pattern_inst, symbol_table, eval_context) {
                Ok(pattern) => {
                    if let Some(net_id) = space.netlist.get_net_by_name(policy.net_id.as_str()) {
                        route_net_policies.insert(net_id, pattern);
                    } else {
                        eprintln!(
                            "[ROUTER] WARNING: Route net policy for unknown net '{}', skipping",
                            policy.net_id
                        );
                    }
                }
                Err(e) => {
                    eprintln!(
                        "[ROUTER] WARNING: Failed to instantiate pattern for net '{}': {}",
                        policy.net_id, e
                    );
                }
            }
        }
    }

    route_net_policies
}

/// Process all routes: manual routes get realized, automatic routes collected for batch routing.
pub fn process_routes(
    space: &mut hwc_engine::HardwareSpace,
    ctx: &super::CompilationContext,
    routing_mode: hwc_parser::RoutingMode,
) -> Result<Vec<hwc_parser::Route>, IrError> {
    let mut auto_routes = Vec::new();

    
    for id in ctx.sorted_ids.iter() {
        let &item_idx = ctx.item_map.get(id).unwrap();
        let contextual_item = &ctx.placement_items[item_idx];
        let item = &contextual_item.item;
        let item_eval_context = &contextual_item.eval_context;

        if let PlacementItem::Route(route) = item {
           

            crate::ir::routing::register_net_for_route(
                space,
                route,
                ctx.symbol_table,
                item_eval_context,
                ctx.stackup_manager,
                ctx.profile,
                Some(ctx.space_def),
            )?;

            if !crate::ir::routing::needs_automatic_routing(route) {
                crate::ir::routing::route_trace(
                    space,
                    route,
                    ctx.origin,
                    ctx.symbol_table,
                    item_eval_context,
                    ctx.stackup_manager,
                    ctx.profile,
                )?;
            } else {
                if routing_mode == hwc_parser::RoutingMode::ManualOnly {
                    return Err(IrError::RoutingError(format!(
                        "Automatic routing found in 'manual_only' space: {} to {}",
                        crate::ir::routing::helpers::endpoint_label(&route.from),
                        crate::ir::routing::helpers::endpoint_label(&route.to)
                    )));
                }
                auto_routes.push(route.clone());
            }
        }
    }

    Ok(auto_routes)
}

/// Run the batch auto-router for all automatic routes.
pub fn auto_route(
    space: &mut hwc_engine::HardwareSpace,
    auto_routes: Vec<hwc_parser::Route>,
    symbol_table: &SymbolTable,
    eval_context: &hwc_parser::EvaluationContext,
    stackup_manager: &crate::ir::stackup_manager::StackupManager,
    profile: Option<&hwc_parser::ProfileDefinition>,
) -> Result<(), IrError> {
    if auto_routes.is_empty() {
        return Ok(());
    }

    // v0.2.0: PRE-ROUTING VALIDATION CHECKPOINT
    // Verify all routing layers are valid before starting routing
    eprintln!("[VALIDATION] Checking routing layer database...");
    if let Err(errors) = space.routing_layer_db.validate() {
        for err in &errors {
            eprintln!("[VALIDATION] ERROR: {}", err);
        }
        return Err(IrError::RoutingLayerError {
            message: format!("{} routing layer validation errors", errors.len()),
            hint: "Fix the stackup definition to ensure all routable layers have valid Z ranges.".into(),
        });
    }
    eprintln!(
        "[VALIDATION] Routing layer database OK: {} routable layers",
        space.routing_layer_db.routable_layer_count()
    );

    // Verify all vias have connection points
    eprintln!("[VALIDATION] Checking via connection points...");
    let routing_z_map = space.routing_layer_db.routing_z_map();
    let stackup = &space.stackup_layers;
    if let Err(errors) = space.layer_connection_db.validate(&routing_z_map, stackup) {
        for err in &errors {
            eprintln!("[VALIDATION] WARNING: {}", err);
        }
        eprintln!(
            "[VALIDATION] {} via connection mismatches detected (routing may produce incorrect geometry)",
            errors.len()
        );
    } else {
        eprintln!(
            "[VALIDATION] Via connection database OK: {} connections",
            space.layer_connection_db.connection_count()
        );
    }

    eprintln!(
        "[ROUTER] Processing {} automatic routes using AutoRouter",
        auto_routes.len()
    );
    let mut auto_router = crate::ir::routing::AutoRouter::new(
        space,
        symbol_table,
        eval_context,
        stackup_manager,
        profile,
        rustc_hash::FxHashMap::default(),
        auto_routes,
        rustc_hash::FxHashMap::default(),
    );

    auto_router.route_all_nets()?;

    // v0.2.0: POST-ROUTING VALIDATION CHECKPOINT
    // Verify trace Z elevations are within layer bounds
    eprintln!("[VALIDATION] Post-routing: checking trace Z elevations...");
    let mut post_errors = Vec::new();
    for trace in &space.analytic_routes {
        for seg in &trace.segments {
            if seg.start.z == seg.end.z {
                // Horizontal segment — verify Z is within a valid layer
                let seg_z = seg.start.z;
                let in_valid_layer = space.stackup_layers.iter().any(|l| {
                    l.is_routable && seg_z >= l.z_bottom && seg_z <= l.z_top
                });
                if !in_valid_layer {
                    post_errors.push(format!(
                        "Net '{}': horizontal trace at Z={}nm is not within any routing layer bounds",
                        trace.net_name, seg_z
                    ));
                }
            }
        }
    }

    if !post_errors.is_empty() {
        for err in &post_errors {
            eprintln!("[VALIDATION] ERROR: {}", err);
        }
        return Err(IrError::PostRoutingValidationFailed {
            net: "multiple".into(),
            problem: format!("{} trace segments have Z coordinates outside routing layer bounds", post_errors.len()),
            hint: "This indicates via connection Z mismatches. Check that all vias connect to the correct routing layers.".into(),
        });
    }
    eprintln!("[VALIDATION] Post-routing checks passed");

    Ok(())
}
