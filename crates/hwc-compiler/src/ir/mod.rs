//! IR Integration Module: Transform parser AST into voxel grid representation.
//!
//! This module bridges System 1 (Parser & Compiler) and System 2 (Voxel Engine).
//! It takes the parsed AST with Symbol Table and transforms it into a fully populated
//! voxel grid with placed components and routed traces.
//!
//! ## Module Structure
//! - `errors`: Error types for IR transformation
//! - `conversions`: Unit conversions and coordinate transformations
//! - `space_builder`: Hardware space creation
//! - `placement`: Component and substrate placement
//! - `routing`: Trace routing (automatic and manual)
//! - `logic`: Logic synthesis integration (directly places into HardwareSpace)
//! - `tests`: Integration tests
//!
//! ## Known Limitations (v0.1.7)
//! - **Realization Lag**: Physical boundaries of pours/contacts referencing component anchors may default
//!   to [0,0,0] if the anchor isn't fully realized at the time of evaluation.
//! - **No Collision Avoidance**: Conductive pours can interpenetrate component geometry.
//! - **Voxel Aliasing**: Fixed 1um resolution may cause blockiness at SoC scales.

pub mod bridge_validator;
pub mod conversions;
pub mod errors;
pub mod logic;
pub mod meander_injection; // v0.1.8: Post-route meander injection (two-phase physical synthesis)
pub mod parametric_unroller; // Sprint 3.4: Parametric unrolling
pub mod placement;
pub mod routing;
pub mod space_builder;
pub mod spatial_dependency_graph; // Gap 7: Spatial dependency graph
pub mod stackup_manager;
pub mod units; // Phase 2: P45 Forbidden Junction detection

// Re-export commonly used items
pub use errors::IrError;
pub use routing::route_trace;
pub use space_builder::create_hardware_space;
pub use stackup_manager::StackupManager;
pub use units::{format_distance, format_position_mm, nm_to_mm};

use crate::SymbolTable;
use hwc_engine::HardwareSpace;
use hwc_parser::Program;

/// Compile a single space definition into a HardwareSpace.
///
/// This is the shared implementation used by both `program_to_space` and `program_to_spaces`.
///
/// v0.1.8: Accepts an optional `QueryStore` for memoized per-G-cell routing.
/// Returns the space and the query store (which may have been populated with
/// cached results for incremental rebuilds).
fn compile_single_space(
    space_def: &hwc_parser::SpaceDefinition,
    symbol_table: &SymbolTable,
    collector: &hwc_diagnostics::DiagnosticCollector,
    lockfile_path: Option<&std::path::Path>,
    source_content: Option<&str>,
    force_reroute: bool,
    query_store: Option<hwc_engine::geometry_router::query_engine::QueryStore>,
) -> Result<(HardwareSpace, Option<hwc_engine::geometry_router::query_engine::QueryStore>), IrError> {
    // Sprint 3.8: Process statements in textual order to support anchor references
    //
    // Physical Reality: When element B references element A's position, A must be placed first.
    // The old approach (place all pours, then all components) breaks this dependency.
    //
    // New approach: Unroll loops inline and maintain textual order for placement.
    // eprintln!($3"[DEBUG program_to_space] Processing {} statements in textual order...", space_def.statements.len());

    // Unroll all statements while preserving textual order
    #[derive(Debug, Clone)]
    enum PlacementItem {
        Substrate(hwc_parser::SubstratePlacement),
        Component(Box<hwc_parser::ComponentPlacement>),
        Pour(hwc_parser::PourPlacement),
        Plane(hwc_parser::PlanePlacement),
        Contact(hwc_parser::ContactPlacement),
        Route(hwc_parser::Route),
    }

    let mut placement_items = Vec::new();

    for statement in space_def.statements.iter() {
        match statement {
            hwc_parser::SpaceTopLevelStatement::Substrate(sub) => {
                placement_items.push(PlacementItem::Substrate(sub.clone()));
            }
            hwc_parser::SpaceTopLevelStatement::Component(comp) => {
                // eprintln!($3"[DEBUG program_to_space]   Statement {}/{}: Component '{}'",
                // i + 1, space_def.statements.len(),
                // comp.name.as_ref().map(|n| n.to_string()).unwrap_or_else(|| comp.component_type.to_string().into()));
                placement_items.push(PlacementItem::Component(Box::new((**comp).clone())));
            }
            hwc_parser::SpaceTopLevelStatement::Pour(pour) => {
                // eprintln!($3"[DEBUG program_to_space]   Statement {}/{}: Pour '{}'",
                // i + 1, space_def.statements.len(), pour.name);
                placement_items.push(PlacementItem::Pour(pour.clone()));
            }
            hwc_parser::SpaceTopLevelStatement::Plane(plane) => {
                placement_items.push(PlacementItem::Plane(plane.clone()));
            }
            hwc_parser::SpaceTopLevelStatement::Contact(contact) => {
                // eprintln!($3"[DEBUG program_to_space]   Statement {}/{}: Contact",
                // i + 1, space_def.statements.len());
                placement_items.push(PlacementItem::Contact(contact.clone()));
            }
            hwc_parser::SpaceTopLevelStatement::ForLoop(for_loop) => {
                // eprintln!($3"[DEBUG program_to_space]   Statement {}/{}: ForLoop '{}' in {}..{}",
                // i + 1, space_def.statements.len(), for_loop.variable, for_loop.start, for_loop.end);
                let unrolled = parametric_unroller::unroll_for_loop(for_loop, symbol_table)?;
                // eprintln!($3"[DEBUG program_to_space]     Unrolled: {} components, {} pours, {} contacts, {} routes",
                // unrolled.components.len(), unrolled.pours.len(), unrolled.contacts.len(), unrolled.routes.len());

                // Add unrolled items in order
                for comp in unrolled.components {
                    placement_items.push(PlacementItem::Component(Box::new(comp)));
                }
                for pour in unrolled.pours {
                    placement_items.push(PlacementItem::Pour(pour));
                }
                for plane in unrolled.planes {
                    placement_items.push(PlacementItem::Plane(plane));
                }
                for contact in unrolled.contacts {
                    placement_items.push(PlacementItem::Contact(contact));
                }
                for route in unrolled.routes {
                    placement_items.push(PlacementItem::Route(route));
                }
            }
            hwc_parser::SpaceTopLevelStatement::Route(route) => {
                // eprintln!($3"[DEBUG program_to_space]   Statement {}/{}: Route",
                // i + 1, space_def.statements.len());
                placement_items.push(PlacementItem::Route(route.clone()));
            }
            hwc_parser::SpaceTopLevelStatement::Polygon(_)
            | hwc_parser::SpaceTopLevelStatement::Expose(_)
            | hwc_parser::SpaceTopLevelStatement::RouteNetPolicy(_) => {
                // RouteNetPolicy: stored but not yet wired to engine (v0.1.8)
                // These don't affect placement order
            }
        }
    }

    // Count items by type for logging
    let _component_count = placement_items
        .iter()
        .filter(|i| matches!(i, PlacementItem::Component(_)))
        .count();
    let _pour_count = placement_items
        .iter()
        .filter(|i| matches!(i, PlacementItem::Pour(_)))
        .count();
    let _contact_count = placement_items
        .iter()
        .filter(|i| matches!(i, PlacementItem::Contact(_)))
        .count();
    let _route_count = placement_items
        .iter()
        .filter(|i| matches!(i, PlacementItem::Route(_)))
        .count();

    // eprintln!($3"[DEBUG program_to_space] Statement processing complete: {} total components, {} total pours, {} total contacts, {} total routes",
    // component_count, pour_count, contact_count, route_count);

    // eprintln!($3"[DEBUG program_to_space] Creating hardware space...");
    let mut space = create_hardware_space(space_def, symbol_table)?;
    // eprintln!($3"[DEBUG program_to_space] Hardware space created");

    let origin = space_def.origin.unwrap_or_default();

    // Resolve profile and extract solder mask thickness (library-driven, not hardcoded)
    let profile = space_def
        .profile
        .as_ref()
        .and_then(|p| symbol_table.get_profile(p.as_str()).ok());

    let solder_mask_thickness_nm = profile
        .as_ref()
        .and_then(|p| p.manufacturing.as_ref())
        .and_then(|m| m.solder_mask_thickness.as_ref())
        .map(|t| crate::ir::conversions::measurement_to_nm(t, symbol_table))
        .unwrap_or(0); // Opt-in: 0 = disabled unless profile explicitly declares solder_mask_thickness

    let stackup_manager = crate::ir::stackup_manager::StackupManager::new(
        profile.as_ref().and_then(|prof| prof.stackup.as_ref()),
        symbol_table,
        space.voxel_size.z_nm,
        origin.z,
        solder_mask_thickness_nm,
    )
    .unwrap_or_else(|_| {
        // Fallback for pure Assembly mode or missing profile
        crate::ir::stackup_manager::StackupManager::new(
            None,
            symbol_table,
            space.voxel_size.z_nm,
            origin.z,
            solder_mask_thickness_nm,
        )
        .expect("Failed to create fallback StackupManager")
    });
    // eprintln!($3"[DEBUG program_to_space] Origin: {:?}", origin);

    // Sprint 3, Task 3.1: Initialize BoundingBoxTracker for relative positioning
    let mut bbox_tracker = crate::bounding_box_tracker::BoundingBoxTracker::new();
    // eprintln!($3"[DEBUG program_to_space] BoundingBoxTracker initialized");

    // v0.1.6 UNIVERSAL CONTEXT: Build the evaluation context ONCE for the entire space
    // This contains all constants (PI, e, etc.) and eliminates the "Initialization Storm"
    // where we were rebuilding this dictionary 24+ times for just 8 components.
    let eval_context = crate::constraint_solver::ConstraintSolver::build_eval_context(symbol_table);
    // eprintln!($3"[DEBUG program_to_space] Universal evaluation context built ({} constants)",
    // symbol_table.get_all_constants().len());

    // Get profile definition if specified in space
    let profile = space_def
        .profile
        .as_ref()
        .and_then(|profile_name| symbol_table.get_profile(profile_name.as_str()).ok());

    // v0.1.7: Substrates are now part of the unified statement stream.
    // This allows multi-layer dielectric stacks (Silicon/Gold/Silicon, etc.)
    // and preserves textual order for 'last' keyword support.

    // Sprint 3.8 / Gap 7: Topological Sorting of Placement Items
    //
    // Physical Reality: When element B references element A's position, A must be placed first.
    // In v0.1.5, this required strict textual order.
    // In v0.1.6 Gap 7, we build a dependency graph and sort them, allowing forward references.

    let mut graph = spatial_dependency_graph::SpatialDependencyGraph::new();
    let mut item_map = rustc_hash::FxHashMap::default();
    let mut last_component_name: Option<compact_str::CompactString> = None;

    // 1. Build the dependency graph and item map (Pass 1: Register all items)
    for (i, item) in placement_items.iter().enumerate() {
        let item_id = match item {
            PlacementItem::Substrate(_) => format!("__substrate_{}", i).into(),
            PlacementItem::Component(c) => c
                .name
                .as_ref()
                .map(|n| n.to_string())
                .unwrap_or_else(|| format!("__comp_{}", i).into()),
            PlacementItem::Pour(p) => p.name.to_string(),
            PlacementItem::Plane(p) => p.name.to_string(),
            PlacementItem::Contact(c) => c
                .name
                .as_ref()
                .map(|n| n.to_string())
                .unwrap_or_else(|| format!("__contact_{}", i).into()),
            PlacementItem::Route(_) => format!("__route_{}", i).into(),
        };

        graph.add_component(item_id.clone());
        item_map.insert(item_id, item);
    }

    // Pass 2: Extract dependencies now that all components are registered in the graph
    for (i, item) in placement_items.iter().enumerate() {
        let item_id = match item {
            PlacementItem::Substrate(_) => format!("__substrate_{}", i).into(),
            PlacementItem::Component(c) => c
                .name
                .as_ref()
                .map(|n| n.to_string())
                .unwrap_or_else(|| format!("__comp_{}", i).into()),
            PlacementItem::Pour(p) => p.name.to_string(),
            PlacementItem::Plane(p) => p.name.to_string(),
            PlacementItem::Contact(c) => c
                .name
                .as_ref()
                .map(|n| n.to_string())
                .unwrap_or_else(|| format!("__contact_{}", i).into()),
            PlacementItem::Route(_) => format!("__route_{}", i).into(),
        };

        match item {
            PlacementItem::Substrate(s) => {
                graph.extract_dependencies_from_coord(
                    &item_id,
                    &s.from,
                    last_component_name.as_ref(),
                );
                graph.extract_dependencies_from_coord(
                    &item_id,
                    &s.to,
                    last_component_name.as_ref(),
                );
            }
            PlacementItem::Component(c) => {
                graph.extract_dependencies_from_coord(
                    &item_id,
                    &c.position,
                    last_component_name.as_ref(),
                );
                // Update 'last' pointer ONLY for components (as per spec)
                last_component_name = Some(item_id);
            }
            PlacementItem::Pour(p) => {
                if let Some(boundary) = &p.boundary {
                    match boundary {
                        hwc_parser::PourBoundary::Rect(from, to) => {
                            graph.extract_dependencies_from_coord(
                                &item_id,
                                from,
                                last_component_name.as_ref(),
                            );
                            graph.extract_dependencies_from_coord(
                                &item_id,
                                to,
                                last_component_name.as_ref(),
                            );
                        }
                        hwc_parser::PourBoundary::Circle { center, radius } => {
                            graph.extract_dependencies_from_coord(
                                &item_id,
                                center,
                                last_component_name.as_ref(),
                            );
                            graph.extract_dependencies_from_expr(
                                &item_id,
                                radius,
                                last_component_name.as_ref(),
                            );
                        }
                    }
                }
            }
            PlacementItem::Plane(p) => {
                if let Some(from) = &p.from {
                    graph.extract_dependencies_from_coord(
                        &item_id,
                        from,
                        last_component_name.as_ref(),
                    );
                }
                if let Some(to) = &p.to {
                    graph.extract_dependencies_from_coord(
                        &item_id,
                        to,
                        last_component_name.as_ref(),
                    );
                }
            }
            PlacementItem::Contact(c) => {
                graph.extract_dependencies_from_coord(
                    &item_id,
                    &c.position,
                    last_component_name.as_ref(),
                );
            }
            PlacementItem::Route(r) => {
                // Routes depend on the components they connect
                // Must resolve component_index (e.g. J0[0]) to match item_map keys
                let resolve_name = |pr: &hwc_parser::PinReference| -> compact_str::CompactString {
                    match &pr.component_index {
                        Some(idx) => {
                            if let Ok(val) = crate::ir::routing::evaluate_index_expression(idx) {
                                format!("{}[{}]", pr.component, val).into()
                            } else {
                                pr.component.clone()
                            }
                        }
                        None => pr.component.clone(),
                    }
                };
                let from_name = resolve_name(&r.from);
                let to_name = resolve_name(&r.to);
                graph.add_dependency(item_id.clone(), from_name);
                graph.add_dependency(item_id.clone(), to_name);

                // Routes depend on variables used in width
                if let Some(w) = &r.width {
                    graph.extract_dependencies_from_expr(&item_id, w, last_component_name.as_ref());
                }

                // Routes depend on variables used in strategy parameters
                for (_, expr) in &r.strategy_params {
                    graph.extract_dependencies_from_expr(
                        &item_id,
                        expr,
                        last_component_name.as_ref(),
                    );
                }

                // Routes depend on variables used in path coordinates
                if let Some(path) = &r.path {
                    for wp in path {
                        graph.extract_dependencies_from_coord(
                            &item_id,
                            wp,
                            last_component_name.as_ref(),
                        );
                    }
                }
            }
        }
    }

    // 2. Perform topological sort (detects circular dependencies automatically)
    let sorted_ids = graph.topological_sort()?;

    // 3. Two-Pass Realization Engine (v0.1.7 roadmap: fixes "Anchor Realization Lag")
    // Pass 1 places ALL non-route items (substrates, components, pours, contacts).
    // This locks every bounding box with final post-transform (rotation/offset) geometry baked in.
    // Pass 2 (below) places routes/traces ONLY after the BBoxTracker is frozen.
    // Result: pours and traces no longer query stale pre-bake bboxes -> eliminates wedge/stretched artifacts.

    // v0.1.7: Opt-in solder mask generation.
    // Layers are only created when the active profile explicitly declares
    // `manufacturing.solder_mask_thickness`.  When the property is absent the
    // resolved thickness is 0 and the block is skipped entirely, keeping bare-
    // metal / silicon layouts free of implicit insulator geometry.
    if solder_mask_thickness_nm > 0 {
        let width_nm = space.dimensions.width_nm;
        let height_nm = space.dimensions.height_nm;
        let stackup_height_nm = stackup_manager.board_thickness_nm();

        // Check if user already added solder mask layers to prevent duplicates
        let has_solder_mask = space
            .entity_graph
            .get_substrate_layers()
            .iter()
            .any(|l| l.layer_type == hwc_engine::geometry_router::substrate_types::SubstrateLayerType::SolderMask);

        if !has_solder_mask {
            let mask_material_id = space.material_registry.get_id("SolderMask").ok_or_else(|| {
                IrError::UndeclaredMaterial { material: "SolderMask".into() }
            })?;

            // Top solder mask: sits directly ON TOP of the top copper layer
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
                0, // No net (insulator)
                top_mask_bbox,
                hwc_engine::geometry_router::substrate_types::SubstrateLayerType::SolderMask,
            );

            // Bottom solder mask: sits directly UNDERNEATH the bottom copper layer
            let bottom_mask_bbox = hwc_engine::geometry::BoundingBox::new(
                hwc_engine::geometry::Point3D::new(0, 0, -solder_mask_thickness_nm),
                hwc_engine::geometry::Point3D::new(width_nm, height_nm, 0),
            );
            space.entity_graph.add_substrate_layer(
                mask_material_id,
                0, // No net (insulator)
                bottom_mask_bbox,
                hwc_engine::geometry_router::substrate_types::SubstrateLayerType::SolderMask,
            );
        }
    }

    let mut total_placement_time = std::time::Duration::ZERO;
    let mut component_count = 0;

    for id in sorted_ids.iter() {
        let item = item_map.get(id).unwrap();
        let item_start = std::time::Instant::now();

        let place_ctx = placement::context::PlacementContext {
            symbol_table,
            eval_context: &eval_context,
            stackup_manager: &stackup_manager,
            collector,
            profile,
            origin,
        };

        match item {
            PlacementItem::Substrate(sub) => {
                placement::place_substrate(&mut space, sub, &mut bbox_tracker, &place_ctx)?;
            }
            PlacementItem::Pour(pour) => {
                placement::place_pour(&mut space, pour, &mut bbox_tracker, &place_ctx)?;
            }
            PlacementItem::Plane(plane) => {
                placement::place_plane(&mut space, plane, &mut bbox_tracker, &place_ctx)?;
            }
            PlacementItem::Contact(contact) => {
                placement::place_contact(
                    &mut space,
                    contact,
                    origin,
                    symbol_table,
                    &eval_context,
                    &stackup_manager,
                    profile,
                )?;
            }
            PlacementItem::Component(component) => {
                component_count += 1;
                placement::place_component(
                    &mut space,
                    component,
                    &space_def.layouts,
                    &mut bbox_tracker,
                    &place_ctx,
                )?;

                let elapsed = item_start.elapsed();
                total_placement_time += elapsed;
            }
            PlacementItem::Route(_) => {
                continue;
            }
        }
    }

    // Pass 2: Realize traces against the now-complete, frozen BBoxTracker + HardwareSpace component_bboxes.
    // Every component transformation is baked; anchors like last.right or M1.top reflect final rotated geometry.
    //
    // v0.1.7: 3-Phase Routing Engine
    // Phase 1: Manual Realization (process routes with path:)
    // Phase 2: Obstacle Blitting (Implicit - registered components + manual traces)
    // Phase 3: Auto-Routing (Deferred batch process)
    let mut auto_routes = Vec::new();

    // v0.1.8: Collect route net policies from `route net:` statements.
    // These map net names -> RoutingPattern for pattern-guided auto routing.
    let mut route_net_policies: rustc_hash::FxHashMap<hwc_engine::netlist::NetId, hwc_engine::RoutingPattern> =
        rustc_hash::FxHashMap::default();
    for policy in &space_def.route_net_policies() {
        if let Some(ref pattern_inst) = policy.pattern {
            match routing::instantiate_pattern(pattern_inst, symbol_table) {
                Ok(pattern) => {
                    // Resolve net name to NetId
                    if let Some(net_id) = space.netlist.get_net_by_name(policy.net_id.as_str()) {
                        // eprintln!(
                        //     "[ROUTER] Route net policy: '{}' -> pattern '{}' ({} steps)",
                        //     policy.net_id, pattern.name, pattern.steps.len()
                        // );
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
    let routing_mode = space_def
        .routing_config
        .as_ref()
        .map(|c| c.mode)
        .unwrap_or(hwc_parser::RoutingMode::Mixed);

    for id in sorted_ids.iter() {
        if let Some(PlacementItem::Route(route)) = item_map.get(id) {
            // v0.1.7: Register net connectivity in the netlist for all routes
            // This ensures both manual and automatic routes are represented in the logical netlist.
            routing::register_net_for_route(&mut space, route, symbol_table)?;

            if !routing::needs_automatic_routing(route) {
                // Phase 1: Manual Route (Absolute Control)
                routing::route_trace(
                    &mut space,
                    route,
                    origin,
                    symbol_table,
                    &eval_context,
                    &stackup_manager,
                    profile,
                )?;
            } else {
                // Phase 3 Candidate: Automatic or Patterned Route
                if routing_mode == hwc_parser::RoutingMode::ManualOnly {
                    return Err(IrError::RoutingError(format!(
                        "Automatic routing found in 'manual_only' space: {}.{} to {}.{}",
                        route.from.component, route.from.pin, route.to.component, route.to.pin
                    )));
                }
                auto_routes.push(route.clone());
            }
        }
    }

    // Phase 3: Execute Auto-Routing Batch
    // v0.1.7: AVS Lock-File Determinism - Load cached routes before routing
    // Priority: try new rkyv binary lockfile first, then fall back to old JSON lockfile
    let mut routes_loaded_from_lock = false;
    if let Some(path) = lockfile_path {
        if force_reroute {
            eprintln!(
                "[LOCK] --force-reroute: Skipping lockfile load, will re-run A* solver"
            );
        } else {
            let current_fingerprint = hwc_engine::geometry_router::compute_fingerprint_from_space(&space);

            // Try new rkyv binary lockfile first
            match hwc_engine::geometry_router::load_lockfile(path) {
                Ok(loaded) => {
                    if hwc_engine::geometry_router::is_valid(&loaded, &current_fingerprint) {
                        let copper_id = space.material_registry.get_id("Copper").ok_or_else(|| {
                            IrError::UndeclaredMaterial { material: "Copper".into() }
                        })?;
                        let layer_z_map = hwc_engine::geometry_router::build_layer_z_map(
                            &space.entity_graph,
                        );
                        let cached_traces = hwc_engine::geometry_router::lockfile_to_traces(
                            &loaded,
                            copper_id,
                            &space.netlist,
                            &layer_z_map,
                        );
                        let route_count = cached_traces.len();
                        for trace in cached_traces {
                            space.add_analytic_route(trace);
                        }
                        eprintln!(
                            "[LOCK] Binary match found for '{}'. Bypassing A* solver.",
                            space.name
                        );
                        eprintln!(
                            "[LOCK]   Loaded {} cached routes (rkyv binary)",
                            route_count
                        );
                        routes_loaded_from_lock = true;
                    } else {
                        eprintln!(
                            "[LOCK] Binary lockfile fingerprint mismatch for '{}'. Re-routing.",
                            space.name
                        );
                    }
                }
                Err(_) => {
                    // Binary lockfile doesn't exist or is corrupt — try legacy JSON
                    match hwc_engine::geometry_router::CompactLockfile::load(path) {
                        Ok(lockfile) => {
                            let current_hash = hwc_engine::geometry_router::compute_placement_hash(&space);
                            if lockfile.validate_placement(&current_hash) && !lockfile.instances.is_empty() {
                                let copper_id = space.material_registry.get_id("Copper").ok_or_else(|| {
                                    IrError::UndeclaredMaterial { material: "Copper".into() }
                                })?;
                                let cached_traces = lockfile.to_analytic_traces(copper_id, &space.netlist);
                                let route_count = cached_traces.len();
                                for trace in cached_traces {
                                    space.add_analytic_route(trace);
                                }
                                eprintln!(
                                    "[LOCK] Legacy match for '{}'. Bypassing A* solver. Loading routes from JSON lockfile.",
                                    space.name
                                );
                                eprintln!(
                                    "[LOCK]   Loaded {} cached routes (legacy JSON)",
                                    route_count
                                );
                                routes_loaded_from_lock = true;
                            } else if !lockfile.validate_placement(&current_hash) {
                                eprintln!(
                                    "[LOCK] Invalidation detected for '{}'. Discarding lock file and re-running A* pathfinder",
                                    space.name
                                );
                            }
                        }
                        Err(hwc_engine::geometry_router::LockfileError::ObsoleteVersion(ver)) => {
                            eprintln!(
                                "[LOCK] Obsolete lockfile detected (version {}). Delete the .routes.lock file and rebuild.",
                                ver
                            );
                        }
                        Err(hwc_engine::geometry_router::LockfileError::ParseError(e)) => {
                            eprintln!(
                                "[LOCK] Failed to parse lockfile: {}. Delete the .routes.lock file and rebuild.",
                                e
                            );
                        }
                        Err(hwc_engine::geometry_router::LockfileError::IoError(_)) => {
                            // No lockfile exists — first-time compilation
                        }
                    }
                }
            }
        }
    }

    // v0.1.8: Initialize the memoized query store for per-G-cell routing cache.
    let mut qs = query_store.unwrap_or_else(|| {
        hwc_engine::geometry_router::query_engine::QueryStore::new()
    });

    let has_unrouted_nets = space.netlist.num_nets() > 0;

    if (!auto_routes.is_empty() || has_unrouted_nets) && !routes_loaded_from_lock {
        // v0.1.7: Build net frequencies HashMap from space definition's net declarations
        let mut net_frequencies: rustc_hash::FxHashMap<hwc_engine::netlist::NetId, f64> =
            rustc_hash::FxHashMap::default();
        for net_decl in &space_def.nets {
            if let Some(freq_hz) = net_decl.frequency_hz {
                if let Some(net_id) = space.netlist.get_net_by_name(&net_decl.name) {
                    net_frequencies.insert(net_id, freq_hz);
                }
            }
        }

        let mut auto_router = routing::AutoRouter::new(
            &mut space,
            symbol_table,
            &stackup_manager,
            profile,
            net_frequencies,
            auto_routes,
            route_net_policies,
        );

        // v0.1.8: Wire the memoized query store into the AutoRouter.
        auto_router.set_query_store(qs);

        auto_router.route_all_nets()?;

        // v0.1.8: Retrieve the query store back for the caller to persist.
        qs = auto_router.take_query_store().unwrap_or_else(|| {
            hwc_engine::geometry_router::query_engine::QueryStore::new()
        });
    }

    // v0.1.7: Lock-File Determinism - Save routes to lockfile after routing
    // This runs whether routes were loaded from cache or freshly computed
    if let (Some(path), Some(content)) = (lockfile_path, source_content) {
        save_routes_to_lockfile(path, &space, content);
    }

    // v0.1.7: Synchronize net names from pins to bound pours
    // This ensures that internal component pours (pads/rings) inherit the nets
    // assigned during the routing phase above.
    space.synchronize_nets();

    if component_count > 0 {
        // eprintln!($3"[DEBUG program_to_space] All components placed (avg: {:.6}ms/component)",
        // total_placement_time.as_secs_f64() * 1000.0 / component_count as f64);
    }

    // Phase 2: P45 Forbidden Junction Detection (Assembly Level)
    // Validate manually placed contacts against the profile's bridge rules
    bridge_validator::validate_bridges(&space, profile)?;

    // Sprint 3.3: Automatic Via Insertion
    // Run after all manual placements to detect layer transitions
    // eprintln!($3"[DEBUG program_to_space] Running automatic via insertion...");
    let _auto_via_start = std::time::Instant::now();

    // Load fabrication constraints from profile for AutoViaInserter (v0.1.7 Limitation 7)
    let fab_constraints = space_def.profile.as_ref().and_then(|profile_name| {
        hwc_engine::constraint_manager::load_fabrication_constraints(
            profile_name.as_str(),
            symbol_table,
        )
        .ok()
    });

    let auto_via_inserter = crate::auto_via_inserter::AutoViaInserter::from_profile(
        profile,
        &stackup_manager,
        fab_constraints.as_ref(),
        Some(symbol_table),
    );

    match auto_via_inserter.insert_vias(&space, profile, &stackup_manager) {
        Ok(auto_vias) => {
            // eprintln!($3"[DEBUG program_to_space] Auto via insertion complete: {} vias inserted in {:?}",
            //     auto_vias.len(),
            //     auto_via_start.elapsed()
            // );
            // Place the auto-inserted vias
            for via in &auto_vias {
                placement::place_contact(
                    &mut space,
                    via,
                    origin,
                    symbol_table,
                    &eval_context,
                    &stackup_manager,
                    profile,
                )?;
            }
        }
        Err(_e) => {
            // eprintln!($3"[DEBUG program_to_space] ⚠️  Auto via insertion failed: {}",
            //     e
            // );
            // Non-fatal: Continue without auto vias
        }
    }

    // Commit all placements and routes to the visible plane
    // This makes substrate, pours, components, and routes visible to exporters
    let _commit_start = std::time::Instant::now();
    let _stats_before = space.entity_graph.memory_stats();
    let _commit_start2 = std::time::Instant::now();
    space.entity_graph.commit_route();
    // eprintln!($3"[DEBUG program_to_space] commit_route() took {:?}", commit_start2.elapsed());
    let _stats_after = space.entity_graph.memory_stats();
    // eprintln!($3"[DEBUG program_to_space] After commit: {} occupied voxels",
    //     stats_after.occupied_voxels
    // );

    // v0.1.7 DFM: Dummy metal fill (thieving) for manufacturing density balance
    {
        let dummy_fill_config = hwc_engine::DummyFillConfig {
            enabled: true,
            target_density_pct: 45,
            ..hwc_engine::DummyFillConfig::default()
        };
        let mut dummy_fill_engine = hwc_engine::DummyFillEngine::new();
        let fill_stats = dummy_fill_engine.run(&space.entity_graph, &dummy_fill_config);
        if fill_stats.zones_filled > 0 {
            eprintln!(
                "[DFM] Dummy fill: {} zones analyzed, {} zones filled, {} dummies placed (avg density before: {:.1}%)",
                fill_stats.zones_analyzed,
                fill_stats.zones_filled,
                fill_stats.total_dummies_placed,
                fill_stats.average_density_before,
            );
        }
    }

    // SPARSE-VOXEL HANDSHAKE: The three-step lookup in get_material() handles
    // substrate layers efficiently without syncing to voxels. No validation needed.
    // The handshake works: voxels → substrate_layers → default_insulator

    // Sprint 9 (Task 9.1): PLACEMENT GATE
    // This is the "rustc model": collect up to N errors, then stop.
    if collector.has_errors() {
        let n = collector.error_count();
        return Err(IrError::CompilationError(format!(
            "aborting due to {} previous error{}",
            n,
            if n == 1 { "" } else { "s" }
        )));
    }

    Ok((space, Some(qs)))
}

/// Transform a parsed program into a hardware space with voxel grid.
///
/// This is the main entry point for IR integration.
pub fn program_to_space(
    program: &Program,
    symbol_table: &SymbolTable,
    collector: &hwc_diagnostics::DiagnosticCollector,
) -> Result<HardwareSpace, IrError> {
    let space_def = program
        .definitions
        .iter()
        .find_map(|def| {
            if let hwc_parser::Definition::Space(space) = def {
                Some(space)
            } else {
                None
            }
        })
        .ok_or(IrError::NoSpaceDefinition)?;

    let (space, _qs) = compile_single_space(space_def, symbol_table, collector, None, None, false, None)?;
    Ok(space)
}

/// Transform a parsed program into multiple hardware spaces (one per space definition).
///
/// Returns a HashMap keyed by space name. Each space is compiled independently.
pub fn program_to_spaces(
    program: &Program,
    symbol_table: &SymbolTable,
    collector: &hwc_diagnostics::DiagnosticCollector,
) -> Result<rustc_hash::FxHashMap<compact_str::CompactString, HardwareSpace>, IrError> {
    program_to_spaces_with_lockfile(program, symbol_table, collector, None, None, false)
}

/// Compile all space definitions into HardwareSpaces with lockfile support.
///
/// This is the extended version that supports route lockfiles for deterministic builds.
pub fn program_to_spaces_with_lockfile(
    program: &Program,
    symbol_table: &SymbolTable,
    collector: &hwc_diagnostics::DiagnosticCollector,
    lockfile_path: Option<&std::path::Path>,
    source_content: Option<&str>,
    force_reroute: bool,
) -> Result<rustc_hash::FxHashMap<compact_str::CompactString, HardwareSpace>, IrError> {
    let space_defs: Vec<&hwc_parser::SpaceDefinition> = program
        .definitions
        .iter()
        .filter_map(|def| {
            if let hwc_parser::Definition::Space(space) = def {
                Some(space)
            } else {
                None
            }
        })
        .collect();

    if space_defs.is_empty() {
        return Err(IrError::NoSpaceDefinition);
    }

    let mut spaces = rustc_hash::FxHashMap::default();
    // v0.1.8: Share a single QueryStore across all space compilations for
    // cross-space memoization (e.g., shared G-cells between adjacent spaces).
    let mut shared_qs: Option<hwc_engine::geometry_router::query_engine::QueryStore> = None;

    for space_def in space_defs {
        let space_name: compact_str::CompactString = space_def.name.to_string().into();
        let (space, qs) = compile_single_space(
            space_def,
            symbol_table,
            collector,
            lockfile_path,
            source_content,
            force_reroute,
            shared_qs.take(),
        )?;
        shared_qs = qs;
        spaces.insert(space_name, space);
    }

    Ok(spaces)
}

/// Save routes from a HardwareSpace to a rkyv binary lockfile (v0.1.7).
fn save_routes_to_lockfile(
    path: &std::path::Path,
    space: &HardwareSpace,
    _source_content: &str,
) {
    let fingerprint = hwc_engine::geometry_router::compute_fingerprint_from_space(space);
    let binary_lockfile = hwc_engine::geometry_router::traces_to_lockfile(space, fingerprint);

    let route_count = binary_lockfile.arcs.len();

    if let Err(e) = hwc_engine::geometry_router::write_lockfile(&binary_lockfile, path) {
        eprintln!("[LOCK] Failed to save binary lockfile: {}", e);
    } else {
        eprintln!(
            "[LOCK] Saved {} arc segments to {} (rkyv binary)",
            route_count,
            path.display()
        );
    }
}


