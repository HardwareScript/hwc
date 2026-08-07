use crate::ir::errors::IrError;
use crate::ir::placement_item::{ContextualPlacementItem, PlacementItem};
use compact_str::CompactString;

/// Build the dependency graph from placement items and return topologically sorted IDs.
pub fn build_and_sort(
    placement_items: &[ContextualPlacementItem],
    _symbol_table: &crate::SymbolTable,
) -> Result<Vec<compact_str::CompactString>, IrError> {
    let mut graph = crate::ir::spatial_dependency_graph::SpatialDependencyGraph::new();
    let mut last_component_name: Option<compact_str::CompactString> = None;

    // Pass 1: Register all items
    for (i, contextual_item) in placement_items.iter().enumerate() {
        let item_id = contextual_item.item_id(i);
        graph.add_component(item_id);
    }

    // Pass 2: Extract dependencies
    for (i, contextual_item) in placement_items.iter().enumerate() {
        let item_id = contextual_item.item_id(i);
        let item = &contextual_item.item;

        match item {
            PlacementItem::Region(r) => {
                // v0.2.0: Process region dependencies
                if let Some(anchor) = &r.anchor {
                    match anchor {
                        hwc_parser::RegionAnchor::Absolute(_) => {
                            // No dependencies for absolute positioning
                        }
                        hwc_parser::RegionAnchor::Expression(expr) => {
                            graph.extract_dependencies_from_expr(
                                &item_id,
                                expr,
                                last_component_name.as_ref(),
                            );
                        }
                        hwc_parser::RegionAnchor::Offset { base, offset, .. } => {
                            graph.extract_dependencies_from_expr(
                                &item_id,
                                base,
                                last_component_name.as_ref(),
                            );
                            graph.extract_dependencies_from_coord(
                                &item_id,
                                offset,
                                last_component_name.as_ref(),
                            );
                        }
                    }
                }
                // Process relational constraints
                for constraint in &r.constraints {
                    let target_name = CompactString::from(constraint.target.as_str());
                    graph.add_dependency(item_id.clone(), target_name);
                    if let Some(spacing) = &constraint.spacing {
                        graph.extract_dependencies_from_expr(
                            &item_id,
                            spacing,
                            last_component_name.as_ref(),
                        );
                    }
                }
            }
            PlacementItem::Substrate(s) => {
                graph.extract_dependencies_from_coord(
                    &item_id,
                    &s.from,
                    last_component_name.as_ref(),
                );
                graph.extract_dependencies_from_coord(
                    &item_id,
                    &s.to,
                    last_component_name.as_ref(),
                );
            }
            PlacementItem::Component(c) => {
                if let Some(position) = &c.position {
                    graph.extract_dependencies_from_coord(
                        &item_id,
                        position,
                        last_component_name.as_ref(),
                    );
                }
                for constraint in &c.relational_constraints {
                    match constraint {
                        hwc_parser::RelationalConstraint::Align { target, .. } => {
                            // v0.2.1: AlignmentTarget is now an enum (Entity or Expression)
                            match target {
                                hwc_parser::AlignmentTarget::Entity(entity_name) => {
                                    graph.add_dependency(item_id.clone(), entity_name.base.clone());
                                }
                                hwc_parser::AlignmentTarget::Expression(expr) => {
                                    // Extract all entity references from the expression
                                    graph.extract_dependencies_from_expr(
                                        &item_id,
                                        expr,
                                        last_component_name.as_ref(),
                                    );
                                }
                            }
                        }
                        hwc_parser::RelationalConstraint::Directional(dir) => {
                            let target = match dir {
                                hwc_parser::DirectionalConstraint::Above { target, .. }
                                | hwc_parser::DirectionalConstraint::Below { target, .. }
                                | hwc_parser::DirectionalConstraint::RightOf { target, .. }
                                | hwc_parser::DirectionalConstraint::LeftOf { target, .. } => {
                                    target
                                }
                            };
                            graph.add_dependency(item_id.clone(), target.base.clone());
                        }
                    }
                }
                last_component_name = Some(item_id);
            }
            PlacementItem::Pour(p) => {
                if let Some(boundary) = &p.boundary {
                    match boundary {
                        hwc_parser::PourBoundary::Rect(from, to) => {
                            graph.extract_dependencies_from_coord(
                                &item_id,
                                from,
                                last_component_name.as_ref(),
                            );
                            graph.extract_dependencies_from_coord(
                                &item_id,
                                to,
                                last_component_name.as_ref(),
                            );
                        }
                        hwc_parser::PourBoundary::Circle { center, radius } => {
                            graph.extract_dependencies_from_coord(
                                &item_id,
                                center,
                                last_component_name.as_ref(),
                            );
                            graph.extract_dependencies_from_expr(
                                &item_id,
                                radius,
                                last_component_name.as_ref(),
                            );
                        }
                    }
                }
                // v0.2.1 FIX: Extract dependencies from relational constraints on pours.
                // Without this, pours that use `align:` / `right_of:` / etc. are not
                // ordered after the entities they reference, causing bbox_tracker misses.
                for constraint in &p.relational_constraints {
                    match constraint {
                        hwc_parser::RelationalConstraint::Align { target, .. } => match target {
                            hwc_parser::AlignmentTarget::Entity(entity_name) => {
                                graph.add_dependency(item_id.clone(), entity_name.base.clone());
                            }
                            hwc_parser::AlignmentTarget::Expression(expr) => {
                                graph.extract_dependencies_from_expr(
                                    &item_id,
                                    expr,
                                    last_component_name.as_ref(),
                                );
                            }
                        },
                        hwc_parser::RelationalConstraint::Directional(dir) => {
                            let target = match dir {
                                hwc_parser::DirectionalConstraint::Above { target, .. }
                                | hwc_parser::DirectionalConstraint::Below { target, .. }
                                | hwc_parser::DirectionalConstraint::RightOf { target, .. }
                                | hwc_parser::DirectionalConstraint::LeftOf { target, .. } => {
                                    target
                                }
                            };
                            graph.add_dependency(item_id.clone(), target.base.clone());
                        }
                    }
                }
            }
            PlacementItem::Plane(p) => {
                if let Some(from) = &p.from {
                    graph.extract_dependencies_from_coord(
                        &item_id,
                        from,
                        last_component_name.as_ref(),
                    );
                }
                if let Some(to) = &p.to {
                    graph.extract_dependencies_from_coord(
                        &item_id,
                        to,
                        last_component_name.as_ref(),
                    );
                }
            }
            PlacementItem::Contact(c) => {
                if let Some(pos) = &c.position {
                    graph.extract_dependencies_from_coord(
                        &item_id,
                        pos,
                        last_component_name.as_ref(),
                    );
                }
                // v0.2.0: Handle relational anchor dependencies
                if let Some(anchor) = &c.relational_anchor {
                    graph.add_dependency(item_id.clone(), anchor.region_name.to_string().into());
                }
                // v0.2.1 FIX: Extract dependencies from relational constraints on contacts.
                // Contacts that use `align: center_x with SomeEntity` must be placed AFTER
                // SomeEntity, but this edge was not being registered in the dependency graph.
                for constraint in &c.relational_constraints {
                    match constraint {
                        hwc_parser::RelationalConstraint::Align { target, .. } => match target {
                            hwc_parser::AlignmentTarget::Entity(entity_name) => {
                                graph.add_dependency(item_id.clone(), entity_name.base.clone());
                            }
                            hwc_parser::AlignmentTarget::Expression(expr) => {
                                graph.extract_dependencies_from_expr(
                                    &item_id,
                                    expr,
                                    last_component_name.as_ref(),
                                );
                            }
                        },
                        hwc_parser::RelationalConstraint::Directional(dir) => {
                            let target = match dir {
                                hwc_parser::DirectionalConstraint::Above { target, .. }
                                | hwc_parser::DirectionalConstraint::Below { target, .. }
                                | hwc_parser::DirectionalConstraint::RightOf { target, .. }
                                | hwc_parser::DirectionalConstraint::LeftOf { target, .. } => {
                                    target
                                }
                            };
                            graph.add_dependency(item_id.clone(), target.base.clone());
                        }
                    }
                }
            }
            PlacementItem::SpaceInstance(space_inst) => {
                // v0.2.1: Space instances may depend on other placement items through position expressions
                // Extract dependencies from position coordinate expressions
                graph.extract_dependencies_from_coord(
                    &item_id,
                    &space_inst.position,
                    last_component_name.as_ref(),
                );

                // Note: net_map dependencies are handled during netlist compilation, not placement
            }
            PlacementItem::Route(r) => {
                let resolve_name =
                    |endpoint: &hwc_parser::RouteEndpointSpec| -> compact_str::CompactString {
                        match endpoint {
                            hwc_parser::RouteEndpointSpec::ComponentPin {
                                component_name,
                                component_index,
                                ..
                            } => {
                                if let Some(idx) = component_index {
                                    if let Ok(val) =
                                        crate::ir::routing::evaluate_index_expression(idx)
                                    {
                                        format!("{}[{}]", component_name, val).into()
                                    } else {
                                        component_name.clone()
                                    }
                                } else {
                                    component_name.clone()
                                }
                            }
                            hwc_parser::RouteEndpointSpec::SpaceEntity { name, index, .. } => {
                                if let Some(idx) = index {
                                    if let Ok(val) =
                                        crate::ir::routing::evaluate_index_expression(idx)
                                    {
                                        format!("{}[{}]", name, val).into()
                                    } else {
                                        name.clone()
                                    }
                                } else {
                                    name.clone()
                                }
                            }
                        }
                    };
                let from_name = resolve_name(&r.from);
                let to_name = resolve_name(&r.to);
                graph.add_dependency(item_id.clone(), from_name);
                graph.add_dependency(item_id.clone(), to_name);

                if let Some(w) = &r.width {
                    graph.extract_dependencies_from_expr(&item_id, w, last_component_name.as_ref());
                }

                for (_, expr) in &r.strategy_params {
                    graph.extract_dependencies_from_expr(
                        &item_id,
                        expr,
                        last_component_name.as_ref(),
                    );
                }

                if let Some(path) = &r.path {
                    for wp in path {
                        graph.extract_dependencies_from_coord(
                            &item_id,
                            wp,
                            last_component_name.as_ref(),
                        );
                    }
                }
            }
        }
    }

    graph.topological_sort()
}
