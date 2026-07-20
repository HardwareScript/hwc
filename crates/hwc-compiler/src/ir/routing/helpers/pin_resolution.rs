use crate::ir::errors::IrError;
use hwc_engine::{HardwareSpace, Point3D};

use super::endpoint_resolution::{
    construct_entity_name, evaluate_index_expression, list_available_endpoints,
};

/// Get start and goal positions from route endpoints (v0.1.8).
pub fn get_pin_positions(
    space: &HardwareSpace,
    route: &hwc_parser::Route,
) -> Result<(Point3D, Point3D), IrError> {
    let resolve_entity_position =
        |endpoint: &hwc_parser::RouteEndpointSpec| -> Result<Point3D, IrError> {
            let entity_name = construct_entity_name(endpoint)?;

            let entity_id = match endpoint {
                hwc_parser::RouteEndpointSpec::ComponentPin { .. } => {
                    let full_comp_name = construct_entity_name(endpoint)?;
                    let pin_name = match endpoint {
                        hwc_parser::RouteEndpointSpec::ComponentPin {
                            pin_name,
                            pin_index,
                            ..
                        } => {
                            if let Some(ref idx) = pin_index {
                                let val = evaluate_index_expression(idx)?;
                                format!("{}[{}]", pin_name, val)
                            } else {
                                pin_name.to_string()
                            }
                        }
                        _ => unreachable!(),
                    };
                    hwc_engine::geometry::EntityId::from_semantic(&format!(
                        "pin:{}:{}",
                        full_comp_name, pin_name
                    ))
                }
                hwc_parser::RouteEndpointSpec::SpaceEntity { .. } => {
                    hwc_engine::geometry::EntityId::from_semantic(&format!("space:{}", entity_name))
                }
            };

            let entity_data = space
            .entity_graph
            .get_entity_data(entity_id)
            .map_err(|_| {
                let available = list_available_endpoints(space);
                IrError::UnresolvedEndpoint {
                    endpoint: entity_name.to_string(),
                    span: miette::SourceSpan::from((endpoint.span().start, endpoint.span().end)),
                    help_message: format!("Verify that the component, pin, or space pour/pad exists and is correctly named.{}", available),
                }
            })?;

            let pos = Point3D::new(
                (entity_data.bbox.min.x + entity_data.bbox.max.x) / 2,
                (entity_data.bbox.min.y + entity_data.bbox.max.y) / 2,
                (entity_data.bbox.min.z + entity_data.bbox.max.z) / 2,
            );

            Ok(pos)
        };

    let start_pos = resolve_entity_position(&route.from)?;
    let goal_pos = resolve_entity_position(&route.to)?;

    eprintln!(
        "[get_pin_positions] Queried EntityGraph: {} = ({}, {}, {}), {} = ({}, {}, {})",
        super::endpoint_label(&route.from),
        start_pos.x,
        start_pos.y,
        start_pos.z,
        super::endpoint_label(&route.to),
        goal_pos.x,
        goal_pos.y,
        goal_pos.z
    );

    Ok((start_pos, goal_pos))
}

/// Get start and goal pin IDs from route endpoints (v0.1.8).
pub fn get_pin_ids(
    space: &HardwareSpace,
    route: &hwc_parser::Route,
) -> Result<(hwc_engine::netlist::PinId, hwc_engine::netlist::PinId), IrError> {
    eprintln!(
        "[DEBUG get_pin_ids] Called with {} entities registered",
        space.entity_graph.iter_entity_ids().count()
    );

    let resolve_endpoint =
        |endpoint: &hwc_parser::RouteEndpointSpec| -> Result<hwc_engine::netlist::PinId, IrError> {
            let entity_name = construct_entity_name(endpoint)?;

            let entity_id = match endpoint {
                hwc_parser::RouteEndpointSpec::ComponentPin { .. } => {
                    let full_comp_name = construct_entity_name(endpoint)?;
                    let pin_name = match endpoint {
                        hwc_parser::RouteEndpointSpec::ComponentPin {
                            pin_name,
                            pin_index,
                            ..
                        } => {
                            if let Some(ref idx) = pin_index {
                                let val = evaluate_index_expression(idx)?;
                                format!("{}[{}]", pin_name, val)
                            } else {
                                pin_name.to_string()
                            }
                        }
                        _ => unreachable!(),
                    };
                    hwc_engine::geometry::EntityId::from_semantic(&format!(
                        "pin:{}:{}",
                        full_comp_name, pin_name
                    ))
                }
                hwc_parser::RouteEndpointSpec::SpaceEntity { .. } => {
                    let entity_name = construct_entity_name(endpoint)?;
                    eprintln!(
                        "[DEBUG] Constructing EntityId for space entity: space:{}",
                        entity_name
                    );
                    hwc_engine::geometry::EntityId::from_semantic(&format!("space:{}", entity_name))
                }
            };

            eprintln!("[DEBUG] Looking up EntityId: {}", entity_id);

            let entity_data = space.entity_graph.get_entity_data(entity_id)
            .map_err(|_| {
                let available = list_available_endpoints(space);
                eprintln!("[DEBUG] get_entity_data FAILED for EntityId: {}", entity_id);
                IrError::UnresolvedEndpoint {
                    endpoint: entity_name.to_string(),
                    span: miette::SourceSpan::from((endpoint.span().start, endpoint.span().end)),
                    help_message: format!("Verify that the component, pin, or space pour/pad exists and is correctly named.{}", available),
                }
            })?;

            eprintln!(
                "[DEBUG] Found entity '{}', net_id: {:?}",
                entity_data.name, entity_data.net_id
            );

            let _net_id = entity_data.net_id.ok_or_else(|| {
            eprintln!("[DEBUG] Entity '{}' has NO net_id!", entity_name);
            IrError::UnresolvedEndpoint {
                endpoint: format!("Entity '{}' has no net assignment (check PDK/Script)", entity_name),
                span: miette::SourceSpan::from((endpoint.span().start, endpoint.span().end)),
                help_message: "Ensure the entity has a net: binding in the space definition or component layout.".to_string(),
            }
        })?;

            eprintln!(
                "[DEBUG] Entity '{}' has valid net_id, continuing...",
                entity_name
            );

            if let hwc_engine::geometry_router::entity_graph::EntityType::ComponentPin =
                entity_data.entity_type
            {
                let comp_name = match endpoint {
                    hwc_parser::RouteEndpointSpec::ComponentPin { component_name, .. } => {
                        component_name.as_str()
                    }
                    _ => unreachable!(),
                };
                let comp_id = space
                    .netlist
                    .get_component_by_name(comp_name)
                    .ok_or_else(|| IrError::PinNotFound {
                        component: comp_name.into(),
                        pin: entity_data.name.to_string(),
                    })?;

                let pins = space.netlist.get_component_pins(comp_id);
                pins.iter()
                    .find(|&&pid| {
                        if let Some(pin) = space.netlist.get_pin(pid) {
                            pin.name == entity_data.name.split('.').next_back().unwrap_or("")
                        } else {
                            false
                        }
                    })
                    .copied()
                    .ok_or_else(|| IrError::PinNotFound {
                        component: comp_name.into(),
                        pin: entity_data.name.to_string(),
                    })
            } else {
                let virtual_pin_name = format!("__virtual_{}", entity_name);
                let mut found_pin = None;
                for cid in 0..space.netlist.component_count() {
                    if let Some(pin_id) = space.netlist.get_pin_by_name(
                        hwc_engine::netlist::ComponentId::new(cid as u32),
                        &virtual_pin_name,
                    ) {
                        found_pin = Some(pin_id);
                        break;
                    }
                }
                if let Some(pin_id) = found_pin {
                    Ok(pin_id)
                } else {
                    let available = list_available_endpoints(space);
                    Err(IrError::UnresolvedEndpoint {
                    endpoint: entity_name.to_string(),
                    span: miette::SourceSpan::from((endpoint.span().start, endpoint.span().end)),
                    help_message: format!("Verify that the component, pin, or space pour/pad exists and is correctly named.{}", available),
                })
                }
            }
        };

    let start_pin_id = resolve_endpoint(&route.from)?;
    let goal_pin_id = resolve_endpoint(&route.to)?;

    Ok((start_pin_id, goal_pin_id))
}
