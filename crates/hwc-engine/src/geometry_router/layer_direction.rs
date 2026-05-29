//! Layer Direction Management for Manhattan Routing
//!
//! This module handles layer direction rules for Manhattan routing,
//! which restricts movement on each layer to prevent self-blocking.

use crate::constraint_manager::LayerDirection;
use crate::geometry::Point3D;

/// Check if a move is valid according to layer direction rules.
///
/// Manhattan routing restricts movement on each layer to prevent self-blocking:
/// - NorthSouth layers: Only Y-axis movement (North/South)
/// - EastWest layers: Only X-axis movement (East/West)
/// - Any layers: All directions allowed (power/ground planes)
/// - Vias (Z-axis): Always allowed if X,Y unchanged
///
/// **Algorithm**:
/// 1. If Z changes (via), X and Y must be unchanged
/// 2. If same layer, check layer direction rules
/// 3. NorthSouth: only Y can change
/// 4. EastWest: only X can change
/// 5. Any: all movements allowed
///
/// **Documentation Reference**:
/// - `Docs/v0.1.3/ROUTING-AND-PHYSICS.md` (lines 500-600, Manhattan Routing)
///
/// # Arguments
/// * `from` - Starting point
/// * `to` - Destination point
/// * `layer_direction` - Direction restriction for this layer
///
/// # Returns
/// True if the move is valid, false otherwise
///
/// # Examples
/// ```
/// use hwc_engine::geometry_router::is_valid_move;
/// use hwc_engine::{Point3D, constraint_manager::LayerDirection};
///
/// // NorthSouth layer allows Y movement
/// let from = Point3D::new(0, 0, 0);  // (x, y, z)
/// let to = Point3D::new(0, 1_000_000, 0);  // Move north (Y increases)
/// assert!(is_valid_move(from, to, LayerDirection::NorthSouth));
///
/// // NorthSouth layer blocks X movement
/// let to = Point3D::new(1_000_000, 0, 0);  // Move east (X increases)
/// assert!(!is_valid_move(from, to, LayerDirection::NorthSouth));
///
/// // Via (Z change) always allowed
/// let to = Point3D::new(0, 0, 1_000_000);  // Move up (Z increases)
/// assert!(is_valid_move(from, to, LayerDirection::NorthSouth));
/// ```
pub fn is_valid_move(from: Point3D, to: Point3D, layer_direction: LayerDirection) -> bool {
    let dz = to.z - from.z;
    let dx = to.x - from.x;
    let dy = to.y - from.y;

    // Via (Z-axis change): Always allowed if X,Y unchanged
    if dz != 0 {
        return dx == 0 && dy == 0;
    }

    // Same layer: Check layer direction rules
    match layer_direction {
        LayerDirection::NorthSouth => {
            // Only Y-axis movement allowed
            dx == 0 && dy != 0
        }
        LayerDirection::EastWest => {
            // Only X-axis movement allowed
            dx != 0 && dy == 0
        }
        LayerDirection::Any => {
            // All movements allowed
            true
        }
    }
}

/// Assign layer directions for Manhattan routing.
///
/// Alternates between NorthSouth and EastWest for each layer to prevent
/// self-blocking during routing. This is a standard PCB routing technique.
///
/// **Pattern**:
/// - Layer 0: NorthSouth (Y-axis)
/// - Layer 1: EastWest (X-axis)
/// - Layer 2: NorthSouth (Y-axis)
/// - Layer 3: EastWest (X-axis)
/// - ...
///
/// **Documentation Reference**:
/// - `Docs/v0.1.3/ROUTING-AND-PHYSICS.md` (lines 500-600, Manhattan Routing)
///
/// # Arguments
/// * `num_layers` - Total number of routing layers
///
/// # Returns
/// Vector of layer directions (index = layer number)
///
/// # Examples
/// ```
/// use hwc_engine::geometry_router::assign_layer_directions;
/// use hwc_engine::constraint_manager::LayerDirection;
///
/// let directions = assign_layer_directions(4);
/// assert_eq!(directions[0], LayerDirection::NorthSouth);
/// assert_eq!(directions[1], LayerDirection::EastWest);
/// assert_eq!(directions[2], LayerDirection::NorthSouth);
/// assert_eq!(directions[3], LayerDirection::EastWest);
/// ```
pub fn assign_layer_directions(num_layers: usize) -> Vec<LayerDirection> {
    (0..num_layers)
        .map(|layer| {
            if layer % 2 == 0 {
                LayerDirection::NorthSouth
            } else {
                LayerDirection::EastWest
            }
        })
        .collect()
}
