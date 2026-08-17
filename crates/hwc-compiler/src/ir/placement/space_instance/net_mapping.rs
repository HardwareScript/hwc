//! Net ID remapping and child-netlist flattening for space instantiation.

use crate::ir::errors::IrError;
use hwc_engine::netlist::NetId;
use hwc_engine::netlist::NetlistArena;
use rustc_hash::FxHashMap;

use super::transform::FixedTransform2D;

/// Build the net ID remapping table from net_map
///
/// Maps child's local net names to parent's NetIds.
/// NO FALLBACKS: All nets in net_map must exist in both child and parent.
pub(super) fn build_net_id_map(
    net_map: &FxHashMap<compact_str::CompactString, compact_str::CompactString>,
    child_netlist: &NetlistArena,
    parent_netlist: &NetlistArena,
) -> Result<FxHashMap<NetId, NetId>, IrError> {
    let mut net_id_map = FxHashMap::default();

    for (child_net_name, parent_net_name) in net_map {
        // Look up child net ID
        let child_net_id = child_netlist
            .get_net_by_name(child_net_name)
            .ok_or_else(|| {
                IrError::PlacementError(format!(
                    "Child net '{}' not found in child space netlist",
                    child_net_name
                ))
            })?;

        // Look up parent net ID
        let parent_net_id = parent_netlist
            .get_net_by_name(parent_net_name)
            .ok_or_else(|| {
                IrError::PlacementError(format!(
                    "Parent net '{}' not found in parent space netlist",
                    parent_net_name
                ))
            })?;

//         eprintln!(
//             "[HIERARCHICAL] Mapping net '{}' (child NetId {}) -> '{}' (parent NetId {})",
//             child_net_name,
//             child_net_id.raw(),
//             parent_net_name,
//             parent_net_id.raw()
//         );

        net_id_map.insert(child_net_id, parent_net_id);
    }

    Ok(net_id_map)
}

/// Transform and copy the child netlist into the parent netlist (v0.2.1)
///
/// Renames components and pins with hierarchical prefixes (e.g., "PMOS_Inst.M1")
/// and maps virtual pins (e.g., "__virtual_Out_Pad" -> "__virtual_PMOS_Inst.Out_Pad")
/// to ensure complete netlist flattening and cross-instance routing resolution.
///
/// This enables:
/// - Proper SPICE netlist export with all hierarchical connections
/// - Cross-instance routing resolution via virtual pin lookups
/// - LVS verification with complete device topology
pub(super) fn transform_netlist(
    child_netlist: &NetlistArena,
    parent_netlist: &mut NetlistArena,
    transform: &FixedTransform2D,
    net_id_map: &FxHashMap<NetId, NetId>,
    instance_name: &str,
) -> Result<(), IrError> {
//     eprintln!(
//         "[HIERARCHICAL] Transforming child netlist: {} components",
//         child_netlist.component_count()
//     );

    // Single-pass: create each component and immediately add its pins.
    //
    // CRITICAL: NetlistArena::add_component() captures
    //   first_pin = self.pins.len()
    // at the moment the component is created.  If all components are created
    // first and pins are added in a second pass, every component gets
    // first_pin = 0 (because the pin vector is still empty).  That makes
    // get_component_pins() return the same first-slot pins for every component,
    // which is exactly what caused every component to show the wrong virtual
    // pin name (__virtual_PMOS_Inst.VDD_Rail for all components instead of the
    // correct per-component name).  By interleaving component creation and pin
    // addition we ensure first_pin is always correct.

    for cid in 0..child_netlist.component_count() {
        let child_comp_id = hwc_engine::netlist::ComponentId::new(cid as u32);
        let child_comp = match child_netlist.get_component(child_comp_id) {
            Some(c) => c,
            None => continue,
        };

        let parent_comp_name = format!("{}.{}", instance_name, child_comp.name);

        // Transform the component's 3D position.
        let (tx, ty, tz) = transform.transform_point(
            child_comp.position_nm.0,
            child_comp.position_nm.1,
            child_comp.position_nm.2,
        )?;

        // Create the component in the parent netlist (or look it up if it
        // already exists, e.g. from a prior call).
        let parent_comp_id =
            if let Some(id) = parent_netlist.get_component_by_name(&parent_comp_name) {
//                 eprintln!(
//                     "[HIERARCHICAL] Component '{}' already exists in parent",
//                     parent_comp_name
//                 );
                id
            } else {
                let id = parent_netlist.add_component(
                    parent_comp_name.clone().into(),
                    child_comp.component_type.clone(),
                    (tx, ty, tz),
                );
//                 eprintln!(
//                     "[HIERARCHICAL] Added component '{}' at ({}, {}, {})",
//                     parent_comp_name, tx, ty, tz
//                 );
                id
            };

        // Immediately add this component's pins while first_pin is correct.
        let child_pins = child_netlist.get_component_pins(child_comp_id);

//         eprintln!(
//             "[HIERARCHICAL] Processing pins for child component '{}' (child_id={})",
//             child_comp_name_str, cid
//         );
//         eprintln!("[HIERARCHICAL]   Child has {} pins", child_pins.len());

        // `child_pins` is a Vec<PinId>; consume it directly to avoid the
        // E0614 compile error that came from the old `*child_pin_id` deref
        // attempt on an already-owned PinId value.
        for child_pin_id in child_pins {
            let child_pin = match child_netlist.get_pin(child_pin_id) {
                Some(p) => p,
                None => continue,
            };

//             eprintln!("[HIERARCHICAL]   Processing child pin '{}'", child_pin.name);

            // Rename virtual pins with the hierarchical instance prefix.
            // e.g. "__virtual_Out_Pad" -> "__virtual_PMOS_Inst.Out_Pad"
            let parent_pin_name = if child_pin.name.starts_with("__virtual_") {
                let core_name = &child_pin.name[10..]; // strip "__virtual_"
                let hierarchical_name = format!("__virtual_{}.{}", instance_name, core_name);
//                 eprintln!(
//                     "[HIERARCHICAL] Renaming virtual pin: '{}' -> '{}'",
//                     child_pin.name, hierarchical_name
//                 );
                hierarchical_name.into()
            } else {
                child_pin.name.clone()
            };

            let parent_pin_id = parent_netlist.add_pin(
                parent_comp_id,
                parent_pin_name.clone(),
                child_pin.local_offset_nm,
                child_pin.pad_shape.clone(),
            );

            // Remap and connect the net.
            if let Some(child_net_id) = child_pin.connected_net {
                if let Some(&parent_net_id) = net_id_map.get(&child_net_id) {
                    parent_netlist.connect_pin(parent_pin_id, parent_net_id);
//                     eprintln!(
//                         "[HIERARCHICAL] Connected pin '{}' to net {}",
//                         parent_pin_name,
//                         parent_net_id.raw()
//                     );
                }
            }
        }
    }

//     eprintln!(
//         "[HIERARCHICAL] Netlist transformation complete: {} virtual pins created",
//         virtual_pins_created
//     );

    Ok(())
}
