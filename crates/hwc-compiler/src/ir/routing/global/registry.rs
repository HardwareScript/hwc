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
                c.technology
                    .as_ref()
                    .is_some_and(|t| t.to_lowercase() == "asic")
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

        // STRUCTURAL FIX: For 3D paths with Z transitions, create segments directly from waypoints
        // instead of using manhattan_path_to_segments which has buggy collinear logic for 3D
        let has_z_transitions = path.windows(2).any(|w| w[0].z != w[1].z);
        
        let segments = if has_z_transitions {
            eprintln!("[REGISTRY] Path has Z transitions - creating segments directly from {} waypoints", path.len());
            let mut segs = Vec::new();
            for i in 0..path.len() - 1 {
                segs.push(hwc_engine::LineSegment::new(path[i], path[i + 1]));
            }
            eprintln!("[REGISTRY] Created {} segments directly", segs.len());
            segs
        } else {
            eprintln!("[REGISTRY] Path is planar - using manhattan_path_to_segments");
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
        let layer_z_range = if let Some(first_seg) = segments.first() {
            // Check if this is a horizontal trace (all segments at same Z)
            let is_horizontal = segments
                .iter()
                .all(|s| s.start.z == first_seg.start.z && s.end.z == first_seg.start.z);

            if is_horizontal {
                let centerline_z = first_seg.start.z;
                // Look up the layer from HardwareSpace's stackup (single source of truth)
                self.space
                    .find_layer_at_z(centerline_z)
                    .map(|layer| (layer.z_bottom, layer.z_top))
            } else {
                // Via or multi-layer trace: segments encode their own Z spans
                None
            }
        } else {
            None
        };

        let trace = AnalyticTrace::with_layer_z_range(
            net_id,
            hwc_engine::space::CrossSection::new(trace_width_nm, thickness_nm),
            segments,
            copper_id,
            net_name.into(),
            hwc_engine::space::CurrentRating::new(net_actual_current_ma, current_limit_ma),
            layer_z_range,
        );

        self.space.analytic_routes.push(trace);
        Ok(())
    }
}
