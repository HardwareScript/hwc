//! Automatic routing using topological ray-casting.

use super::super::errors::IrError;
use super::helpers::get_pin_positions;
use compact_str::CompactString;
use hwc_engine::netlist::NetId;
use hwc_engine::{HardwareSpace, Point3D};
use rustc_hash::FxHashMap;

/// Resolve the conductor material for a trace at the given Z position.
/// Looks up the stackup layer at that Z and returns the material ID from the registry.
fn resolve_material_for_z(
    z_nm: i64,
    stackup_manager: &crate::ir::stackup_manager::StackupManager,
    material_registry: &hwc_engine::material::MaterialRegistry,
    profile: Option<&hwc_parser::ProfileDefinition>,
) -> Result<hwc_engine::material::MaterialId, crate::ir::errors::IrError> {
    if let Some(layer_name) = stackup_manager.get_layer_name_at_z(z_nm) {
        if let Some(mat_name) = profile
            .and_then(|p| p.stackup.as_ref())
            .and_then(|stackup| {
                stackup
                    .layers
                    .iter()
                    .find(|l| l.name.name == layer_name)
                    .map(|l| l.material.clone())
            })
        {
            return material_registry.get_id(&mat_name).ok_or_else(|| {
                crate::ir::errors::IrError::UndeclaredMaterial { material: mat_name }
            });
        }
    }
    Err(crate::ir::errors::IrError::UndeclaredMaterial {
        material: format!(
            "No material found at Z={}nm (check stackup definition)",
            z_nm
        )
        .into(),
    })
}

/// Route a trace automatically using topological ray-casting.
///
/// Implements the 3-phase routing pipeline:
/// 1. Constraint Manager: Generate geometric constraints from physics
/// 2. Geometry Router: Topological ray-casting with Manhattan routing
/// 3. Design Rule Check: Validate physics compliance
pub fn route_automatic(
    space: &mut HardwareSpace,
    route: &hwc_parser::Route,
    symbol_table: &crate::SymbolTable,
    stackup_manager: &crate::ir::stackup_manager::StackupManager,
    profile: Option<&hwc_parser::ProfileDefinition>,
) -> Result<(), IrError> {
    let from_name = super::helpers::construct_entity_name(&route.from)?;
    let to_name = super::helpers::construct_entity_name(&route.to)?;

    // PHASE 1: CONSTRAINT MANAGER
    // v0.1.8: Use profile constraints when available, fail if missing
    let min_clearance_nm = space.fabrication_constraints.as_ref()
        .map(|c| c.trace.min_spacing_nm)
        .ok_or_else(|| IrError::MissingAsicConstraint {
            message: "Route requires fabrication constraints for clearance calculation but none are loaded.".into(),
            hint: "Declare a profile with 'clearance:' constraints in the space definition.".into(),
        })?;

    // v0.1.8: Read current limit from route declaration — fail if missing
    let current_ma: f64 = if let Some(ref ac) = route.current_limit_ac {
        let _rms = crate::ir::conversions::evaluate_expression_to_ma(&ac.rms, symbol_table)
            .map_err(|e| IrError::InvalidRouteExpression {
                expression: "current_limit_ac.rms".into(),
                reason: e.to_string(),
            })?;
        let peak = crate::ir::conversions::evaluate_expression_to_ma(&ac.peak, symbol_table)
            .map_err(|e| IrError::InvalidRouteExpression {
                expression: "current_limit_ac.peak".into(),
                reason: e.to_string(),
            })?;
        peak
    } else {
        return Err(IrError::MissingAsicConstraint {
            message:
                "Route has no current_limit declaration. All routes must declare current capacity."
                    .into(),
            hint:
                "Add 'current_limit_ac: { rms: <value>, peak: <value> }' to the route declaration."
                    .into(),
        });
    };

    let is_external = true;

    let temp_rise_c = profile
        .and_then(|p| p.thermal.as_ref())
        .map(|t| t.max_temp_rise.value as i64)
        .ok_or_else(|| IrError::MissingAsicConstraint {
            message: "Route requires thermal constraints for trace width calculation but none are declared.".into(),
            hint: "Declare 'thermal: { max_temp_rise: <value> }' in the profile.".into(),
        })?;

    let _min_trace_width_nm = hwc_engine::constraint_manager::calculate_trace_width_nm(
        current_ma as i64,
        temp_rise_c,
        is_external,
    );

    // v0.1.7: Resolve explicit width if provided, otherwise use calculated minimum
    let trace_width_nm = if let Some(width_expr) = &route.width {
        crate::ir::conversions::evaluate_expression_to_nm(width_expr, symbol_table)
            .map_err(IrError::InvalidExpression)?
    } else {
        // v0.1.8: No hardcoded defaults. Must come from profile.
        profile.and_then(|p| p.trace.as_ref())
            .map(|t| crate::ir::conversions::measurement_to_nm(&t.min_width, symbol_table))
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

    // Task 15.11: Warn if net has no current_limit declared (DRC will skip current-density check)
    if route.current_limit_ac.is_none() {
        eprintln!(
            "[ROUTER] WARNING: Net {} -> {} has no current_limit declared. DRC will skip current-density check.",
            from_name, to_name
        );
    }

    // v0.1.7: Unified Boundary-Docking Model
    // Instead of routing to pin centers, we calculate exact boundary points.
    let (start_boundary, goal_boundary, start_dir, goal_dir) =
        calculate_boundary_points(space, route, trace_width_nm)?;

    // DEBUG: Check if boundary points are corrupted
    eprintln!("[BOUNDARY DEBUG] Route: {} -> {}", from_name, to_name);
    eprintln!(
        "[BOUNDARY DEBUG]   start_boundary: ({},{},{})",
        start_boundary.x, start_boundary.y, start_boundary.z
    );
    eprintln!(
        "[BOUNDARY DEBUG]   goal_boundary: ({},{},{})",
        goal_boundary.x, goal_boundary.y, goal_boundary.z
    );

    // v0.1.8: Resolve target layer override
    // If `route.layer` is specified, override the Z coordinate of the route
    // to force it onto the specified layer. The auto-via inserter will handle
    // the layer transitions at the pin boundaries.
    let target_z_nm = if let Some(ref layer_id) = route.layer {
        let layer_name = layer_id.name.as_str();
        eprintln!(
            "[ROUTER] route.layer='{}' specified, resolving Z from stackup...",
            layer_name
        );
        let z = stackup_manager
            .get_layer_start_z(layer_name)
            .ok_or_else(|| IrError::InvalidRouteExpression {
                expression: format!("layer '{}'", layer_name),
                reason: format!("Unknown routing layer '{}' in stackup", layer_name),
            })?;
        eprintln!(
            "[ROUTER] Resolved layer '{}' -> Z={}nm (pin_z={})",
            layer_name, z, start_boundary.z
        );
        Some(z)
    } else {
        None
    };

    // v0.1.7: External Seeding
    // To prevent "hooks" inside pads, the topological router starts one grid unit OUTSIDE the pad.
    let resolution_nm = space.resolution_nm;

    // Helper to snap a coordinate to the nearest grid center
    let _snap_to_center = |coord: i64, res_nm: i64| (coord / res_nm) * res_nm + (res_nm / 2);

    let mut start_pos = hwc_engine::Point3D::new(
        start_boundary.x + (start_dir.0 * resolution_nm),
        start_boundary.y + (start_dir.1 * resolution_nm),
        start_boundary.z,
    );

    let mut goal_pos = hwc_engine::Point3D::new(
        goal_boundary.x + (goal_dir.0 * resolution_nm),
        goal_boundary.y + (goal_dir.1 * resolution_nm),
        goal_boundary.z,
    );

    // v0.1.7: Seed Alignment (Orthogonal Snapping)
    // To prevent the initial diagonal "bump" from the pin to the grid, we snap
    // the non-escape axis of the seed to the grid center.
    if start_dir.0 != 0 {
        // East/West escape: Lock Y to boundary coordinate
        start_pos.y = start_boundary.y;
    } else if start_dir.1 != 0 {
        // North/South escape: Lock X to boundary coordinate
        start_pos.x = start_boundary.x;
    }

    if goal_dir.0 != 0 {
        // East/West escape: Lock Y to boundary coordinate
        goal_pos.y = goal_boundary.y;
    } else if goal_dir.1 != 0 {
        // North/South escape: Lock X to boundary coordinate
        goal_pos.x = goal_boundary.x;
    }

    // v0.1.8: Override start/goal Z when target layer is specified
    // The route is stamped on the target layer. Transition segments at the
    // pin boundaries create Z changes that the auto-via inserter detects.
    if let Some(z) = target_z_nm {
        start_pos.z = z;
        goal_pos.z = z;
    }

    // PHASE 2: GEOMETRY ROUTER
    // v0.1.7: Register net connectivity in the netlist
    // This ensures both pins share the same logical net ID.
    let net_id = super::helpers::register_net_for_route(
        space,
        route,
        symbol_table,
        stackup_manager,
        profile,
        None,
    )?;
    let net_name = space
        .netlist
        .get_net(net_id)
        .ok_or_else(|| IrError::InvalidRouteExpression {
            expression: format!("net ID {}", net_id.raw()),
            reason: "Net not found after registration".into(),
        })?
        .name
        .clone();

    eprintln!("[BOX-MODEL-DEBUG] Net: {}", net_name);
    eprintln!(
        "[BOX-MODEL-DEBUG]   Start Boundary: ({}, {}, {})",
        start_boundary.x, start_boundary.y, start_boundary.z
    );
    eprintln!("[BOX-MODEL-DEBUG]   Start Dir: {:?}", start_dir);
    eprintln!(
        "[BOX-MODEL-DEBUG]   Start Seed (Router Start): ({}, {}, {})",
        start_pos.x, start_pos.y, start_pos.z
    );
    eprintln!(
        "[BOX-MODEL-DEBUG]   Goal Boundary: ({}, {}, {})",
        goal_boundary.x, goal_boundary.y, goal_boundary.z
    );
    eprintln!("[BOX-MODEL-DEBUG]   Goal Dir: {:?}", goal_dir);
    eprintln!(
        "[BOX-MODEL-DEBUG]   Goal Seed (Router Goal): ({}, {}, {})",
        goal_pos.x, goal_pos.y, goal_pos.z
    );

    // PHASE 2: GEOMETRY ROUTER
    // v0.1.7: Register net connectivity in the netlist
    // This ensures both pins share the same logical net ID.
    let net_id = super::helpers::register_net_for_route(
        space,
        route,
        symbol_table,
        stackup_manager,
        profile,
        None,
    )?;
    let net_name = space
        .netlist
        .get_net(net_id)
        .ok_or_else(|| IrError::InvalidRouteExpression {
            expression: format!("net ID {}", net_id.raw()),
            reason: "Net not found after registration".into(),
        })?
        .name
        .clone();

    eprintln!("[BOX-MODEL-DEBUG] Net: {}", net_name);
    eprintln!(
        "[BOX-MODEL-DEBUG]   Start Boundary: ({}, {}, {})",
        start_boundary.x, start_boundary.y, start_boundary.z
    );
    eprintln!("[BOX-MODEL-DEBUG]   Start Dir: {:?}", start_dir);
    eprintln!(
        "[BOX-MODEL-DEBUG]   Start Seed (Router Start): ({}, {}, {})",
        start_pos.x, start_pos.y, start_pos.z
    );
    eprintln!(
        "[BOX-MODEL-DEBUG]   Goal Boundary: ({}, {}, {})",
        goal_boundary.x, goal_boundary.y, goal_boundary.z
    );
    eprintln!("[BOX-MODEL-DEBUG]   Goal Dir: {:?}", goal_dir);
    eprintln!(
        "[BOX-MODEL-DEBUG]   Goal Seed (Router Goal): ({}, {}, {})",
        goal_pos.x, goal_pos.y, goal_pos.z
    );

    // Resolve material ID from stackup layer at the route's target Z position.
    let route_z = target_z_nm.unwrap_or(start_pos.z);
    let copper_id =
        resolve_material_for_z(route_z, stackup_manager, &space.material_registry, profile)?;

    // Identify component names for DRC exemption
    let from_component_name = super::helpers::construct_entity_name(&route.from)?;
    let to_component_name = super::helpers::construct_entity_name(&route.to)?;

    // v0.1.9: Use TopologicalRouter as the single authoritative routing engine
    // Obstacles are queried via the DynamicSpatialIndex (per-layer sorted vectors, i64 nm)
    let topo_router =
        hwc_engine::geometry_router::TopologicalRouter::new(trace_width_nm, space.resolution_nm, min_clearance_nm);

    let board_bounds = hwc_engine::BoundingBox::new(
        hwc_engine::Point3D::new(0, 0, 0),
        hwc_engine::Point3D::new(
            space.dimensions.width_nm,
            space.dimensions.height_nm,
            space.dimensions.depth_nm,
        ),
    );

    // Build spatial index for obstacle queries (layered, i64 nm)
    let spatial_index = {
        let mut idx = hwc_engine::geometry_router::DynamicSpatialIndex::new();
        // Copy layer Z-ranges from the entity graph's spatial index
        if let Some(z_ranges) = space.entity_graph.spatial().layer_z_ranges() {
            idx.set_layer_z_ranges(&z_ranges);
        }
        
        // Add substrate layers (pours, contacts, etc.) — CRITICAL for obstacle detection!
        for (layer_idx, layer) in space.entity_graph.get_substrate_layers().iter().enumerate() {
            let width = layer.bbox.max.x - layer.bbox.min.x;
            let height = layer.bbox.max.y - layer.bbox.min.y;
            let trace_seg = hwc_engine::geometry_router::IndexedSegment {
                source: hwc_engine::geometry_router::spatial_index::SpatialEntitySource::SubstrateLayer {
                    index: layer_idx,
                },
                segment_id: layer_idx,
                net_id: layer.net as usize,
                width_nm: width.max(height),
                thickness_nm: layer.bbox.max.z - layer.bbox.min.z,
                start: layer.bbox.min,
                end: layer.bbox.max,
                layer: layer.bbox.min.z,
            };
            eprintln!(
                "[AUTO ROUTE INDEX] Adding substrate layer {}: net_id={}, bbox=({},{},{}) to ({},{},{})",
                layer_idx, layer.net, 
                layer.bbox.min.x, layer.bbox.min.y, layer.bbox.min.z,
                layer.bbox.max.x, layer.bbox.max.y, layer.bbox.max.z
            );
            idx.insert(trace_seg);
        }
        
        // Add component metadata (excluding start and goal components)
        for meta in space.entity_graph.get_component_metadata() {
            // EXEMPTION GUARD: Skip the start and goal components to allow routing from/to them
            if meta.name == from_component_name || meta.name == to_component_name {
                eprintln!(
                    "[AUTO ROUTE INDEX] Skipping component '{}' (is start or goal)",
                    meta.name
                );
                continue;
            }
            
            let width = meta.bbox.max.x - meta.bbox.min.x;
            let height = meta.bbox.max.y - meta.bbox.min.y;
            let trace_seg = hwc_engine::geometry_router::IndexedSegment {
                source: hwc_engine::geometry_router::spatial_index::SpatialEntitySource::ComponentInstance {
                    instance_id: 0,
                },
                segment_id: 0,
                net_id: 0,
                width_nm: width.max(height),
                thickness_nm: meta.bbox.max.z - meta.bbox.min.z,
                start: meta.bbox.min,
                end: meta.bbox.max,
                layer: meta.bbox.min.z,
            };
            idx.insert(trace_seg);
        }
        idx
    };

    // v0.1.9: Use route() without exemptions since we already excluded components from spatial index
    let mut path = topo_router
        .route(start_pos, goal_pos, &spatial_index, &board_bounds)
        .ok_or_else(|| IrError::NoPathFound {
            net: format!(
                "{} -> {}",
                super::helpers::endpoint_label(&route.from),
                super::helpers::endpoint_label(&route.to)
            )
            .into(),
            from_pin: super::helpers::endpoint_label(&route.from).into(),
            to_pin: super::helpers::endpoint_label(&route.to).into(),
        })?
        .waypoints;

    // v0.1.7: Boundary Stitching
    // We prepend/append the actual boundary points to the routed path.
    // This ensures the trace connects perfectly to the pad edge.
    path.insert(0, start_boundary);
    path.push(goal_boundary);

    // v0.1.8: R25 Non-Routable Layer Check (Post-Route)
    // The topological router skips non-routable layers, but this post-route check
    // catches any edge cases where the router may have slipped through (e.g. Z-transitions).
    if let Some(stackup) = profile.and_then(|p| p.stackup.as_ref()) {
        for point in &path {
            if let Some(layer_name) = stackup_manager.get_layer_name_at_z(point.z) {
                if let Some(layer_def) = stackup.layers.iter().find(|l| l.name.name == layer_name) {
                    if let Some(hwc_parser::RoutableMode::False) = layer_def.routable {
                        let material = layer_def.material.clone();
                        return Err(IrError::NonRoutableLayer {
                            layer: layer_name.into(),
                            material,
                        });
                    }
                }
            }
        }
    }

    // v0.1.7: Global Axis Alignment (Neat Routing)
    // If the start and goal share an axis (straight route), lock all intermediate
    // points to that axis to eliminate quantization noise and "bumps".
    if start_boundary.x == goal_boundary.x {
        for point in path.iter_mut() {
            point.x = start_boundary.x;
        }
    } else if start_boundary.y == goal_boundary.y {
        for point in path.iter_mut() {
            point.y = start_boundary.y;
        }
    }

    if path.is_empty() {
        return Err(IrError::EmptyRoute {
            net: format!(
                "{} -> {}",
                super::helpers::endpoint_label(&route.from),
                super::helpers::endpoint_label(&route.to)
            )
            .into(),
        });
    }

    // **v0.1.7: ANALYTIC ROUTE REGISTRATION (GOD-TIER PARADIGM SHIFT)**
    let (start_pin_id, goal_pin_id) = super::helpers::get_pin_ids(space, route)?;

    let _start_pin_name = space
        .netlist
        .get_pin(start_pin_id)
        .ok_or_else(|| IrError::InvalidRouteExpression {
            expression: format!("pin ID {}", start_pin_id.raw()),
            reason: "Start pin not found".into(),
        })?
        .name
        .clone();
    let _goal_pin_name = space
        .netlist
        .get_pin(goal_pin_id)
        .ok_or_else(|| IrError::InvalidRouteExpression {
            expression: format!("pin ID {}", goal_pin_id.raw()),
            reason: "Goal pin not found".into(),
        })?
        .name
        .clone();

    // v0.1.7: Grid-Agnostic Z-Resolution
    // We transform the router's grid-snapped path back into exact physical layer heights
    // using the StackupManager. This eliminates the 21µm "discretization noise".
    let mut refined_path = path.clone();
    let mut trace_thickness_nm = space.resolution_nm; // Default to resolution size

    if refined_path.len() >= 2 {
        // v0.1.9: When target layer is specified, the TopologicalRouter routes on that
        // layer. Skip Z-refinement to avoid the StackupManager incorrectly remapping Z.
        if let Some(fixed_z) = target_z_nm.or(Some(start_pos.z)) {
            // Resolve thickness once from the fixed Z plane
            if let Some(layer_idx) = stackup_manager.get_layer_index_at_z(fixed_z) {
                trace_thickness_nm = stackup_manager.get_thickness_for_layer_index(layer_idx)?;
            }
            /*
            eprintln!(
                "[ROUTER DEBUG] Planar lock: {} points locked to Z={}nm, thickness={}nm",
                refined_path.len(),
                fixed_z,
                trace_thickness_nm
            );
            */
        } else {
            /*
            eprintln!(
                "[ROUTER DEBUG] Refining {} path points via StackupManager...",
                refined_path.len()
            );
            */
            for (_i, point) in refined_path.iter_mut().enumerate() {
                let _old_z = point.z;
                // 1. Identify which PHYSICAL layer this point is in (v0.1.7 Fix: Use StackupManager, not grid math)
                if let Some(layer_idx) = stackup_manager.get_layer_index_at_z(point.z) {
                    // 2. Resolve the EXACT physical starting height for that layer
                    // This bypasses the coarse grid's multiplication formula.
                    let true_z = stackup_manager.get_z_start_nm_for_layer_index(layer_idx)?;

                    // v0.1.7: Extract physical thickness for this layer to prevent "wobbly" 3D meshes
                    trace_thickness_nm =
                        stackup_manager.get_thickness_for_layer_index(layer_idx)?;

                    // 3. Update the point's Z to the physical truth
                    point.z = true_z;

                    /*
                    if old_z != true_z {
                        eprintln!(
                            "[ROUTER DEBUG]   Point {}: Z shifted from {}nm to {}nm (Layer Index: {})",
                            i, old_z, true_z, layer_idx
                        );
                    }
                    */
                } else {
                    // eprintln!("[ROUTER WARNING]   Point {}: Z={}nm could not be mapped to any physical layer!", i, point.z);
                }
            }
        }
    }

    // After the Z-refinement loop, verify thickness was resolved from stackup
    if trace_thickness_nm == space.resolution_nm && refined_path.len() >= 2 {
        return Err(IrError::InvalidRouteExpression {
            expression: format!(
                "route from {} to {}",
                super::helpers::endpoint_label(&route.from),
                super::helpers::endpoint_label(&route.to)
            ),
            reason: format!(
                "Could not resolve trace thickness from stackup at Z={}nm. \
                 Ensure the stackup is properly defined in your PDK profile.",
                refined_path[0].z
            ),
        });
    }

    // Primitives Over Pixels
    let segments = {
        let mut segs = Vec::new();

        // v0.1.8: Layer override transition segments
        // When route.layer is specified, add vertical segments at start/end
        // to transition from the pin's Z to the target layer's Z.
        // The auto-via inserter detects these Z transitions and inserts vias.
        if let Some(target_z) = target_z_nm {
            let pin_z = start_boundary.z;
            eprintln!(
                "[VIA TRANSITION DEBUG] Start: pin_z={}, target_z={}, diff={}",
                pin_z,
                target_z,
                (pin_z - target_z).abs()
            );

            // Only add via transition if there's a significant Z difference (> 50nm tolerance)
            // This prevents unnecessary vias when routing on the same layer
            if (pin_z - target_z).abs() > 50 {
                eprintln!(
                    "  ✅ Adding START via transition: {} -> {}",
                    pin_z, target_z
                );
                // Vertical transition at start: pin Z -> target layer Z
                let start_up = Point3D::new(start_boundary.x, start_boundary.y, target_z);
                segs.push(hwc_engine::LineSegment::new(start_boundary, start_up));
            } else {
                eprintln!("  ⏭️  Skipping START via: pin already on target layer");
            }
        }

        // Collinear merge + PDK min_segment_length filter (required, no default).
        if refined_path.len() >= 2 {
            let min_seg_len_nm = super::helpers::require_min_segment_length_nm(profile)?;
            segs.extend(super::helpers::manhattan_path_to_segments(
                &refined_path,
                min_seg_len_nm,
            ));
        }

        // v0.1.8: Vertical transition at end: target layer Z -> pin Z
        if let Some(target_z) = target_z_nm {
            let pin_z = goal_boundary.z;
            eprintln!(
                "[VIA TRANSITION DEBUG] Goal: pin_z={}, target_z={}, diff={}",
                pin_z,
                target_z,
                (pin_z - target_z).abs()
            );

            // Only add via transition if there's a significant Z difference (> 50nm tolerance)
            if (pin_z - target_z).abs() > 50 {
                eprintln!("  ✅ Adding GOAL via transition: {} -> {}", target_z, pin_z);
                let goal_down = Point3D::new(goal_boundary.x, goal_boundary.y, target_z);
                segs.push(hwc_engine::LineSegment::new(goal_down, goal_boundary));
            } else {
                eprintln!("  ⏭️  Skipping GOAL via: pin already on target layer");
            }
        }

        segs
    };

    eprintln!(
        "[SEGMENT DEBUG] Created {} segments from path:",
        segments.len()
    );
    for (i, seg) in segments.iter().enumerate().take(4) {
        eprintln!(
            "  seg[{}]: ({},{},{}) -> ({},{},{})",
            i, seg.start.x, seg.start.y, seg.start.z, seg.end.x, seg.end.y, seg.end.z
        );
    }
    if segments.len() > 4 {
        eprintln!("  ... and {} more segments", segments.len() - 4);
    }

    // v0.1.7 DFM: Teardrops strengthen pad/trace junctions for manufacturing reliability
    {
        let teardrop_config = hwc_engine::TeardropConfig::class2(trace_width_nm);
        hwc_engine::TeardropEngine::apply_teardrops(
            &space.entity_graph,
            &refined_path,
            start_boundary,
            goal_boundary,
            trace_width_nm,
            &teardrop_config,
            space.resolution_nm,
            hwc_engine::netlist::NetHandle::new(net_id.raw() as u32),
        );
    }

    // Register main trace as analytic primitive
    let net_actual_current_ma = space
        .netlist
        .get_net(net_id)
        .and_then(|n| n.current_ma)
        .unwrap_or(0.0); // Default to 0 if not declared

    let analytic_trace = hwc_engine::AnalyticTrace::new(
        net_id,
        trace_width_nm,
        trace_thickness_nm, // v0.1.7: Exact physical thickness
        segments,
        copper_id,
        net_name.clone(),
        net_actual_current_ma, // Actual operating current from net declaration
        current_ma,            // Route's declared capability from current_limit_ac.peak
    );

    eprintln!(
        "[ROUTER] Net '{}': {} segments registered (start_z={}, goal_z={}, target_z={:?})",
        net_name,
        analytic_trace.segments.len(),
        start_boundary.z,
        goal_boundary.z,
        target_z_nm
    );
    for (i, seg) in analytic_trace.segments.iter().enumerate() {
        if seg.start.z != seg.end.z {
            eprintln!(
                "[ROUTER]   seg[{}]: Z-TRANSITION ({},{},{}) -> ({},{},{})",
                i, seg.start.x, seg.start.y, seg.start.z, seg.end.x, seg.end.y, seg.end.z
            );
        }
    }

    // v0.1.8: EM/Thermal verification using parsed current_limit_ac
    {
        let current_decl = if let Some(ref ac) = route.current_limit_ac {
            let rms_ma = crate::ir::conversions::evaluate_expression_to_ma(&ac.rms, symbol_table)
                .map_err(|e| IrError::InvalidRouteExpression {
                expression: "current_limit_ac.rms".into(),
                reason: e.to_string(),
            })?;
            let peak_ma = crate::ir::conversions::evaluate_expression_to_ma(&ac.peak, symbol_table)
                .map_err(|e| IrError::InvalidRouteExpression {
                    expression: "current_limit_ac.peak".into(),
                    reason: e.to_string(),
                })?;
            hwc_engine::CurrentDeclaration::Ac(hwc_engine::AcCurrent {
                rms: rms_ma / 1000.0,   // mA-to-A conversion — unit constant
                peak: peak_ma / 1000.0, // mA-to-A conversion — unit constant
            })
        } else {
            hwc_engine::CurrentDeclaration::Dc(current_ma / 1000.0) // mA-to-A conversion — unit constant
        };

        // Build IndexedSegments from the analytic route for verification
        let em_segments: Vec<hwc_engine::IndexedSegment> = analytic_trace
            .segments
            .iter()
            .enumerate()
            .map(|(i, seg)| hwc_engine::IndexedSegment {
                source:
                    hwc_engine::geometry_router::spatial_index::SpatialEntitySource::RouteSegment {
                        net_idx: net_id.raw() as usize,
                        seg_idx: i,
                    },
                segment_id: i,
                net_id: net_id.raw() as usize,
                width_nm: analytic_trace.width_nm,
                thickness_nm: trace_thickness_nm,
                start: seg.start,
                end: seg.end,
                layer: 0,
            })
            .collect();

        let em_params = hwc_engine::EmParams {
            j_limit: profile
                .and_then(|p| p.other.get("em_current_density_limit"))
                .and_then(|s| s.parse::<f64>().ok())
                .ok_or_else(|| IrError::MissingAsicConstraint {
                    message:
                        "PDK missing required 'em_current_density_limit' in profile 'other' block."
                            .into(),
                    hint: "Add 'other: em_current_density_limit: <value>' to your profile.".into(),
                })?,
            i_peak: current_decl.peak(),
        };

        let thermal_params = hwc_engine::ThermalParams {
            ambient_temp_c: profile
                .and_then(|p| p.thermal.as_ref())
                .map(|t| t.ambient_temp.value)
                .ok_or_else(|| IrError::MissingAsicConstraint {
                    message: "PDK missing required 'thermal.ambient_temp' constraint.".into(),
                    hint: "Add a 'thermal:' block to your profile with 'ambient_temp: <value>'."
                        .into(),
                })?,
            max_temp_rise_c: profile
                .and_then(|p| p.thermal.as_ref())
                .map(|t| t.max_temp_rise.value)
                .ok_or_else(|| IrError::MissingAsicConstraint {
                    message: "PDK missing required 'thermal.max_temp_rise' constraint.".into(),
                    hint: "Add a 'thermal:' block to your profile with 'max_temp_rise: <value>'."
                        .into(),
                })?,
            copper_thickness_m: trace_thickness_nm as f64 * 1e-9, // nm-to-m conversion — physics constant
            substrate_er: profile
                .and_then(|p| p.stackup.as_ref())
                .and_then(|s| {
                    s.layers.iter().find(|l| {
                        l.material.to_lowercase().contains("fr4")
                            || l.material.to_lowercase().contains("dielectric")
                    })
                })
                .and_then(|l| {
                    let er_key: CompactString = format!("substrate_er_{}", l.name.name).into();
                    profile
                        .and_then(|p| p.other.get(&er_key))
                        .and_then(|s| s.parse::<f64>().ok())
                })
                .ok_or_else(|| IrError::MissingAsicConstraint {
                    message: "PDK missing required substrate dielectric constant (substrate_er)."
                        .into(),
                    hint: "Add 'other: substrate_er_<LayerName>: <value>' to your profile.".into(),
                })?,
        };

        let violations =
            hwc_engine::verify_em_thermal(&em_segments, &current_decl, &em_params, &thermal_params);

        if !violations.is_empty() {
            let msg = violations
                .iter()
                .map(|v| match v {
                    hwc_engine::EmThermalViolation::Em(em) => {
                        format!(
                            "EM violation: current density {:.2} A/m² exceeds limit {:.2} A/m² at ({}, {}), width {}nm, min {}nm",
                            em.current_density, em.limit, em.location.0, em.location.1, em.width_nm, em.min_width_nm
                        )
                    }
                    hwc_engine::EmThermalViolation::Thermal(th) => {
                        format!(
                            "Thermal violation: {:.1}°C rise exceeds {:.1}°C limit at ({}, {})",
                            th.temp_rise_c, th.max_allowed_c, th.location.0, th.location.1
                        )
                    }
                })
                .collect::<Vec<_>>()
                .join("\n  ");
            eprintln!(
                "[ROUTER] ⚠ EM/Thermal violations for route {}:\n  {}",
                net_name, msg
            );
        }
    }

    space.add_analytic_route(analytic_trace);

    // eprintln!("[ROUTER] ✓ Route registered as analytic primitive");

    // Connect both pins to the net (already done in register_net_for_route, but ensure logical binding)
    space.netlist.connect_pin(start_pin_id, net_id);
    space.netlist.connect_pin(goal_pin_id, net_id);

    // eprintln!(
    //     "[ROUTER] ✓ Pins connected: {} ← {} → {}\n",
    //     from_name, net_name, to_name
    // );

    // PHASE 3: ANALYTIC DESIGN RULE CHECK (v0.1.7 - GOD-TIER)
    //
    // Geometry-based DRC using analytic distance calculations.
    // Nanometer-exact with no grid discretization artifacts.

    // Extract full component names from route endpoints to exclude them from clearance checks
    // (pins are on component boundaries, so routes will naturally touch their own components)
    let source_component = from_component_name.clone();
    let dest_component = to_component_name.clone();

    // Check only the CURRENT route (the last one added) against all components
    // This avoids false positives from previous routes
    let current_route = space
        .analytic_routes
        .last()
        .ok_or_else(|| IrError::EmptyRoute {
            net: net_name.clone(),
        })?;

    let mut violations = Vec::new();
    for (comp_name, comp_bbox) in &space.component_bboxes {
        // Skip source and destination components (pins are on boundaries)
        if comp_name == source_component || comp_name == dest_component {
            continue;
        }

        if !current_route.check_clearance(comp_bbox, min_clearance_nm) {
            // Calculate actual clearance for error reporting
            let half_w = current_route.width_nm / 2;
            let mut min_dist = i64::MAX;

            for seg in &current_route.segments {
                let dist = seg.distance_to_bbox(comp_bbox);
                min_dist = min_dist.min(dist);
            }

            let actual_clearance = min_dist - half_w;
            violations.push((
                current_route.net_name.clone(),
                comp_name.clone(),
                actual_clearance,
            ));
        }
    }

    if !violations.is_empty() {
        let _violation_summary = violations
            .iter()
            .map(|(route_name, comp_name, actual_clearance)| {
                format!(
                    "  - Clearance violation [P18]: Route '{}' too close to component '{}': {:.4}mm actual, {:.4}mm required",
                    route_name,
                    comp_name,
                    *actual_clearance as f64 / 1_000_000.0,
                    min_clearance_nm as f64 / 1_000_000.0
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        return Err(IrError::NoPathFound {
            net: net_name.clone(),
            from_pin: super::helpers::endpoint_label(&route.from).into(),
            to_pin: super::helpers::endpoint_label(&route.to).into(),
        });
    }

    Ok(())
}

/// Calculate boundary points and exit directions for routing.
pub fn calculate_boundary_points(
    space: &HardwareSpace,
    route: &hwc_parser::Route,
    trace_width_nm: i64,
) -> Result<(Point3D, Point3D, (i64, i64), (i64, i64)), IrError> {
    use hwc_engine::geometry_router::port_escape::{
        calculate_rect_escape, CardinalPort, EdgeOffset, NamedPosition,
    };

    // v0.1.9: Extract physical space boundaries for clamping (Fix #1)
    let board_bounds = space.entity_graph.total_bounding_box();

    // Helper to resolve parser EdgeOffsetSpec to engine EdgeOffset
    let resolve_offset = |spec: &Option<hwc_parser::EdgeOffsetSpec>| -> EdgeOffset {
        match spec {
            Some(hwc_parser::EdgeOffsetSpec::Named(pos)) => match pos {
                hwc_parser::NamedPosition::Top => EdgeOffset::Named(NamedPosition::Top),
                hwc_parser::NamedPosition::Bottom => EdgeOffset::Named(NamedPosition::Bottom),
                hwc_parser::NamedPosition::Center => EdgeOffset::Center,
            },
            Some(hwc_parser::EdgeOffsetSpec::Percentage(p)) => EdgeOffset::Percentage(*p),
            Some(hwc_parser::EdgeOffsetSpec::Measurement(m)) => EdgeOffset::Measurement(*m),
            None => EdgeOffset::Center,
        }
    };

    // Get pin center positions for heuristic direction
    let (start_pin_center, goal_pin_center) = get_pin_positions(space, route)?;

    // v0.1.7: Smart Auto-Port Heuristic (Multi-Segment Awareness)
    //
    // Instead of a naive dx > dy check, we prioritize row/column transitions
    // if the secondary axis displacement is significant (> 2mm). This prevents
    // the "box" artifacts where linking routes (e.g. Row 0 -> Row 1) exit
    // from the East/West ports instead of the North/South ports.
    let dx = goal_pin_center.x - start_pin_center.x;
    let dy = goal_pin_center.y - start_pin_center.y;

    let auto_exit_port = if dy.abs() >= 1_000_000 && dy.abs() > dx.abs() / 4 {
        // Significant vertical move: prefer North/South to exit the row
        if dy > 0 {
            CardinalPort::North
        } else {
            CardinalPort::South
        }
    } else if dx.abs() > 0 {
        // Primarily horizontal or small vertical move: prefer East/West
        if dx > 0 {
            CardinalPort::East
        } else {
            CardinalPort::West
        }
    } else {
        // Pure vertical or zero move
        if dy > 0 {
            CardinalPort::North
        } else {
            CardinalPort::South
        }
    };

    let auto_enter_port = if dy.abs() >= 1_000_000 && dy.abs() > dx.abs() / 4 {
        // Significant vertical move: enter from North/South
        if dy > 0 {
            CardinalPort::South
        } else {
            CardinalPort::North
        }
    } else if dx.abs() > 0 {
        // Primarily horizontal: enter from East/West
        if dx > 0 {
            CardinalPort::West
        } else {
            CardinalPort::East
        }
    } else {
        if dy > 0 {
            CardinalPort::South
        } else {
            CardinalPort::North
        }
    };

    // Helper to resolve a port+offset spec to an EscapePoint
    // v0.1.9.1: Handle both ComponentPin and SpaceEntity endpoints using entity registry
    let resolve_point = |endpoint: &hwc_parser::RouteEndpointSpec,
                         port: CardinalPort,
                         offset: EdgeOffset,
                         z: i64| {
        let bbox_opt = match endpoint {
            hwc_parser::RouteEndpointSpec::ComponentPin {
                component_name,
                pin_name,
                ..
            } => {
                // v0.1.9.1: Look up component pin bbox directly from entity registry
                space
                    .entity_graph
                    .get_component_pin_bbox(component_name.as_str(), pin_name.as_str())
            }
            hwc_parser::RouteEndpointSpec::SpaceEntity { name, .. } => {
                // v0.1.9.1: Look up space entity bbox directly from entity registry
                space.entity_graph.get_space_entity_bbox(name.as_str())
            }
        };

        bbox_opt.map(|bbox| {
            // v0.1.7: Use half trace width as clearance to ensure trace touches pad edge
            // but does not penetrate the interior ("physically touching" model)
            let boundary_clearance = trace_width_nm / 2;
            // v0.1.9: Pass board_bounds to prevent out-of-bounds projection (Fix #1)
            calculate_rect_escape(
                &bbox,
                port,
                offset,
                trace_width_nm,
                boundary_clearance,
                z,
                board_bounds.as_ref(),
            )
        })
    };

    let from_label = super::helpers::construct_entity_name(&route.from)?;
    let to_label = super::helpers::construct_entity_name(&route.to)?;

    // Start Escape
    let start_esc = if let Some(exit_escape) = &route.exit_escape {
        let port = match exit_escape.port {
            hwc_parser::CardinalDirection::North => CardinalPort::North,
            hwc_parser::CardinalDirection::South => CardinalPort::South,
            hwc_parser::CardinalDirection::East => CardinalPort::East,
            hwc_parser::CardinalDirection::West => CardinalPort::West,
        };
        let offset = resolve_offset(&exit_escape.offset);
        resolve_point(&route.from, port, offset, start_pin_center.z)
    } else {
        resolve_point(
            &route.from,
            auto_exit_port,
            EdgeOffset::Center,
            start_pin_center.z,
        )
    }
    .ok_or_else(|| IrError::NoPathFound {
        net: format!("{} -> {}", from_label, to_label).into(),
        from_pin: from_label.clone(),
        to_pin: to_label.clone(),
    })?;

    // Goal Escape
    let goal_esc = if let Some(enter_escape) = &route.enter_escape {
        let port = match enter_escape.port {
            hwc_parser::CardinalDirection::North => CardinalPort::North,
            hwc_parser::CardinalDirection::South => CardinalPort::South,
            hwc_parser::CardinalDirection::East => CardinalPort::East,
            hwc_parser::CardinalDirection::West => CardinalPort::West,
        };
        let offset = resolve_offset(&enter_escape.offset);
        resolve_point(&route.to, port, offset, goal_pin_center.z)
    } else {
        resolve_point(
            &route.to,
            auto_enter_port,
            EdgeOffset::Center,
            goal_pin_center.z,
        )
    }
    .ok_or_else(|| IrError::NoPathFound {
        net: format!("{} -> {}", from_label, to_label).into(),
        from_pin: from_label.clone(),
        to_pin: to_label.clone(),
    })?;

    Ok((
        start_esc.point,
        goal_esc.point,
        start_esc.direction,
        goal_esc.direction,
    ))
}

/// Re-register all resolved routes from the analytic routes database into the physical entity graph.
///
/// v0.1.9.1: This function ensures that only the final, detour-aware routes are registered
/// in the physical database, preventing "Double-Registration" bugs where the original
/// straight-line path and the detour path coexist (causing Clipper2 to weld them into a solid sheet).
pub fn re_register_resolved_routes(space: &mut HardwareSpace) -> Result<(), IrError> {
    // 1. Clear ALL old, unrouted physical segments from the database
    // This ensures we have a clean slate before re-populating from the analytic source of truth.
    let net_ids_to_clear: Vec<_> = space
        .entity_graph
        .get_all_routes()
        .iter()
        .map(|(net_id, _)| *net_id)
        .collect();
    for net_id in net_ids_to_clear {
        space.entity_graph.clear_routes_for_net(net_id);
    }

    // 2. Read the resolved paths strictly from the compiled analytic routes
    // Deduplicate by NetId. If multiple routes exist for the same net,
    // prefer the one with more segments (likely a resolved detour) over
    // a single-segment straight line.
    let mut unique_routes: FxHashMap<NetId, hwc_engine::AnalyticTrace> = FxHashMap::default();
    for trace in &space.analytic_routes {
        let entry = unique_routes.entry(trace.net_id);
        match entry {
            std::collections::hash_map::Entry::Vacant(e) => {
                e.insert(trace.clone());
            }
            std::collections::hash_map::Entry::Occupied(mut e) => {
                // If the new trace has more segments, it's likely the resolved detour.
                // The original straight line usually has only 1 segment.
                if trace.segments.len() > e.get().segments.len() {
                    eprintln!(
                        "[AUTO-ROUTER RE-REGISTER] Replacing route for net_id={} ({} segments) with detour ({} segments)",
                        trace.net_id.raw(),
                        e.get().segments.len(),
                        trace.segments.len()
                    );
                    e.insert(trace.clone());
                } else {
                    eprintln!(
                        "[AUTO-ROUTER RE-REGISTER] Skipping redundant route for net_id={} ({} segments) as we already have {} segments",
                        trace.net_id.raw(),
                        trace.segments.len(),
                        e.get().segments.len()
                    );
                }
            }
        }
    }

    for (net_id, route_trace) in unique_routes {
        eprintln!(
            "[AUTO-ROUTER RE-REGISTER] net_id={}, {} segments from analytic_routes",
            net_id.raw(),
            route_trace.segments.len()
        );

        let trace_segments: Vec<hwc_engine::geometry::TraceSegment> = route_trace
            .segments
            .iter()
            .map(|line_seg| {
                hwc_engine::geometry::TraceSegment::new(
                    line_seg.start,
                    line_seg.end,
                    route_trace.width_nm,
                    route_trace.material as u8,
                )
            })
            .collect();

        // Register in entity_graph for DRC and continuity checking
        if !trace_segments.is_empty() {
            space
                .entity_graph
                .register_trace_segments(net_id, trace_segments);
        }
    }

    Ok(())
}
