//! Layer direction assignment for Manhattan routing.
//!
//! This module handles the assignment of routing directions to layers
//! to prevent self-blocking during Manhattan routing.

use crate::constraint_manager::types::LayerDirection;
use rustc_hash::FxHashMap;

/// Assign layer directions for Manhattan routing.
///
/// Alternates between North/South and East/West for each layer
/// to prevent self-blocking during routing.
///
/// # Arguments
/// * `num_layers` - Total number of routing layers
///
/// # Returns
/// Map of layer index to routing direction
///
/// # Examples
/// ```
/// use hwc_engine::geometry_router::assign_layer_directions;
///
/// let directions = assign_layer_directions(4);
///
/// // Layer 0: NorthSouth, Layer 1: EastWest, Layer 2: NorthSouth, Layer 3: EastWest
/// assert_eq!(directions.len(), 4);
/// ```
pub fn assign_layer_directions(num_layers: usize) -> FxHashMap<usize, LayerDirection> {
    let mut directions = FxHashMap::default();

    for layer in 0..num_layers {
        // Alternate between NorthSouth and EastWest
        let direction = if layer % 2 == 0 {
            LayerDirection::NorthSouth
        } else {
            LayerDirection::EastWest
        };

        directions.insert(layer, direction);
    }

    directions
}
