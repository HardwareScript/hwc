//! Soft Corridor Planner: Translates coarse partition routes into
//! Z-locked cost fields for detailed routing.
//!
//! This module generates soft routing corridors from coarse route segments
//! and provides cost evaluation for pathfinding within G-cells.

use crate::geometry::{BoundingBox, Point3D};
use crate::geometry_router::partition::GCellId;
use crate::netlist::NetId;

/// Cost levels for the soft corridor model.
pub mod cost {
    /// Base cost of routing along the corridor center line.
    pub const CENTER_LINE: i64 = 1;
    /// Minor penalty for deviating into the preferred envelope.
    pub const PREFERRED_ENVELOPE: i64 = 10;
    /// Moderate penalty for routing outside the allocated corridor.
    pub const NON_ALLOCATED: i64 = 100;
    /// Infinite cost for occupied obstacles.
    pub const INFINITE: i64 = i64::MAX / 2;
}

/// A soft routing corridor for a single net within a G-cell.
#[derive(Clone, Debug)]
pub struct SoftCorridor {
    pub net_id: NetId,
    pub cell_id: GCellId,
    /// The preferred routing envelope (slightly larger than center line).
    pub envelope: BoundingBox,
    /// The strict center line of the corridor.
    pub center_line: BoundingBox,
    /// Corridor direction: true = horizontal preferred, false = vertical preferred.
    pub prefer_horizontal: bool,
}

/// Evaluate the routing cost at a specific point within a G-cell.
///
/// Returns the cost based on the soft corridor model:
/// - Center line: cost 1
/// - Preferred envelope: cost +10
/// - Non-allocated region: cost +100
/// - Obstacle: cost INFINITE
#[inline]
pub fn corridor_cost(point: Point3D, corridor: &SoftCorridor, obstacles: &[BoundingBox]) -> i64 {
    // Check obstacles first
    for obs in obstacles {
        if obs.contains(point) {
            return cost::INFINITE;
        }
    }

    if corridor.center_line.contains(point) {
        return cost::CENTER_LINE;
    }

    if corridor.envelope.contains(point) {
        return cost::CENTER_LINE + cost::PREFERRED_ENVELOPE;
    }

    cost::CENTER_LINE + cost::NON_ALLOCATED
}

/// Generate soft corridors for all nets in a G-cell.
///
/// Takes the coarse route segments for the cell and expands them into
/// corridors. Each route segment becomes a `SoftCorridor` with a center
/// line and a preferred envelope expanded by `track_pitch_nm`.
pub fn generate_corridors(
    cell_id: GCellId,
    cell_bounds: &BoundingBox,
    net_routes: &[(NetId, Vec<Point3D>)],
    track_pitch_nm: i64,
) -> Vec<SoftCorridor> {
    let mut corridors = Vec::with_capacity(net_routes.len());

    for &(net_id, ref waypoints) in net_routes {
        if waypoints.len() < 2 {
            continue;
        }

        // For each pair of consecutive waypoints, create a corridor segment
        for window in waypoints.windows(2) {
            let start = window[0];
            let end = window[1];

            // Determine if this segment is horizontal or vertical
            let is_horizontal = start.y == end.y;

            let (min_x, max_x) = if start.x <= end.x {
                (start.x, end.x)
            } else {
                (end.x, start.x)
            };
            let (min_y, max_y) = if start.y <= end.y {
                (start.y, end.y)
            } else {
                (end.y, start.y)
            };

            let z = start.z;

            let center_line =
                BoundingBox::new(Point3D::new(min_x, min_y, z), Point3D::new(max_x, max_y, z));

            // Expand center line by track_pitch for envelope
            let envelope = BoundingBox::new(
                Point3D::new(min_x - track_pitch_nm, min_y - track_pitch_nm, z),
                Point3D::new(max_x + track_pitch_nm, max_y + track_pitch_nm, z),
            );

            // Clip envelope to cell bounds
            let envelope = clip_to_cell(&envelope, cell_bounds);

            corridors.push(SoftCorridor {
                net_id,
                cell_id,
                envelope,
                center_line,
                prefer_horizontal: is_horizontal,
            });
        }
    }

    corridors
}

/// Check if a point is inside any corridor's center line.
///
/// Returns the `NetId` of the first corridor containing the point, or `None`.
pub fn is_on_center_line(point: Point3D, corridors: &[SoftCorridor]) -> Option<NetId> {
    for corridor in corridors {
        if corridor.center_line.contains(point) {
            return Some(corridor.net_id);
        }
    }
    None
}

/// Check if a point is inside any corridor's preferred envelope.
///
/// Returns the `NetId` of the first corridor containing the point, or `None`.
pub fn is_in_envelope(point: Point3D, corridors: &[SoftCorridor]) -> Option<NetId> {
    for corridor in corridors {
        if corridor.envelope.contains(point) {
            return Some(corridor.net_id);
        }
    }
    None
}

/// Clip a bounding box to the cell bounds.
fn clip_to_cell(bbox: &BoundingBox, cell_bounds: &BoundingBox) -> BoundingBox {
    BoundingBox::new(
        Point3D::new(
            bbox.min.x.max(cell_bounds.min.x),
            bbox.min.y.max(cell_bounds.min.y),
            bbox.min.z.max(cell_bounds.min.z),
        ),
        Point3D::new(
            bbox.max.x.min(cell_bounds.max.x),
            bbox.max.y.min(cell_bounds.max.y),
            bbox.max.z.min(cell_bounds.max.z),
        ),
    )
}
