//! Routing methods: single net, all nets, priority-based, and length-constrained routing

use super::super::pathfinding::route_net_deterministic;
use super::super::types::{NetRoute, RoutedNet, RoutingError};
use super::core::GeometryRouter;
use crate::constraint_manager::LayerDirection;

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

        // Determine layer from Z coordinate
        let layer = (route.start.z / self.voxel_size_nm) as usize;
        let layer_direction = if layer < self.layer_directions.len() {
            self.layer_directions[layer]
        } else {
            LayerDirection::Any
        };

        // Run A* pathfinding with full clearance and crosstalk detection
        let clearance_zones = &self.constraints.clearance_zones;

        // Convert occupied voxels map to FxHashSet for pathfinding
        let occupied_set: rustc_hash::FxHashSet<_> = self.occupied_voxels.keys().copied().collect();

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
            exempt_components: &[],             // v0.1.7: No exemptions for legacy router
        };

        let path = route_net_deterministic(route.start, route.goal, &routing_params);

        match path {
            Some(path) => {
                // Extract vias from path (detect layer changes)
                let detected_vias = self.extract_vias_from_path(&path, route.net_id);

                // Validate and stamp each via, collecting only successfully placed ones
                let mut placed_vias = Vec::new();
                for via in detected_vias {
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

                // Validate and stamp each via
                let mut placed_vias = Vec::new();
                for via in detected_vias {
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
}
