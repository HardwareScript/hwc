use crate::ir::errors::IrError;
use crate::ir::placement_item::PlacementItem;

/// Execute the placement loop: place all non-route items.
pub fn execute_placement(
    space: &mut hwc_engine::HardwareSpace,
    ctx: &super::CompilationContext,
    bbox_tracker: &mut crate::bounding_box_tracker::BoundingBoxTracker,
) -> Result<(), IrError> {
    let mut component_count = 0;

    eprintln!(
        "[DEBUG] Starting placement loop with {} sorted items",
        ctx.sorted_indices.len()
    );

    // Pure integer iteration: no string keys, no hash lookups. `item_index` is
    // the item's own slot in `placement_items`, so this is a direct index.
    for &item_idx in ctx.sorted_indices.iter() {
        let item = ctx.placement_items[item_idx].item;

        // Loop variables and loop-scoped `let` bindings are already substituted
        // into each arena node during unrolling, so every item shares the
        // space-level evaluation context.
        let item_eval_context = ctx.eval_context;

        let place_ctx = crate::ir::placement::context::PlacementContext {
            symbol_table: ctx.symbol_table,
            eval_context: item_eval_context,
            stackup_manager: ctx.stackup_manager,
            collector: ctx.collector,
            profile: ctx.profile,
            origin: ctx.origin,
            arena: ctx.arena,
        };

        match item {
            PlacementItem::Region(region_id) => {
                let region = &ctx.arena.regions[region_id];
                eprintln!("[DEBUG] Registering region: {}", region.name);
                crate::ir::placement::register_region(
                    crate::ir::placement::RegisterRegionParams {
                        region,
                        bbox_tracker,
                        symbol_table: ctx.symbol_table,
                        eval_context: item_eval_context,
                        origin: ctx.origin,
                        space_dimensions: &space.dimensions,
                        stackup_manager: ctx.stackup_manager,
                        profile: ctx.profile,
                    },
                )?;
            }
            PlacementItem::Substrate(substrate_id) => {
                eprintln!("[DEBUG] Placing substrate");
                let sub = &ctx.arena.substrates[substrate_id];
                crate::ir::placement::place_substrate(space, sub, bbox_tracker, &place_ctx)?;
            }
            PlacementItem::Pour(pour_id) => {
                let pour = &ctx.arena.pours[pour_id];
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

                    resolved_pour.position = Some(resolved_position.clone());

                    // Create or update boundary from resolved position.
                    // If boundary exists (from dimensions+at in parser), update its center.
                    // If boundary is None (dimensions only, no at:), leave it None — place_pour
                    // will build it from position + width/height in the position+dimensions path.
                    if let Some(hwc_parser::PourBoundary::Rect(from, to)) = &resolved_pour.boundary
                    {
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
                                    right: Box::new(hwc_parser::Expression::Literal {
                                        value: 2,
                                        span: hwc_parser::Span::new(0, 0),
                                    }),
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
                                    right: Box::new(hwc_parser::Expression::Literal {
                                        value: 2,
                                        span: hwc_parser::Span::new(0, 0),
                                    }),
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
                                    right: Box::new(hwc_parser::Expression::Literal {
                                        value: 2,
                                        span: hwc_parser::Span::new(0, 0),
                                    }),
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
                                    right: Box::new(hwc_parser::Expression::Literal {
                                        value: 2,
                                        span: hwc_parser::Span::new(0, 0),
                                    }),
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

                    // v0.2.1 FIX: Clear relational_constraints after resolution.
                    //
                    // place_pour has this guard:
                    //   if boundary.is_none() && !relational_constraints.is_empty() { return Ok(()); }
                    //
                    // This guard exists to defer pours whose constraints can't be resolved yet.
                    // But at this point we've ALREADY resolved them and set `position`. If we
                    // leave constraints non-empty, place_pour sees boundary=None + constraints≠empty
                    // and early-returns without registering the entity in the bbox_tracker.
                    // Clearing them signals "constraints resolved; proceed with position+dimensions."
                    resolved_pour.relational_constraints = smallvec::smallvec![];
                }

                crate::ir::placement::place_pour(space, &resolved_pour, bbox_tracker, &place_ctx)?;
                eprintln!(
                    "[DEBUG] Pour placed successfully, entity count: {}",
                    space.entity_graph.iter_entity_ids().count()
                );
            }
            PlacementItem::Plane(plane_id) => {
                let plane = &ctx.arena.planes[plane_id];
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
            PlacementItem::Contact(contact_id) => {
                eprintln!("[DEBUG] Placing contact");

                let mut resolved_contact = ctx.arena.contacts[contact_id].clone();

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

                crate::ir::placement::place_contact(crate::ir::placement::PlaceContactParams {
                    space,
                    contact: &resolved_contact,
                    origin: ctx.origin,
                    symbol_table: ctx.symbol_table,
                    eval_context: item_eval_context,
                    stackup_manager: ctx.stackup_manager,
                    profile: ctx.profile,
                    // v0.2.0: Added for relational anchor resolution
                    bbox_tracker,
                })?;
            }
            PlacementItem::Component(component_id) => {
                let component = &ctx.arena.components[component_id];
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
            PlacementItem::SpaceInstance(space_inst_id) => {
                // v0.2.1: Hierarchical space instantiation
                let space_inst = &ctx.arena.space_instances[space_inst_id];
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
                    ctx.unit_registry,
                    ctx.arena,
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
