//! Coordinate-Locked Port Escapes & Dynamic Edge-Offset Constraints
//!
//! This module implements the Via-and-Pad-Port-Escape specification for v0.1.7.
//! It provides:
//! - Cardinal port mapping (N, S, E, W) to bounding box edges
//! - Interpolated edge-offset heuristic (percentage/measurement positioning)
//! - Smart Corner Clamping to prevent trace overhang
//! - Radial Projection for circular pads/vias
//!
//! Reference: `Docs/v0.1.7/VIA-AND-PAD-PORT-ESCAPE-SPECIFICATION.md`

use crate::geometry::{BoundingBox, Point3D};

/// Cardinal port directions for routing escapes.
///
/// Maps to the four edges of a bounding box:
/// ```text
///                       North (N) / Top [0, 1]
///                                  ▲
///                                  │
///    West (W) / Left [-1, 0] ◄─────┼─────► East (E) / Right [1, 0]
///                                  │
///                                  ▼
///                      South (S) / Bottom [0, -1]
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CardinalPort {
    North,
    South,
    East,
    West,
}

impl CardinalPort {
    /// Get the direction vector for this port.
    pub fn direction_vector(&self) -> (i64, i64) {
        match self {
            CardinalPort::North => (0, 1),
            CardinalPort::South => (0, -1),
            CardinalPort::East => (1, 0),
            CardinalPort::West => (-1, 0),
        }
    }
}

/// Edge offset specification for fine-grained positioning along a pad edge.
///
/// Supports three modes:
/// - `Center`: Snap to the exact center of the edge (default)
/// - `Percentage`: Position as a ratio from 0.0 to 1.0 along the edge
/// - `Measurement`: Absolute offset from the edge center in nanometers
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EdgeOffset {
    /// Center of the edge (50% position) - default when no modifier specified
    Center,
    /// Position as a normalized ratio (0.0 = min, 1.0 = max)
    Percentage(f64),
    /// Absolute offset from the edge center in nanometers (positive = toward max)
    Measurement(i64),
    /// Named position: "top" = 100%, "bottom" = 0%, "center" = 50%
    Named(NamedPosition),
}

/// Named positions for quick edge snapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamedPosition {
    Top,
    Bottom,
    Center,
}

/// Resolved escape point with direction information.
#[derive(Debug, Clone, Copy)]
pub struct EscapePoint {
    /// The exact coordinate where the trace should start/end
    pub point: Point3D,
    /// The direction the trace should move away from the pad
    pub direction: (i64, i64),
    /// The port that was used to calculate this escape
    pub port: CardinalPort,
}

/// Smart Corner Clamping - prevents traces from extending past pad boundaries.
///
/// Given a trace width and edge length, returns the safe min/max ratio range.
///
/// # Formula
/// ```text
/// Min Ratio = (W_trace / 2) / L_edge
/// Max Ratio = 1.0 - (W_trace / 2) / L_edge
/// ```
pub fn smart_corner_clamp(trace_width_nm: i64, edge_length_nm: i64) -> (f64, f64) {
    if edge_length_nm <= 0 {
        return (0.0, 1.0);
    }

    let half_trace = trace_width_nm as f64 / 2.0;
    let edge = edge_length_nm as f64;

    let min_ratio = (half_trace / edge).max(0.0);
    let max_ratio = (1.0 - half_trace / edge).min(1.0);

    (min_ratio, max_ratio)
}

/// Clamp an edge ratio to safe bounds using Smart Corner Clamping.
fn clamp_ratio(ratio: f64, trace_width_nm: i64, edge_length_nm: i64) -> f64 {
    let (min_ratio, max_ratio) = smart_corner_clamp(trace_width_nm, edge_length_nm);
    ratio.max(min_ratio).min(max_ratio)
}

/// Resolve an edge offset to a normalized ratio (0.0 to 1.0).
fn resolve_offset_to_ratio(offset: EdgeOffset) -> f64 {
    match offset {
        EdgeOffset::Center => 0.5,
        EdgeOffset::Percentage(r) => r.max(0.0).min(1.0),
        EdgeOffset::Measurement(_offset_nm) => {
            // Measurement is relative to center, normalized by edge length
            // This is handled at the coordinate level, not ratio level
            0.5 // Placeholder - actual offset applied in coordinate calculation
        }
        EdgeOffset::Named(pos) => match pos {
            NamedPosition::Top => 1.0,
            NamedPosition::Bottom => 0.0,
            NamedPosition::Center => 0.5,
        },
    }
}

/// Calculate the escape point for a rectangular pad given a port and offset.
///
/// # Arguments
/// * `bbox` - Bounding box of the pad/via (in nanometers)
/// * `port` - Cardinal direction (N, S, E, W)
/// * `offset` - Edge offset specification
/// * `trace_width_nm` - Width of the trace for corner clamping
/// * `clearance_nm` - Clearance distance from pad edge
/// * `z` - Z coordinate (layer) for the escape point
///
/// # Returns
/// An `EscapePoint` with the exact coordinate and exit direction.
pub fn calculate_rect_escape(
    bbox: &BoundingBox,
    port: CardinalPort,
    offset: EdgeOffset,
    trace_width_nm: i64,
    clearance_nm: i64,
    z: i64,
) -> EscapePoint {
    let direction = port.direction_vector();

    // Get edge bounds based on port
    let (edge_min, edge_max, edge_length) = match port {
        CardinalPort::North | CardinalPort::South => {
            // Horizontal edge: X range
            let min = bbox.min.x;
            let max = bbox.max.x;
            (min, max, max - min)
        }
        CardinalPort::East | CardinalPort::West => {
            // Vertical edge: Y range
            let min = bbox.min.y;
            let max = bbox.max.y;
            (min, max, max - min)
        }
    };

    // Calculate the base ratio from offset
    let mut ratio = resolve_offset_to_ratio(offset);

    // Apply measurement offset if specified
    if let EdgeOffset::Measurement(offset_nm) = offset {
        let center = (edge_min + edge_max) / 2;
        let clamped_offset = offset_nm.max(-(edge_length / 2)).min(edge_length / 2);
        let coordinate = center + clamped_offset;
        return EscapePoint {
            point: match port {
                CardinalPort::North => Point3D::new(coordinate, bbox.max.y + clearance_nm, z),
                CardinalPort::South => Point3D::new(coordinate, bbox.min.y - clearance_nm, z),
                CardinalPort::East => Point3D::new(bbox.max.x + clearance_nm, coordinate, z),
                CardinalPort::West => Point3D::new(bbox.min.x - clearance_nm, coordinate, z),
            },
            direction,
            port,
        };
    }

    // Apply Smart Corner Clamping
    ratio = clamp_ratio(ratio, trace_width_nm, edge_length);

    // Calculate the coordinate along the edge
    let coordinate = edge_min + ((edge_max - edge_min) as f64 * ratio) as i64;

    // Calculate the escape point with clearance
    let point = match port {
        CardinalPort::North => Point3D::new(coordinate, bbox.max.y + clearance_nm, z),
        CardinalPort::South => Point3D::new(coordinate, bbox.min.y - clearance_nm, z),
        CardinalPort::East => Point3D::new(bbox.max.x + clearance_nm, coordinate, z),
        CardinalPort::West => Point3D::new(bbox.min.x - clearance_nm, coordinate, z),
    };

    EscapePoint {
        point,
        direction,
        port,
    }
}

/// Calculate the escape point for a circular pad/via using Radial Projection.
///
/// # Arguments
/// * `center` - Center of the circle (in nanometers)
/// * `radius_nm` - Radius of the pad
/// * `port` - Cardinal direction (N, S, E, W)
/// * `offset` - Edge offset specification
/// * `trace_width_nm` - Width of the trace for corner clamping
/// * `clearance_nm` - Clearance distance from pad edge
/// * `z` - Z coordinate (layer) for the escape point
///
/// # Returns
/// An `EscapePoint` with the exact coordinate and exit direction.
///
/// # Algorithm
/// 1. Project a virtual bounding box of size 2R × 2R around the circle
/// 2. Calculate the box coordinate using the standard 1D interpolation
/// 3. Project the box coordinate onto the circle perimeter
/// 4. The escape point is offset outward along the radial direction
pub fn calculate_circular_escape(
    center: (i64, i64),
    radius_nm: i64,
    port: CardinalPort,
    offset: EdgeOffset,
    trace_width_nm: i64,
    clearance_nm: i64,
    z: i64,
) -> EscapePoint {
    let (cx, cy) = center;

    // Step 1: Create virtual bounding box around circle
    let virtual_bbox = BoundingBox::new(
        Point3D::new(cx - radius_nm, cy - radius_nm, z),
        Point3D::new(cx + radius_nm, cy + radius_nm, z),
    );

    // Step 2: Get the edge bounds for the virtual box
    let (edge_min, edge_max, edge_length) = match port {
        CardinalPort::North | CardinalPort::South => {
            let min = virtual_bbox.min.x;
            let max = virtual_bbox.max.x;
            (min, max, max - min)
        }
        CardinalPort::East | CardinalPort::West => {
            let min = virtual_bbox.min.y;
            let max = virtual_bbox.max.y;
            (min, max, max - min)
        }
    };

    // Step 3: Calculate ratio with clamping
    let mut ratio = resolve_offset_to_ratio(offset);

    if let EdgeOffset::Measurement(offset_nm) = offset {
        let center_coord = (edge_min + edge_max) / 2;
        let clamped_offset = offset_nm.max(-(edge_length / 2)).min(edge_length / 2);
        let coordinate = center_coord + clamped_offset;

        // Project onto circle
        let (bx, by) = match port {
            CardinalPort::North | CardinalPort::South => (coordinate, cy),
            CardinalPort::East | CardinalPort::West => (cx, coordinate),
        };

        // Calculate direction from center to box point
        let dx = bx - cx;
        let dy = by - cy;
        let dist = ((dx * dx + dy * dy) as f64).sqrt();

        if dist < 1e-10 {
            // Degenerate case: point at center, use port direction
            let dir = port.direction_vector();
            let px = cx + dir.0 * radius_nm;
            let py = cy + dir.1 * radius_nm;
            return EscapePoint {
                point: Point3D::new(px + dir.0 * clearance_nm, py + dir.1 * clearance_nm, z),
                direction: dir,
                port,
            };
        }

        // Unit vector from center to box point
        let ux = dx as f64 / dist;
        let uy = dy as f64 / dist;

        // Project onto circle perimeter
        let px = cx + (radius_nm as f64 * ux) as i64;
        let py = cy + (radius_nm as f64 * uy) as i64;

        // Escape point with clearance
        let ex = px + (clearance_nm as f64 * ux) as i64;
        let ey = py + (clearance_nm as f64 * uy) as i64;

        return EscapePoint {
            point: Point3D::new(ex, ey, z),
            direction: (ux as i64, uy as i64),
            port,
        };
    }

    ratio = clamp_ratio(ratio, trace_width_nm, edge_length);

    // Calculate the coordinate along the virtual edge
    let coordinate = edge_min + ((edge_max - edge_min) as f64 * ratio) as i64;

    // Step 4: Get the box coordinate
    let (bx, by) = match port {
        CardinalPort::North | CardinalPort::South => (coordinate, cy),
        CardinalPort::East | CardinalPort::West => (cx, coordinate),
    };

    // Step 5: Calculate direction from center to box point
    let dx = bx - cx;
    let dy = by - cy;
    let dist = ((dx * dx + dy * dy) as f64).sqrt();

    if dist < 1e-10 {
        // Degenerate case: point at center, use port direction
        let dir = port.direction_vector();
        let px = cx + dir.0 * radius_nm;
        let py = cy + dir.1 * radius_nm;
        return EscapePoint {
            point: Point3D::new(px + dir.0 * clearance_nm, py + dir.1 * clearance_nm, z),
            direction: dir,
            port,
        };
    }

    // Unit vector from center to box point
    let ux = dx as f64 / dist;
    let uy = dy as f64 / dist;

    // Step 6: Project onto circle perimeter
    let px = cx + (radius_nm as f64 * ux) as i64;
    let py = cy + (radius_nm as f64 * uy) as i64;

    // Step 7: Escape point with clearance
    let ex = px + (clearance_nm as f64 * ux) as i64;
    let ey = py + (clearance_nm as f64 * uy) as i64;

    EscapePoint {
        point: Point3D::new(ex, ey, z),
        direction: (ux as i64, uy as i64),
        port,
    }
}

/// Parse a port escape specification string into a CardinalPort and EdgeOffset.
///
/// Supported formats:
/// - "East" -> CardinalPort::East, EdgeOffset::Center
/// - "East at center" -> CardinalPort::East, EdgeOffset::Center
/// - "East at top" -> CardinalPort::East, EdgeOffset::Named(Top)
/// - "East at bottom" -> CardinalPort::East, EdgeOffset::Named(Bottom)
/// - "East at 80%" -> CardinalPort::East, EdgeOffset::Percentage(0.8)
/// - "East at +150um" -> CardinalPort::East, EdgeOffset::Measurement(150000)
/// - "East at -50um" -> CardinalPort::East, EdgeOffset::Measurement(-50000)
pub fn parse_port_escape(spec: &str) -> Option<(CardinalPort, EdgeOffset)> {
    let spec = spec.trim();
    let parts: Vec<&str> = spec.splitn(2, " at ").collect();

    let port = match parts[0].to_lowercase().as_str() {
        "north" | "n" | "top" => CardinalPort::North,
        "south" | "s" | "bottom" => CardinalPort::South,
        "east" | "e" | "right" => CardinalPort::East,
        "west" | "w" | "left" => CardinalPort::West,
        _ => return None,
    };

    let offset = if parts.len() == 1 {
        EdgeOffset::Center
    } else {
        let offset_str = parts[1].trim();
        parse_edge_offset(offset_str)?
    };

    Some((port, offset))
}

/// Parse an edge offset string into an EdgeOffset.
fn parse_edge_offset(s: &str) -> Option<EdgeOffset> {
    let s = s.trim().to_lowercase();

    // Named positions
    match s.as_str() {
        "center" | "centre" | "mid" | "middle" => return Some(EdgeOffset::Center),
        "top" | "max" | "high" | "upper" => return Some(EdgeOffset::Named(NamedPosition::Top)),
        "bottom" | "min" | "low" | "lower" => {
            return Some(EdgeOffset::Named(NamedPosition::Bottom));
        }
        _ => {}
    }

    // Percentage (e.g., "80%")
    if let Some(pct) = s.strip_suffix('%') {
        if let Ok(val) = pct.trim().parse::<f64>() {
            return Some(EdgeOffset::Percentage(val / 100.0));
        }
    }

    // Measurement (e.g., "+150um", "-50um", "150um")
    if let Some(val) = s.strip_suffix("um") {
        if let Ok(val) = val.trim().parse::<f64>() {
            return Some(EdgeOffset::Measurement((val * 1000.0) as i64));
        }
    }

    // Plain number as percentage (e.g., "0.8" = 80%)
    if let Ok(val) = s.parse::<f64>() {
        if (0.0..=1.0).contains(&val) {
            return Some(EdgeOffset::Percentage(val));
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_smart_corner_clamp() {
        // 100um trace on 600um edge
        let (min, max) = smart_corner_clamp(100_000, 600_000);
        assert!((min - 0.0833).abs() < 0.001);
        assert!((max - 0.9167).abs() < 0.001);
    }

    #[test]
    fn test_rect_escape_center() {
        let bbox = BoundingBox::new(
            Point3D::new(0, 0, 0),
            Point3D::new(600_000, 600_000, 35_000),
        );
        let escape = calculate_rect_escape(
            &bbox,
            CardinalPort::East,
            EdgeOffset::Center,
            100_000,
            50_000,
            35_000,
        );
        // Center of East edge (600um) + clearance (50um)
        assert_eq!(escape.point.x, 650_000);
        assert_eq!(escape.point.y, 300_000);
    }

    #[test]
    fn test_circular_escape_east() {
        let center = (300_000, 300_000);
        let escape = calculate_circular_escape(
            center,
            300_000,
            CardinalPort::East,
            EdgeOffset::Center,
            100_000,
            50_000,
            35_000,
        );
        // East of circle center + radius + clearance
        assert_eq!(escape.point.x, 650_000);
        assert_eq!(escape.point.y, 300_000);
    }

    #[test]
    fn test_parse_port_escape() {
        assert_eq!(
            parse_port_escape("East"),
            Some((CardinalPort::East, EdgeOffset::Center))
        );
        assert_eq!(
            parse_port_escape("North at top"),
            Some((CardinalPort::North, EdgeOffset::Named(NamedPosition::Top)))
        );
        assert_eq!(
            parse_port_escape("West at 80%"),
            Some((CardinalPort::West, EdgeOffset::Percentage(0.8)))
        );
        assert_eq!(
            parse_port_escape("South at -50um"),
            Some((CardinalPort::South, EdgeOffset::Measurement(-50_000)))
        );
    }
}
