use crate::ir::errors::IrError;
use crate::ir::placement_item::PlacementItem;

/// Build the item_map from placement items.
pub fn build_item_map(
    placement_items: &[crate::ir::placement_item::ContextualPlacementItem],
) -> rustc_hash::FxHashMap<compact_str::CompactString, usize> {
    let mut item_map = rustc_hash::FxHashMap::default();
    for (i, contextual_item) in placement_items.iter().enumerate() {
        let item_id = contextual_item.item_id(i);
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
        let contextual_item = &ctx.placement_items[item_idx];
        let item = &contextual_item.item;
        
        // v0.2.1: Use the item's evaluation context (contains loop-scoped let bindings)
        let item_eval_context = &contextual_item.eval_context;

        let place_ctx = crate::ir::placement::context::PlacementContext {
            symbol_table: ctx.symbol_table,
            eval_context: item_eval_context,
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
                    item_eval_context,
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
                
                let mut resolved_pour = pour.clone();

                // v0.2.1: Resolve relational constraints if present
                if !resolved_pour.relational_constraints.is_empty() {
                    let resolved_position =
                        crate::ir::relational_resolver::compute_position_from_constraints(
                            &resolved_pour.relational_constraints,
                            &Some(resolved_pour.name.clone()),
                            bbox_tracker,
                            ctx.symbol_table,
                            item_eval_context,
                            ctx.origin,
                            &space.dimensions,
                        )?;

                    eprintln!(
                        "[DEBUG] Resolved pour '{}' center position from relational constraints: ({:?}, {:?})",
                        resolved_pour.name, resolved_position.x(), resolved_position.y()
                    );

                    // Create or update boundary from resolved position
                    // If boundary exists (from dimensions+at in parser), update its center
                    // If boundary is None (dimensions only, no at:), create it from dimensions
                    if let Some(hwc_parser::PourBoundary::Rect(from, to)) = &resolved_pour.boundary {
                        // Boundary exists - extract dimensions and recompute with new center
                        let width_expr = hwc_parser::Expression::Binary {
                            left: Box::new(to.x().clone()),
                            operator: hwc_parser::BinaryOperator::Subtract,
                            right: Box::new(from.x().clone()),
                            span: hwc_parser::Span::new(0, 0),
                        };

                        let height_expr = hwc_parser::Expression::Binary {
                            left: Box::new(to.y().clone()),
                            operator: hwc_parser::BinaryOperator::Subtract,
                            right: Box::new(from.y().clone()),
                            span: hwc_parser::Span::new(0, 0),
                        };

                        // Use resolved position expressions directly
                        let center_x = resolved_position.x().clone();
                        let center_y = resolved_position.y().clone();

                        let new_from = hwc_parser::Coordinate::Positional {
                            x: hwc_parser::Expression::Binary {
                                left: Box::new(center_x.clone()),
                                operator: hwc_parser::BinaryOperator::Subtract,
                                right: Box::new(hwc_parser::Expression::Binary {
                                    left: Box::new(width_expr.clone()),
                                    operator: hwc_parser::BinaryOperator::Divide,
                                    right: Box::new(hwc_parser::Expression::Literal { value: 2, span: hwc_parser::Span::new(0, 0) }),
                                    span: hwc_parser::Span::new(0, 0),
                                }),
                                span: hwc_parser::Span::new(0, 0),
                            },
                            y: hwc_parser::Expression::Binary {
                                left: Box::new(center_y.clone()),
                                operator: hwc_parser::BinaryOperator::Subtract,
                                right: Box::new(hwc_parser::Expression::Binary {
                                    left: Box::new(height_expr.clone()),
                                    operator: hwc_parser::BinaryOperator::Divide,
                                    right: Box::new(hwc_parser::Expression::Literal { value: 2, span: hwc_parser::Span::new(0, 0) }),
                                    span: hwc_parser::Span::new(0, 0),
                                }),
                                span: hwc_parser::Span::new(0, 0),
                            },
                            z: from.z().clone(),
                            span: hwc_parser::Span::new(0, 0),
                        };

                        let new_to = hwc_parser::Coordinate::Positional {
                            x: hwc_parser::Expression::Binary {
                                left: Box::new(center_x),
                                operator: hwc_parser::BinaryOperator::Add,
                                right: Box::new(hwc_parser::Expression::Binary {
                                    left: Box::new(width_expr),
                                    operator: hwc_parser::BinaryOperator::Divide,
                                    right: Box::new(hwc_parser::Expression::Literal { value: 2, span: hwc_parser::Span::new(0, 0) }),
                                    span: hwc_parser::Span::new(0, 0),
                                }),
                                span: hwc_parser::Span::new(0, 0),
                            },
                            y: hwc_parser::Expression::Binary {
                                left: Box::new(center_y),
                                operator: hwc_parser::BinaryOperator::Add,
                                right: Box::new(hwc_parser::Expression::Binary {
                                    left: Box::new(height_expr),
                                    operator: hwc_parser::BinaryOperator::Divide,
                                    right: Box::new(hwc_parser::Expression::Literal { value: 2, span: hwc_parser::Span::new(0, 0) }),
                                    span: hwc_parser::Span::new(0, 0),
                                }),
                                span: hwc_parser::Span::new(0, 0),
                            },
                            z: to.z().clone(),
                            span: hwc_parser::Span::new(0, 0),
                        };

                        resolved_pour.boundary = Some(hwc_parser::PourBoundary::Rect(
                            Box::new(new_from),
                            Box::new(new_to),
                        ));
                    }
                }

                crate::ir::placement::place_pour(space, &resolved_pour, bbox_tracker, &place_ctx)?;
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
                            item_eval_context,
                            ctx.origin,
                            &space.dimensions,
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
                
                let mut resolved_contact = contact.clone();

                // v0.2.1: Resolve relational constraints if present
                if !resolved_contact.relational_constraints.is_empty()
                    && resolved_contact.position.is_none()
                    && resolved_contact.relational_anchor.is_none()
                {
                    let resolved_position =
                        crate::ir::relational_resolver::compute_position_from_constraints(
                            &resolved_contact.relational_constraints,
                            &Some(resolved_contact.name.clone()),
                            bbox_tracker,
                            ctx.symbol_table,
                            item_eval_context,
                            ctx.origin,
                            &space.dimensions,
                        )?;

                    // Convert to coordinate and store in position field
                    resolved_contact.position = Some(hwc_parser::Coordinate::Positional {
                        x: resolved_position.x().clone(),
                        y: resolved_position.y().clone(),
                        z: hwc_parser::Expression::Measurement {
                            value: 0.0,
                            unit: hwc_parser::Unit::Nanometer,
                            span: hwc_parser::Span::new(0, 0),
                        },
                        span: hwc_parser::Span::new(0, 0),
                    });

                    eprintln!(
                        "[DEBUG] Resolved contact '{}' position from relational constraints: ({:?}, {:?})",
                        resolved_contact.name.base, resolved_position.x(), resolved_position.y()
                    );
                }

                crate::ir::placement::place_contact(
                    space,
                    &resolved_contact,
                    ctx.origin,
                    ctx.symbol_table,
                    item_eval_context,
                    ctx.stackup_manager,
                    ctx.profile,
                    bbox_tracker, // v0.2.0: Added for relational anchor resolution
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
                            item_eval_context,
                            ctx.origin,
                            &space.dimensions,
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
            PlacementItem::SpaceInstance(space_inst) => {
                // v0.2.1: Hierarchical space instantiation
                eprintln!(
                    "[DEBUG] Instantiating sub-space: {} as {}",
                    space_inst.space_name, space_inst.instance_name.base
                );
                
                // Pass the full space object so we have access to the netlist
                crate::ir::placement::instantiate_sub_space(
                    space_inst,
                    ctx.symbol_table,
                    item_eval_context,
                    ctx.origin,
                    space,
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
