//! Build the spatial dependency graph and topologically sort placement items.
//!
//! v0.2.x (pure Arena): nodes are dense `usize` indices matching
//! `ContextualPlacementItem::item_index`, and the sort returns `Vec<usize>`.
//! Entity names are read straight from the arena during registration so no
//! per-item string keys are allocated or stored.

use crate::ir::errors::IrError;
use crate::ir::placement_item::{ContextualPlacementItem, PlacementItem};
use hwc_parser::ast::arena::AstArena;

/// Entity name an item can be referenced by, if it has one.
///
/// Substrates and routes are never referenced by name, so they register as
/// anonymous nodes (which still participate in the sort).
fn item_name<'a>(item: &PlacementItem, arena: &'a AstArena) -> Option<&'a str> {
    match item {
        PlacementItem::Region(id) => Some(arena.regions[*id].name.as_str()),
        PlacementItem::Component(id) => {
            arena.components[*id].name.as_ref().map(|n| n.base.as_str())
        }
        PlacementItem::Pour(id) => Some(arena.pours[*id].name.as_str()),
        PlacementItem::Plane(id) => Some(arena.planes[*id].name.as_str()),
        PlacementItem::Contact(id) => Some(arena.contacts[*id].name.base.as_str()),
        PlacementItem::SpaceInstance(id) => {
            Some(arena.space_instances[*id].instance_name.base.as_str())
        }
        PlacementItem::Substrate(_) | PlacementItem::Route(_) => None,
    }
}

/// Build the dependency graph from placement items and return topologically
/// sorted **item indices** (not names).
pub fn build_and_sort(
    placement_items: &[ContextualPlacementItem],
    _symbol_table: &crate::SymbolTable,
    arena: &AstArena,
) -> Result<Vec<usize>, IrError> {
    let mut graph = crate::ir::spatial_dependency_graph::SpatialDependencyGraph::with_capacity(
        placement_items.len(),
    );
    let mut last_component: Option<usize> = None;

    // Pass 1: Register all nodes (and the names they can be referenced by).
    for contextual_item in placement_items.iter() {
        let name = item_name(&contextual_item.item, arena);
        graph.add_node(contextual_item.item_index, name);
    }

    // Pass 2: Extract dependencies.
    for contextual_item in placement_items.iter() {
        let node = contextual_item.item_index;
        let item = &contextual_item.item;

        match item {
            PlacementItem::Region(region_id) => {
                let r = &arena.regions[*region_id];
                if let Some(anchor) = &r.anchor {
                    match anchor {
                        hwc_parser::RegionAnchor::Absolute(_) => {}
                        hwc_parser::RegionAnchor::Expression(expr) => {
                            graph.extract_dependencies_from_expr(node, expr, last_component);
                        }
                        hwc_parser::RegionAnchor::Offset { base, offset, .. } => {
                            graph.extract_dependencies_from_expr(node, base, last_component);
                            graph.extract_dependencies_from_coord(node, offset, last_component);
                        }
                    }
                }
                for constraint in &r.constraints {
                    graph.add_dependency(node, constraint.target.as_str());
                    if let Some(spacing) = &constraint.spacing {
                        graph.extract_dependencies_from_expr(node, spacing, last_component);
                    }
                }
            }
            PlacementItem::Substrate(substrate_id) => {
                let s = &arena.substrates[*substrate_id];
                graph.extract_dependencies_from_coord(node, &s.from, last_component);
                graph.extract_dependencies_from_coord(node, &s.to, last_component);
            }
            PlacementItem::Component(c) => {
                let comp = &arena.components[*c];
                if let Some(position) = &comp.position {
                    graph.extract_dependencies_from_coord(node, position, last_component);
                }
                for constraint in &comp.relational_constraints {
                    add_relational_constraint(&mut graph, node, constraint, last_component);
                }
                last_component = Some(node);
            }
            PlacementItem::Pour(p) => {
                let pour = &arena.pours[*p];
                if let Some(boundary) = &pour.boundary {
                    match boundary {
                        hwc_parser::PourBoundary::Rect(from, to) => {
                            graph.extract_dependencies_from_coord(node, from, last_component);
                            graph.extract_dependencies_from_coord(node, to, last_component);
                        }
                        hwc_parser::PourBoundary::Circle { center, radius } => {
                            graph.extract_dependencies_from_coord(node, center, last_component);
                            graph.extract_dependencies_from_expr(node, radius, last_component);
                        }
                    }
                }
                for constraint in &pour.relational_constraints {
                    add_relational_constraint(&mut graph, node, constraint, last_component);
                }
            }
            PlacementItem::Plane(p) => {
                let plane = &arena.planes[*p];
                if let Some(from) = &plane.from {
                    graph.extract_dependencies_from_coord(node, from, last_component);
                }
                if let Some(to) = &plane.to {
                    graph.extract_dependencies_from_coord(node, to, last_component);
                }
            }
            PlacementItem::Contact(c) => {
                let contact = &arena.contacts[*c];
                if let Some(pos) = &contact.position {
                    graph.extract_dependencies_from_coord(node, pos, last_component);
                }
                for constraint in &contact.relational_constraints {
                    add_relational_constraint(&mut graph, node, constraint, last_component);
                }
            }
            PlacementItem::SpaceInstance(space_inst) => {
                let si = &arena.space_instances[*space_inst];
                graph.extract_dependencies_from_coord(node, &si.position, last_component);
            }
            PlacementItem::Route(r) => {
                let route = &arena.routes[*r];
                add_route_endpoint_dependency(&mut graph, node, &route.from);
                add_route_endpoint_dependency(&mut graph, node, &route.to);

                if let Some(w) = &route.width {
                    graph.extract_dependencies_from_expr(node, w, last_component);
                }

                for (_, expr) in &route.strategy_params {
                    graph.extract_dependencies_from_expr(node, expr, last_component);
                }

                if let Some(path) = &route.path {
                    for wp in path {
                        graph.extract_dependencies_from_coord(node, wp, last_component);
                    }
                }
            }
        }
    }

    graph.topological_sort()
}

/// Register the dependency implied by an `align:`/directional relational constraint.
fn add_relational_constraint(
    graph: &mut crate::ir::spatial_dependency_graph::SpatialDependencyGraph,
    node: usize,
    constraint: &hwc_parser::RelationalConstraint,
    last_component: Option<usize>,
) {
    match constraint {
        hwc_parser::RelationalConstraint::Align { target, .. } => match target {
            hwc_parser::AlignmentTarget::Entity(entity_name) => {
                graph.add_dependency(node, entity_name.base.as_str());
            }
            hwc_parser::AlignmentTarget::Expression(expr) => {
                graph.extract_dependencies_from_expr(node, expr, last_component);
            }
        },
        hwc_parser::RelationalConstraint::Directional(dir) => {
            let target = match dir {
                hwc_parser::DirectionalConstraint::Above { target, .. }
                | hwc_parser::DirectionalConstraint::Below { target, .. }
                | hwc_parser::DirectionalConstraint::RightOf { target, .. }
                | hwc_parser::DirectionalConstraint::LeftOf { target, .. } => target,
            };
            graph.add_dependency(node, target.base.as_str());
        }
    }
}

/// Register the dependency on a route endpoint's referenced entity.
fn add_route_endpoint_dependency(
    graph: &mut crate::ir::spatial_dependency_graph::SpatialDependencyGraph,
    node: usize,
    endpoint: &hwc_parser::RouteEndpointSpec,
) {
    let (name, index) = match endpoint {
        hwc_parser::RouteEndpointSpec::ComponentPin {
            component_name,
            component_index,
            ..
        } => (component_name, component_index),
        hwc_parser::RouteEndpointSpec::SpaceEntity { name, index, .. } => (name, index),
    };

    if let Some(idx) = index {
        if let Ok(val) = crate::ir::routing::evaluate_index_expression(idx) {
            // Indexed reference (e.g. `J0[2]`): try the exact name first, then
            // fall back to the base name inside `add_dependency`.
            graph.add_dependency(node, &format!("{}[{}]", name, val));
            return;
        }
    }

    graph.add_dependency(node, name.as_str());
}
