use crate::ir::errors::IrError;
use crate::ir::placement_item::PlacementItem;

/// Build the item_map from placement items.
pub fn build_item_map(
    placement_items: &[PlacementItem],
) -> rustc_hash::FxHashMap<compact_str::CompactString, usize> {
    let mut item_map = rustc_hash::FxHashMap::default();
    for (i, item) in placement_items.iter().enumerate() {
        let item_id = item.item_id(i);
        item_map.insert(item_id, i);
    }
    item_map
}

/// Execute the placement loop: place all non-route items.
pub fn execute_placement(
    space: &mut hwc_engine::HardwareSpace,
    ctx: &super::CompilationContext,
    bbox_tracker: &mut crate::bounding_box_tracker::BoundingBoxTracker,
) -> Result<(), IrError> {
    let mut component_count = 0;

    eprintln!(
        "[DEBUG] Starting placement loop with {} sorted items",
        ctx.sorted_ids.len()
    );

    for id in ctx.sorted_ids.iter() {
        let &item_idx = ctx.item_map.get(id).unwrap();
        let item = &ctx.placement_items[item_idx];

        let place_ctx = crate::ir::placement::context::PlacementContext {
            symbol_table: ctx.symbol_table,
            eval_context: ctx.eval_context,
            stackup_manager: ctx.stackup_manager,
            collector: ctx.collector,
            profile: ctx.profile,
            origin: ctx.origin,
        };

        match item {
            PlacementItem::Region(region) => {
                eprintln!("[DEBUG] Registering region: {}", region.name);
                crate::ir::placement::register_region(
                    region,
                    bbox_tracker,
                    ctx.symbol_table,
                    ctx.eval_context,
                    ctx.origin,
                    &space.dimensions,
                    ctx.stackup_manager,
                    ctx.profile,
                )?;
            }
            PlacementItem::Substrate(sub) => {
                eprintln!("[DEBUG] Placing substrate");
                crate::ir::placement::place_substrate(space, sub, bbox_tracker, &place_ctx)?;
            }
            PlacementItem::Pour(pour) => {
                eprintln!("[DEBUG] Placing pour: {}", pour.name);
                crate::ir::placement::place_pour(space, pour, bbox_tracker, &place_ctx)?;
                eprintln!(
                    "[DEBUG] Pour placed successfully, entity count: {}",
                    space.entity_graph.iter_entity_ids().count()
                );
            }
            PlacementItem::Plane(plane) => {
                eprintln!("[DEBUG] Placing plane: {}", plane.name);

                let mut resolved_plane = plane.clone();
                if resolved_plane.from.is_none()
                    && !resolved_plane.relational_constraints.is_empty()
                {
                    let resolved_position =
                        crate::ir::relational_resolver::compute_position_from_constraints(
                            &resolved_plane.relational_constraints,
                            &Some(resolved_plane.name.clone()),
                            bbox_tracker,
                            ctx.symbol_table,
                            ctx.eval_context,
                            ctx.origin,
                        )?;
                    resolved_plane.from = Some(resolved_position);
                }

                crate::ir::placement::place_plane(
                    space,
                    &resolved_plane,
                    bbox_tracker,
                    &place_ctx,
                )?;
            }
            PlacementItem::Contact(contact) => {
                eprintln!("[DEBUG] Placing contact");
                crate::ir::placement::place_contact(
                    space,
                    contact,
                    ctx.origin,
                    ctx.symbol_table,
                    ctx.eval_context,
                    ctx.stackup_manager,
                    ctx.profile,
                )?;
            }
            PlacementItem::Component(component) => {
                eprintln!("[DEBUG] Placing component: {:?}", component.name);
                component_count += 1;

                let mut resolved_component = component.clone();
                if resolved_component.position.is_none()
                    && !resolved_component.relational_constraints.is_empty()
                {
                    let resolved_position =
                        crate::ir::relational_resolver::compute_position_from_constraints(
                            &resolved_component.relational_constraints,
                            &resolved_component.name,
                            bbox_tracker,
                            ctx.symbol_table,
                            ctx.eval_context,
                            ctx.origin,
                        )?;
                    resolved_component.position = Some(resolved_position);
                }

                crate::ir::placement::place_component(
                    space,
                    &resolved_component,
                    &ctx.space_def.layouts,
                    bbox_tracker,
                    &place_ctx,
                )?;
            }
            PlacementItem::Route(_) => {
                eprintln!("[DEBUG] Skipping route during placement phase");
                continue;
            }
        }
    }

    let _ = component_count;
    Ok(())
}

/// Static Geometry Guard: detect coplanar short circuits before routing.
pub fn check_static_shorts(space: &hwc_engine::HardwareSpace) -> Result<(), IrError> {
    let guard_violations =
        hwc_engine::geometry_router::check_static_shorts(&space.entity_graph, &space.netlist);
    if !guard_violations.is_empty() {
        for v in &guard_violations {
            eprintln!("[STATIC GUARD] P42: {}", v);
        }
        return Err(IrError::StaticGeometryShort {
            net_a: guard_violations[0].net_a.clone(),
            net_b: guard_violations[0].net_b.clone(),
            x_nm: guard_violations[0].bbox.min.x,
            y_nm: guard_violations[0].bbox.min.y,
            z_nm: guard_violations[0].bbox.min.z,
        });
    }
    Ok(())
}
