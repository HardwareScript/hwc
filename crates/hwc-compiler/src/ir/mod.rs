//! IR Integration Module: Transform parser AST into continuous physical representation.
//!
//! This module bridges System 1 (Parser & Compiler) and System 2 (Continuous Engine).
//! It takes the parsed AST with Symbol Table and transforms it into a fully populated
//! entity graph with placed components and routed traces.
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
//! - **Analytic Complexity**: Very large designs may require spatial index optimization.

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
/// Returns the space, the query store (which may have been populated with
/// cached results for incremental rebuilds), and a boolean indicating whether
/// routes were loaded from the lockfile cache.
fn compile_single_space(
    space_def: &hwc_parser::SpaceDefinition,
    symbol_table: &SymbolTable,
    collector: &hwc_diagnostics::DiagnosticCollector,
    lockfile_path: Option<&std::path::Path>,
    _source_content: Option<&str>,
    force_reroute: bool,
    query_store: Option<hwc_engine::geometry_router::query_engine::QueryStore>,
) -> Result<(HardwareSpace, Option<hwc_engine::geometry_router::query_engine::QueryStore>, bool), IrError> {
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

    // v0.1.8: No Implicit Defaults rule for ASIC builds.
    // Under ASIC technology, every route width, material property, and physical
    // constraint must be explicitly declared. If missing, halt with a clear error
    // instead of silently falling back to PCB-scale defaults.
    space_builder::validate_asic_constraints(space_def, symbol_table)?;

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
        .and_then(|t| crate::ir::conversions::measurement_to_nm(t, symbol_table).ok())
        .unwrap_or(0); // Opt-in: 0 = disabled unless profile explicitly declares solder_mask_thickness

    let stackup_manager = crate::ir::stackup_manager::StackupManager::new(
        profile.as_ref().and_then(|prof| prof.stackup.as_ref()),
        symbol_table,
        space.resolution_nm,
        origin.z,
        solder_mask_thickness_nm,
    )
    .unwrap_or_else(|_| {
        // Fallback for pure Assembly mode or missing profile
        crate::ir::stackup_manager::StackupManager::new(
            None,
            symbol_table,
            space.resolution_nm,
            origin.z,
            solder_mask_thickness_nm,
        )
        .expect("Failed to create fallback StackupManager")
    });

    // v0.1.9: Write stackup layer thicknesses into MaterialRegistry.
    // This is the authoritative source for conductor thickness — the material
    // definition declares resistivity/thermal conductivity, and the stackup
    // declares how thick each layer is. Both are needed for physics calculations.
    if let Some(stackup) = profile.as_ref().and_then(|p| p.stackup.as_ref()) {
        for layer in &stackup.layers {
            if let Ok(thickness_nm) = crate::ir::conversions::evaluate_expression_to_nm(&layer.thickness, symbol_table) {
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

    // Pass 1.5: Static Geometry Guard (Fail-Fast Before Routing)
    // Detects coplanar short circuits between different-net conductors
    // before the expensive A* routing phase begins.
    {
        let guard_violations = hwc_engine::geometry_router::check_static_shorts(
            &space.entity_graph,
            &space.netlist,
        );
        if !guard_violations.is_empty() {
            for v in &guard_violations {
                eprintln!("[STATIC GUARD] P42: {}", v);
            }
            return Err(IrError::StaticGeometryShort {
                net_a: guard_violations[0].net_a.clone(),
                net_b: guard_violations[0].net_b.clone(),
                x_nm: guard_violations[0].bbox.min.x,
                y_nm: guard_violations[0].bbox.min.y,
                z_nm: guard_violations[0].bbox.min.z,
            });
        }
    }

    // Pass 2: Realize traces against the now-complete, frozen BBoxTracker + HardwareSpace component_bboxes.
    // Every component transformation is baked; anchors like last.right or M1.top reflect final rotated geometry.
    //
    // v0.1.7: 3-Phase Routing Engine
    // Phase 0: Lockfile Check (if valid, skip all routing)
    // Phase 1: Manual Realization (process routes with path:)
    // Phase 2: Obstacle Blitting (Implicit - registered components + manual traces)
    // Phase 3: Auto-Routing (Deferred batch process)
    
    // Phase 0: Try to load routes from lockfile BEFORE any routing
    let mut routes_loaded_from_lock = false;
    if let Some(path) = lockfile_path {
        if !force_reroute {
            let current_fingerprint = hwc_engine::geometry_router::compute_fingerprint_from_space(&space);

            match hwc_engine::geometry_router::load_lockfile(path) {
                Ok(loaded) => {
                    if hwc_engine::geometry_router::is_valid(&loaded, &current_fingerprint) {
                        let layer_z_map = hwc_engine::geometry_router::build_layer_z_map(
                            &space.entity_graph,
                        );
                        match hwc_engine::geometry_router::lockfile_to_traces(
                            &loaded,
                            &space.netlist,
                            &layer_z_map,
                            &space.material_registry,
                        ) {
                            Ok(cached_traces) => {
                                // Load cached routes into entity graph and analytic routes
                                for trace in cached_traces {
                                    let trace_segments: Vec<hwc_engine::geometry::TraceSegment> = trace
                                        .segments
                                        .iter()
                                        .map(|line_seg| {
                                            hwc_engine::geometry::TraceSegment::new(
                                                line_seg.start,
                                                line_seg.end,
                                                trace.width_nm,
                                                trace.material as u8,
                                            )
                                        })
                                        .collect();
                                    
                                    space.entity_graph.register_trace_segments(trace.net_id, trace_segments);
                                    space.add_analytic_route(trace);
                                }
                                
                                space.entity_graph.rebuild_spatial_index(&space.material_registry);
                                
                                eprintln!(
                                    "[LOCK] Valid lockfile loaded for '{}'. Skipping all routing (manual + auto).",
                                    space.name
                                );
                                routes_loaded_from_lock = true;
                            }
                            Err(e) => {
                                eprintln!("[LOCK] Lockfile load failed: {}. Will compute routes fresh.", e);
                            }
                        }
                    } else {
                        eprintln!(
                            "[LOCK] Lockfile fingerprint mismatch for '{}'. Will compute routes fresh.",
                            space.name
                        );
                    }
                }
                Err(_) => {
                    // No lockfile exists - will compute routes
                }
            }
        } else {
            eprintln!(
                "[LOCK] --force-reroute: Skipping lockfile load, will compute all routes fresh"
            );
        }
    }
    
    let mut auto_routes = Vec::new();

    // v0.1.8: Collect route net policies from `route net:` statements.
    // These map net names -> RoutingPattern for pattern-guided auto routing.
    let mut route_net_policies: rustc_hash::FxHashMap<hwc_engine::netlist::NetId, hwc_engine::RoutingPattern> =
        rustc_hash::FxHashMap::default();

    // Only process routing if lockfile wasn't loaded
    if !routes_loaded_from_lock {
        for policy in &space_def.route_net_policies() {
            if let Some(ref pattern_inst) = policy.pattern {
                match routing::instantiate_pattern(pattern_inst, symbol_table) {
                    Ok(pattern) => {
                        // Resolve net name to NetId
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
    }
    let routing_mode = space_def
        .routing_config
        .as_ref()
        .map(|c| c.mode)
        .unwrap_or(hwc_parser::RoutingMode::Mixed);

    // Only process routes if lockfile wasn't loaded
    if !routes_loaded_from_lock {
        for id in sorted_ids.iter() {
            if let Some(PlacementItem::Route(route)) = item_map.get(id) {
                // v0.1.7: Register net connectivity in the netlist for all routes
                // This ensures both manual and automatic routes are represented in the logical netlist.
                routing::register_net_for_route(&mut space, route, symbol_table, &stackup_manager, profile)?;

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

    // v0.1.7: Synchronize net names from pins to bound pours
    // This ensures that internal component pours (pads/rings) inherit the nets
    // assigned during the routing phase above.
    space.synchronize_nets();

    // v0.1.8: G-Cell Sweep DRC — P45 Forbidden Junction Detection
    // Run the unified sweep-line DRC engine after routing to catch
    // same-net different-material intersections (Copper-on-Silicon),
    // clearance violations, and forbidden junctions.
    {
        use hwc_engine::geometry_router::gcell_sweep::verify_gcell_sweep;
        use hwc_engine::geometry_router::partition::PartitionGrid;
        use hwc_engine::geometry_router::spatial_index::{DynamicSpatialIndex, IndexedSegment, SpatialEntitySource};
        use hwc_engine::material::MaterialId;

        // Build layer_to_material map from the stackup definition
        let mut layer_to_material: rustc_hash::FxHashMap<i64, MaterialId> = rustc_hash::FxHashMap::default();
                if let Some(stackup_def) = profile.and_then(|p| p.stackup.as_ref()) {
            for (idx, layer) in stackup_def.layers.iter().enumerate() {
                let material_name = &layer.material;
                if let Some(mat_id) = space.material_registry.get_id(material_name.as_str()) {
                    layer_to_material.insert(idx as i64, mat_id);
                }
            }
        }

        // Build BridgeTable from profile bridge rules
        // The engine's BridgeTable is FxHashMap<CompactString, CompactString>
        // mapping "MatA:MatB" -> bridge_material_name
        let mut bridge_table: rustc_hash::FxHashMap<compact_str::CompactString, compact_str::CompactString> = rustc_hash::FxHashMap::default();
        if let Some(profile_def) = profile {
            for bridge_rule in &profile_def.bridges {
                let key: compact_str::CompactString = format!("{}:{}", bridge_rule.from, bridge_rule.to).into();
                let value = bridge_rule.interface_material.clone();
                bridge_table.insert(key, value);
            }
        }

        // Build DynamicSpatialIndex from committed route segments
        let mut spatial_index = DynamicSpatialIndex::new();
        let mut seg_id = 0usize;
        for (net_id, segments) in space.entity_graph.get_all_routes() {
            for seg in segments {
                // v0.1.8: Resolve the physical layer ID from the segment's Z-position.
                // This is CRITICAL for the G-Cell sweep DRC. If all segments default
                // to Layer 0, the 2D sweep-line will falsely detect short circuits
                // between traces on different vertical layers (e.g. M1 and M2).
                let mid_z = (seg.start.z + seg.end.z) / 2;
                let layer_id = stackup_manager
                    .get_layer_index_at_z(mid_z)
                    .map(|idx| idx as i64)
                    .unwrap_or(0);

                let thickness_nm = stackup_manager
                    .get_thickness_for_layer_index(layer_id as usize)
                    .unwrap_or(0);
                spatial_index.insert(IndexedSegment::new(
                    SpatialEntitySource::RouteSegment {
                        net_idx: net_id.0 as usize,
                        seg_idx: seg_id,
                    },
                    seg_id,
                    net_id.0 as usize,
                    seg,
                    layer_id,
                    thickness_nm,
                ));
                seg_id += 1;
            }
        }

        // Build PartitionGrid for DRC sweep
        let grid_bbox = hwc_engine::geometry::BoundingBox::new(
            hwc_engine::geometry::Point3D::new(0, 0, 0),
            hwc_engine::geometry::Point3D::new(
                space.dimensions.width_nm,
                space.dimensions.height_nm,
                space.dimensions.depth_nm,
            ),
        );
        let cell_size_nm = 10_000_000; // 10mm G-cells
        let track_pitch = space.resolution_nm;
        let max_clearance = space.fabrication_constraints.as_ref()
            .map(|c| c.trace.min_width_nm)
            .unwrap_or(200_000);
        let partition_grid = PartitionGrid::new(grid_bbox, cell_size_nm, cell_size_nm, track_pitch, max_clearance);

        // Run the G-Cell sweep DRC
        let default_clearance_nm = space.fabrication_constraints.as_ref()
            .map(|c| c.trace.min_width_nm)
            .unwrap_or(200_000);
        let violations = verify_gcell_sweep(
            &partition_grid,
            &spatial_index,
            &[], // No virtual junctions for now
            default_clearance_nm,
            &layer_to_material,
            &space.material_registry,
            &bridge_table,
        );

        if !violations.is_empty() {
            eprintln!("[DRC] G-Cell sweep found {} violations:", violations.len());
            for v in &violations {
                eprintln!("  - {:?}", v);
            }
            // For now, log violations as warnings. In the future, these should
            // be converted to IrError and halt compilation.
        }
    }

    if component_count > 0 {
        // eprintln!($3"[DEBUG program_to_space] All components placed (avg: {:.6}ms/component)",
        // total_placement_time.as_secs_f64() * 1000.0 / component_count as f64);
    }

    // Phase 2: P45 Forbidden Junction Detection (Assembly Level)
    // Validate manually placed contacts against the profile's bridge rules
    bridge_validator::validate_bridges(&space, profile)?;

    // v0.1.7: Synchronize net names from pins to bound pours
    // This ensures that internal component pours (pads/rings) inherit the nets
    // assigned during the routing phase above.
    space.synchronize_nets();

    // Sprint 3.3: Native Via Resolution (v0.1.8)
    // Replaces legacy AutoViaInserter with data-driven ViaResolver.
    // Run after net synchronization to ensure elements have correct nets.
    {
        let _resolver_start = std::time::Instant::now();
        let via_resolver = crate::via_resolver::ViaResolver::from_profile(
            profile,
            &stackup_manager,
            symbol_table,
        )?;
        via_resolver.resolve_connectivity(&mut space, &stackup_manager)?;
    }

    // Commit all placements and routes to the visible plane
    // This makes substrate, pours, components, and routes visible to exporters
    let _commit_start = std::time::Instant::now();
    // commit_route() is gone in v0.1.8

    // v0.1.8: REBUILD SPATIAL INDEX (The Master Database)
    // This ensures that the R*-tree in the EntityGraph is perfectly synchronized
    // with all placed components, substrate layers, and routed segments.
    // This is the source of truth for all DRC checks.
    space.entity_graph.rebuild_spatial_index(&space.material_registry);

    let _commit_start2 = std::time::Instant::now();

    // v0.1.7 DFM: Dummy metal fill (thieving) for manufacturing density balance
    {
        let dummy_fill_config = hwc_engine::DummyFillConfig {
            enabled: true,
            target_density_pct: 45,
            ..hwc_engine::DummyFillConfig::default()
        };
        let mut dummy_fill_engine = hwc_engine::DummyFillEngine::new();
        let fill_stats = dummy_fill_engine.run(&mut space.entity_graph, &dummy_fill_config);
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

    // Substrate-layer handshake: the three-step lookup in get_material() handles
    // substrate layers efficiently. No validation needed.

    // Sprint 9 (Task 9.1): PLACEMENT GATE
    // This is the "rustc model": collect up to N errors, then stop.
    if collector.has_errors() {
        let n = collector.error_count();
        return Err(IrError::CompilationAborted { error_count: n });
    }

    Ok((space, Some(qs), routes_loaded_from_lock))
}

// v0.1.8: G-Cell Sweep DRC — P45 Forbidden Junction Detection

/// Transform a parsed program into a hardware space.
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

    let (space, _qs, _from_cache) = compile_single_space(space_def, symbol_table, collector, None, None, false, None)?;
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
        let (space, qs, _from_cache) = compile_single_space(
            space_def,
            symbol_table,
            collector,
            lockfile_path,
            source_content,
            force_reroute,
            shared_qs.take(),
        )?;
        
        // v0.1.9: LOCKFILE DETERMINISM FIX
        // Lockfile saving has been moved to build_cmd AFTER validation passes.
        // This ensures we never save a lockfile for a build that fails validation,
        // preventing corruption of previously-working cached routes.
        // The from_cache flag is still tracked to avoid overwriting lockfiles
        // when routes were loaded from cache (no new routing was performed).
        
        shared_qs = qs;
        spaces.insert(space_name, space);
    }

    Ok(spaces)
}

/// Save routes from a HardwareSpace to a rkyv binary lockfile (v0.1.7).
/// 
/// v0.1.9: This function is now public and called from build_cmd AFTER validation
/// passes, ensuring lockfiles are only created for successfully validated builds.
pub fn save_routes_to_lockfile(
    path: &std::path::Path,
    space: &HardwareSpace,
    _source_content: &str,
) {
    let fingerprint = hwc_engine::geometry_router::compute_fingerprint_from_space(space);
    let binary_lockfile = match hwc_engine::geometry_router::traces_to_lockfile(space, fingerprint) {
        Ok(lockfile) => lockfile,
        Err(e) => {
            eprintln!("[LOCK] FATAL: failed to build lockfile: {}", e);
            return;
        }
    };

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


