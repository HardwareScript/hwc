//! Layer connection database surface registration for planes.

use hwc_engine::layer_connection_database::ConnectionType;
use hwc_engine::stackup::LayerKind;
use hwc_engine::space::HardwareSpace;
use hwc_engine::Point3D;

/// Register the plane surface in the layer connection database so the router can
/// resolve connections on the plane's routing layer.
///
/// **v0.2.3**: Uses strong typing to handle mask layers and non-routable layers.
/// **v0.3.0**: Updated to use LayerKind (pure type-driven classification).
/// Only routable conductive layers participate in routing registration.
///
/// # Returns
/// - `Ok(())` if the plane was registered or correctly skipped
/// - `Err` if the layer doesn't exist in the stackup
pub fn register_plane_surface(
    space: &mut HardwareSpace,
    plane_name: &str,
    layer_name: &str,
    start_with_z: Point3D,
    end_with_z: Point3D,
    material_id: u8,
) -> Result<(), String> {
    // Get layer definition from routing database
    let layer = space
        .routing_layer_db
        .get_layer(layer_name)
        .map_err(|e| format!("Layer lookup failed for '{}': {}", layer_name, e))?;

    // Handle based on layer kind using pattern matching
    match layer.kind {
        LayerKind::LithoMask | LayerKind::Dielectric => {
            // Non-routable layers: log and skip registration
            eprintln!(
                "[PLACE_PLANE] Plane '{}' on non-routable layer '{}' (kind={:?}, Z={}nm) - skipped routing registration",
                plane_name, layer_name, layer.kind, layer.z_bottom
            );
            Ok(())
        }
        LayerKind::SemiconductorActive | LayerKind::ConductiveInterconnect => {
            // Routable layers: register surface for pathfinder
            let plane_center_x = (start_with_z.x + end_with_z.x) / 2;
            let plane_center_y = (start_with_z.y + end_with_z.y) / 2;
            let routing_z = layer.routing_z;

            space
                .layer_connection_db
                .register_surface(
                    plane_name,
                    layer_name,
                    routing_z,
                    (plane_center_x, plane_center_y),
                    material_id,
                    ConnectionType::PourSurface,
                )
                .map_err(|e| format!("Failed to register plane '{}': {}", plane_name, e))?;

            eprintln!(
                "[PLACE_PLANE] Registered plane '{}' on {} layer '{}' at routing Z={}nm",
                plane_name,
                match layer.kind {
                    LayerKind::SemiconductorActive => "SEMICONDUCTOR ACTIVE",
                    LayerKind::ConductiveInterconnect => "CONDUCTIVE INTERCONNECT",
                    _ => unreachable!(),
                },
                layer_name,
                routing_z
            );
            Ok(())
        }
    }
}
