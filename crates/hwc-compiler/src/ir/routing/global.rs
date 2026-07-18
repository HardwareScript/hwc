//! Global automatic routing for all nets in the netlist.
//!
//! This module implements the top-level "route everything" logic used by the CLI.
//! v0.1.7: Uses `GeometryRouter::route_space()` which selects between Pass-Through
//! (flat) and Hierarchical (G-Cell + Rayon) modes based on net count and board area.

use crate::ir::errors::IrError;
use compact_str::CompactString;
use hwc_engine::{geometry::Point3D, geometry_router::GridBounds, netlist::NetId, HardwareSpace};
use rustc_hash::FxHashMap;

/// Global automatic router for connecting all pins in the netlist.
pub struct AutoRouter<'a> {
    space: &'a mut HardwareSpace,
    /// Stackup manager for Z-axis resolution
    stackup_manager: &'a crate::ir::stackup_manager::StackupManager,
    /// Active profile definition (for ASIC detection and layer info)
    profile: Option<&'a hwc_parser::ProfileDefinition>,
    /// v0.1.7: Net frequencies in Hz for SI-aware routing (high-speed void avoidance).
    net_frequencies: FxHashMap<NetId, f64>,
    /// v0.1.7: Individual route requests (from Hardware Script 'route' statements)
    auto_routes: Vec<hwc_parser::Route>,
    /// v0.1.8: Salsa-style memoized query store for per-G-cell routing cache.
    /// When present, hierarchical G-Cell routing results are memoized so that
    /// unchanged G-cells return cached results on incremental rebuilds.
    query_store: Option<hwc_engine::geometry_router::query_engine::QueryStore>,
    /// v0.1.8: Per-net routing pattern policies from `route net:` statements.
    route_net_policies: FxHashMap<NetId, hwc_engine::RoutingPattern>,
}

#[derive(Debug, Clone)]
struct PinInfo {
    position: Point3D,
}

impl<'a> AutoRouter<'a> {
    /// Create a new global automatic router.
    pub fn new(
        space: &'a mut HardwareSpace,
        _symbol_table: &'a crate::SymbolTable,
        stackup_manager: &'a crate::ir::stackup_manager::StackupManager,
        profile: Option<&'a hwc_parser::ProfileDefinition>,
        net_frequencies: FxHashMap<NetId, f64>,
        auto_routes: Vec<hwc_parser::Route>,
        route_net_policies: FxHashMap<NetId, hwc_engine::RoutingPattern>,
    ) -> Self {
        Self {
            space,
            stackup_manager,
            profile,
            net_frequencies,
            auto_routes,
            query_store: None,
            route_net_policies,
        }
    }

    /// v0.1.8: Set the memoized query store for per-G-cell routing cache.
    ///
    /// When a QueryStore is provided, hierarchical G-Cell routing results
    /// are memoized so that unchanged G-cells return cached results on
    /// incremental rebuilds. Call this before `route_all_nets()`.
    pub fn set_query_store(
        &mut self,
        query_store: hwc_engine::geometry_router::query_engine::QueryStore,
    ) {
        self.query_store = Some(query_store);
    }

    /// Resolve a copper material ID from the stackup for use as the routing material.
    ///
    /// Samples the first routed path's Z position to determine the layer material.
    /// Returns the material ID from the registry, or `IrError::UndeclaredMaterial` if
    /// the stackup or material is not properly declared.
    fn resolve_sample_copper_id(&self) -> Result<hwc_engine::material::MaterialId, IrError> {
        let sample_z = self.space.resolution_nm; // Default: bottom of board
        if let Some(layer_name) = self.stackup_manager.get_layer_name_at_z(sample_z) {
            let mat_name = self
                .profile
                .and_then(|p| p.stackup.as_ref())
                .and_then(|stackup| {
                    stackup
                        .layers
                        .iter()
                        .find(|l| l.name.name == layer_name)
                        .map(|l| l.material.clone())
                })
                .ok_or_else(|| IrError::UndeclaredMaterial {
                    material: format!("No material defined for layer '{}'", layer_name).into(),
                })?;
            self.space
                .material_registry
                .get_id(&mat_name)
                .ok_or_else(|| IrError::UndeclaredMaterial { material: mat_name })
        } else {
            Err(IrError::UndeclaredMaterial {
                material: "No stackup layer found for routing material resolution".into(),
            })
        }
    }

    /// v0.1.8: Retrieve the memoized query store after routing completes.
    ///
    /// The caller should hold onto this store and pass it back via
    /// `set_query_store()` on the next compilation to enable incremental
    /// rebuilds where unchanged G-cells return cached results.
    pub fn take_query_store(
        &mut self,
    ) -> Option<hwc_engine::geometry_router::query_engine::QueryStore> {
        self.query_store.take()
    }

    /// v0.1.8: Invalidate memoized routing results for specific G-cells.
    ///
    /// When a route is edited, call this to invalidate only the affected
    /// G-cells. All other G-cells remain cached for the next compilation.
    ///
    /// # Arguments
    /// * `file_id` - The file/space identifier
    /// * `affected_gcell_ids` - G-cell indices whose routing results are stale
    pub fn invalidate_gcells(&mut self, file_id: u64, affected_gcell_ids: &[u32]) {
        if let Some(ref mut qs) = self.query_store {
            for &gcell_id in affected_gcell_ids {
                qs.invalidate_gcell(file_id, gcell_id);
            }
        }
    }

    /// v0.1.8: Invalidate memoized routing results for boundary port relocations.
    ///
    /// When a boundary port moves, only the two adjacent G-cells are affected.
    /// All other G-cells remain cached.
    pub fn invalidate_boundary_port(&mut self, file_id: u64, adjacent_cell_ids: (u32, u32)) {
        if let Some(ref mut qs) = self.query_store {
            qs.invalidate_boundary_port(file_id, adjacent_cell_ids);
        }
    }

    /// Route all nets in the design using the GeometryRouter adaptive pipeline.
    ///
    /// v0.1.7: Replaces the per-net SDF loop with a single call to
    /// `GeometryRouter::route_space()`, which selects between Pass-Through
    /// (flat) and Hierarchical (G-Cell + Rayon) modes based on net count
    /// and board area. The result is converted back to `AnalyticTrace`
    /// primitives for the rest of the pipeline.
    pub fn route_all_nets(&mut self) -> Result<(), IrError> {
        use hwc_engine::geometry::BoundingBox;

        // Phase 1: Build the nets HashMap required by GeometryRouter::route_space()
        // v0.1.7 Chain-Link Logic: If explicit 'route' statements exist, we route them
        // as individual segments to preserve the user's intended topology.
        let mut geo_nets: FxHashMap<NetId, Vec<Point3D>> = FxHashMap::default();
        let mut explicit_segments: Vec<(NetId, Vec<Point3D>)> = Vec::new();
        let mut net_id_to_name: FxHashMap<NetId, CompactString> = FxHashMap::default();
        let mut route_segments: Vec<(NetId, Vec<Point3D>)> = Vec::new();
        // v0.1.8: Track which nets have a layer override for vertical transition segments.
        // Keyed by net name (stable across Chain-Link ID remapping).
        let mut net_layer_targets: FxHashMap<CompactString, i64> = FxHashMap::default();
        // v0.1.8: Track declared route widths per net for analytic trace registration.
        // Keyed by net name. Falls back to profile min_width if not specified.
        let mut net_declared_widths: FxHashMap<CompactString, i64> = FxHashMap::default();
        // v0.1.8: Track declared route currents per net for thermal validation.
        // Keyed by net name. Falls back to 20mA if not specified.
        let mut net_currents_ma: FxHashMap<CompactString, f64> = FxHashMap::default();

        if !self.auto_routes.is_empty() {
            // eprintln!("[ROUTER] Using Chain-Link mode ({} explicit routes)", self.auto_routes.len());
            let auto_routes = self.auto_routes.clone();

            // v0.1.7: Port Occupancy Tracking
            // Tracks which ports are already used on each pad to ensure chain connections
            // use different ports for entry and exit.
            // Key: (Component, Pin) -> Vec<CardinalPort>
            let mut used_ports: FxHashMap<
                (CompactString, CompactString),
                Vec<hwc_engine::geometry_router::port_escape::CardinalPort>,
            > = FxHashMap::default();

            for route in &auto_routes {
                // Get or create net ID for this route
                let net_id = self.find_net_id_for_name("TEMP_NET")?; // Dummy for resolution
                let actual_net_id = crate::ir::routing::register_net_for_route(
                    self.space,
                    route,
                    &crate::SymbolTable::new(),
                    self.stackup_manager,
                    self.profile,
                    None,
                )
                .unwrap_or(net_id);
                let net_name = self
                    .space
                    .netlist
                    .get_net(actual_net_id)
                    .map(|n| n.name.clone())
                    .unwrap_or_else(|| "unnamed".into());

                // Resolve pin positions to boundary points (GOD-TIER ALIGNMENT)
                let min_width = self.space.fabrication_constraints.as_ref().map(|c| c.trace.min_width_nm)
                    .ok_or_else(|| IrError::MissingAsicConstraint {
                        message: format!("Route requires trace width constraint but none are loaded."),
                        hint: "Ensure a profile with 'trace:' constraints is declared in the space definition.".into(),
                    })?;

                // v0.1.7: Smart Chain-Link Port Selection
                // We manually resolve ports here if they haven't been specified,
                // checking for collisions with already-used ports on the same net.
                let mut modified_route = route.clone();
                let from_key = match &route.from {
                    hwc_parser::RouteEndpointSpec::ComponentPin {
                        component_name,
                        pin_name,
                        ..
                    } => (component_name.clone(), pin_name.clone()),
                    hwc_parser::RouteEndpointSpec::SpaceEntity { name, .. } => {
                        (name.clone(), CompactString::default())
                    }
                };
                let to_key = match &route.to {
                    hwc_parser::RouteEndpointSpec::ComponentPin {
                        component_name,
                        pin_name,
                        ..
                    } => (component_name.clone(), pin_name.clone()),
                    hwc_parser::RouteEndpointSpec::SpaceEntity { name, .. } => {
                        (name.clone(), CompactString::default())
                    }
                };

                if modified_route.exit_escape.is_none() {
                    // Try to find a free port
                    if let Ok((start, goal, _, _)) =
                        crate::ir::routing::calculate_boundary_points(self.space, route, min_width)
                    {
                        // Check if the default port is already used
                        let dx = goal.x - start.x;
                        let dy = goal.y - start.y;

                        // Re-run heuristic locally to see what was picked
                        let mut port = if dy.abs() >= 1_000_000 && dy.abs() > dx.abs() / 4 {
                            if dy > 0 {
                                hwc_engine::geometry_router::port_escape::CardinalPort::North
                            } else {
                                hwc_engine::geometry_router::port_escape::CardinalPort::South
                            }
                        } else {
                            if dx > 0 {
                                hwc_engine::geometry_router::port_escape::CardinalPort::East
                            } else {
                                hwc_engine::geometry_router::port_escape::CardinalPort::West
                            }
                        };

                        // v0.1.7: Flow-Through Preference
                        // If this pad already has an entry port, prefer the OPPOSITE port
                        // for the exit to create a clean "flow-through" chain.
                        if let Some(used) = used_ports.get(&from_key) {
                            if !used.is_empty() {
                                let last_port = used[0];
                                let opposite = match last_port {
                                    hwc_engine::geometry_router::port_escape::CardinalPort::North => hwc_engine::geometry_router::port_escape::CardinalPort::South,
                                    hwc_engine::geometry_router::port_escape::CardinalPort::South => hwc_engine::geometry_router::port_escape::CardinalPort::North,
                                    hwc_engine::geometry_router::port_escape::CardinalPort::East => hwc_engine::geometry_router::port_escape::CardinalPort::West,
                                    hwc_engine::geometry_router::port_escape::CardinalPort::West => hwc_engine::geometry_router::port_escape::CardinalPort::East,
                                };
                                if !used.contains(&opposite) {
                                    port = opposite;
                                }
                            }

                            // Still collision? Cycle through remaining
                            if used.contains(&port) {
                                let ports = [
                                    hwc_engine::geometry_router::port_escape::CardinalPort::East,
                                    hwc_engine::geometry_router::port_escape::CardinalPort::West,
                                    hwc_engine::geometry_router::port_escape::CardinalPort::North,
                                    hwc_engine::geometry_router::port_escape::CardinalPort::South,
                                ];
                                for p in ports {
                                    if !used.contains(&p) {
                                        port = p;
                                        break;
                                    }
                                }
                            }
                        }

                        used_ports.entry(from_key).or_default().push(port);
                        modified_route.exit_escape = Some(hwc_parser::RouteEscape {
                            port: match port {
                                hwc_engine::geometry_router::port_escape::CardinalPort::North => {
                                    hwc_parser::CardinalDirection::North
                                }
                                hwc_engine::geometry_router::port_escape::CardinalPort::South => {
                                    hwc_parser::CardinalDirection::South
                                }
                                hwc_engine::geometry_router::port_escape::CardinalPort::East => {
                                    hwc_parser::CardinalDirection::East
                                }
                                hwc_engine::geometry_router::port_escape::CardinalPort::West => {
                                    hwc_parser::CardinalDirection::West
                                }
                            },
                            offset: None,
                            span: route.span,
                        });
                    }
                }

                if modified_route.enter_escape.is_none() {
                    // Try to find a free port for the target
                    if let Ok((start, goal, _, _)) =
                        crate::ir::routing::calculate_boundary_points(self.space, route, min_width)
                    {
                        let dx = goal.x - start.x;
                        let dy = goal.y - start.y;

                        let mut port = if dy.abs() >= 1_000_000 && dy.abs() > dx.abs() / 4 {
                            if dy > 0 {
                                hwc_engine::geometry_router::port_escape::CardinalPort::South
                            } else {
                                hwc_engine::geometry_router::port_escape::CardinalPort::North
                            }
                        } else {
                            if dx > 0 {
                                hwc_engine::geometry_router::port_escape::CardinalPort::West
                            } else {
                                hwc_engine::geometry_router::port_escape::CardinalPort::East
                            }
                        };

                        if let Some(used) = used_ports.get(&to_key) {
                            // v0.1.7: Flow-Through Preference for destination
                            if !used.is_empty() {
                                let last_port = used[0];
                                let opposite = match last_port {
                                    hwc_engine::geometry_router::port_escape::CardinalPort::North => hwc_engine::geometry_router::port_escape::CardinalPort::South,
                                    hwc_engine::geometry_router::port_escape::CardinalPort::South => hwc_engine::geometry_router::port_escape::CardinalPort::North,
                                    hwc_engine::geometry_router::port_escape::CardinalPort::East => hwc_engine::geometry_router::port_escape::CardinalPort::West,
                                    hwc_engine::geometry_router::port_escape::CardinalPort::West => hwc_engine::geometry_router::port_escape::CardinalPort::East,
                                };
                                if !used.contains(&opposite) {
                                    port = opposite;
                                }
                            }

                            if used.contains(&port) {
                                let ports = [
                                    hwc_engine::geometry_router::port_escape::CardinalPort::West,
                                    hwc_engine::geometry_router::port_escape::CardinalPort::East,
                                    hwc_engine::geometry_router::port_escape::CardinalPort::South,
                                    hwc_engine::geometry_router::port_escape::CardinalPort::North,
                                ];
                                for p in ports {
                                    if !used.contains(&p) {
                                        port = p;
                                        break;
                                    }
                                }
                            }
                        }

                        used_ports.entry(to_key).or_default().push(port);
                        modified_route.enter_escape = Some(hwc_parser::RouteEscape {
                            port: match port {
                                hwc_engine::geometry_router::port_escape::CardinalPort::North => {
                                    hwc_parser::CardinalDirection::North
                                }
                                hwc_engine::geometry_router::port_escape::CardinalPort::South => {
                                    hwc_parser::CardinalDirection::South
                                }
                                hwc_engine::geometry_router::port_escape::CardinalPort::East => {
                                    hwc_parser::CardinalDirection::East
                                }
                                hwc_engine::geometry_router::port_escape::CardinalPort::West => {
                                    hwc_parser::CardinalDirection::West
                                }
                            },
                            offset: None,
                            span: route.span,
                        });
                    }
                }

                // v0.1.9: Resolve the route's declared width for boundary point calculation
                // The boundary clearance must use the ACTUAL trace width, not the PDK minimum.
                // Otherwise, wide traces (e.g., 50µm power rails) will penetrate into pads.
                let route_width_nm = if let Some(ref width_expr) = route.width {
                    crate::ir::conversions::evaluate_expression_to_nm(
                        width_expr,
                        &crate::SymbolTable::new(),
                    )
                    .unwrap_or(min_width)
                } else {
                    min_width
                };

                match crate::ir::routing::calculate_boundary_points(
                    self.space,
                    &modified_route,
                    route_width_nm, // Use actual route width, not PDK minimum
                ) {
                    Ok((start, goal, _, _)) => {
                        route_segments.push((actual_net_id, vec![start, goal]));
                        net_id_to_name.insert(actual_net_id, net_name.clone());
                        // v0.1.8: Track layer override for vertical transition segments
                        if let Some(ref layer_id) = route.layer {
                            if let Some(target_z) =
                                self.stackup_manager.get_layer_start_z(&layer_id.name)
                            {
                                net_layer_targets.insert(net_name.clone(), target_z);
                            }
                        }
                        // v0.1.8: Track declared route width for analytic trace registration.
                        // Resolves user-specified width (e.g. `width: 500nm`) to nanometers.
                        if let Some(ref width_expr) = route.width {
                            if let Ok(w_nm) = crate::ir::conversions::evaluate_expression_to_nm(
                                width_expr,
                                &crate::SymbolTable::new(),
                            ) {
                                net_declared_widths.insert(net_name.clone(), w_nm);
                            }
                        }
                        // v0.1.8: Track declared route current for thermal validation.
                        if let Some(ref ac) = route.current_limit_ac {
                            let rms = crate::ir::conversions::evaluate_expression_to_ma(
                                &ac.rms,
                                &crate::SymbolTable::new(),
                            )
                            .unwrap_or(0.0);
                            let peak = crate::ir::conversions::evaluate_expression_to_ma(
                                &ac.peak,
                                &crate::SymbolTable::new(),
                            )
                            .unwrap_or(rms);
                            net_currents_ma.insert(net_name, peak);
                        }
                    }
                    Err(e) => {
                        eprintln!("[ROUTER WARNING] Failed to calculate boundary points for route on net '{}': {:?} - skipping", net_name, e);
                    }
                }
            }
        }

        // If no explicit routes, fallback to netlist pin grouping (Legacy mode)
        if route_segments.is_empty() {
            let net_pins = self.analyze_nets()?;
            for (net_name, pins) in &net_pins {
                if pins.len() < 2 {
                    continue;
                }
                let net_id = self.find_net_id_for_name(net_name)?;
                let coords: Vec<Point3D> = pins.iter().map(|p| p.position).collect();
                geo_nets.insert(net_id, coords);
                net_id_to_name.insert(net_id, net_name.clone());

                // v0.1.8: Pull current from netlist for legacy/auto-nets
                if let Some(net_data) = self.space.netlist.get_net(net_id) {
                    if let Some(c) = net_data.current_ma {
                        net_currents_ma.insert(net_data.name.clone(), c);
                    }
                }
            }
        } else {
            // v0.1.7: Unified Net IDs for Chain-Link mode.
            // By using the same logical NetId for all segments, the engine's
            // same-net collision bypass and crosstalk-exemption logic kicks in.
            // This ensures traces from the same pad don't "bump" away from each other.
            explicit_segments = route_segments.clone();

            // v0.1.8: We no longer populate geo_nets with explicit segments here.
            // Doing so causes the Adaptive Router to route them a second time
            // (once via Chain-Link and once via Steiner/Hierarchical), creating
            // redundant parallel traces. The "0 nets routed" log is preferred
            // over physically incorrect redundant copper.
            for (net_id, _points) in &route_segments {
                let name = net_id_to_name
                    .entry(*net_id)
                    .or_insert_with(|| {
                        compact_str::CompactString::from(format!("chain_net_{}", net_id.raw()))
                    })
                    .clone();

                // v0.1.8: Pull current from netlist for explicit routes
                if let Some(net_data) = self.space.netlist.get_net(*net_id) {
                    if let Some(c) = net_data.current_ma {
                        net_currents_ma.entry(name).or_insert(c);
                    }
                }
            }
        }

        if geo_nets.is_empty() && explicit_segments.is_empty() {
            eprintln!(
                "[ROUTER WARNING] No nets to route! geo_nets and explicit_segments are both empty"
            );
            eprintln!("  route_segments.len() = {}", route_segments.len());
            eprintln!("  auto_routes.len() = {}", self.auto_routes.len());
            return Ok(());
        }

        // Phase 3: Collect obstacle bounding boxes from placed components
        let mut obstacle_bboxes: Vec<BoundingBox> = Vec::new();
        for metadata in self.space.entity_graph.get_component_metadata() {
            obstacle_bboxes.push(metadata.bbox);
        }
        
        // v0.1.9: Also collect planes without net assignments as obstacles
        // These are keepout zones, mechanical structures, or non-conductive features
        // eprintln!("[OBSTACLE DEBUG] Collecting substrate layers as obstacles...");
        // eprintln!("[OBSTACLE DEBUG] Total substrate layers: {}", self.space.entity_graph.get_substrate_layers().len());
        for (_idx, layer) in self.space.entity_graph.get_substrate_layers().iter().enumerate() {
            // eprintln!(
            //     "[OBSTACLE DEBUG] Layer {}: net={}, layer_type={:?}, bbox=({},{},{}) to ({},{},{})",
            //     idx,
            //     layer.net,
            //     layer.layer_type,
            //     layer.bbox.min.x, layer.bbox.min.y, layer.bbox.min.z,
            //     layer.bbox.max.x, layer.bbox.max.y, layer.bbox.max.z
            // );
            if layer.net == 0 && layer.layer_type == hwc_engine::geometry_router::entity_graph::SubstrateLayerType::Pour {
                // eprintln!("[OBSTACLE DEBUG] ✓ Adding layer {} as obstacle", idx);
                obstacle_bboxes.push(layer.bbox);
            } else {
                // eprintln!("[OBSTACLE DEBUG] ✗ Skipping layer {} (net={}, type={:?})", idx, layer.net, layer.layer_type);
            }
        }
        
        // Also register manual analytic traces as obstacles
        for trace in &self.space.analytic_routes {
            for segment in &trace.segments {
                obstacle_bboxes.push(segment.to_bounding_box(trace.width_nm));
            }
        }

        // Phase 4: Build grid bbox and create GeometryRouter
        let grid_bbox = BoundingBox::new(
            Point3D::new(0, 0, 0),
            Point3D::new(
                self.space.dimensions.width_nm,
                self.space.dimensions.height_nm,
                self.space.dimensions.depth_nm,
            ),
        );

        let grid_bounds = GridBounds::new(
            self.space.dimensions.width_nm,
            self.space.dimensions.height_nm,
            self.space.dimensions.depth_nm,
        );

        let mut constraints =
            hwc_engine::constraint_manager::ConstraintRulebook::new(self.space.resolution_nm);

        // v0.1.9: Set fabrication constraints from space (required for boundary port resolution)
        if let Some(ref constraint_set) = self.space.fabrication_constraints {
            use hwc_engine::constraint_manager::{FabricationConstraints, StackupInfo};

            let stackup = constraint_set.stackup.as_ref().map(|s| StackupInfo {
                dielectric_height_nm: s.dielectric_height_nm,
                copper_thickness_nm: s.copper_thickness_nm,
                relative_permittivity: s.relative_permittivity,
                default_impedance_ohm: s.default_impedance_ohm,
            });

            let fab_constraints = FabricationConstraints {
                min_trace_width_nm: constraint_set.trace.min_width_nm,
                min_trace_spacing_nm: constraint_set.trace.min_spacing_nm,
                min_via_diameter_nm: constraint_set.via.min_diameter_nm,
                default_via_diameter_nm: constraint_set.via.default_diameter_nm,
                min_annular_ring_nm: constraint_set.via.min_annular_ring_nm,
                min_spacing_nm: constraint_set.via.min_spacing_nm,
                low_voltage_clearance_nm: constraint_set.clearance.low_voltage_nm,
                medium_voltage_clearance_nm: constraint_set.clearance.medium_voltage_nm,
                high_voltage_clearance_nm: constraint_set.clearance.high_voltage_nm,
                safety_factor: constraint_set.clearance.safety_factor,
                stackup,
                solder_mask_expansion_nm: constraint_set.solder_mask_expansion_nm,
                technology: constraint_set.technology.clone(),
            };

            constraints.set_fabrication_constraints(fab_constraints);
        }

        let mut geo_router = hwc_engine::GeometryRouter::new(
            grid_bounds,
            constraints,
            self.space.material_registry.clone(),
        );

        // v0.1.9: Set routing material and trace width so every segment stamped
        // into the EntityGraph carries the correct physical properties.
        // BUG FIX: Use the widest trace width from all declared routes as the default.
        // Individual routes may override this, but we need a sensible default for
        // routes that don't specify a width.
        {
            // Find the maximum declared width across all routes, or fallback to PDK minimum
            let trace_width = net_declared_widths
                .values()
                .max()
                .copied()
                .or_else(|| {
                    self.space
                        .fabrication_constraints
                        .as_ref()
                        .map(|c| c.trace.min_width_nm)
                })
                .ok_or_else(|| IrError::MissingAsicConstraint {
                    message: "PDK missing required 'trace.min_width_nm' constraint".into(),
                    hint: "Add a 'trace:' block to your profile with explicit min_width.".into(),
                })?;
            let routing_copper_id =
                self.resolve_sample_copper_id()
                    .map_err(|e| IrError::InvalidRouteExpression {
                        expression: "routing material".into(),
                        reason: e.to_string(),
                    })?;
            eprintln!("[COMPILER] Setting routing context: trace_width={} nm (from {} declared routes)", 
                trace_width, net_declared_widths.len());
            geo_router.set_routing_context(routing_copper_id, trace_width);
        }

        // v0.1.8: Wire per-net routing pattern policies into the GeometryRouter.
        if !self.route_net_policies.is_empty() {
            // eprintln!(
            //     "[ROUTER] Wiring {} net routing policies (patterns)",
            //     self.route_net_policies.len()
            // );
            geo_router.set_route_net_policies(self.route_net_policies.clone());
        }

        // v0.1.8: Wire the memoized query store into the GeometryRouter.
        // This enables per-G-cell routing memoization in hierarchical mode.
        if let Some(qs) = self.query_store.take() {
            geo_router.query_store = Some(qs);
        }

        // v0.1.7: Configure profile mode for the router.
        // ASIC profiles use Manhattan angle restriction (layer-by-layer via unrolling).
        // PCB profiles use Octilinear (single through-hole via for multi-layer transitions).
        // In both cases, layer info is needed for via tower unrolling.
        if let Some(profile) = self.profile {
            let is_manhattan = profile.is_asic();
            let profile_layers: Vec<String> = self.stackup_manager.ordered_layers().to_vec();
            if !profile_layers.is_empty() {
                let layer_z_positions: Vec<i64> = profile_layers
                    .iter()
                    .map(|name| {
                        self.stackup_manager.get_layer_start_z(name).ok_or_else(|| {
                            IrError::InvalidRouteExpression {
                                expression: format!("stackup layer '{}'", name),
                                reason: "Layer exists in profile list but not in physical stackup."
                                    .into(),
                            }
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;

                // v0.1.9.1: Build layer-to-material mapping for Z-aware route registration
                let layer_materials: Vec<u8> = profile_layers
                    .iter()
                    .map(|name| {
                        let mat_name = profile
                            .stackup
                            .as_ref()
                            .and_then(|stackup| {
                                stackup
                                    .layers
                                    .iter()
                                    .find(|l| l.name.name == *name)
                                    .map(|l| l.material.clone())
                            })
                            .ok_or_else(|| IrError::UndeclaredMaterial {
                                material: format!("No material defined for layer '{}'", name)
                                    .into(),
                            })?;
                        self.space
                            .material_registry
                            .get_id(&mat_name)
                            .ok_or_else(|| IrError::UndeclaredMaterial { material: mat_name })
                    })
                    .collect::<Result<Vec<_>, _>>()?;

                // eprintln!(
                //     "[ROUTER] {} mode enabled: {} layers, {} layer Z positions",
                //     if is_manhattan { "ASIC" } else { "PCB" },
                //     profile_layers.len(),
                //     layer_z_positions.len()
                // );
                geo_router.set_profile_mode(
                    is_manhattan,
                    profile_layers,
                    layer_z_positions,
                    layer_materials,
                );
            }

            // v0.1.8: Set routing heuristic weights from PDK profile.
            // All cost function weights must come from the profile's `routing:` block.
            // No hardcoded values — the compiler is a deterministic engine.
            if let Some(routing) = &profile.routing {
                let heuristics = hwc_engine::geometry_router::RoutingHeuristics {
                    base_cost: routing.base_cost
                        .ok_or_else(|| IrError::MissingRoutingHeuristics {
                            field: "base_cost".into(),
                            hint: "Add 'base_cost: <value>' to the profile's 'routing:' block.".into(),
                        })?,
                    via_penalty: routing.via_penalty
                        .ok_or_else(|| IrError::MissingRoutingHeuristics {
                            field: "via_penalty".into(),
                            hint: "Add 'via_penalty: <value>' to the profile's 'routing:' block.".into(),
                        })?,
                    direction_penalty: routing.direction_penalty
                        .ok_or_else(|| IrError::MissingRoutingHeuristics {
                            field: "direction_penalty".into(),
                            hint: "Add 'direction_penalty: <value>' to the profile's 'routing:' block.".into(),
                        })?,
                    tight_clearance_penalty: routing.tight_clearance_penalty
                        .ok_or_else(|| IrError::MissingRoutingHeuristics {
                            field: "tight_clearance_penalty".into(),
                            hint: "Add 'tight_clearance_penalty: <value>' to the profile's 'routing:' block.".into(),
                        })?,
                    crosstalk_penalty: routing.crosstalk_penalty
                        .ok_or_else(|| IrError::MissingRoutingHeuristics {
                            field: "crosstalk_penalty".into(),
                            hint: "Add 'crosstalk_penalty: <value>' to the profile's 'routing:' block.".into(),
                        })?,
                    impedance_penalty: routing.impedance_penalty
                        .ok_or_else(|| IrError::MissingRoutingHeuristics {
                            field: "impedance_penalty".into(),
                            hint: "Add 'impedance_penalty: <value>' to the profile's 'routing:' block.".into(),
                        })?,
                    reference_void_penalty: routing.reference_void_penalty
                        .ok_or_else(|| IrError::MissingRoutingHeuristics {
                            field: "reference_void_penalty".into(),
                            hint: "Add 'reference_void_penalty: <value>' to the profile's 'routing:' block.".into(),
                        })?,
                };
                geo_router.set_routing_heuristics(heuristics);
            } else {
                return Err(IrError::MissingRoutingHeuristics {
                    field: "routing".into(),
                    hint: "Add a 'routing:' block to the profile with all heuristic weights."
                        .into(),
                });
            }
        } else {
            return Err(IrError::MissingRoutingHeuristics {
                field: "profile".into(),
                hint: "Declare a profile with routing heuristics in the space definition.".into(),
            });
        }

        // Register component obstacles and pins with the GeometryRouter
        for metadata in self.space.entity_graph.get_component_metadata() {
            geo_router.add_component_obstacle(
                metadata.bbox,
                metadata.material,
                metadata.name.clone(),
                metadata.component_type.clone(),
            );
        }
        // NOTE: Pour substrate layers are NOT registered as component obstacles here.
        // They are already handled in build_routing_spatial_index (section 2) as SubstrateLayer
        // obstacles with proper net_id filtering. Adding them here too would cause the same
        // physical obstacle to appear twice in the spatial index with different segment_ids,
        // resulting in spurious duplicate collision detections during routing.
        for pin in self.space.entity_graph.get_component_pins() {
            // v0.1.8: We no longer add component pins explicitly here if they are already
            // part of the geo_nets/explicit_segments map, as the GeometryRouter handles
            // docking automatically during route_space(). Adding them twice can cause
            // the global planner to create zero-length segments that confuse the local router.
            if !geo_nets
                .values()
                .any(|pts| pts.contains(&Point3D::new(pin.x_nm, pin.y_nm, pin.z_nm)))
            {
                geo_router.add_component_pin(
                    pin.x_nm,
                    pin.y_nm,
                    pin.z_nm,
                    pin.component_name.clone(),
                    pin.pin_name.clone(),
                    pin.net.clone(),
                );
            }
        }

        // Phase 5: Route all nets via GeometryRouter (adaptive mode selection)
        let _t_route = std::time::Instant::now();
        let substrate_layers = self.space.entity_graph.get_substrate_layers();
        let has_substrate = !substrate_layers.is_empty();

        // v0.1.9: Convert net_declared_widths (keyed by name) to NetId-keyed map
        let mut net_trace_widths_by_id = FxHashMap::default();
        for (net_name, &width_nm) in &net_declared_widths {
            if let Some(&net_id) = net_id_to_name.iter().find_map(|(id, name)| {
                if name == net_name {
                    Some(id)
                } else {
                    None
                }
            }) {
                net_trace_widths_by_id.insert(net_id, width_nm);
            }
        }

        eprintln!(
            "[COMPILER] Passing {} net trace widths to router:",
            net_trace_widths_by_id.len()
        );
        for (net_id, width) in &net_trace_widths_by_id {
            eprintln!("  net_id={}: width={} nm", net_id.raw(), width);
        }

        // eprintln!("[ROUTER DEBUG] About to call route_space:");
        // eprintln!("  geo_nets.len() = {}", geo_nets.len());
        // eprintln!("  explicit_segments.len() = {}", explicit_segments.len());
        // eprintln!("  obstacle_bboxes.len() = {}", obstacle_bboxes.len());
        // eprintln!("  has_substrate = {}", has_substrate);

        match geo_router.route_space(
            &grid_bbox,
            &geo_nets,
            if explicit_segments.is_empty() {
                None
            } else {
                Some(&explicit_segments)
            },
            &obstacle_bboxes,
            if has_substrate {
                Some(substrate_layers)
            } else {
                None
            },
            &self.net_frequencies,
            &net_trace_widths_by_id,
        ) {
            Ok(result) => {
                // eprintln!(
                //     "[ROUTER] GeometryRouter complete: {} nets routed, {} vias placed ({}ms)",
                //     result.paths.len(),
                //     result.vias.len(),
                //     t_route.elapsed().as_millis()
                // );

                // v0.1.8: Phase 2 - Post-Route Meander Injection
                // Only processes nets with `route net:` pattern policies (<5% of nets).
                // Injects meander patterns analytically in O(1) per net, then resolves
                // local collisions via O(V) DAG compaction (no full re-route).
                let result = if !self.route_net_policies.is_empty() {
                    let _t_meander = std::time::Instant::now();
                    let trace_width = self.space.fabrication_constraints.as_ref()
                        .map(|c| c.trace.min_width_nm)
                        .ok_or_else(|| IrError::MissingAsicConstraint {
                            message: "Meander injection requires trace constraints but none are loaded.".into(),
                            hint: "Ensure a profile with 'trace:' constraints is declared in the space definition.".into(),
                        })?;
                    let min_clearance = self.space.fabrication_constraints.as_ref()
                        .map(|c| c.trace.min_spacing_nm)
                        .ok_or_else(|| IrError::MissingAsicConstraint {
                            message: "Meander injection requires trace spacing constraints but none are loaded.".into(),
                            hint: "Ensure a profile with 'trace:' constraints is declared in the space definition.".into(),
                        })?;
                    let injector = crate::ir::meander_injection::MeanderInjector::new(
                        &self.route_net_policies,
                        &obstacle_bboxes,
                        trace_width,
                        min_clearance,
                    );
                    let result = injector.inject(result);

                    // v0.1.8: CRITICAL STATE-SYNC
                    // The meander injector mutated paths in the RouteResult, but the
                    // EntityGraph still holds the original, stale straight segments.
                    // Sync the expanded meander paths back so exporters see the real geometry.
                    let routing_copper_id = self.resolve_sample_copper_id()?;
                    let mut _total_meander_segments = 0usize;
                    for (&net_id, mutated_paths) in &result.paths {
                        self.space.entity_graph.clear_routes_for_net(net_id);
                        for path in mutated_paths {
                            self.space.entity_graph.register_route(
                                net_id,
                                path,
                                routing_copper_id,
                                trace_width,
                            );
                            _total_meander_segments += path.len().saturating_sub(1);
                        }
                    }
                    // eprintln!(
                    //     "[ROUTER] Meander injection: {} policy nets processed, {} expanded segments synced to EntityGraph ({}ms)",
                    //     self.route_net_policies.len(),
                    //     total_meander_segments,
                    //     t_meander.elapsed().as_millis()
                    // );
                    result
                } else {
                    result
                };

                // Convert RouteResult paths back to AnalyticTrace primitives
                // v0.1.7: Resolve copper thickness from the stackup for the routing layer.
                let trace_thickness_nm = {
                    let default_thickness = self.space.resolution_nm;
                    let sample_z = result
                        .paths
                        .values()
                        .next()
                        .and_then(|segments| segments.first())
                        .and_then(|p| p.first())
                        .map(|p| p.z)
                        .unwrap_or(0);
                    self.stackup_manager
                        .get_layer_index_at_z(sample_z)
                        .map(|idx| self.stackup_manager.get_thickness_for_layer_index(idx))
                        .unwrap_or(Ok(default_thickness))?
                };

                for (net_id_raw, segments) in &result.paths {
                    // v0.1.7: Resolve original NetId from temp Chain-Link ID
                    let actual_net_id = if !self.auto_routes.is_empty() {
                        NetId::new(net_id_raw.raw() % 10000)
                    } else {
                        *net_id_raw
                    };

                    let net_name = net_id_to_name.get(net_id_raw).cloned().unwrap_or_else(|| {
                        CompactString::from(format!("net_{}", actual_net_id.raw()))
                    });

                    for path in segments {
                        if path.len() < 2 {
                            continue;
                        }

                        // v0.1.8: Apply 45° miter chamfers to 90° corners.
                        // This maintains constant impedance (Z₀) across bends,
                        // preventing signal reflection and EMI on high-speed lines.
                        let trace_width = self.space.fabrication_constraints.as_ref()
                            .map(|c| c.trace.min_width_nm)
                            .ok_or_else(|| IrError::MissingAsicConstraint {
                                message: "Miter chamfer requires trace width constraint but none is loaded.".into(),
                                hint: "Ensure a profile with 'trace:' constraints is declared in the space definition.".into(),
                            })?;

                        // DEBUG: Print path BEFORE miter
                        // eprintln!("[MITER DEBUG] Path BEFORE miter ({} points):", path.len());
                        // for (i, p) in path.iter().enumerate().take(4) {
                        //     eprintln!("  pre_miter[{}]: ({},{},{})", i, p.x, p.y, p.z);
                        // }
                        // if path.len() > 4 {
                        //     eprintln!("  ... and {} more points", path.len() - 4);
                        // }

                        let miter_engine = hwc_engine::MiterEngine::new(trace_width);
                        let mitered_path = miter_engine.apply_miter_pass(path);

                        // DEBUG: Print path AFTER miter
                        eprintln!(
                            "[MITER DEBUG] Path AFTER miter ({} points):",
                            mitered_path.len()
                        );
                        for (i, p) in mitered_path.iter().enumerate().take(4) {
                            eprintln!("  post_miter[{}]: ({},{},{})", i, p.x, p.y, p.z);
                        }
                        if mitered_path.len() > 4 {
                            eprintln!("  ... and {} more points", mitered_path.len() - 4);
                        }

                        // v0.1.7: Grid-Agnostic Z-Resolution via StackupManager
                        let mut refined_path = mitered_path;
                        let mut actual_thickness = trace_thickness_nm;

                        let target_z = {
                            let first_z = refined_path.first().map(|p| p.z).unwrap_or(0);
                            let last_z = refined_path.last().map(|p| p.z).unwrap_or(0);
                            let first_layer = self.stackup_manager.get_layer_index_at_z(first_z);
                            let last_layer = self.stackup_manager.get_layer_index_at_z(last_z);

                            match (first_layer, last_layer) {
                                (Some(a), Some(b)) if a == b => {
                                    // Resolve physical thickness for this specific layer
                                    actual_thickness =
                                        self.stackup_manager.get_thickness_for_layer_index(a)?;
                                    Some((first_z + last_z) / 2)
                                }
                                (Some(a), _) => {
                                    actual_thickness =
                                        self.stackup_manager.get_thickness_for_layer_index(a)?;
                                    Some(first_z)
                                }
                                _ => None,
                            }
                        };

                        // Save the original pin Z BEFORE the Z-override below,
                        // because the override sets all points to target_z.
                        let original_pin_z = refined_path.first().map(|p| p.z).unwrap_or(0);

                        if let Some(z) = target_z {
                            for point in refined_path.iter_mut() {
                                point.z = z;
                            }
                        } else {
                            // Fallback: per-point refinement
                            for point in refined_path.iter_mut() {
                                if let Some(layer_idx) =
                                    self.stackup_manager.get_layer_index_at_z(point.z)
                                {
                                    point.z = self
                                        .stackup_manager
                                        .get_z_start_nm_for_layer_index(layer_idx)?;
                                }
                            }
                        }

                        // v0.1.8: Layer Transition Segments
                        // When `route.layer` is specified, the route runs at the target layer Z
                        // but the pins are at a different Z (e.g. active). Add vertical transition
                        // segments at start/end so the auto-via inserter detects the Z change
                        // and stamps vias for the layer transition.
                        if let Some(&target_z) = net_layer_targets.get::<str>(net_name.as_ref()) {
                            // Check if pin and target are on different LAYERS (not just different Z values)
                            let pin_layer =
                                self.stackup_manager.get_layer_index_at_z(original_pin_z);
                            let target_layer = self.stackup_manager.get_layer_index_at_z(target_z);

                            eprintln!(
                                "[VIA DEBUG] Checking layer transition for net '{}':",
                                net_name
                            );
                            eprintln!(
                                "  original_pin_z={}nm (layer {:?})",
                                original_pin_z, pin_layer
                            );
                            eprintln!("  target_z={}nm (layer {:?})", target_z, target_layer);

                            // Only add via transitions if crossing DIFFERENT layers
                            let needs_via = match (pin_layer, target_layer) {
                                (Some(p), Some(t)) if p != t => {
                                    eprintln!("  ✅ Different layers - adding via transitions");
                                    true
                                }
                                (Some(p), Some(t)) if p == t => {
                                    eprintln!("  ⏭️  Same layer - skipping via transitions");
                                    false
                                }
                                _ => {
                                    eprintln!("  ⚠️  Layer unknown - adding via transitions (safe fallback)");
                                    true
                                }
                            };

                            if needs_via && original_pin_z != target_z {
                                let start_point = *refined_path.first().unwrap();
                                refined_path.insert(
                                    0,
                                    Point3D::new(start_point.x, start_point.y, original_pin_z),
                                );
                                refined_path.insert(
                                    1,
                                    Point3D::new(start_point.x, start_point.y, target_z),
                                );

                                let end_point = *refined_path.last().unwrap();
                                refined_path.push(Point3D::new(end_point.x, end_point.y, target_z));
                                refined_path.push(Point3D::new(
                                    end_point.x,
                                    end_point.y,
                                    original_pin_z,
                                ));
                            }
                        }

                        let declared_width =
                            net_declared_widths.get::<str>(net_name.as_ref()).copied();

                        // v0.1.8: Enforce current declaration for ASICs. No hardcoded defaults.
                        let current_ma = net_currents_ma.get::<str>(net_name.as_ref()).copied()
                            .ok_or_else(|| {
                                let is_asic = self.profile.as_ref().map_or(false, |p| p.is_asic());
                                if is_asic {
                                    IrError::MissingAsicConstraint {
                                        message: format!("Net '{}' has no current_limit or net current declaration.", net_name),
                                        hint: "Add 'current_limit_ac: { rms: <val>, peak: <val> }' to the route OR 'current: <val>' to the nets: declaration.".into(),
                                    }
                                } else {
                                    // PCB builds still allow implicit 0mA (skips DRC)
                                    IrError::MissingAsicConstraint {
                                        message: "Internal error: current missing in global router".into(),
                                        hint: "".into(),
                                    }
                                }
                            });

                        // If it's not an ASIC, we can just use 0.0 (skips DRC)
                        let current_ma = match current_ma {
                            Ok(c) => c,
                            Err(e) => {
                                if self.profile.as_ref().map_or(false, |p| p.is_asic()) {
                                    return Err(e);
                                } else {
                                    0.0
                                }
                            }
                        };

                        self.register_analytic_route(
                            actual_net_id,
                            &net_name,
                            refined_path,
                            actual_thickness,
                            declared_width,
                            current_ma,
                        )?;
                    }
                }
            }
            Err(_e) => {
                return Err(IrError::NoPathFound {
                    net: "batch".into(),
                    from_pin: "batch".into(),
                    to_pin: "batch".into(),
                });
            }
        }

        // Commit all batch routes
        self.space.entity_graph.commit_route();

        // v0.1.9.1: CRITICAL FIX - Re-register ALL routes from analytic source of truth.
        // This prevents the "Double-Registration" bug where the original straight-line path
        // coexists with the detour path in the physical database.
        super::automatic::re_register_resolved_routes(self.space)?;

        // v0.1.8: Localized Legalization Engine (Stage 4)
        // After all routes are registered, run legalization to fix clearance violations.
        // This implements the v0.1.8 architecture's "Localized Legalization Engine" that
        // uses continuous convex optimization to nudge traces apart without global re-routing.
        {
            let min_clearance = self.space.fabrication_constraints.as_ref()
                .map(|c| c.trace.min_spacing_nm)
                .ok_or_else(|| IrError::MissingAsicConstraint {
                    message: "Legalization requires trace spacing constraints but none are loaded.".into(),
                    hint: "Ensure a profile with 'trace:' constraints is declared in the space definition.".into(),
                })?;

            let legalizer = hwc_engine::geometry_router::Legalizer::new(min_clearance);

            // Collect all segments and net_ids from entity_graph
            let all_routes = self.space.entity_graph.get_all_routes();
            let mut all_segments = Vec::new();
            let mut all_net_ids = Vec::new();
            for (net_id, segments) in all_routes {
                for seg in segments {
                    all_segments.push(seg.clone());
                    all_net_ids.push(*net_id);
                }
            }

            if !all_segments.is_empty() {
                let (legalized_segments, legalized_net_ids) = legalizer.legalize(
                    &all_segments,
                    &all_net_ids,
                    &self.space.material_registry,
                    self.space.entity_graph.spatial(),
                    10, // max_iterations
                );

                // Re-register legalized segments
                // First clear all routes
                let net_ids_to_clear: Vec<_> = self
                    .space
                    .entity_graph
                    .get_all_routes()
                    .iter()
                    .map(|(net_id, _)| *net_id)
                    .collect();
                for net_id in net_ids_to_clear {
                    self.space.entity_graph.clear_routes_for_net(net_id);
                }

                // Then re-register legalized segments
                for (idx, seg) in legalized_segments.iter().enumerate() {
                    let net_id = legalized_net_ids[idx];
                    self.space
                        .entity_graph
                        .register_trace_segments(net_id, vec![seg.clone()]);
                }
            }
        }

        // v0.1.9.1: Configure layer Z-ranges on entity_graph's spatial index
        // before rebuilding. The geo_router has its own spatial index configured
        // via set_profile_mode, but space.entity_graph needs explicit configuration.
        if let Some(_profile) = self.profile {
            let profile_layers: Vec<String> = self.stackup_manager.ordered_layers().to_vec();
            if !profile_layers.is_empty() {
                let mut z_ranges = Vec::with_capacity(profile_layers.len());
                for i in 0..profile_layers.len() {
                    let z_min = self.stackup_manager
                        .get_layer_start_z(&profile_layers[i])
                        .unwrap_or(0);
                    let z_max = if i + 1 < profile_layers.len() {
                        self.stackup_manager
                            .get_layer_start_z(&profile_layers[i + 1])
                            .unwrap_or(self.space.dimensions.depth_nm)
                    } else {
                        self.space.dimensions.depth_nm
                    };
                    z_ranges.push((z_min, z_max));
                }
                eprintln!("[COMPILER] Configuring entity_graph spatial index with {} layer Z-ranges", z_ranges.len());
                self.space.entity_graph.set_spatial_layer_z_ranges(&z_ranges);
            }
        }

        // NOTE: entity_graph's spatial index is no longer used. Each routing operation
        // builds its own independent spatial index via build_routing_spatial_index.
        // self.space.entity_graph.rebuild_spatial_index(&self.space.material_registry); // REMOVED

        // v0.1.8: Rebuild analytic_routes from legalized entity_graph.
        // Legalization nudges traces apart, but analytic_routes (used by DRC)
        // are generated during routing BEFORE legalization. We must rebuild them
        // from the legalized entity_graph routes so DRC sees correct geometry.
        {
            let all_routes = self.space.entity_graph.get_all_routes();
            let mut new_analytic_routes = Vec::new();
            for (net_id, segments) in all_routes {
                if segments.is_empty() {
                    continue;
                }
                let net_name: CompactString = self
                    .space
                    .netlist
                    .get_net(*net_id)
                    .map(|n| n.name.clone().into())
                    .unwrap_or_else(|| format!("net_{}", net_id.raw()).into());

                let width_nm = segments.first().map(|s| s.width_nm).unwrap_or(250);
                let material = segments.first().map(|s| s.material_id).unwrap_or(0);
                let thickness_nm = self
                    .space
                    .material_registry
                    .get_material(material)
                    .map(|m| m.thickness_nm)
                    .unwrap_or(400);

                let line_segments: Vec<hwc_engine::LineSegment> = segments
                    .iter()
                    .map(|seg| hwc_engine::LineSegment {
                        start: seg.start,
                        end: seg.end,
                    })
                    .collect();

                let current_ma = self
                    .space
                    .netlist
                    .get_net(*net_id)
                    .and_then(|n| n.current_ma)
                    .unwrap_or(0.0);
                let current_limit_ma = 0.0;

                new_analytic_routes.push(hwc_engine::AnalyticTrace {
                    net_id: *net_id,
                    width_nm,
                    thickness_nm,
                    segments: line_segments,
                    material,
                    net_name,
                    current_ma,
                    current_limit_ma,
                });
            }
            self.space.analytic_routes = new_analytic_routes;
        }

        // v0.1.8: Retrieve the QueryStore back from the GeometryRouter so it
        // can be reused for subsequent compilations (incremental rebuilds).
        self.query_store = geo_router.query_store.take();

        Ok(())
    }

    fn analyze_nets(&self) -> Result<FxHashMap<CompactString, Vec<PinInfo>>, IrError> {
        let mut net_pins: FxHashMap<CompactString, Vec<PinInfo>> = FxHashMap::default();

        // v0.1.8: In v0.1.8, physical connectivity is driven by the EntityGraph's
        // component pins. To prevent "Pad Pouches" and redundant routing to logical
        // origins (corners), we deduplicate pins per component.
        // If a component has both an "anchor" pin (physical) and a logical pin,
        // we strictly prefer the anchor.
        let component_pins = self.space.entity_graph.get_component_pins();

        // Group pins by (component_name, net_name)
        let mut grouped_pins: FxHashMap<
            (CompactString, CompactString),
            Vec<&hwc_engine::ComponentPin>,
        > = FxHashMap::default();
        for pin in component_pins {
            if let Some(net_name) = &pin.net {
                grouped_pins
                    .entry((pin.component_name.clone(), net_name.clone()))
                    .or_default()
                    .push(pin);
            }
        }

        for ((_comp_name, net_name), pins) in grouped_pins {
            let entry = net_pins.entry(net_name).or_default();

            // Prefer "anchor" pins if they exist
            if let Some(anchor) = pins.iter().find(|p| p.pin_name == "anchor") {
                let pos = Point3D::new(anchor.x_nm, anchor.y_nm, anchor.z_nm);
                if !entry.iter().any(|p| p.position == pos) {
                    entry.push(PinInfo { position: pos });
                }
            } else {
                // Otherwise, use all unique positions (fallback)
                for pin in pins {
                    let pos = Point3D::new(pin.x_nm, pin.y_nm, pin.z_nm);
                    if !entry.iter().any(|p| p.position == pos) {
                        entry.push(PinInfo { position: pos });
                    }
                }
            }
        }

        Ok(net_pins)
    }

    fn find_net_id_for_name(&mut self, name: &str) -> Result<NetId, IrError> {
        let is_asic = self
            .space
            .fabrication_constraints
            .as_ref()
            .map_or(false, |c| {
                c.technology
                    .as_ref()
                    .map_or(false, |t| t.to_lowercase() == "asic")
            });
        let min_width = self
            .space
            .fabrication_constraints
            .as_ref()
            .map(|c| c.trace.min_width_nm)
            .ok_or_else(|| IrError::MissingAsicConstraint {
                message: format!("Net '{}' requires fabrication constraints but none are loaded.", name),
                hint: "Ensure a profile with 'trace:' constraints is declared in the space definition.".into(),
            })?;

        Ok(self
            .space
            .netlist
            .get_or_create_net_with_technology(name, is_asic, min_width))
    }

    fn register_analytic_route(
        &mut self,
        net_id: NetId,
        net_name: &str,
        path: Vec<Point3D>,
        thickness_nm: i64,
        declared_width_nm: Option<i64>,
        current_limit_ma: f64,
    ) -> Result<(), IrError> {
        use hwc_engine::AnalyticTrace;

        if path.len() < 2 {
            return Ok(());
        }

        // Step 1: Deduplicate consecutive identical points
        let path: Vec<Point3D> = path
            .iter()
            .copied()
            .enumerate()
            .filter(|(i, p)| *i == 0 || *p != path[i - 1])
            .map(|(_, p)| p)
            .collect();

        if path.len() < 2 {
            return Ok(());
        }

        let min_width_nm = self
            .space
            .fabrication_constraints
            .as_ref()
            .map(|c| c.trace.min_width_nm)
            .ok_or_else(|| IrError::MissingAsicConstraint {
                message: "Analytic route requires trace width constraint but none is loaded.".into(),
                hint: "Ensure a profile with 'trace:' constraints is declared in the space definition.".into(),
            })?;

        // v0.1.8: Use declared route width if specified, otherwise fall back to profile minimum.
        // This ensures the AnalyticTrace carries the user-specified width (e.g. 500nm)
        // for accurate via enclosure checks in the auto-via inserter.
        let trace_width_nm = declared_width_nm.unwrap_or(min_width_nm);

        // Collinear merge + PDK min_segment_length filter (required, no default).
        let min_seg_len_nm =
            crate::ir::routing::helpers::require_min_segment_length_nm(self.profile)?;
        let segments =
            crate::ir::routing::helpers::manhattan_path_to_segments(&path, min_seg_len_nm);
        if segments.is_empty() {
            return Err(IrError::EmptyRoute {
                net: net_name.into(),
            });
        }

        // Resolve material from stackup layer at the route's target Z position.
        // path[0] is the pin Z (active layer), path[1] is the routing layer target.
        // Use the second point for material resolution to match the actual routing layer.
        let sample_z = if path.len() > 1 { path[1].z } else { path[0].z };
        let copper_id = if let Some(layer_name) = self.stackup_manager.get_layer_name_at_z(sample_z)
        {
            let mat_name = self
                .profile
                .and_then(|p| p.stackup.as_ref())
                .and_then(|stackup| {
                    stackup
                        .layers
                        .iter()
                        .find(|l| l.name.name == layer_name)
                        .map(|l| l.material.clone())
                })
                .ok_or_else(|| IrError::UndeclaredMaterial {
                    material: format!(
                        "No material defined for layer '{}' at Z={}nm",
                        layer_name, sample_z
                    )
                    .into(),
                })?;
            self.space
                .material_registry
                .get_id(&mat_name)
                .ok_or_else(|| IrError::UndeclaredMaterial { material: mat_name })?
        } else {
            return Err(IrError::UndeclaredMaterial {
                material: format!("No stackup layer found at Z={}nm", sample_z).into(),
            });
        };

        let net_actual_current_ma = self
            .space
            .netlist
            .get_net(net_id)
            .and_then(|n| n.current_ma)
            .unwrap_or(0.0);

        let trace = AnalyticTrace::new(
            net_id,
            trace_width_nm,
            thickness_nm,
            segments,
            copper_id,
            net_name.into(),
            net_actual_current_ma, // Actual operating current from net
            current_limit_ma,      // Route's declared capability
        );

        self.space.analytic_routes.push(trace);

        // DEBUG: Verify what was stored in analytic_routes
        if let Some(stored_trace) = self.space.analytic_routes.last() {
            eprintln!(
                "[MANUAL ROUTE STORED] net_id={}, {} segments in analytic_routes",
                stored_trace.net_id.0,
                stored_trace.segments.len()
            );
            for (i, seg) in stored_trace.segments.iter().take(2).enumerate() {
                eprintln!(
                    "  stored_seg[{}]: start=({},{},{}), end=({},{},{})",
                    i, seg.start.x, seg.start.y, seg.start.z, seg.end.x, seg.end.y, seg.end.z
                );
            }
            if stored_trace.segments.len() > 2 {
                eprintln!(
                    "  ... and {} more segments",
                    stored_trace.segments.len() - 2
                );
            }
        }

        Ok(())
    }
}
