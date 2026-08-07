//! Layer connection database surface registration for pours.

use hwc_engine::geometry::BoundingBox;
use hwc_engine::layer_connection_database::ConnectionType;
use hwc_engine::space::HardwareSpace;
use hwc_parser::PourPlacement;

/// Register the pour surface in the layer connection database so the router can
/// resolve connections on the pour's routing layer.
///
/// Pours exist on a single Z plane, so they register as a `PourSurface` type.
/// The official routing Z (not `bbox.min.z`) is used so base layers and
/// interconnect layers route at the correct elevation.
pub fn register_pour_surface(
    space: &mut HardwareSpace,
    pour: &PourPlacement,
    layer_name: &str,
    bbox: BoundingBox,
) {
    let pour_center_x = (bbox.min.x + bbox.max.x) / 2;
    let pour_center_y = (bbox.min.y + bbox.max.y) / 2;

    // v0.2.1 FIX: Use the routing layer's official routing_z, not bbox.min.z
    // Base layers (polyres, active) route at z_top, interconnect (metal1+) at z_bottom
    let routing_z = match space.routing_layer_db.get_routing_z(layer_name) {
        Ok(z) => z,
        Err(_) => {
            // Layer not found or not routable - use z_bottom as fallback
            eprintln!(
                "[PLACE_POUR] WARNING: Layer '{}' not found in routing database, using z_bottom={}nm",
                layer_name, bbox.min.z
            );
            bbox.min.z
        }
    };

    let material_id = space
        .material_registry
        .get_id(&pour.material)
        .unwrap_or_default();

    if let Err(e) = space.layer_connection_db.register_surface(
        &pour.name.to_string(),
        layer_name,
        routing_z,
        (pour_center_x, pour_center_y),
        material_id,
        ConnectionType::PourSurface,
    ) {
        eprintln!(
            "[PLACE_POUR] WARNING: Failed to register pour '{}' connection: {}",
            pour.name, e
        );
    } else {
        eprintln!(
            "[PLACE_POUR] Registered pour '{}' surface on layer '{}' at routing Z={}nm (routing layer elevation)",
            pour.name, layer_name, routing_z
        );
    }
}
