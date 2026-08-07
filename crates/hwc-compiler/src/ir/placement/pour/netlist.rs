//! Netlist and metadata registration for placed pours.

use compact_str::CompactString;
use hwc_engine::geometry::BoundingBox;
use hwc_engine::space::{DeviceBinding, HardwareSpace, PourMetadata};
use hwc_engine::Point3D;
use hwc_parser::PourPlacement;

/// Resolve the pour's device binding and compute its effective net name.
pub fn resolve_pour_net(space: &HardwareSpace, pour: &PourPlacement) -> Option<CompactString> {
    let mut resolved_net_name = pour.net.as_ref().map(|n| n.base.clone());

    if let Some(binding) = &pour.device {
        let resolved_opt = (|| {
            let netlist = &space.netlist;
            let comp_id = netlist.get_component_by_name(binding.device_name.as_str())?;
            let pins = netlist.get_component_pins(comp_id);

            pins.iter().find_map(|&pin_id| {
                let pin_data = netlist.get_pin(pin_id)?;
                if pin_data.name == binding.terminal {
                    let net_id = pin_data.connected_net?;
                    let net_data = netlist.get_net(net_id)?;
                    Some(net_data.name.clone())
                } else {
                    None
                }
            })
        })();

        if let Some(net_name) = resolved_opt {
            resolved_net_name = Some(net_name);
        }
    }

    resolved_net_name
}

/// Push pour metadata and register all netlist components/pins/anchors for the
/// placed pour. Returns the raw net id (0 if unnetted).
#[allow(clippy::too_many_lines)]
pub fn register_pour_netlist(
    space: &mut HardwareSpace,
    pour: &PourPlacement,
    resolved_net_name: Option<CompactString>,
    z_start_nm: i64,
    start_with_z: Point3D,
    end_with_z: Point3D,
    material_id: u8,
) -> u32 {
    let device_binding = pour.device.as_ref().map(|binding| DeviceBinding {
        device_name: binding.device_name.clone(),
        terminal: binding.terminal.clone(),
    });

    space.pours.push(PourMetadata {
        name: pour.name.to_string(),
        material_name: pour.material.clone(),
        z_bottom_nm: z_start_nm,
        net: resolved_net_name.clone(),
        area_nm2: 0,
        bbox: Some(BoundingBox::new(start_with_z, end_with_z)),
        device_binding,
        merged_region_id: None,
        waivers: pour.waivers.clone(),
    });

    let net_id = if let Some(net_name) = resolved_net_name.as_ref() {
        let center_x = (start_with_z.x + end_with_z.x) / 2;
        let center_y = (start_with_z.y + end_with_z.y) / 2;
        let center_z = (start_with_z.z + end_with_z.z) / 2;

        let pour_component_id = space.netlist.add_component(
            pour.name.to_string(),
            format!("Pour({})", pour.material).into(),
            (center_x, center_y, center_z),
        );

        let anchor_pin_id =
            space
                .netlist
                .add_pin(pour_component_id, "anchor".into(), (0, 0, 0), None);

        // v0.1.8: Also create a virtual pin for routing endpoint resolution.
        //
        // FIX: local_offset_nm must be (0, 0, 0) — NOT (center_x, center_y, center_z).
        let virtual_pin_name = format!("__virtual_{}", pour.name);
        let _virtual_pin_id =
            space
                .netlist
                .add_pin(pour_component_id, virtual_pin_name.into(), (0, 0, 0), None);

        let net_id_handle =
            if let Some(existing_net) = space.netlist.get_net_by_name(net_name.as_str()) {
                existing_net
            } else {
                space
                    .netlist
                    .add_net(net_name.clone(), 100_000, material_id.into())
            };

        space.netlist.connect_pin(anchor_pin_id, net_id_handle);
        space.netlist.connect_pin(_virtual_pin_id, net_id_handle);

        if let Some(binding) = &pour.device {
            if let Some(target_comp_id) = space.netlist.get_component_by_name(&binding.device_name)
            {
                if let Some(target_pin_id) = space
                    .netlist
                    .get_pin_by_name(target_comp_id, &binding.terminal)
                {
                    space.netlist.connect_pin(target_pin_id, net_id_handle);
                    space.entity_graph.set_pin_net(
                        &binding.device_name,
                        &binding.terminal,
                        net_name.as_str(),
                    );
                }
            }
        }

        let comp_name_for_pin = if let Some(binding) = &pour.device {
            binding.device_name.clone()
        } else {
            pour.name.to_string()
        };

        space.entity_graph.add_component_pin(
            center_x,
            center_y,
            center_z,
            comp_name_for_pin,
            "anchor".into(),
            Some(net_name.clone()),
        );

        net_id_handle.raw()
    } else {
        0
    };

    net_id
}
