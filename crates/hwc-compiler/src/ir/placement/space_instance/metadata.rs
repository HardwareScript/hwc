//! Transform and copy miscellaneous child-space metadata (pours, contacts, etc.).

use crate::ir::errors::IrError;
use hwc_engine::netlist::NetId;
use hwc_engine::HardwareSpace;
use rustc_hash::FxHashMap;

use super::transform::FixedTransform2D;

/// Transform and copy child pours to the parent space
pub(super) fn transform_pours(
    child_space: &HardwareSpace,
    parent_space: &mut HardwareSpace,
    transform: &FixedTransform2D,
    net_map: &FxHashMap<compact_str::CompactString, compact_str::CompactString>,
    instance_name: &str,
) -> Result<(), IrError> {
//     eprintln!(
//         "[HIERARCHICAL] Transforming {} pours",
//         child_space.pours.len()
//     );

    for pour in &child_space.pours {
        let parent_pour_name = format!("{}.{}", instance_name, pour.name);

        let parent_net = if let Some(child_net_name) = &pour.net {
            if let Some(parent_net_name) = net_map.get(child_net_name) {
                Some(parent_net_name.clone())
            } else {
                Some(format!("{}.{}", instance_name, child_net_name).into())
            }
        } else {
            None
        };

        let parent_bbox = if let Some(ref child_bbox) = pour.bbox {
            Some(transform.transform_bbox(child_bbox)?)
        } else {
            None
        };

        let parent_device_binding =
            pour.device_binding
                .as_ref()
                .map(|db| {
                    let def_path = db.def_path.as_ref().map(|p| {
                        let mut new_path = hwc_types::DefPath::root(parent_space.name.as_str());
                        new_path.push_mut(instance_name);
                        for segment in &p.segments {
                            if segment.as_str() != child_space.name.as_str() {
                                new_path.push_mut(segment.clone());
                            }
                        }
                        new_path
                    }).unwrap_or_else(|| {
                        hwc_types::DefPath::root(parent_space.name.as_str())
                            .push(instance_name)
                            .push(db.device_name.as_str())
                    });

                    hwc_engine::space::DeviceBinding {
                        device_name: format!("{}.{}", instance_name, db.device_name).into(),
                        terminals: db.terminals.clone(), // v0.2.2: Clone all terminals
                        priority: db.priority, // v0.2.2: Already engine type, no conversion needed
                        def_path: Some(def_path),
                    }
                });

        parent_space.pours.push(hwc_engine::space::PourMetadata {
            name: parent_pour_name.into(),
            material_name: pour.material_name.clone(),
            layer_name: pour.layer_name.clone(),
            z_bottom_nm: pour.z_bottom_nm + transform.offset_z_nm,
            net: parent_net,
            area_nm2: pour.area_nm2,
            bbox: parent_bbox,
            device_binding: parent_device_binding,
            merged_region_id: pour.merged_region_id.clone(),
            waivers: pour.waivers.clone(),
        });
    }

    Ok(())
}

/// Transform and copy child contacts to the parent space
pub(super) fn transform_contacts(
    child_space: &HardwareSpace,
    parent_space: &mut HardwareSpace,
    transform: &FixedTransform2D,
    net_map: &FxHashMap<compact_str::CompactString, compact_str::CompactString>,
    instance_name: &str,
) -> Result<(), IrError> {
//     eprintln!(
//         "[HIERARCHICAL] Transforming {} contacts",
//         child_space.contacts.len()
//     );

    for contact in &child_space.contacts {
        let parent_contact_name = format!("{}.{}", instance_name, contact.name);

        let parent_net = if let Some(child_net_name) = &contact.net {
            if let Some(parent_net_name) = net_map.get(child_net_name) {
                Some(parent_net_name.clone())
            } else {
                Some(format!("{}.{}", instance_name, child_net_name).into())
            }
        } else {
            None
        };

        let parent_bbox = if let Some(ref child_bbox) = contact.bbox {
            Some(transform.transform_bbox(child_bbox)?)
        } else {
            None
        };

        // Register transformed contact in parent ViaInstanceDatabase to prevent auto-via insertion duplicates
        if let (Some(ref p_bbox), Some(ref from_layer), Some(ref to_layer), Some(ref p_net_name)) =
            (&parent_bbox, &contact.from_layer, &contact.to_layer, &parent_net)
        {
            let parent_net_id = parent_space
                .netlist
                .get_net_by_name(p_net_name.as_str())
                .unwrap_or_else(|| {
                    parent_space.netlist.add_net(
                        p_net_name.clone(),
                        100_000,
                        0,
                    )
                });

            let xy_bbox = (
                p_bbox.min.x,
                p_bbox.min.y,
                p_bbox.max.x,
                p_bbox.max.y,
            );
            let z_range = (
                contact.z_start_nm + transform.offset_z_nm,
                contact.z_end_nm + transform.offset_z_nm,
            );

            parent_space.via_instance_db.register(
                &parent_contact_name,
                parent_net_id,
                from_layer.as_str(),
                to_layer.as_str(),
                xy_bbox,
                z_range,
            );
        }

        parent_space
            .contacts
            .push(hwc_engine::space::ContactMetadata {
                name: parent_contact_name.into(),
                material_name: contact.material_name.clone(),
                z_start_nm: contact.z_start_nm + transform.offset_z_nm,
                z_end_nm: contact.z_end_nm + transform.offset_z_nm,
                net: parent_net,
                bridge: contact.bridge.clone(),
                bbox: parent_bbox,
                drill_diameter_nm: contact.drill_diameter_nm,
                is_tented: contact.is_tented,
                mask_clearance_diameter_nm: contact.mask_clearance_diameter_nm,
                from_layer: contact.from_layer.clone(),
                to_layer: contact.to_layer.clone(),
            });
    }

    Ok(())
}

/// Transform and copy child keep-out zones to the parent space
pub(super) fn transform_keep_out_zones(
    child_space: &HardwareSpace,
    parent_space: &mut HardwareSpace,
    transform: &FixedTransform2D,
    net_id_map: &FxHashMap<NetId, NetId>,
    net_map: &FxHashMap<compact_str::CompactString, compact_str::CompactString>,
    instance_name: &str,
) -> Result<(), IrError> {
//     eprintln!(
//         "[HIERARCHICAL] Transforming {} keep-out zones",
//         child_space.keep_out_zones.len()
//     );

    for koz in &child_space.keep_out_zones {
        let parent_bbox = transform.transform_bbox(&koz.bbox)?;

        let parent_net_id = if let Some(child_net_id) = koz.net_id {
            net_id_map.get(&child_net_id).copied()
        } else {
            None
        };

        let parent_exempted_nets = koz
            .exempted_nets
            .iter()
            .map(|child_net_name| {
                if let Some(parent_net_name) = net_map.get(child_net_name) {
                    parent_net_name.clone()
                } else {
                    format!("{}.{}", instance_name, child_net_name).into()
                }
            })
            .collect();

        parent_space
            .keep_out_zones
            .push(hwc_engine::space::KeepOutZone {
                bbox: parent_bbox,
                net_id: parent_net_id,
                allow_vias: koz.allow_vias,
                allow_routing: koz.allow_routing,
                exempted_nets: parent_exempted_nets,
            });
    }

    Ok(())
}

/// Transform and copy child component bounding boxes to the parent space
pub(super) fn transform_component_bboxes(
    child_space: &HardwareSpace,
    parent_space: &mut HardwareSpace,
    transform: &FixedTransform2D,
    instance_name: &str,
) -> Result<(), IrError> {
//     eprintln!(
//         "[HIERARCHICAL] Transforming {} component bounding boxes",
//         child_space.component_bboxes.len()
//     );

    for (child_comp_name, child_bbox) in &child_space.component_bboxes {
        let parent_comp_name = format!("{}.{}", instance_name, child_comp_name);
        let parent_bbox = transform.transform_bbox(child_bbox)?;
        parent_space
            .component_bboxes
            .insert(parent_comp_name.into(), parent_bbox);
    }

    Ok(())
}
