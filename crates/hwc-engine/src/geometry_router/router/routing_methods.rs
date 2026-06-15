//! Routing methods: single net, all nets, priority-based, length-constrained, and Steiner routing

use super::super::pathfinding::route_net_deterministic;
use super::super::types::{NetRoute, RoutedNet, RouteResult, RoutingError};
use super::core::GeometryRouter;
use crate::constraint_manager::LayerDirection;
use crate::geometry::Point3D;
use crate::netlist::NetId;
use rustc_hash::FxHashMap;

impl GeometryRouter {
    /// Route a single net.
    ///
    /// Updates the occupied voxels map after successful routing to prevent
    /// future routes from overlapping with this one. Also detects and tracks
    /// vias (layer changes) for drill file generation.
    ///
    /// # Arguments
    /// * `route` - Net route request
    ///
    /// # Returns
    /// Routed path with via information or error
    pub fn route_net(&mut self, route: &NetRoute) -> Result<RoutedNet, RoutingError> {
        // Get constraints for this net
        let net_constraints = self
            .constraints
            .get_net_constraints(route.net_id)
            .cloned()
            .unwrap_or_default();

        // Clamp start/goal to valid voxel range to prevent snap-to-voxel
        // from pushing coordinates outside grid bounds.
        // The pathfinder snaps coords to voxel centers: index = coord / voxel_size,
        // center = index * voxel_size + voxel_size/2. If coord == bounds.max,
        // the snapped center exceeds bounds. Clamping to bounds - voxel_size
        // ensures the snapped center stays within the grid.
        let max_x = self.bounds.width_nm - self.voxel_size_nm;
        let max_y = self.bounds.height_nm - self.voxel_size_nm;
        let max_z = self.bounds.depth_nm - self.voxel_size_nm;
        let clamp_coord = |p: crate::geometry::Point3D| -> crate::geometry::Point3D {
            crate::geometry::Point3D::new(
                p.x.max(0).min(max_x),
                p.y.max(0).min(max_y),
                p.z.max(0).min(max_z),
            )
        };
        let start = clamp_coord(route.start);
        let goal = clamp_coord(route.goal);

        // Determine layer from Z coordinate
        let layer = (start.z / self.voxel_size_nm) as usize;
        let layer_direction = if layer < self.layer_directions.len() {
            self.layer_directions[layer]
        } else {
            LayerDirection::Any
        };

        // Run A* pathfinding with full clearance and crosstalk detection
        let clearance_zones = &self.constraints.clearance_zones;

        // Convert occupied voxels map to FxHashSet for pathfinding
        let occupied_set: rustc_hash::FxHashSet<_> = self.occupied_voxels.keys().copied().collect();

        // v0.1.7: Identify components that own the start and goal pins,
        // so the router can escape the source and approach the drain through their bodies.
        let mut exempt_components_vec: smallvec::SmallVec<[compact_str::CompactString; 4]> =
            smallvec::SmallVec::new();
        let tolerance_nm = self.voxel_size_nm;
        for pin in self.voxel_grid.get_component_pins() {
            let dx_s = (pin.x_nm - start.x).abs();
            let dy_s = (pin.y_nm - start.y).abs();
            let dz_s = (pin.z_nm - start.z).abs();
            let dx_g = (pin.x_nm - goal.x).abs();
            let dy_g = (pin.y_nm - goal.y).abs();
            let dz_g = (pin.z_nm - goal.z).abs();
            if (dx_s <= tolerance_nm && dy_s <= tolerance_nm && dz_s <= tolerance_nm)
                || (dx_g <= tolerance_nm && dy_g <= tolerance_nm && dz_g <= tolerance_nm)
            {
                if !exempt_components_vec.contains(&pin.component_name) {
                    exempt_components_vec.push(pin.component_name.clone());
                }
            }
        }

        let routing_params = crate::geometry_router::pathfinding::RoutingParams {
            net_id: route.net_id,
            constraints: &net_constraints,
            bounds: self.bounds,
            layer_direction,
            voxel_size: crate::space::VoxelSize {
                x_nm: self.voxel_size_nm,
                y_nm: self.voxel_size_nm,
                z_nm: self.voxel_size_nm,
            },
            clearance_zones,
            occupied_voxels: &occupied_set,
            voxel_grid: Some(&self.voxel_grid), // Enable Binary Collision Skip!
            corridor: None,                     // No corridor constraint by default
            fixed_z_nm: None,                   // v0.1.7: No fixed Z for legacy router
            exempt_components: &exempt_components_vec, // v0.1.7: Exempt source/drain components
            substrate_layers: None,             // v0.1.7: No substrate context in basic routing
            is_high_speed_net: false,           // v0.1.7: Default to non-high-speed
        };

        let path = route_net_deterministic(start, goal, &routing_params);

        match path {
            Some(path) => {
                // Extract vias from path (detect layer changes)
                let detected_vias = self.extract_vias_from_path(&path, route.net_id);

                // v0.1.7: Unroll multi-layer vias into layer-by-layer vias for ASIC profiles
                let unrolled_vias: Vec<_> = detected_vias
                    .iter()
                    .flat_map(|via| self.unroll_detected_via(via))
                    .collect();

                // Validate and stamp each via, collecting only successfully placed ones
                let mut placed_vias = Vec::new();
                for via in unrolled_vias {
                    // Validate via can be placed
                    if self.can_place_via(via.position, via.from_z_nm, via.to_z_nm) {
                        // Stamp via footprint on all layers
                        self.stamp_via(&via);

                        // Record via for drill file generation
                        self.vias.push(via.clone());
                        placed_vias.push(via);
                    }
                }

                // Mark all voxels in the path as occupied by this net
                for point in &path {
                    self.occupied_voxels.insert(*point, route.net_id);

                    // Also update VoxelGrid for Binary Collision Skip
                    let (x, y, z) = crate::voxel_grid::VoxelGrid::nm_to_voxel(
                        *point,
                        &crate::space::VoxelSize {
                            x_nm: self.voxel_size_nm,
                            y_nm: self.voxel_size_nm,
                            z_nm: self.voxel_size_nm,
                        },
                    );
                    self.voxel_grid.set_occupied(
                        x,
                        y,
                        z,
                        2,
                        crate::netlist::NetHandle::new(route.net_id.0),
                    ); // Material 2 = Copper
                }

                Ok(RoutedNet {
                    net_id: route.net_id,
                    path,
                    vias: placed_vias,
                })
            }
            None => Err(RoutingError::NoPathFound {
                net_id: route.net_id,
                start: route.start,
                goal: route.goal,
            }),
        }
    }

    /// Route a net with length matching constraints.
    ///
    /// Uses constraint-aware A* pathfinding to route a net with a specific
    /// target length. This is the proper way to implement length matching -
    /// feeding constraints into the pathfinder BEFORE routing starts.
    ///
    /// **Architecture Reference:** CONSTRAINT-AWARE-ROUTING.md
    ///
    /// # Arguments
    /// * `route` - Net route request
    /// * `target_length_nm` - Target path length in nanometers
    /// * `pattern` - Optional routing pattern for macro-moves
    ///
    /// # Returns
    /// Routed path with exact target length or error
    pub fn route_net_with_length_constraint(
        &mut self,
        route: &NetRoute,
        target_length_nm: i64,
        pattern: &Option<super::super::routing_patterns::RoutingPattern>,
    ) -> Result<RoutedNet, RoutingError> {
        use super::super::constraint_aware::constraint_aware_astar;

        // Convert target length from nanometers to voxels
        let target_voxels = target_length_nm / self.voxel_size_nm;

        // Get occupied voxels set
        let occupied_set: rustc_hash::FxHashSet<_> = self.occupied_voxels.keys().copied().collect();

        // Get bounds as tuple
        let bounds = (
            self.bounds.width_nm,
            self.bounds.height_nm,
            self.bounds.depth_nm,
        );

        // Run constraint-aware A* pathfinding
        let path_result = constraint_aware_astar(
            route.start,
            route.goal,
            target_voxels,
            pattern,
            &occupied_set,
            bounds,
            self.voxel_size_nm,
        );

        match path_result {
            Ok(path) => {
                // Extract vias from path
                let detected_vias = self.extract_vias_from_path(&path, route.net_id);

                // v0.1.7: Unroll multi-layer vias into layer-by-layer vias for ASIC profiles
                let unrolled_vias: Vec<_> = detected_vias
                    .iter()
                    .flat_map(|via| self.unroll_detected_via(via))
                    .collect();

                // Validate and stamp each via
                let mut placed_vias = Vec::new();
                for via in unrolled_vias {
                    if self.can_place_via(via.position, via.from_z_nm, via.to_z_nm) {
                        self.stamp_via(&via);
                        self.vias.push(via.clone());
                        placed_vias.push(via);
                    }
                }

                // Mark all voxels in the path as occupied
                for point in &path {
                    self.occupied_voxels.insert(*point, route.net_id);

                    // Also update VoxelGrid for Binary Collision Skip
                    let (x, y, z) = crate::voxel_grid::VoxelGrid::nm_to_voxel(
                        *point,
                        &crate::space::VoxelSize {
                            x_nm: self.voxel_size_nm,
                            y_nm: self.voxel_size_nm,
                            z_nm: self.voxel_size_nm,
                        },
                    );
                    self.voxel_grid.set_occupied(
                        x,
                        y,
                        z,
                        2,
                        crate::netlist::NetHandle::new(route.net_id.0),
                    ); // Material 2 = Copper
                }

                Ok(RoutedNet {
                    net_id: route.net_id,
                    path,
                    vias: placed_vias,
                })
            }
            Err(err) => Err(RoutingError::ConstraintFailed {
                net_id: route.net_id,
                message: err,
            }),
        }
    }

    /// Route all nets in sequence (single-threaded for determinism).
    ///
    /// Routes nets one at a time, updating the grid after each successful route.
    /// This ensures deterministic results - the same input always produces the
    /// same output.
    ///
    /// # Arguments
    /// * `nets` - Vector of net route requests
    ///
    /// # Returns
    /// Vector of routed nets or first error encountered
    ///
    /// # Examples
    /// ```
    /// use hwc_engine::geometry_router::{GeometryRouter, GridBounds, NetRoute};
    /// use hwc_engine::{Point3D, netlist::NetId, constraint_manager::ConstraintRulebook};
    ///
    /// let bounds = GridBounds::new(50_000_000, 50_000_000, 10_000_000);
    /// let constraints = ConstraintRulebook::new(500_000);
    /// let mut router = GeometryRouter::new(bounds, constraints);
    ///
    /// let nets = vec![
    ///     NetRoute {
    ///         net_id: NetId::new(0),
    ///         start: Point3D::new(0, 0, 0),
    ///         goal: Point3D::new(0, 10_000_000, 0),
    ///     },
    /// ];
    ///
    /// let result = router.route_all_nets(&nets);
    /// assert!(result.is_ok());
    /// ```
    pub fn route_all_nets(&mut self, nets: &[NetRoute]) -> Result<Vec<RoutedNet>, RoutingError> {
        let mut routed_nets = Vec::with_capacity(nets.len());

        // Route nets sequentially for determinism
        for net in nets {
            let routed = self.route_net(net)?;

            // Note: Caller is responsible for marking voxels as occupied
            // using mark_route_occupied() with their VoxelGrid

            routed_nets.push(routed);
        }

        Ok(routed_nets)
    }

    /// Route all nets with priority-based ordering.
    ///
    /// Automatically sorts nets by priority before routing:
    /// 1. Critical (clocks, oscillators)
    /// 2. Power (VCC, GND)
    /// 3. HighSpeed (DDR, PCIe, USB)
    /// 4. DataBus (SPI, I2C, UART)
    /// 5. LowSpeed (unknown signals)
    /// 6. GPIO (general purpose I/O)
    ///
    /// Higher priority nets are routed first, ensuring critical signals
    /// get optimal paths. This is the recommended method for production routing.
    ///
    /// # Arguments
    /// * `nets` - Slice of net routes to process
    /// * `netlist` - Netlist arena for looking up net names
    ///
    /// # Returns
    /// Vector of successfully routed nets in original order, or error on first failure
    ///
    /// # Examples
    /// ```
    /// use hwc_engine::geometry_router::{GeometryRouter, GridBounds, NetRoute};
    /// use hwc_engine::{Point3D, netlist::{NetId, NetlistArena}, constraint_manager::ConstraintRulebook};
    ///
    /// let bounds = GridBounds::new(50_000_000, 50_000_000, 10_000_000);
    /// let constraints = ConstraintRulebook::new(500_000);
    /// let mut router = GeometryRouter::new(bounds, constraints);
    /// let mut netlist = NetlistArena::new();
    ///
    /// // Add nets to netlist (width_nm: 200_000 = 0.2mm, material: 1 = Copper)
    /// let clk_net = netlist.add_net("CLK_100MHz".into(), 200_000, 1);
    /// let gpio_net = netlist.add_net("GPIO_0".into(), 200_000, 1);
    ///
    /// let nets = vec![
    ///     NetRoute { net_id: gpio_net, start: Point3D::new(0, 0, 0), goal: Point3D::new(0, 10_000_000, 0) },
    ///     NetRoute { net_id: clk_net, start: Point3D::new(10_000_000, 0, 0), goal: Point3D::new(10_000_000, 10_000_000, 0) },
    /// ];
    ///
    /// let result = router.route_all_nets_with_priority(&nets, &netlist);
    /// // CLK_100MHz will be routed first (Critical priority)
    /// // GPIO_0 will be routed second (GPIO priority)
    /// assert!(result.is_ok());
    /// ```
    pub fn route_all_nets_with_priority(
        &mut self,
        nets: &[NetRoute],
        netlist: &crate::netlist::NetlistArena,
    ) -> Result<Vec<RoutedNet>, RoutingError> {
        use super::super::priority::NetPriority;
        use rustc_hash::FxHashMap;

        // Calculate priorities for all nets
        let mut priorities = FxHashMap::default();
        for net in nets {
            if let Some(net_data) = netlist.get_net(net.net_id) {
                let priority = NetPriority::from_net_name(&net_data.name);
                priorities.insert(net.net_id, priority);
            } else {
                // Unknown net, use default priority
                priorities.insert(net.net_id, NetPriority::LowSpeed);
            }
        }

        // Create a sorted copy of nets (highest priority first)
        let mut sorted_nets: Vec<NetRoute> = nets.to_vec();
        sorted_nets.sort_by(|a, b| {
            let priority_a = priorities.get(&a.net_id).unwrap();
            let priority_b = priorities.get(&b.net_id).unwrap();
            priority_b.cmp(priority_a) // Reverse order for highest first
        });

        // Route nets in priority order
        let mut routed_map = FxHashMap::default();
        for net in sorted_nets {
            let routed = self.route_net(&net)?;
            routed_map.insert(net.net_id, routed);
        }

        // Return routed nets in original order
        let mut result = Vec::with_capacity(nets.len());
        for net in nets {
            if let Some(routed) = routed_map.get(&net.net_id) {
                result.push(routed.clone());
            }
        }

        Ok(result)
    }

    // =========================================================================
    // v0.1.7 Steiner Net Tapping & Dynamic Target Expansion
    // =========================================================================

    /// Find the nearest point on any existing segment of the same net.
    ///
    /// When routing Pin N of a multi-pin net, all previously routed segments
    /// of that net become valid targets (not just original pins). This enables
    /// Steiner Minimum Tree branching via physical T-junctions.
    ///
    /// **Architecture Reference:** `Docs/v0.1.7/Unified-2.5D-3D-Routing-and-Placement.md` §4.1
    ///
    /// # Arguments
    /// * `new_pin` - The pin we are trying to connect
    /// * `existing_paths` - All previously routed segments of this net
    ///
    /// # Returns
    /// The closest point on any existing segment, or `new_pin` if no segments exist.
    pub fn find_nearest_target_on_net(
        &self,
        new_pin: Point3D,
        existing_paths: &[Vec<Point3D>],
    ) -> Point3D {
        if existing_paths.is_empty() {
            return new_pin;
        }

        // Flatten all path segments and find the point with minimum Euclidean distance²
        existing_paths
            .iter()
            .flatten()
            .min_by_key(|&&pt| {
                let dx = pt.x - new_pin.x;
                let dy = pt.y - new_pin.y;
                let dz = pt.z - new_pin.z;
                dx * dx + dy * dy + dz * dz
            })
            .copied()
            .unwrap_or(new_pin)
    }

    /// Route a multi-pin net using Steiner Minimum Tree (SMT) approximation.
    ///
    /// Instead of daisy-chaining pins (Pin1→Pin2, Pin1→Pin3, Pin1→Pin4),
    /// this method dynamically expands the target set: after each sub-route
    /// completes, the resulting path becomes a valid target for subsequent pins.
    /// This produces branching T-junctions that minimize total trace length.
    ///
    /// **Algorithm:**
    /// 1. Route Pin[0] → Pin[1] (initial trunk segment)
    /// 2. For each subsequent Pin[i]:
    ///    a. Find nearest point on any existing net segment
    ///    b. Route Pin[i] → nearest_target (creates T-junction)
    ///    c. Push resulting path into `net_paths`
    /// 3. Terminate A* the moment it intersects any coordinate on the same net
    ///
    /// **Architecture Reference:** `Docs/v0.1.7/Unified-2.5D-3D-Routing-and-Placement.md` §4.1
    ///
    /// # Arguments
    /// * `net_id` - Net ID to route
    /// * `pins` - All pin coordinates for this net
    ///
    /// # Returns
    /// Complete routed path with T-junctions, or error if any sub-route fails.
    pub fn route_net_steiner(
        &mut self,
        net_id: NetId,
        pins: &[Point3D],
    ) -> Result<RoutedNet, RoutingError> {
        if pins.len() < 2 {
            return Err(RoutingError::NoPathFound {
                net_id,
                start: pins[0],
                goal: pins[0],
            });
        }

        // Track all path segments for this net (for dynamic target expansion)
        let mut net_paths: Vec<Vec<Point3D>> = Vec::new();
        let mut all_vias = Vec::new();

        // Route Pin[0] → Pin[1] as the initial trunk
        let initial_route = NetRoute {
            net_id,
            start: pins[0],
            goal: pins[1],
        };
        let initial_routed = self.route_net(&initial_route)?;
        net_paths.push(initial_routed.path.clone());
        all_vias.extend(initial_routed.vias);

        // For each subsequent pin, find nearest target on existing net segments
        for &pin in &pins[2..] {
            // Dynamic Target Set: search all existing segments, not just original pins
            let target = self.find_nearest_target_on_net(pin, &net_paths);

            let sub_route = NetRoute {
                net_id,
                start: pin,
                goal: target,
            };

            match self.route_net(&sub_route) {
                Ok(routed) => {
                    net_paths.push(routed.path.clone());
                    all_vias.extend(routed.vias);
                }
                Err(RoutingError::NoPathFound { .. }) => {
                    // If direct T-junction route fails, try routing to nearest original pin
                    // as a fallback (ensures connectivity even without optimal Steiner branching)
                    let mut best_fallback: Option<(Point3D, i64)> = None;
                    for &original_pin in pins.iter().take(pins.len()) {
                        let dx = original_pin.x - pin.x;
                        let dy = original_pin.y - pin.y;
                        let dz = original_pin.z - pin.z;
                        let dist_sq = dx * dx + dy * dy + dz * dz;
                        if best_fallback.is_none() || dist_sq < best_fallback.unwrap().1 {
                            best_fallback = Some((original_pin, dist_sq));
                        }
                    }

                    if let Some((fallback_target, _)) = best_fallback {
                        let fallback_route = NetRoute {
                            net_id,
                            start: pin,
                            goal: fallback_target,
                        };
                        let fallback_routed = self.route_net(&fallback_route)?;
                        net_paths.push(fallback_routed.path.clone());
                        all_vias.extend(fallback_routed.vias);
                    }
                }
                Err(e) => return Err(e),
            }
        }

        // Merge all sub-paths into a single unified path
        let mut merged_path = Vec::new();
        for segment in &net_paths {
            merged_path.extend(segment);
        }

        Ok(RoutedNet {
            net_id,
            path: merged_path,
            vias: all_vias,
        })
    }

    /// Route all multi-pin nets using Steiner Minimum Tree approximation.
    ///
    /// This is the recommended method for routing designs with multi-pin nets.
    /// Each net is routed with dynamic target expansion to produce T-junctions
    /// instead of daisy chains, minimizing total trace length.
    ///
    /// # Arguments
    /// * `nets` - Map of net IDs to their pin coordinates
    ///
    /// # Returns
    /// Unified `RouteResult` with all paths and vias, or first error.
    pub fn route_all_nets_steiner(
        &mut self,
        nets: &FxHashMap<NetId, Vec<Point3D>>,
    ) -> Result<RouteResult, RoutingError> {
        let mut result = RouteResult::new();

        // Sort nets by ID for deterministic ordering
        let mut sorted_nets: Vec<_> = nets.iter().collect();
        sorted_nets.sort_by_key(|(id, _)| id.0);

        for (&net_id, pins) in &sorted_nets {
            if pins.len() < 2 {
                continue;
            }

            let routed = self.route_net_steiner(net_id, pins)?;
            result.paths.insert(net_id, routed.path);
            result.vias.extend(routed.vias);
        }

        Ok(result)
    }

    /// Route a single net globally (skips component obstacle checking).
    ///
    /// Used for cross-cell nets in hierarchical routing. These routes span
    /// the full board and need to pass through areas occupied by components.
    /// Only checks occupied voxels (trace-vs-trace) and bounds, not component bodies.
    pub fn route_net_global(&mut self, route: &NetRoute) -> Result<RoutedNet, RoutingError> {
        let net_constraints = self
            .constraints
            .get_net_constraints(route.net_id)
            .cloned()
            .unwrap_or_default();

        let clearance_zones = &self.constraints.clearance_zones;
        let occupied_set: rustc_hash::FxHashSet<_> = self.occupied_voxels.keys().copied().collect();

        // Clamp start/goal Z to valid voxel range to avoid boundary snapping issues.
        // Pins at the exact board boundary (e.g. z=depth_nm) snap outside bounds.
        let max_valid_z = self.bounds.depth_nm - self.voxel_size_nm;
        let clamp_z = |p: Point3D| -> Point3D {
            let z = p.z.min(max_valid_z).max(0);
            Point3D::new(p.x, p.y, z)
        };
        let start = clamp_z(route.start);
        let goal = clamp_z(route.goal);

        let layer = (start.z / self.voxel_size_nm) as usize;
        let layer_direction = if layer < self.layer_directions.len() {
            self.layer_directions[layer]
        } else {
            LayerDirection::Any
        };

        let routing_params = crate::geometry_router::pathfinding::RoutingParams {
            net_id: route.net_id,
            constraints: &net_constraints,
            bounds: self.bounds,
            layer_direction,
            voxel_size: crate::space::VoxelSize {
                x_nm: self.voxel_size_nm,
                y_nm: self.voxel_size_nm,
                z_nm: self.voxel_size_nm,
            },
            clearance_zones,
            occupied_voxels: &occupied_set,
            voxel_grid: None, // Skip component obstacle checking for global routes
            corridor: None,
            fixed_z_nm: None,
            exempt_components: &[],
            substrate_layers: None,
            is_high_speed_net: false,
        };

        match route_net_deterministic(start, goal, &routing_params) {
            Some(path) => {
                let detected_vias = self.extract_vias_from_path(&path, route.net_id);

                // v0.1.7: Unroll multi-layer vias into layer-by-layer vias for ASIC profiles
                let unrolled_vias: Vec<_> = detected_vias
                    .iter()
                    .flat_map(|via| self.unroll_detected_via(via))
                    .collect();

                let mut placed_vias = Vec::new();
                for via in unrolled_vias {
                    if self.can_place_via(via.position, via.from_z_nm, via.to_z_nm) {
                        self.stamp_via(&via);
                        self.vias.push(via.clone());
                        placed_vias.push(via);
                    }
                }

                for point in &path {
                    self.occupied_voxels.insert(*point, route.net_id);
                    let (x, y, z) = crate::voxel_grid::VoxelGrid::nm_to_voxel(
                        *point,
                        &crate::space::VoxelSize {
                            x_nm: self.voxel_size_nm,
                            y_nm: self.voxel_size_nm,
                            z_nm: self.voxel_size_nm,
                        },
                    );
                    self.voxel_grid.set_occupied(
                        x, y, z, 2,
                        crate::netlist::NetHandle::new(route.net_id.0),
                    );
                }

                // Restore original pin positions at endpoints
                let mut final_path = path;
                if !final_path.is_empty() {
                    final_path[0] = route.start;
                    *final_path.last_mut().unwrap() = route.goal;
                }

                Ok(RoutedNet {
                    net_id: route.net_id,
                    path: final_path,
                    vias: placed_vias,
                })
            }
            None => Err(RoutingError::NoPathFound {
                net_id: route.net_id,
                start: route.start,
                goal: route.goal,
            }),
        }
    }

    /// Route all nets globally using Steiner routing (skips component obstacles).
    ///
    /// Used for cross-cell nets in hierarchical routing.
    pub fn route_all_nets_steiner_global(
        &mut self,
        nets: &FxHashMap<NetId, Vec<Point3D>>,
    ) -> Result<RouteResult, RoutingError> {
        let mut result = RouteResult::new();

        let mut sorted_nets: Vec<_> = nets.iter().collect();
        sorted_nets.sort_by_key(|(id, _)| id.0);

        for (&net_id, pins) in &sorted_nets {
            if pins.len() < 2 {
                continue;
            }

            let routed = self.route_net_steiner_global(net_id, pins)?;
            result.paths.insert(net_id, routed.path);
            result.vias.extend(routed.vias);
        }

        Ok(result)
    }

    /// Route a single multi-pin net globally using Steiner approximation.
    pub(crate) fn route_net_steiner_global(
        &mut self,
        net_id: NetId,
        pins: &[Point3D],
    ) -> Result<RoutedNet, RoutingError> {
        if pins.len() < 2 {
            return Err(RoutingError::NoPathFound {
                net_id,
                start: pins[0],
                goal: pins[0],
            });
        }

        let mut net_paths: Vec<Vec<Point3D>> = Vec::new();
        let mut all_vias = Vec::new();

        let initial_route = NetRoute {
            net_id,
            start: pins[0],
            goal: pins[1],
        };
        let initial_routed = self.route_net_global(&initial_route)?;
        net_paths.push(initial_routed.path.clone());
        all_vias.extend(initial_routed.vias);

        for &pin in &pins[2..] {
            let target = self.find_nearest_target_on_net(pin, &net_paths);

            let sub_route = NetRoute {
                net_id,
                start: pin,
                goal: target,
            };

            match self.route_net_global(&sub_route) {
                Ok(routed) => {
                    net_paths.push(routed.path.clone());
                    all_vias.extend(routed.vias);
                }
                Err(RoutingError::NoPathFound { .. }) => {
                    let mut best_fallback: Option<(Point3D, i64)> = None;
                    for &original_pin in pins.iter().take(pins.len()) {
                        let dx = original_pin.x - pin.x;
                        let dy = original_pin.y - pin.y;
                        let dz = original_pin.z - pin.z;
                        let dist_sq = dx * dx + dy * dy + dz * dz;
                        if best_fallback.is_none() || dist_sq < best_fallback.unwrap().1 {
                            best_fallback = Some((original_pin, dist_sq));
                        }
                    }

                    if let Some((fallback_target, _)) = best_fallback {
                        let fallback_route = NetRoute {
                            net_id,
                            start: pin,
                            goal: fallback_target,
                        };
                        let fallback_routed = self.route_net_global(&fallback_route)?;
                        net_paths.push(fallback_routed.path.clone());
                        all_vias.extend(fallback_routed.vias);
                    }
                }
                Err(e) => return Err(e),
            }
        }

        let mut merged_path = Vec::new();
        for segment in &net_paths {
            merged_path.extend(segment);
        }

        Ok(RoutedNet {
            net_id,
            path: merged_path,
            vias: all_vias,
        })
    }
}
