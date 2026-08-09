//! Netlist and pour-metadata registration for placed planes.

use compact_str::CompactString;
use hwc_engine::geometry::BoundingBox;
use hwc_engine::material::MaterialId;
use hwc_engine::space::{HardwareSpace, PourMetadata};
use hwc_engine::Point3D;
use hwc_parser::PlanePlacement;

/// Push the plane's `PourMetadata` into the space.
///
/// Planes are recorded as pours so that downstream collision checks, DRC, and
/// export stages treat them uniformly with explicit pours.
pub fn push_plane_metadata(
    space: &mut HardwareSpace,
    plane: &PlanePlacement,
    resolved_net_name: Option<CompactString>,
    z_start_nm: i64,
    area_nm2: i64,
    bbox: BoundingBox,
) {
    space.pours.push(PourMetadata {
        name: plane.name.to_string(),
        material_name: plane.material.clone(),
        z_bottom_nm: z_start_nm,
        net: resolved_net_name,
        area_nm2,
        bbox: Some(bbox),
        device_binding: None,
        merged_region_id: None,
        waivers: Default::default(),
    });
}

/// Register the plane's netlist component, virtual anchor pin, and net binding.
///
/// Returns the raw net id, or `0` when the plane declares no net.
///
/// v0.1.9: The anchor pin uses the `__virtual_<name>` naming convention so the
/// router can resolve the plane as a routing endpoint.
pub fn register_plane_netlist(
    space: &mut HardwareSpace,
    plane: &PlanePlacement,
    resolved_net_name: Option<&CompactString>,
    start_with_z: Point3D,
    end_with_z: Point3D,
    material_id: MaterialId,
) -> u32 {
    let Some(net_name) = resolved_net_name else {
        return 0;
    };

    let center_x = (start_with_z.x + end_with_z.x) / 2;
    let center_y = (start_with_z.y + end_with_z.y) / 2;
    let center_z = (start_with_z.z + end_with_z.z) / 2;

    let plane_component_id = space.netlist.add_component(
        plane.name.to_string(),
        format!("Plane({})", plane.material).into(),
        (center_x, center_y, center_z),
    );

    // v0.1.9: Use __virtual_ naming convention for routing compatibility
    let virtual_pin_name = format!("__virtual_{}", plane.name);
    let anchor_pin_id = space.netlist.add_pin(
        plane_component_id,
        virtual_pin_name.clone().into(),
        (0, 0, 0),
        None,
    );

    let net_id_handle = if let Some(existing_net) = space.netlist.get_net_by_name(net_name.as_str())
    {
        existing_net
    } else {
        space
            .netlist
            .add_net(net_name.clone(), 100_000, material_id)
    };

    space.netlist.connect_pin(anchor_pin_id, net_id_handle);

    space.entity_graph.add_component_pin(
        center_x,
        center_y,
        center_z,
        plane.name.to_string(),
        virtual_pin_name.into(),
        Some(net_name.clone()),
    );

    net_id_handle.raw()
}
