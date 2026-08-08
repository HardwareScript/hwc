use super::helpers::get_prop_nm;
use crate::ir::errors::IrError;
use crate::SymbolTable;
use hwc_engine::{HardwareSpace, Point3D};
use hwc_parser::{ContactPlacement, EvaluationContext};

pub(super) struct NetlistRegistration<'a> {
    pub space: &'a mut HardwareSpace,
    pub contact: &'a ContactPlacement,
    pub diameter_nm: i64,
    pub material_id: u8,
    pub xy_point: Point3D,
    pub start_z: i64,
    pub end_z: i64,
    pub symbol_table: &'a SymbolTable,
    pub eval_context: &'a EvaluationContext,
}

pub(super) fn register_contact_in_netlist(args: NetlistRegistration) -> Result<(), IrError> {
    let NetlistRegistration {
        space,
        contact,
        diameter_nm,
        material_id,
        xy_point,
        start_z,
        end_z,
        symbol_table,
        eval_context,
    } = args;
    if let Some(net_name) = &contact.net {
        let contact_name: compact_str::CompactString = contact.name.base.clone();

        let contact_component_id = space.netlist.add_component(
            contact_name.clone(),
            format!("Contact({})", contact.material).into(),
            (xy_point.x, xy_point.y, (start_z + end_z) / 2),
        );

        let virtual_pin_name = format!("__virtual_{}", contact_name);
        let contact_pin_id = space.netlist.add_pin(
            contact_component_id,
            virtual_pin_name.clone().into(),
            (0, 0, 0),
            None,
        );

        let net_id =
            if let Some(existing_net) = space.netlist.get_net_by_name(net_name.base.as_str()) {
                existing_net
            } else {
                space
                    .netlist
                    .add_net(net_name.to_string(), diameter_nm, material_id)
            };

        space.netlist.connect_pin(contact_pin_id, net_id);

        space.entity_graph.add_component_pin(
            xy_point.x,
            xy_point.y,
            (start_z + end_z) / 2,
            contact_name.clone(),
            virtual_pin_name.into(),
            Some(net_name.base.clone()),
        );
    }
    let _ = (symbol_table, eval_context);
    Ok(())
}

pub(super) struct ContactMetadataStorage<'a> {
    pub space: &'a mut HardwareSpace,
    pub contact: &'a ContactPlacement,
    pub from_bottom_nm: i64,
    pub to_bottom_nm: i64,
    pub diameter_nm: i64,
    pub pad_bbox: hwc_engine::geometry::BoundingBox,
    pub is_tented: bool,
    pub bridge_material_name: Option<compact_str::CompactString>,
    pub contact_name_debug: &'a str,
    pub symbol_table: &'a SymbolTable,
    pub eval_context: &'a EvaluationContext,
}

pub(super) fn store_contact_metadata(args: ContactMetadataStorage) {
    let ContactMetadataStorage {
        space,
        contact,
        from_bottom_nm,
        to_bottom_nm,
        diameter_nm,
        pad_bbox,
        is_tented,
        bridge_material_name,
        contact_name_debug,
        symbol_table,
        eval_context,
    } = args;
    let contact_name: compact_str::CompactString = contact.name.base.clone();

    println!(
        "[PLACE_CONTACT] '{}' Storing contact metadata: bbox=({},{}-{},{}), z={}→{}nm, net={:?}",
        contact_name_debug,
        pad_bbox.min.x,
        pad_bbox.min.y,
        pad_bbox.max.x,
        pad_bbox.max.y,
        from_bottom_nm,
        to_bottom_nm,
        contact.net
    );
    space.contacts.push(hwc_engine::ContactMetadata {
        name: contact_name,
        material_name: contact.material.clone(),
        z_start_nm: from_bottom_nm,
        z_end_nm: to_bottom_nm,
        net: contact.net.as_ref().map(|n| n.to_string()),
        bridge: bridge_material_name,
        bbox: Some(pad_bbox),
        drill_diameter_nm: Some(diameter_nm),
        is_tented,
        mask_clearance_diameter_nm: get_prop_nm(
            contact,
            "mask_clearance_diameter",
            symbol_table,
            eval_context,
        ),
    });
}
