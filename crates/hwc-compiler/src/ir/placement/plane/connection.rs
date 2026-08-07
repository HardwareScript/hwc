//! Layer connection database surface registration for planes.

use hwc_engine::layer_connection_database::ConnectionType;
use hwc_engine::space::HardwareSpace;
use hwc_engine::Point3D;

/// Register the plane surface in the layer connection database so the router can
/// resolve connections on the plane's routing layer.
///
/// v0.2.0: Planes exist on a single Z plane, so they register as a
/// `PourSurface` type.
///
/// v0.2.1 FIX: Uses the routing layer's official `routing_z`, not
/// `start_with_z.z`. Base layers (polyres, active) route at `z_top` while
/// interconnect layers (metal1+) route at `z_bottom`.
pub fn register_plane_surface(
    space: &mut HardwareSpace,
    plane_name: &str,
    layer_name: &str,
    start_with_z: Point3D,
    end_with_z: Point3D,
    material_id: u8,
) {
    let plane_center_x = (start_with_z.x + end_with_z.x) / 2;
    let plane_center_y = (start_with_z.y + end_with_z.y) / 2;

    let routing_z = match space.routing_layer_db.get_routing_z(layer_name) {
        Ok(z) => z,
        Err(_) => {
            // Layer not found or not routable - use z_bottom as fallback
            eprintln!(
                "[PLACE_PLANE] WARNING: Layer '{}' not found in routing database, using z_bottom={}nm",
                layer_name, start_with_z.z
            );
            start_with_z.z
        }
    };

    if let Err(e) = space.layer_connection_db.register_surface(
        plane_name,
        layer_name,
        routing_z,
        (plane_center_x, plane_center_y),
        material_id,
        ConnectionType::PourSurface,
    ) {
        eprintln!(
            "[PLACE_PLANE] WARNING: Failed to register plane '{}' connection: {}",
            plane_name, e
        );
    } else {
        eprintln!(
            "[PLACE_PLANE] Registered plane '{}' surface on layer '{}' at routing Z={}nm (routing layer elevation)",
            plane_name, layer_name, routing_z
        );
    }
}
