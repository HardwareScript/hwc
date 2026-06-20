use crate::geometry::{BoundingBox, Point3D};
use crate::geometry_router::scene_graph::{ComponentInstance, ComponentStamp, SceneGraph};
use crate::placement::{BakedComponent, PadShape};

/// Convert a `BakedComponent` into a fully populated `ComponentStamp`.
///
/// The stamp is anchored at the origin `[0, 0, 0]` ("zero-stamping" principle).
/// All geometry — bounding boxes, pin offsets, polygons — is stored in local coordinates.
#[inline]
pub fn bake_stamp(stamp_id: usize, component: &BakedComponent) -> ComponentStamp {
    let local_bbox = BoundingBox::new(
        Point3D::new(0, 0, 0),
        Point3D::new(component.width_nm, component.height_nm, 0),
    );

    let mut local_aabb_children = Vec::new();
    let mut local_polygons = Vec::new();

    for pin in &component.pins {
        match &pin.pad_shape {
            PadShape::Circle { diameter_nm } => {
                let pad_bbox = BoundingBox::new(
                    Point3D::new(
                        pin.local_offset.x - diameter_nm / 2,
                        pin.local_offset.y - diameter_nm / 2,
                        0,
                    ),
                    Point3D::new(
                        pin.local_offset.x + diameter_nm / 2,
                        pin.local_offset.y + diameter_nm / 2,
                        0,
                    ),
                );
                local_aabb_children.push(pad_bbox);
            }
            PadShape::Rectangle {
                width_nm,
                height_nm,
            } => {
                let pad_bbox = BoundingBox::new(
                    Point3D::new(
                        pin.local_offset.x - width_nm / 2,
                        pin.local_offset.y - height_nm / 2,
                        0,
                    ),
                    Point3D::new(
                        pin.local_offset.x + width_nm / 2,
                        pin.local_offset.y + height_nm / 2,
                        0,
                    ),
                );
                local_aabb_children.push(pad_bbox);
            }
            PadShape::Obround {
                width_nm,
                height_nm,
            } => {
                let pad_bbox = BoundingBox::new(
                    Point3D::new(
                        pin.local_offset.x - width_nm / 2,
                        pin.local_offset.y - height_nm / 2,
                        0,
                    ),
                    Point3D::new(
                        pin.local_offset.x + width_nm / 2,
                        pin.local_offset.y + height_nm / 2,
                        0,
                    ),
                );
                local_aabb_children.push(pad_bbox);
            }
            PadShape::RoundedRect {
                width_nm,
                height_nm,
                ..
            } => {
                let pad_bbox = BoundingBox::new(
                    Point3D::new(
                        pin.local_offset.x - width_nm / 2,
                        pin.local_offset.y - height_nm / 2,
                        0,
                    ),
                    Point3D::new(
                        pin.local_offset.x + width_nm / 2,
                        pin.local_offset.y + height_nm / 2,
                        0,
                    ),
                );
                local_aabb_children.push(pad_bbox);
            }
            PadShape::Polygon { points } => {
                if points.is_empty() {
                    continue;
                }
                let min_x = points.iter().map(|p| p.x).min().unwrap_or(0);
                let max_x = points.iter().map(|p| p.x).max().unwrap_or(0);
                let min_y = points.iter().map(|p| p.y).min().unwrap_or(0);
                let max_y = points.iter().map(|p| p.y).max().unwrap_or(0);

                let pad_bbox = BoundingBox::new(
                    Point3D::new(
                        pin.local_offset.x + min_x,
                        pin.local_offset.y + min_y,
                        0,
                    ),
                    Point3D::new(
                        pin.local_offset.x + max_x,
                        pin.local_offset.y + max_y,
                        0,
                    ),
                );
                local_aabb_children.push(pad_bbox);

                let polygon: Vec<Point3D> = points
                    .iter()
                    .map(|p| Point3D::new(pin.local_offset.x + p.x, pin.local_offset.y + p.y, 0))
                    .collect();
                local_polygons.push(polygon);
            }
        }
    }

    let local_pin_offsets: Vec<(String, Point3D)> = component
        .pins
        .iter()
        .map(|pin| (pin.name.to_string(), pin.local_offset))
        .collect();

    ComponentStamp::new(
        stamp_id,
        component.name.to_string(),
        local_bbox,
        Vec::new(),
        local_aabb_children,
        local_polygons,
        local_pin_offsets,
    )
}

/// Convenience function for simple rectangular components.
///
/// Delegates to `ComponentStamp::rectangle()`.
#[inline]
pub fn bake_stamp_from_rect(
    stamp_id: usize,
    name: String,
    width_nm: i64,
    height_nm: i64,
) -> ComponentStamp {
    ComponentStamp::rectangle(stamp_id, name, width_nm, height_nm)
}

/// Batch-register a list of baked components into the SceneGraph.
///
/// Deduplicates by component name — if a stamp with the same name already exists,
/// it is skipped. Returns `(stamp_id, scene_graph_index)` pairs for all registered stamps.
pub fn register_baked_stamps(
    scene_graph: &mut SceneGraph,
    components: &[BakedComponent],
) -> Vec<(usize, usize)> {
    let mut results = Vec::with_capacity(components.len());
    for component in components {
        if scene_graph.get_stamp_by_name(component.name.as_str()).is_some() {
            continue;
        }
        let stamp_id = scene_graph.stamp_count();
        let stamp = bake_stamp(stamp_id, component);
        let scene_graph_index = scene_graph.register_stamp(stamp);
        results.push((stamp_id, scene_graph_index));
    }
    results
}

/// Compute the global position of a named pin on a placed component instance.
///
/// Looks up the pin's local offset from the stamp, then applies the instance's
/// `FixedTransform2D` to produce world-space coordinates.
#[inline]
pub fn stamp_pin_global_position(
    stamp: &ComponentStamp,
    pin_name: &str,
    instance: &ComponentInstance,
) -> Option<Point3D> {
    let (_name, local_offset) = stamp.local_pin_offsets.iter().find(|(n, _)| n == pin_name)?;
    let (gx, gy) = instance.transform.transform_point(local_offset.x, local_offset.y);
    Some(Point3D::new(gx, gy, local_offset.z))
}
