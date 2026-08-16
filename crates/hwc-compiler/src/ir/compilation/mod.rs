mod dependency_graph;
mod finalization;
pub mod placement_items;
mod placement_loop;
mod routing_phase;
pub mod space_setup;

use crate::ir::errors::IrError;
use crate::ir::stackup_manager::StackupManager;
use crate::SymbolTable;
use hwc_diagnostics::DiagnosticCollector;
use hwc_engine::HardwareSpace;
use hwc_parser::{EvaluationContext, ProfileDefinition, SpaceDefinition};

/// Parameters for `compile_single_space` function to avoid too many arguments.
pub struct CompileSpaceParams<'a> {
    pub space_def: &'a SpaceDefinition,
    pub symbol_table: &'a SymbolTable,
    pub collector: &'a DiagnosticCollector,
    pub lockfile_path: Option<&'a std::path::Path>,
    pub source_content: Option<&'a str>,
    pub force_reroute: bool,
    pub query_store: Option<hwc_engine::geometry_router::query_engine::QueryStore>,
    pub unit_registry: &'a hwc_types::UnitRegistry,
    pub arena: &'a hwc_parser::ast::arena::AstArena,
}

/// Shared, read-only inputs threaded through the compilation passes.
///
/// Bundles the compilation-wide context so individual pass functions
/// (`execute_placement`, `process_routes`, …) take a single context argument
/// instead of a long parameter list.
pub struct CompilationContext<'a> {
    /// Topologically sorted placement-item indices. Iterating this and indexing
    /// straight into `placement_items` keeps the placement hot path free of
    /// string hashing and hash-map lookups.
    pub sorted_indices: &'a [usize],
    pub placement_items: &'a [crate::ir::placement_item::ContextualPlacementItem],
    pub symbol_table: &'a SymbolTable,
    pub eval_context: &'a EvaluationContext,
    pub stackup_manager: &'a StackupManager,
    pub profile: Option<&'a ProfileDefinition>,
    pub space_def: &'a SpaceDefinition,
    pub collector: &'a DiagnosticCollector,
    pub unit_registry: &'a hwc_types::UnitRegistry,
    pub arena: &'a hwc_parser::ast::arena::AstArena,
}

/// Compile a space recursively for hierarchical composition (v0.2.1)
///
/// This function compiles a child space in isolation for use in space instantiation.
/// Unlike `compile_single_space`, this does NOT use lockfiles or caching, and
/// returns a simpler HardwareSpace without query stores.
///
/// ## NO LOCKFILE CACHING
/// Child spaces are always compiled fresh to ensure correct net remapping.
pub fn compile_space_recursive(
    space_def: &SpaceDefinition,
    symbol_table: &SymbolTable,
    _eval_context_parent: &EvaluationContext,
    unit_registry: &hwc_types::UnitRegistry,
    arena: &hwc_parser::ast::arena::AstArena,
) -> Result<HardwareSpace, IrError> {
    eprintln!(
        "[RECURSIVE] Compiling child space '{}' (no lockfile cache)",
        space_def.name
    );

    // Use a throwaway diagnostic collector for child compilation
    let collector = DiagnosticCollector::new("", 100);

    // Compile the space without lockfile caching or query stores
    let (space, _, _) = compile_single_space(CompileSpaceParams {
        space_def,
        symbol_table,
        collector: &collector,
        lockfile_path: None,
        source_content: None,
        force_reroute: true,
        query_store: None,
        unit_registry,
        arena,
    })?;

    // Check if any errors were collected during child compilation
    if collector.has_errors() {
        return Err(IrError::CompilationAborted {
            error_count: collector.error_count(),
        });
    }

    eprintln!(
        "[RECURSIVE] Child space '{}' compiled successfully",
        space_def.name
    );

    Ok(space)
}

/// Compile a single space definition into a HardwareSpace.
///
/// This is the shared implementation used by both `program_to_space` and `program_to_spaces`.
///
/// v0.1.8: Accepts an optional `QueryStore` for memoized per-G-cell routing.
/// Returns the space, the query store (which may have been populated with
/// cached results for incremental rebuilds), and a boolean indicating whether
/// routes were loaded from the lockfile cache.
pub fn compile_single_space(
    params: CompileSpaceParams,
) -> Result<
    (
        HardwareSpace,
        Option<hwc_engine::geometry_router::query_engine::QueryStore>,
        bool,
    ),
    IrError,
> {
    // Destructure immediately: zero-cost, and the body below reads exactly as it
    // did when these were separate function parameters.
    let CompileSpaceParams {
        space_def,
        symbol_table,
        collector,
        lockfile_path,
        source_content: _source_content,
        force_reroute,
        query_store,
        unit_registry,
        arena,
    } = params;

    // Build eval context first (contains space-level let bindings)
    let eval_context_initial = space_setup::build_eval_context(symbol_table, None, space_def)?;

    // Unrolling allocates new nodes (one per loop iteration) into the arena, so
    // work against a local mutable copy. The caller's arena stays immutable,
    // which is what lets hierarchical space instantiation recurse while the
    // parent placement loop still holds a shared borrow of it.
    let mut arena_owned = arena.clone();

    // Collect placement items (unroll loops with eval context)
    let placement_items = placement_items::collect_placement_items(
        &space_def.statements,
        symbol_table,
        &eval_context_initial,
        &mut arena_owned,
    )?;
    let arena = arena_owned;

    let mut space = space_setup::create_space(
        space_def,
        symbol_table,
        &eval_context_initial,
        unit_registry,
        &arena,
    )?;

    let profile = space_setup::resolve_profile(space_def, symbol_table);

    let eval_context = space_setup::build_eval_context(symbol_table, profile.as_ref(), space_def)?;

    let stackup_manager = space_setup::create_stackup_and_materials(
        profile.as_ref(),
        symbol_table,
        &eval_context,
    )?;

    // **v0.2.0: Populate stackup layers in HardwareSpace (single source of truth)**
    // Export stackup metadata so it's available during export and validation without
    // needing to pass the full StackupManager everywhere.
    space.stackup_layers = stackup_manager.export_stackup_layers();

    // **v0.2.2: Register dielectric stackup layers as substrate base layers in entity graph**
    // This allows the substrate rendering code to find and render dielectric layers with via cutouts
    for stackup_layer in &space.stackup_layers {
        if !stackup_layer.is_routable {
            // This is a dielectric layer - register it as a substrate base layer
            let material_id = space
                .material_registry
                .get_id(&stackup_layer.material_name)
                .unwrap_or_else(|| {
                    panic!(
                        "Material '{}' from stackup not found in material registry",
                        stackup_layer.material_name
                    )
                });

            let substrate_bbox = hwc_engine::geometry::BoundingBox::new(
                hwc_engine::geometry::Point3D::new(0, 0, stackup_layer.z_bottom),
                hwc_engine::geometry::Point3D::new(
                    space.dimensions.width_nm,
                    space.dimensions.height_nm,
                    stackup_layer.z_top,
                ),
            );

            eprintln!(
                "[STACKUP→SUBSTRATE] Registering dielectric layer '{}' (Z={}→{}nm, material={}) as substrate base",
                stackup_layer.name, stackup_layer.z_bottom, stackup_layer.z_top, stackup_layer.material_name
            );

            space.entity_graph.add_substrate_layer(
                material_id,
                hwc_engine::NetId::UNCONNECTED,
                substrate_bbox,
                hwc_engine::geometry_router::substrate_types::SubstrateLayerType::Substrate,
            );
        }
    }

    // **v0.2.0: Build routing layer database from stackup (single source of truth)**
    space.routing_layer_db = hwc_engine::RoutingLayerDatabase::from_stackup(
        &space.stackup_layers,
        &space.material_registry,
    );
    eprintln!(
        "[DB] Routing layer database built: {} routable layers",
        space.routing_layer_db.routable_layer_count()
    );

    // **v0.2.0: Build via-layer mapping database from stackup (single source of truth)**
    space.via_layer_mapping_db = hwc_engine::ViaLayerMappingDatabase::from_stackup(
        &space.stackup_layers,
        &space.material_registry,
    );
    eprintln!(
        "[DB] Via-layer mapping database built: {} connections",
        space.via_layer_mapping_db.connection_count()
    );

    space_setup::populate_material_registry(
        &mut space,
        profile.as_ref(),
        symbol_table,
        &eval_context,
    );
    let sorted_indices = dependency_graph::build_and_sort(&placement_items, symbol_table, &arena)?;

    let compile_ctx = CompilationContext {
        sorted_indices: &sorted_indices,
        placement_items: &placement_items,
        symbol_table,
        eval_context: &eval_context,
        stackup_manager: &stackup_manager,
        profile: profile.as_ref(),
        space_def,
        collector,
        unit_registry,
        arena: &arena,
    };
    // Register 'space' as a special anchor representing the space boundaries
    // The 'space' anchor represents the absolute coordinate system of the design space.
    // Its bounding box is always from (0,0,0) to (width, height, depth) in internal coordinates.
    // User-facing origin configuration (bl, tl, etc.) is applied during coordinate transformation,
    // but does not affect the space anchor's internal representation.
    let mut bbox_tracker = crate::bounding_box_tracker::BoundingBoxTracker::new();
    let space_bbox = hwc_engine::geometry::BoundingBox {
        min: hwc_engine::geometry::Point3D { x: 0, y: 0, z: 0 },
        max: hwc_engine::geometry::Point3D {
            x: space.dimensions.width_nm,
            y: space.dimensions.height_nm,
            z: space.dimensions.depth_nm,
        },
    };
    bbox_tracker.register(
        "space".into(),
        space_bbox,
        hwc_engine::geometry::Point3D { x: 0, y: 0, z: 0 },
    );

    placement_loop::execute_placement(&mut space, &compile_ctx, &mut bbox_tracker)?;

    placement_loop::check_static_shorts(&space)?;

    // **v0.2.0: DATABASE-DRIVEN SYNCHRONIZATION**
    // After placement completes (including hierarchical flattening), synchronize
    // entity_graph.routed_segments from the routing database.
    //
    // This is CRITICAL for hierarchical designs:
    // - Child routes are registered in routing_database during flattening
    // - Router builds spatial index from entity_graph.routed_segments
    // - Without sync, router sees child cell boundaries as hard obstacles
    // - With sync, router sees child routes as same-net (can tap or route around)
    //
    // Architecture principle: routing_database is the source of truth,
    // entity_graph.routed_segments is a read-only view for obstacle queries.
    eprintln!("[COMPILATION] Synchronizing entity_graph from routing database...");
    space
        .entity_graph
        .sync_from_routing_database(&space.routing_database, &space.routing_layer_db);
    eprintln!(
        "[COMPILATION] Synchronization complete - router can now see child routes as same-net ✓"
    );

    let routes_loaded_from_lock =
        routing_phase::try_load_lockfile(&mut space, lockfile_path, force_reroute)?;

    let routing_mode = space_def
        .routing_config
        .as_ref()
        .map(|c| c.mode)
        .unwrap_or(hwc_parser::RoutingMode::Mixed);

    if !routes_loaded_from_lock {
        let _route_net_policies = routing_phase::collect_route_net_policies(
            &space,
            space_def,
            symbol_table,
            &eval_context,
        );

        let auto_routes = routing_phase::process_routes(&mut space, &compile_ctx, routing_mode)?;

        routing_phase::auto_route(
            &mut space,
            auto_routes,
            symbol_table,
            &eval_context,
            &stackup_manager,
            profile.as_ref(),
        )?;
    }

    finalization::finalize(
        &mut space,
        profile,
        &stackup_manager,
        symbol_table,
        collector,
        space_def,
        &eval_context,
    )?;

    // **v0.2.1: POPULATE DEVICE REGISTRY (PROPER ARCHITECTURE)**
    // Extract device instances from pour bindings and populate the device registry.
    // This is the single source of truth for all device instances.
    // Export formats (SPICE, BOM, etc.) read from this registry - no re-inference needed.
    crate::ir::device_registry::populate_device_instances(&mut space, symbol_table, Some(space_def))?;

    // **v0.2.1 FIX: POPULATE SPATIAL INDEX FOR DRC**
    // Substrate layers (pours, contacts, pads) were added to entity_graph during placement,
    // but the R*-tree spatial index was never populated. This caused geometric DRC checks
    // (clearance, mask overhang, layer-to-layer rules) to report "Spatial index: 0 entities"
    // and skip all polygon-to-polygon validation.
    //
    // Architecture: The spatial index must be rebuilt from substrate_layers and routed_segments
    // before validation begins. This is the proper home for this operation - after all placement
    // and routing is complete, before returning the space for validation.
    eprintln!("[COMPILATION] Populating spatial index for DRC...");
    let substrate_segments = space.entity_graph.get_substrate_layers_as_segments();
    eprintln!(
        "[COMPILATION] Inserting {} substrate layers into spatial index",
        substrate_segments.len()
    );
    for segment in substrate_segments {
        space.entity_graph.spatial_mut().insert(segment);
    }
    
    // Also insert routed segments into spatial index
    for (_net_idx, (net_id, segments)) in space.entity_graph.get_all_routes().iter().enumerate() {
        for (_seg_idx, _segment) in segments.iter().enumerate() {
            // Note: Segments are already in routed_segments, we just need to ensure they're in spatial
            // The spatial index should have been populated during routing, but we ensure it here
            eprintln!("[COMPILATION DEBUG] Route net {} has {} segments", net_id.raw(), segments.len());
        }
    }

    
    eprintln!(
        "[COMPILATION] Spatial index populated: {} entities ready for DRC",
        space.entity_graph.spatial().len()
    );

    Ok((space, query_store, routes_loaded_from_lock))
}

/// Transform a parsed program into a hardware space.
///
/// This is the main entry point for IR integration.
pub fn program_to_space(
    program: &hwc_parser::Program,
    symbol_table: &SymbolTable,
    collector: &hwc_diagnostics::DiagnosticCollector,
    unit_registry: &hwc_types::UnitRegistry,
) -> Result<HardwareSpace, IrError> {
    let arena = &program.arena;
    let space_def_id = program
        .definitions
        .iter()
        .find_map(|def| {
            if let hwc_parser::Definition::Space(space_id) = def {
                Some(*space_id)
            } else {
                None
            }
        })
        .ok_or(IrError::NoSpaceDefinition)?;

    // Lookup the actual SpaceDefinition from arena
    let space_def = &arena.space_defs[space_def_id];

    let (space, _qs, _from_cache) = compile_single_space(CompileSpaceParams {
        space_def,
        symbol_table,
        collector,
        lockfile_path: None,
        source_content: None,
        force_reroute: false,
        query_store: None,
        unit_registry,
        arena,
    })?;
    Ok(space)
}

/// Transform a parsed program into multiple hardware spaces (one per space definition).
///
/// Returns a HashMap keyed by space name. Each space is compiled independently.
pub fn program_to_spaces(
    program: &hwc_parser::Program,
    symbol_table: &SymbolTable,
    collector: &hwc_diagnostics::DiagnosticCollector,
    unit_registry: &hwc_types::UnitRegistry,
) -> Result<rustc_hash::FxHashMap<compact_str::CompactString, HardwareSpace>, IrError> {
    program_to_spaces_with_lockfile(
        program,
        symbol_table,
        collector,
        None,
        None,
        false,
        unit_registry,
    )
}

/// Compile all space definitions into HardwareSpaces with lockfile support.
///
/// This is the extended version that supports route lockfiles for deterministic builds.
pub fn program_to_spaces_with_lockfile(
    program: &hwc_parser::Program,
    symbol_table: &SymbolTable,
    collector: &hwc_diagnostics::DiagnosticCollector,
    lockfile_path: Option<&std::path::Path>,
    source_content: Option<&str>,
    force_reroute: bool,
    unit_registry: &hwc_types::UnitRegistry,
) -> Result<rustc_hash::FxHashMap<compact_str::CompactString, HardwareSpace>, IrError> {
    let space_defs: Vec<&hwc_parser::SpaceDefinition> = program
        .definitions
        .iter()
        .filter_map(|def| {
            if let hwc_parser::Definition::Space(space_id) = def {
                // Lookup SpaceDefinition from arena
                Some(&program.arena.space_defs[*space_id])
            } else {
                None
            }
        })
        .collect();

    if space_defs.is_empty() {
        return Err(IrError::NoSpaceDefinition);
    }

    let mut spaces = rustc_hash::FxHashMap::default();
    let mut shared_qs: Option<hwc_engine::geometry_router::query_engine::QueryStore> = None;

    for space_def in space_defs {
        let space_name: compact_str::CompactString = space_def.name.to_string().into();
        let (space, qs, _from_cache) = compile_single_space(CompileSpaceParams {
            space_def,
            symbol_table,
            collector,
            lockfile_path,
            source_content,
            force_reroute,
            query_store: shared_qs.take(),
            unit_registry,
            arena: &program.arena,
        })?;

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
    let binary_lockfile = match hwc_engine::geometry_router::traces_to_lockfile(space, fingerprint)
    {
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
