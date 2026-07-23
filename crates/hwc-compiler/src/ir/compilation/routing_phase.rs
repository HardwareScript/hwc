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
            let layer_z_map = hwc_engine::geometry_router::build_layer_z_map(&space.entity_graph);
            match hwc_engine::geometry_router::lockfile_to_traces(
                &loaded,
                &space.netlist,
                &layer_z_map,
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
                        space.add_analytic_route(trace);
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

    eprintln!(
        "[DEBUG] Starting route processing, entity count: {}",
        space.entity_graph.iter_entity_ids().count()
    );

    for id in ctx.sorted_ids.iter() {
        let &item_idx = ctx.item_map.get(id).unwrap();
        let item = &ctx.placement_items[item_idx];

        if let PlacementItem::Route(route) = item {
            eprintln!(
                "[DEBUG] Processing route: {} to {}",
                crate::ir::routing::helpers::endpoint_label(&route.from),
                crate::ir::routing::helpers::endpoint_label(&route.to)
            );

            crate::ir::routing::register_net_for_route(
                space,
                route,
                ctx.symbol_table,
                ctx.eval_context,
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
                    ctx.eval_context,
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
    eprintln!("[ROUTER] Automatic routing complete");
    Ok(())
}
