use super::config::AutoRouter;
use crate::ir::errors::IrError;
use hwc_engine::geometry::Point3D;
use hwc_engine::netlist::NetId;

impl<'a> AutoRouter<'a> {
    pub(crate) fn find_net_id_for_name(&mut self, name: &str) -> Result<NetId, IrError> {
        let is_asic = self
            .space
            .fabrication_constraints
            .as_ref()
            .is_some_and(|c| {
                c.technology.is_asic()
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

    pub(crate) fn resolve_sample_copper_id(
        &self,
    ) -> Result<hwc_engine::material::MaterialId, IrError> {
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

    pub(crate) fn register_analytic_route(
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

        let trace_width_nm = declared_width_nm.unwrap_or(min_width_nm);

        // STRUCTURAL FIX: Detect if path contains diagonal (non-Manhattan) segments
        // Diagonal segments indicate intentional geometric features (miters, etc.) that must be preserved exactly
        let has_z_transitions = path.windows(2).any(|w| w[0].z != w[1].z);
        let has_diagonal_segments = path.windows(2).any(|w| {
            let dx = (w[1].x - w[0].x).abs();
            let dy = (w[1].y - w[0].y).abs();
            let dz = (w[1].z - w[0].z).abs();
            // Diagonal if moving in 2+ dimensions simultaneously
            (dx > 0 && dy > 0) || (dx > 0 && dz > 0) || (dy > 0 && dz > 0)
        });
        
        let segments = if has_z_transitions || has_diagonal_segments {
            eprintln!("[REGISTRY] Path has Z transitions or diagonals - creating segments directly from {} waypoints", path.len());
            eprintln!("[REGISTRY]   has_z_transitions={}, has_diagonal_segments={}", has_z_transitions, has_diagonal_segments);
            let mut segs = Vec::new();
            for i in 0..path.len() - 1 {
                segs.push(hwc_engine::LineSegment::new(path[i], path[i + 1]));
            }
            eprintln!("[REGISTRY] Created {} segments directly", segs.len());
            segs
        } else {
            eprintln!("[REGISTRY] Path is pure Manhattan - using manhattan_path_to_segments");
            let min_seg_len_nm =
                crate::ir::routing::helpers::require_min_segment_length_nm(self.profile)?;
            crate::ir::routing::helpers::manhattan_path_to_segments(&path, min_seg_len_nm)
        };
        
        if segments.is_empty() {
            return Err(IrError::EmptyRoute {
                net: net_name.into(),
            });
        }

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

        // **v0.2.0 STRUCTURAL FIX: Compute layer_z_range for horizontal traces**
        // Find the Z of the first horizontal segment (start.z == end.z) and look up its
        // layer. Traces can have via-stitch segments at the start/end while still being
        // a single-layer route, so we must not require ALL segments to share the same Z.
        let layer_z_range = segments
            .iter()
            .find(|s| s.start.z == s.end.z)
            .and_then(|s| self.space.find_layer_at_z(s.start.z))
            .map(|layer| (layer.z_bottom, layer.z_top));

        let trace = AnalyticTrace::with_layer_z_range(
            net_id,
            hwc_engine::space::CrossSection::new(trace_width_nm, thickness_nm),
            segments,
            copper_id,
            net_name.into(),
            hwc_engine::space::CurrentRating::new(net_actual_current_ma, current_limit_ma),
            layer_z_range,
        );

        // v0.2.0: Register directly in the routing database (single source of truth)
        let from_entity = format!("auto_route_{}_start", net_name);
        let to_entity = format!("auto_route_{}_end", net_name);

        self.space.routing_database.register_autorouter_route(
            trace,
            from_entity.into(),
            to_entity.into(),
        ).map_err(|e| IrError::RoutingError(e))?;

        Ok(())
    }
}
