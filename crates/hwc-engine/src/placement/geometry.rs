//! Geometric transformations for component placement.

use crate::geometry::{BoundingBox, Point3D};

use super::component_definition::{ComponentDefinition, Footprint};

/// Calculate global bounding box after rotation.
///
/// COORDINATE SYSTEM: Top-Left Anchor with Center-Based Rotation
/// - `position` is the Top-Left-Front corner of the UNROTATED component
/// - After rotation around center, we calculate the axis-aligned bounding box
/// - Returns conservative AABB that contains the entire rotated component
pub(super) fn calculate_global_bounding_box(
    definition: &ComponentDefinition,
    position: Point3D,
    rotation_deg: f64,
) -> BoundingBox {
    let (width, height, depth) = match definition.footprint {
        Footprint::Rectangle {
            width_nm,
            height_nm,
            depth_nm,
        } => (width_nm, height_nm, depth_nm),
    };

    if rotation_deg.abs() < 0.001 {
        // No rotation - simple case
        return BoundingBox::new(
            Point3D::new(position.x, position.y, position.z),
            Point3D::new(position.x + width, position.y + height, position.z + depth),
        );
    }

    // Calculate component center
    let center_x = position.x + width / 2;
    let center_y = position.y + height / 2;

    // Calculate the 4 corners of the rectangle relative to center
    let half_w = width / 2;
    let half_h = height / 2;

    let corners = [
        (-half_w, -half_h), // Top-left
        (half_w, -half_h),  // Top-right
        (half_w, half_h),   // Bottom-right
        (-half_w, half_h),  // Bottom-left
    ];

    // Rotate each corner and find min/max
    let angle_rad = rotation_deg.to_radians();
    let cos_theta = angle_rad.cos();
    let sin_theta = angle_rad.sin();

    let mut min_x = i64::MAX;
    let mut max_x = i64::MIN;
    let mut min_y = i64::MAX;
    let mut max_y = i64::MIN;

    for (cx, cy) in corners.iter() {
        let rotated_x = (*cx as f64 * cos_theta - *cy as f64 * sin_theta) as i64;
        let rotated_y = (*cx as f64 * sin_theta + *cy as f64 * cos_theta) as i64;

        let global_x = center_x + rotated_x;
        let global_y = center_y + rotated_y;

        min_x = min_x.min(global_x);
        max_x = max_x.max(global_x);
        min_y = min_y.min(global_y);
        max_y = max_y.max(global_y);
    }

    // Z doesn't rotate (2D rotation only)
    BoundingBox::new(
        Point3D::new(min_x, min_y, position.z),
        Point3D::new(max_x, max_y, position.z + depth),
    )
}

/// Transform pin position from local to global coordinates with rotation.
///
/// COORDINATE SYSTEM: Top-Left Anchor with Center-Based Rotation
/// - `local_offset` is the pin's offset from component's top-left corner (unrotated)
/// - `component_position` is the component's top-left corner in global space
/// - Rotation happens around the component's CENTER, not the anchor
/// - Result is absolute pin position after rotation
///
/// Algorithm:
/// 1. Calculate component center from anchor + half dimensions
/// 2. Convert pin offset to center-relative coordinates
/// 3. Rotate around center
/// 4. Convert back to global coordinates
pub(super) fn transform_pin_position(
    local_offset: Point3D,
    component_position: Point3D,
    component_dimensions: (i64, i64, i64), // (width, height, depth)
    rotation_deg: f64,
) -> Point3D {
    if rotation_deg.abs() < 0.001 {
        // No rotation - simple addition
        return Point3D::new(
            component_position.x + local_offset.x,
            component_position.y + local_offset.y,
            component_position.z + local_offset.z,
        );
    }

    let (width, height, _depth) = component_dimensions;

    // Step 1: Calculate component center
    let center_x = component_position.x + width / 2;
    let center_y = component_position.y + height / 2;

    // Step 2: Convert pin offset to center-relative
    // Pin offset is from top-left, so: center_relative = offset - (width/2, height/2)
    let pin_from_center_x = local_offset.x - width / 2;
    let pin_from_center_y = local_offset.y - height / 2;

    // Step 3: Rotate around center (Z-axis rotation in XY plane)
    let angle_rad = rotation_deg.to_radians();
    let cos_theta = angle_rad.cos();
    let sin_theta = angle_rad.sin();

    let rotated_x =
        (pin_from_center_x as f64 * cos_theta - pin_from_center_y as f64 * sin_theta) as i64;
    let rotated_y =
        (pin_from_center_x as f64 * sin_theta + pin_from_center_y as f64 * cos_theta) as i64;

    // Step 4: Convert back to global coordinates
    Point3D::new(
        center_x + rotated_x,
        center_y + rotated_y,
        component_position.z + local_offset.z, // Z doesn't rotate (2D rotation only)
    )
}
