//! Pin ID resolution for routing endpoints
//!
//! v0.2.0: Database-driven architecture - all position queries go through LayerConnectionDatabase

use crate::ir::errors::IrError;
use hwc_engine::HardwareSpace;

/// Get start and goal pin IDs from route endpoints.
/// This function resolves route endpoints to PinId values for netlist connectivity.
///
/// # v0.2.0 Note
/// This function only resolves PinIds for netlist registration.
/// For routing geometry (XYZ positions), query LayerConnectionDatabase directly.
pub fn get_pin_ids(
    space: &HardwareSpace,
    route: &hwc_parser::Route,
) -> Result<(hwc_engine::netlist::PinId, hwc_engine::netlist::PinId), IrError> {
    eprintln!(
        "[DEBUG get_pin_ids] Called with {} entities registered",
        space.entity_graph.iter_entity_ids().count()
    );

    // Resolve the from/to entity names
    let from_label = crate::ir::routing::helpers::construct_entity_name(&route.from)?;
    let to_label = crate::ir::routing::helpers::construct_entity_name(&route.to)?;

    eprintln!(
        "[DEBUG] Constructing EntityId for space entity: space:{}",
        from_label
    );
    let from_entity_id = hwc_engine::EntityId::from_semantic(&format!("space:{}", from_label));

    eprintln!("[DEBUG] Looking up EntityId: {:?}", from_entity_id);
    let from_entity_data = space
        .entity_graph
        .get_entity_data(from_entity_id)
        .map_err(|_| IrError::InvalidRouteExpression {
            expression: format!("route from {}", from_label),
            reason: format!("Entity '{}' not found in entity graph", from_label),
        })?;

    eprintln!(
        "[DEBUG] Found entity '{}', net_id: {:?}",
        from_label, from_entity_data.net_id
    );

    if from_entity_data.net_id.is_none() {
        eprintln!("[DEBUG] Entity '{}' has no net_id!", from_label);
        return Err(IrError::InvalidRouteExpression {
            expression: format!("route from {}", from_label),
            reason: format!("Entity '{}' has no associated net", from_label),
        });
    }

    eprintln!(
        "[DEBUG] Entity '{}' has valid net_id, continuing...",
        from_label
    );

    // Look up the virtual pin for this entity
    let virtual_pin_name = format!("__virtual_{}", from_label);
    eprintln!("[DEBUG] Looking for virtual pin: {}", virtual_pin_name);

    let from_pin_id = find_virtual_pin(space, &virtual_pin_name, &from_label)?;

    // Same for 'to' endpoint
    eprintln!(
        "[DEBUG] Constructing EntityId for space entity: space:{}",
        to_label
    );
    let to_entity_id = hwc_engine::EntityId::from_semantic(&format!("space:{}", to_label));

    eprintln!("[DEBUG] Looking up EntityId: {:?}", to_entity_id);
    let to_entity_data = space
        .entity_graph
        .get_entity_data(to_entity_id)
        .map_err(|_| IrError::InvalidRouteExpression {
            expression: format!("route to {}", to_label),
            reason: format!("Entity '{}' not found in entity graph", to_label),
        })?;

    eprintln!(
        "[DEBUG] Found entity '{}', net_id: {:?}",
        to_label, to_entity_data.net_id
    );

    if to_entity_data.net_id.is_none() {
        return Err(IrError::InvalidRouteExpression {
            expression: format!("route to {}", to_label),
            reason: format!("Entity '{}' has no associated net", to_label),
        });
    }

    eprintln!(
        "[DEBUG] Entity '{}' has valid net_id, continuing...",
        to_label
    );

    let virtual_pin_name = format!("__virtual_{}", to_label);
    eprintln!("[DEBUG] Looking for virtual pin: {}", virtual_pin_name);

    let to_pin_id = find_virtual_pin(space, &virtual_pin_name, &to_label)?;

    Ok((from_pin_id, to_pin_id))
}

fn find_virtual_pin(
    space: &HardwareSpace,
    virtual_pin_name: &str,
    entity_label: &str,
) -> Result<hwc_engine::netlist::PinId, IrError> {
    let comp_count = space.netlist.component_count();
    eprintln!("[DEBUG] Searching through {} components", comp_count);

    for idx in 0..comp_count {
        let comp_id = hwc_engine::ComponentId::new(idx as u32);
        if let Some(comp) = space.netlist.get_component(comp_id) {
            eprintln!("[DEBUG] Component {}: '{}'", idx, comp.name);

            let pin_ids = space.netlist.get_component_pins(comp_id);
            eprintln!("[DEBUG]   Has {} pins", pin_ids.len());

            for pin_id in pin_ids {
                if let Some(pin) = space.netlist.get_pin(pin_id) {
                    eprintln!("[DEBUG]     Pin: '{}'", pin.name);

                    if pin.name == virtual_pin_name {
                        eprintln!("[DEBUG] ✓ Found matching virtual pin!");
                        return Ok(pin_id);
                    }
                }
            }
        }
    }

    Err(IrError::InvalidRouteExpression {
        expression: format!("route involving {}", entity_label),
        reason: format!(
            "Virtual pin '{}' not found. Entity may not have been registered as a routing endpoint.",
            virtual_pin_name
        ),
    })
}
