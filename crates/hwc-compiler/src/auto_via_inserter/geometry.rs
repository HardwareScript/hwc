use compact_str::CompactString;
use hwc_engine::{
    geometry::{BoundingBox, Point3D},
    AnalyticTrace, PourMetadata,
};
use hwc_parser::{Coordinate, Expression};
use rustc_hash::FxHashMap;

use super::{AutoViaInserter, LayerTransition, OverlapRegion, ViaArrayConfig, ViaType};

impl AutoViaInserter {
    pub(crate) fn group_pours_by_net<'a>(
        &self,
        pours: &'a [PourMetadata],
    ) -> FxHashMap<CompactString, Vec<&'a PourMetadata>> {
        let mut by_net: FxHashMap<CompactString, Vec<&PourMetadata>> = FxHashMap::default();

        for pour in pours {
            if let Some(net_name) = &pour.net {
                by_net.entry(net_name.clone()).or_default().push(pour);
            }
        }

        by_net
    }

    pub(crate) fn find_layer_transitions(
        &self,
        net_name: &str,
        pours: &[&PourMetadata],
        stackup_manager: &crate::ir::stackup_manager::StackupManager,
    ) -> Vec<LayerTransition> {
        let mut transitions = Vec::new();

        for i in 0..pours.len() {
            for j in (i + 1)..pours.len() {
                let pour1 = pours[i];
                let pour2 = pours[j];

                if pour1.z_bottom_nm == pour2.z_bottom_nm {
                    continue;
                }

                let (Some(bbox1), Some(bbox2)) = (&pour1.bbox, &pour2.bbox) else {
                    continue;
                };

                let (
                    lower_bbox,
                    upper_bbox,
                    lower_pour,
                    upper_pour,
                    lower_material,
                    upper_material,
                ) = if pour1.z_bottom_nm < pour2.z_bottom_nm {
                    (
                        bbox1,
                        bbox2,
                        &pour1.name,
                        &pour2.name,
                        &pour1.material_name,
                        &pour2.material_name,
                    )
                } else {
                    (
                        bbox2,
                        bbox1,
                        &pour2.name,
                        &pour1.name,
                        &pour2.material_name,
                        &pour1.material_name,
                    )
                };

                let from_z_mid = (lower_bbox.min.z + lower_bbox.max.z) / 2;
                let to_z_mid = (upper_bbox.min.z + upper_bbox.max.z) / 2;
                let from_layer = stackup_manager.get_layer_index_at_z(from_z_mid);
                let to_layer = stackup_manager.get_layer_index_at_z(to_z_mid);

                if let (Some(from_layer), Some(to_layer)) = (from_layer, to_layer) {
                    if from_layer != to_layer {
                        let from_name = stackup_manager
                            .get_layer_index_at_z(from_z_mid)
                            .and_then(|i| stackup_manager.ordered_layers().get(i).cloned());
                        let to_name = stackup_manager
                            .get_layer_index_at_z(to_z_mid)
                            .and_then(|i| stackup_manager.ordered_layers().get(i).cloned());

                        transitions.push(LayerTransition {
                            net_name: net_name.to_string().into(),
                            from_layer,
                            to_layer,
                            from_z_nm: lower_bbox.min.z,
                            to_z_nm: upper_bbox.max.z,
                            from_pour: lower_pour.clone(),
                            to_pour: upper_pour.clone(),
                            from_material: lower_material.clone(),
                            to_material: upper_material.clone(),
                            from_bbox: *lower_bbox,
                            to_bbox: *upper_bbox,
                            from_layer_name: from_name.map(|n| n.into()),
                            to_layer_name: to_name.map(|n| n.into()),
                        });
                    }
                }
            }
        }

        transitions
    }

    /// Detect Z-axis layer transitions in analytic routes (manual routes).
    ///
    /// Manual routes store waypoints as `LineSegment` endpoints. When a segment
    /// has different start.z and end.z, that's a layer transition requiring a via.
    /// This method scans all analytic routes and creates `LayerTransition` objects
    /// for each Z change, enabling the auto via inserter to place vias at those points.
    pub(crate) fn find_transitions_in_analytic_routes(
        &self,
        routes: &[AnalyticTrace],
        stackup_manager: &crate::ir::stackup_manager::StackupManager,
        profile: Option<&hwc_parser::ProfileDefinition>,
    ) -> Result<Vec<LayerTransition>, String> {
        let mut transitions = Vec::new();

        for route in routes {
            let net_name = route.net_name.clone();
            let half_w = route.width_nm / 2;

            for seg in &route.segments {
                if seg.start.z == seg.end.z {
                    continue;
                }

                let (lower_z, upper_z) = if seg.start.z < seg.end.z {
                    (seg.start.z, seg.end.z)
                } else {
                    (seg.end.z, seg.start.z)
                };

                let from_layer = stackup_manager.get_layer_index_at_z(lower_z);
                let to_layer = stackup_manager.get_layer_index_at_z(upper_z);

                let (Some(from_layer), Some(to_layer)) = (from_layer, to_layer) else {
                    continue;
                };

                if from_layer == to_layer {
                    continue;
                }

                let from_material = resolve_material_for_z(lower_z, stackup_manager, profile)?;
                let to_material = resolve_material_for_z(upper_z, stackup_manager, profile)?;

                let transition_x = seg.start.x;
                let transition_y = seg.start.y;

                let from_bbox = BoundingBox::new(
                    Point3D::new(transition_x - half_w, transition_y - half_w, lower_z),
                    Point3D::new(transition_x + half_w, transition_y + half_w, lower_z),
                );
                let to_bbox = BoundingBox::new(
                    Point3D::new(transition_x - half_w, transition_y - half_w, upper_z),
                    Point3D::new(transition_x + half_w, transition_y + half_w, upper_z),
                );

                let from_layer_name = stackup_manager.get_layer_name_at_z(lower_z);
                let to_layer_name = stackup_manager.get_layer_name_at_z(upper_z);

                transitions.push(LayerTransition {
                    net_name: net_name.clone(),
                    from_layer,
                    to_layer,
                    from_z_nm: lower_z,
                    to_z_nm: upper_z,
                    from_pour: format!("trace_L{}", from_layer).into(),
                    to_pour: format!("trace_L{}", to_layer).into(),
                    from_material,
                    to_material,
                    from_bbox,
                    to_bbox,
                    from_layer_name: from_layer_name.map(|n| n.into()),
                    to_layer_name: to_layer_name.map(|n| n.into()),
                });

                println!("   │  [TRANSITION] Route '{}' seg ({},{},{})→({},{},{}) : L{}→L{}, z {}nm→{}nm",
                    net_name,
                    seg.start.x, seg.start.y, seg.start.z,
                    seg.end.x, seg.end.y, seg.end.z,
                    from_layer, to_layer, lower_z, upper_z);
            }
        }

        Ok(transitions)
    }

    pub(crate) fn find_overlap(
        &self,
        bbox1: &BoundingBox,
        bbox2: &BoundingBox,
    ) -> Result<OverlapRegion, String> {
        let overlap_min_x = bbox1.min.x.max(bbox2.min.x);
        let overlap_max_x = bbox1.max.x.min(bbox2.max.x);
        let overlap_min_y = bbox1.min.y.max(bbox2.min.y);
        let overlap_max_y = bbox1.max.y.min(bbox2.max.y);

        if overlap_min_x >= overlap_max_x || overlap_min_y >= overlap_max_y {
            return Err("No XY overlap between pours".into());
        }

        Ok(OverlapRegion {
            bbox: BoundingBox::new(
                Point3D::new(overlap_min_x, overlap_min_y, 0),
                Point3D::new(overlap_max_x, overlap_max_y, 0),
            ),
            center_x_nm: (overlap_min_x + overlap_max_x) / 2,
            center_y_nm: (overlap_min_y + overlap_max_y) / 2,
        })
    }

    pub(crate) fn validate_via_stack(
        &self,
        transition: &LayerTransition,
        overlap: &OverlapRegion,
        is_power_or_ground: bool,
    ) -> Result<(), String> {
        let from = transition.from_layer;
        let to = transition.to_layer;

        if self
            .via_library
            .find_via_for_layers(from, to, is_power_or_ground)
            .is_some()
        {
            return Ok(());
        }

        for layer in from..to {
            let via_type = self
                .via_library
                .find_via_for_layers(layer, layer + 1, is_power_or_ground)
                .ok_or_else(|| {
                    format!(
                        "Via stack validation failed: No via type found to connect layer {} to {}. \
                         Transitions: {} (L{}) -> {} (L{})",
                        layer,
                        layer + 1,
                        transition.from_pour,
                        transition.from_layer,
                        transition.to_pour,
                        transition.to_layer
                    )
                })?;

            self.verify_enclosure(overlap, via_type).map_err(|error| {
                format!(
                    "Via stack enclosure error at L{}->L{}: {}",
                    layer,
                    layer + 1,
                    error
                )
            })?;
        }

        Ok(())
    }

    pub(crate) fn calculate_via_array(
        &self,
        overlap: &OverlapRegion,
        via_type: &ViaType,
        profile: Option<&hwc_parser::ProfileDefinition>,
    ) -> Result<ViaArrayConfig, String> {
        let spacing_mm = profile
            .and_then(|profile| profile.via.as_ref())
            .and_then(|via| via.min_spacing.as_ref())
            .map(|measurement| measurement.value)
            .unwrap_or(via_type.diameter_mm);

        let spacing_nm = ((spacing_mm * 1_000_000.0) as i64).max(self.min_spacing_nm);
        let diameter_nm = (via_type.diameter_mm * 1_000_000.0) as i64;
        let annular_ring_nm = (via_type.min_enclosure_mm * 1_000_000.0) as i64;
        let pitch_nm = diameter_nm + spacing_nm;

        let overlap_width_nm = overlap.bbox.max.x - overlap.bbox.min.x;
        let overlap_height_nm = overlap.bbox.max.y - overlap.bbox.min.y;
        let center_margin_nm = annular_ring_nm + diameter_nm / 2;
        let available_width_nm = overlap_width_nm - 2 * center_margin_nm;
        let available_height_nm = overlap_height_nm - 2 * center_margin_nm;

        if available_width_nm < 0 || available_height_nm < 0 {
            return Err(format!(
                "Overlap region too small for even a single via array entry. Required: {:.3}mm, Available: {:.3}mm x {:.3}mm",
                2.0 * via_type.min_enclosure_mm + via_type.diameter_mm,
                overlap_width_nm as f64 / 1_000_000.0,
                overlap_height_nm as f64 / 1_000_000.0
            ));
        }

        let cols = (available_width_nm as f64 / pitch_nm as f64).floor() as usize + 1;
        let rows = (available_height_nm as f64 / pitch_nm as f64).floor() as usize + 1;
        let total_width_nm = (cols - 1) as i64 * pitch_nm;
        let total_height_nm = (rows - 1) as i64 * pitch_nm;

        Ok(ViaArrayConfig {
            cols,
            rows,
            pitch_x_nm: pitch_nm,
            pitch_y_nm: pitch_nm,
            start_x_nm: overlap.center_x_nm - total_width_nm / 2,
            start_y_nm: overlap.center_y_nm - total_height_nm / 2,
        })
    }

    pub(crate) fn verify_enclosure(
        &self,
        overlap: &OverlapRegion,
        via_type: &ViaType,
    ) -> Result<(), String> {
        let overlap_width_nm = overlap.bbox.max.x - overlap.bbox.min.x;
        let overlap_height_nm = overlap.bbox.max.y - overlap.bbox.min.y;
        let required_size_nm =
            ((via_type.diameter_mm + 2.0 * via_type.min_enclosure_mm) * 1_000_000.0) as i64;

        if overlap_width_nm < required_size_nm || overlap_height_nm < required_size_nm {
            return Err(format!(
                "Overlap region too small for via. Required: {:.4}mm, Available: {:.4}mm x {:.4}mm",
                required_size_nm as f64 / 1_000_000.0,
                overlap_width_nm as f64 / 1_000_000.0,
                overlap_height_nm as f64 / 1_000_000.0
            ));
        }

        Ok(())
    }

    pub(crate) fn coord_to_mm(&self, coord: &Coordinate, axis: char) -> f64 {
        match coord {
            Coordinate::Declarative { x, y, .. } => {
                let expression = if axis == 'x' { x } else { y };
                match expression {
                    Expression::Measurement { value, .. } => *value,
                    Expression::Literal { value, .. } => *value as f64,
                    _ => 0.0,
                }
            }
            _ => 0.0,
        }
    }
}

/// Resolve the material name for a Z position by looking up the layer in the stackup profile.
fn resolve_material_for_z(
    z_nm: i64,
    stackup_manager: &crate::ir::stackup_manager::StackupManager,
    profile: Option<&hwc_parser::ProfileDefinition>,
) -> Result<CompactString, String> {
    let layer_name = stackup_manager.get_layer_name_at_z(z_nm)
        .ok_or_else(|| format!(
            "[VIA] FATAL: no stackup layer found at z={}nm — cannot resolve material",
            z_nm
        ))?;

    if let Some(stackup) = profile.and_then(|p| p.stackup.as_ref()) {
        if let Some(layer) = stackup.layers.iter().find(|l| l.name.name == layer_name) {
            return Ok(layer.material.to_string().into());
        }
    }

    Err(format!(
        "[VIA] FATAL: layer '{}' at z={}nm has no material definition in the stackup",
        layer_name, z_nm
    ))
}
