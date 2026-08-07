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
    eprintln!(
        "[HIERARCHICAL] Transforming {} pours",
        child_space.pours.len()
    );

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

        let parent_device_binding = if let Some(ref db) = pour.device_binding {
            Some(hwc_engine::space::DeviceBinding {
                device_name: format!("{}.{}", instance_name, db.device_name).into(),
                terminal: db.terminal.clone(),
            })
        } else {
            None
        };

        parent_space.pours.push(hwc_engine::space::PourMetadata {
            name: parent_pour_name.into(),
            material_name: pour.material_name.clone(),
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
    eprintln!(
        "[HIERARCHICAL] Transforming {} contacts",
        child_space.contacts.len()
    );

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
    eprintln!(
        "[HIERARCHICAL] Transforming {} keep-out zones",
        child_space.keep_out_zones.len()
    );

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
    eprintln!(
        "[HIERARCHICAL] Transforming {} component bounding boxes",
        child_space.component_bboxes.len()
    );

    for (child_comp_name, child_bbox) in &child_space.component_bboxes {
        let parent_comp_name = format!("{}.{}", instance_name, child_comp_name);
        let parent_bbox = transform.transform_bbox(child_bbox)?;
        parent_space
            .component_bboxes
            .insert(parent_comp_name.into(), parent_bbox);
    }

    Ok(())
}
