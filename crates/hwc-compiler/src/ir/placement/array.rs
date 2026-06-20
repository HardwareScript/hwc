use super::super::conversions::CoordinateContext;
use super::super::errors::IrError;
use super::component::place_component;
use super::context::PlacementContext;
use super::coordinate_evaluation::evaluate_measurement_to_nm;
use super::helpers::offset_coordinate;
use hwc_engine::HardwareSpace;

pub fn place_component_array(
    space: &mut HardwareSpace,
    component: &hwc_parser::ComponentPlacement,
    array_config: &hwc_parser::ArrayConfig,
    layouts: &[hwc_parser::ModuleLayoutBlock],
    bbox_tracker: &mut crate::bounding_box_tracker::BoundingBoxTracker,
    ctx: &PlacementContext,
) -> Result<(), IrError> {
    let pitch_nm = evaluate_measurement_to_nm(&array_config.pitch, ctx.symbol_table)?;

    for i in 0..array_config.count {
        let (offset_x_nm, offset_y_nm) = match array_config.layout {
            hwc_parser::ArrayLayout::HorizontalStack => (i as i64 * pitch_nm, 0),
            hwc_parser::ArrayLayout::VerticalStack => (0, i as i64 * pitch_nm),
            hwc_parser::ArrayLayout::Grid { rows: _, cols: _ } => {
                return Err(IrError::PlacementError(
                    "Grid layout not yet implemented for arrays".into(),
                ));
            }
        };

        let instance_position = offset_coordinate(&component.position, offset_x_nm, offset_y_nm)?;

        let instance_name = component
            .name
            .as_ref()
            .map(|n| format!("{}[{}]", n, i))
            .or_else(|| Some(format!("{}[{}]", component.component_type, i)));

        let instance_component = hwc_parser::ComponentPlacement {
            component_type: component.component_type.clone(),
            parameters: component.parameters.clone(),
            name: instance_name.map(|s| hwc_parser::ComponentName {
                base: s.into(),
                index: None,
                span: component.span,
            }),
            position: instance_position,
            rotation: component.rotation.clone(),
            elevation: component.elevation.clone(),
            mount: component.mount,
            standoff: component.standoff.clone(),
            array_config: None,
            pin_net_bindings: component.pin_net_bindings.clone(),
            waivers: hwc_parser::Waivers {
                merge: if !array_config.merge_terminals.is_empty() {
                    hwc_parser::MergeWaiver::Specific(array_config.merge_terminals.clone())
                } else {
                    component.waivers.merge.clone()
                },
                floating: component.waivers.floating,
                isolated: component.waivers.isolated,
                snap_to_surface: component.waivers.snap_to_surface,
                virtual_component: component.waivers.virtual_component,
                locked: component.waivers.locked,
            },
            span: component.span,
        };

        place_component(space, &instance_component, layouts, bbox_tracker, ctx)?;
    }

    validate_array_collisions(space, component, array_config, pitch_nm, bbox_tracker, ctx)?;

    if !array_config.merge_terminals.is_empty() {
        merge_explicit_terminals(space, component, array_config, bbox_tracker, ctx)?;
    }

    Ok(())
}

fn validate_array_collisions(
    space: &HardwareSpace,
    component: &hwc_parser::ComponentPlacement,
    array_config: &hwc_parser::ArrayConfig,
    pitch_nm: i64,
    bbox_tracker: &mut crate::bounding_box_tracker::BoundingBoxTracker,
    ctx: &PlacementContext,
) -> Result<(), IrError> {
    let comp_def = ctx
        .symbol_table
        .get_component(&component.component_type.name)?;
    let layout = comp_def.layout.as_ref().ok_or_else(|| {
        IrError::PlacementError(format!("Component '{}' missing layout", comp_def.name))
    })?;

    for pour in &layout.internal_pours {
        let terminal_name = pour
            .device
            .as_ref()
            .map(|d| d.terminal.as_str())
            .unwrap_or("");
        if array_config
            .merge_terminals
            .iter()
            .any(|t| t == terminal_name)
        {
            continue;
        }

        let instance_bboxes = calculate_pour_bboxes_for_array(
            space,
            component,
            array_config,
            pour,
            pitch_nm,
            bbox_tracker,
            ctx,
        )?;

        for i in 0..instance_bboxes.len() {
            for j in (i + 1)..instance_bboxes.len() {
                let bbox_a = &instance_bboxes[i].1;
                let bbox_b = &instance_bboxes[j].1;

                if bbox_a.intersects(bbox_b) {
                    let array_name = component
                        .name
                        .as_ref()
                        .map(|n| n.base.as_str())
                        .unwrap_or(&component.component_type.name);

                    let pour_width = (bbox_a.max.x - bbox_a.min.x).max(bbox_a.max.y - bbox_a.min.y);
                    let suggested_pitch_nm = (pour_width as f64 * 1.1) as i64;

                    return Err(IrError::GeometricCollision(Box::new(
                        crate::ir::errors::GeometricCollisionDetails {
                            array_name: array_name.into(),
                            instance_a: i,
                            instance_b: j,
                            pour_name: pour.name.to_string(),
                            terminal_name: terminal_name.into(),
                            bbox_a_min_x: bbox_a.min.x as f64 / 1_000_000.0,
                            bbox_a_min_y: bbox_a.min.y as f64 / 1_000_000.0,
                            bbox_a_max_x: bbox_a.max.x as f64 / 1_000_000.0,
                            bbox_a_max_y: bbox_a.max.y as f64 / 1_000_000.0,
                            bbox_b_min_x: bbox_b.min.x as f64 / 1_000_000.0,
                            bbox_b_min_y: bbox_b.min.y as f64 / 1_000_000.0,
                            bbox_b_max_x: bbox_b.max.x as f64 / 1_000_000.0,
                            bbox_b_max_y: bbox_b.max.y as f64 / 1_000_000.0,
                            current_pitch: pitch_nm as f64 / 1_000_000.0,
                            suggested_pitch: suggested_pitch_nm as f64 / 1_000_000.0,
                        },
                    )));
                }
            }
        }
    }

    Ok(())
}

fn calculate_pour_bboxes_for_array(
    space: &HardwareSpace,
    _component: &hwc_parser::ComponentPlacement,
    array_config: &hwc_parser::ArrayConfig,
    pour: &hwc_parser::PourPlacement,
    pitch_nm: i64,
    _bbox_tracker: &mut crate::bounding_box_tracker::BoundingBoxTracker,
    ctx: &PlacementContext,
) -> Result<Vec<(usize, hwc_engine::geometry::BoundingBox)>, IrError> {
    use crate::ir::conversions::spanning_coordinate_to_point;
    use hwc_engine::geometry::{BoundingBox, Point3D};

    let (from, to) =
        match pour.boundary.as_ref().ok_or_else(|| {
            IrError::PlacementError(format!("Pour '{}' missing boundary", pour.name))
        })? {
            hwc_parser::PourBoundary::Rect(f, t) => ((**f).clone(), (**t).clone()),
            hwc_parser::PourBoundary::Circle { .. } => {
                return Err(IrError::PlacementError(format!(
                    "Circle boundary not yet supported in arrays for pour '{}'",
                    pour.name
                )))
            }
        };

    let mut instance_bboxes = Vec::new();

    for i in 0..array_config.count {
        let (offset_x_nm, offset_y_nm) = match array_config.layout {
            hwc_parser::ArrayLayout::HorizontalStack => (i as i64 * pitch_nm, 0),
            hwc_parser::ArrayLayout::VerticalStack => (0, i as i64 * pitch_nm),
            hwc_parser::ArrayLayout::Grid { .. } => {
                return Err(IrError::PlacementError(
                    "Grid layout not yet implemented for collision detection".into(),
                ));
            }
        };

        let coord_ctx = CoordinateContext {
            voxel_size: &space.voxel_size,
            grid_size: &space.grid,
            origin: ctx.origin,
            space_dimensions: &space.dimensions,
            symbol_table: ctx.symbol_table,
            eval_context: &hwc_parser::EvaluationContext::default(),
            bbox_tracker: None,
            stackup_manager: ctx.stackup_manager,
            profile: ctx.profile,
        };
        let start = spanning_coordinate_to_point(&from, &coord_ctx, false)
            .map_err(IrError::PlacementError)?;

        let end =
            spanning_coordinate_to_point(&to, &coord_ctx, true).map_err(IrError::PlacementError)?;

        let z_bottom_nm = ctx
            .stackup_manager
            .resolve_elevation(&pour.elevation, ctx.symbol_table)?;
        let z_top_nm = ctx.stackup_manager.resolve_elevation_top(
            &pour.elevation,
            ctx.symbol_table,
            space.voxel_size.z_nm,
        )?;

        let instance_start =
            Point3D::new(start.x + offset_x_nm, start.y + offset_y_nm, z_bottom_nm);

        let instance_end = Point3D::new(end.x + offset_x_nm, end.y + offset_y_nm, z_top_nm);

        let bbox = BoundingBox::new(instance_start, instance_end);
        instance_bboxes.push((i, bbox));
    }

    Ok(instance_bboxes)
}

fn merge_explicit_terminals(
    space: &mut HardwareSpace,
    component: &hwc_parser::ComponentPlacement,
    array_config: &hwc_parser::ArrayConfig,
    bbox_tracker: &mut crate::bounding_box_tracker::BoundingBoxTracker,
    ctx: &PlacementContext,
) -> Result<(), IrError> {
    let component_def = ctx
        .symbol_table
        .get_component(&component.component_type.name)?;

    let layout = component_def.layout.as_ref().ok_or_else(|| {
        IrError::PlacementError(format!(
            "Component '{}' has no layout block",
            component.component_type.name
        ))
    })?;

    let pitch_nm = evaluate_measurement_to_nm(&array_config.pitch, ctx.symbol_table)?;

    for terminal_name in &array_config.merge_terminals {
        let terminal_pours: Vec<_> = layout
            .internal_pours
            .iter()
            .filter(|pour| {
                if let Some(binding) = &pour.device {
                    binding.terminal == *terminal_name
                } else {
                    pour.name.as_str() == terminal_name
                }
            })
            .collect();

        if terminal_pours.is_empty() {
            continue;
        }

        for pour in terminal_pours {
            merge_pour_across_instances(
                space,
                component,
                array_config,
                pour,
                pitch_nm,
                bbox_tracker,
                ctx,
            )?;
        }
    }

    Ok(())
}

fn merge_pour_across_instances(
    space: &mut HardwareSpace,
    component: &hwc_parser::ComponentPlacement,
    array_config: &hwc_parser::ArrayConfig,
    pour: &hwc_parser::PourPlacement,
    pitch_nm: i64,
    bbox_tracker: &mut crate::bounding_box_tracker::BoundingBoxTracker,
    ctx: &PlacementContext,
) -> Result<(), IrError> {
    let instance_bboxes = calculate_pour_bboxes_for_array(
        space,
        component,
        array_config,
        pour,
        pitch_nm,
        bbox_tracker,
        ctx,
    )?;

    let mut merged_regions = Vec::new();
    let mut merged_indices = rustc_hash::FxHashSet::default();

    for i in 0..instance_bboxes.len() {
        if merged_indices.contains(&i) {
            continue;
        }

        let mut current_bbox = instance_bboxes[i].1;
        let mut merged_group = vec![i];

        for (offset, (_, next_bbox)) in instance_bboxes.iter().enumerate().skip(i + 1) {
            let j = offset;
            if merged_indices.contains(&j) {
                continue;
            }

            if current_bbox.intersects(next_bbox) {
                current_bbox = current_bbox.union(next_bbox);
                merged_group.push(j);
                merged_indices.insert(j);
            } else {
                break;
            }
        }

        merged_indices.insert(i);
        merged_regions.push((merged_group, current_bbox));
    }

    let material_id = space.material_registry.get_or_register(&pour.material);
    let net_id = if let Some(net_name) = &pour.net {
        if let Some(net) = space.netlist.get_net_by_name(net_name.base.as_str()) {
            net.raw()
        } else {
            let net = space
                .netlist
                .add_net(net_name.to_string(), 100_000, material_id);
            net.raw()
        }
    } else {
        0
    };

    for (group_indices, bbox) in merged_regions {
        let merged_name = if group_indices.len() == 1 {
            format!(
                "{}[{}].{}",
                component
                    .name
                    .as_ref()
                    .map(|n| n.base.as_str())
                    .unwrap_or(&component.component_type.name),
                group_indices[0],
                pour.name
            )
        } else {
            format!(
                "{}[{}-{}].{}",
                component
                    .name
                    .as_ref()
                    .map(|n| n.base.as_str())
                    .unwrap_or(&component.component_type.name),
                group_indices[0],
                group_indices[group_indices.len() - 1],
                pour.name
            )
        };

        let merged_region_id = if group_indices.len() > 1 {
            Some(merged_name.clone().into())
        } else {
            None
        };

        use hwc_engine::ComponentPlacer;
        let placer = ComponentPlacer::new();
        placer
            .place_substrate(
                &mut space.entity_graph,
                &space.voxel_size,
                material_id,
                bbox.min,
                bbox.max,
                net_id,
            )
            .map_err(|e| {
                IrError::PlacementError(format!(
                    "Failed to place merged pour '{}': {}",
                    merged_name, e
                ))
            })?;

        let area_nm2 = (bbox.max.x - bbox.min.x) * (bbox.max.y - bbox.min.y);
        space.pours.push(hwc_engine::space::PourMetadata {
            name: merged_name.into(),
            material_name: pour.material.clone(),
            z_bottom_nm: ctx
                .stackup_manager
                .resolve_elevation(&pour.elevation, ctx.symbol_table)?,
            net: pour.net.as_ref().map(|n| n.to_string()),
            area_nm2,
            bbox: Some(bbox),
            device_binding: pour
                .device
                .as_ref()
                .map(|binding| hwc_engine::space::DeviceBinding {
                    device_name: binding.device_name.clone(),
                    terminal: binding.terminal.clone(),
                }),
            merged_region_id,
            waivers: pour.waivers.clone(),
        });
    }

    Ok(())
}
