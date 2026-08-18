//! Layer connection database surface registration for pours.

use hwc_engine::geometry::BoundingBox;
use hwc_engine::layer_connection_database::ConnectionType;
use hwc_engine::stackup::LayerKind;
use hwc_engine::space::HardwareSpace;
use hwc_parser::PourPlacement;

/// Register the pour surface in the layer connection database so the router can
/// resolve connections on the pour's routing layer.
///
/// **v0.2.3**: Uses strong typing to handle mask layers and non-routable layers.
/// **v0.3.0**: Updated to use LayerKind (pure type-driven classification).
/// Only routable conductive layers participate in routing registration.
///
/// # Returns
/// - `Ok(())` if the pour was registered or correctly skipped
/// - `Err` if the layer doesn't exist in the stackup
pub fn register_pour_surface(
    space: &mut HardwareSpace,
    pour: &PourPlacement,
    layer_name: &str,
    bbox: BoundingBox,
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
                "[PLACE_POUR] Pour '{}' on non-routable layer '{}' (kind={:?}, Z={}nm) - skipped routing registration",
                pour.name, layer_name, layer.kind, layer.z_bottom
            );
            Ok(())
        }
        LayerKind::SemiconductorActive | LayerKind::ConductiveInterconnect => {
            // Routable layers: register surface for pathfinder
            let routing_z = layer.routing_z;
            let pour_center_x = (bbox.min.x + bbox.max.x) / 2;
            let pour_center_y = (bbox.min.y + bbox.max.y) / 2;

            let material_id = space
                .material_registry
                .get_id(&pour.material)
                .ok_or_else(|| {
                    format!("Material '{}' not found in registry", pour.material)
                })?;

            space
                .layer_connection_db
                .register_surface(
                    &pour.name.to_string(),
                    layer_name,
                    routing_z,
                    (pour_center_x, pour_center_y),
                    material_id,
                    ConnectionType::PourSurface,
                )
                .map_err(|e| format!("Failed to register pour '{}': {}", pour.name, e))?;

            eprintln!(
                "[PLACE_POUR] Registered pour '{}' on {} layer '{}' at routing Z={}nm",
                pour.name,
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
