use super::super::super::pathfinding::route_net_deterministic;
use super::super::super::types::{NetRoute, RoutedNet, RoutingError};
use super::super::core::GeometryRouter;
use crate::constraint_manager::LayerDirection;

impl GeometryRouter {
    pub fn route_net(&mut self, route: &NetRoute) -> Result<RoutedNet, RoutingError> {
        let net_constraints = self
            .constraints
            .get_net_constraints(route.net_id)
            .cloned()
            .unwrap_or_default();

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

        let layer = (start.z / self.voxel_size_nm) as usize;
        let layer_direction = if layer < self.layer_directions.len() {
            self.layer_directions[layer]
        } else {
            LayerDirection::Any
        };

        let clearance_zones = &self.constraints.clearance_zones;

        let occupied_set: rustc_hash::FxHashSet<_> = self.occupied_voxels.keys().copied().collect();

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
            if ((dx_s <= tolerance_nm && dy_s <= tolerance_nm && dz_s <= tolerance_nm)
                || (dx_g <= tolerance_nm && dy_g <= tolerance_nm && dz_g <= tolerance_nm))
                && !exempt_components_vec.contains(&pin.component_name)
            {
                exempt_components_vec.push(pin.component_name.clone());
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
            voxel_grid: Some(&self.voxel_grid),
            corridor: None,
            fixed_z_nm: None,
            exempt_components: &exempt_components_vec,
            substrate_layers: None,
            is_high_speed_net: false,
        };

        let path = route_net_deterministic(start, goal, &routing_params);

        match path {
            Some(path) => {
                let detected_vias = self.extract_vias_from_path(&path, route.net_id);

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
                        x,
                        y,
                        z,
                        2,
                        crate::netlist::NetHandle::new(route.net_id.0),
                    );
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

    pub fn route_net_with_length_constraint(
        &mut self,
        route: &NetRoute,
        target_length_nm: i64,
        pattern: &Option<super::super::super::routing_patterns::RoutingPattern>,
    ) -> Result<RoutedNet, RoutingError> {
        use super::super::super::constraint_aware::constraint_aware_astar;

        let target_voxels = target_length_nm / self.voxel_size_nm;

        let occupied_set: rustc_hash::FxHashSet<_> = self.occupied_voxels.keys().copied().collect();

        let bounds = (
            self.bounds.width_nm,
            self.bounds.height_nm,
            self.bounds.depth_nm,
        );

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
                let detected_vias = self.extract_vias_from_path(&path, route.net_id);

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
                        x,
                        y,
                        z,
                        2,
                        crate::netlist::NetHandle::new(route.net_id.0),
                    );
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
}
