use super::shapes::{point_in_polygon, SubstrateLayerShape};
use super::substrate_layer::SubstrateLayer;
use super::CapType;

pub(super) fn contains_nm(layer: &SubstrateLayer, x: i64, y: i64, z: i64) -> bool {
    if !layer.regions.is_empty() {
        let in_any_region = layer.regions.iter().any(|r| {
            x >= r.min.x
                && x <= r.max.x
                && y >= r.min.y
                && y <= r.max.y
                && z >= r.min.z
                && z <= r.max.z
        });
        if !in_any_region {
            return false;
        }
    } else if !(x >= layer.bbox.min.x
        && x <= layer.bbox.max.x
        && y >= layer.bbox.min.y
        && y <= layer.bbox.max.y
        && z >= layer.bbox.min.z
        && z <= layer.bbox.max.z)
    {
        return false;
    }

    match &layer.shape {
        SubstrateLayerShape::Polygon {
            outer_contour,
            holes,
            ..
        } => {
            let center_x = (layer.bbox.min.x + layer.bbox.max.x) / 2;
            let center_y = (layer.bbox.min.y + layer.bbox.max.y) / 2;
            let px = x - center_x;
            let py = y - center_y;

            if !point_in_polygon(px, py, outer_contour) {
                return false;
            }

            for hole in holes.iter() {
                if point_in_polygon(px, py, hole) {
                    return false;
                }
            }
        }
        SubstrateLayerShape::Tube {
            outer_diameter,
            inner_diameter,
            pad_diameter,
            top_cap,
            bottom_cap,
            bottom_outer_diameter,
            ..
        } => {
            let center_x = (layer.bbox.min.x + layer.bbox.max.x) / 2;
            let center_y = (layer.bbox.min.y + layer.bbox.max.y) / 2;
            let dx = x - center_x;
            let dy = y - center_y;
            let dist_sq = dx * dx + dy * dy;

            let top_outer_radius = *outer_diameter as i64 / 2;
            let top_inner_radius = *inner_diameter as i64 / 2;
            let pad_radius = *pad_diameter as i64 / 2;

            let bottom_outer_radius =
                (*bottom_outer_diameter).unwrap_or(*outer_diameter) as i64 / 2;
            let plating_thickness = top_outer_radius - top_inner_radius;

            let height_nm = layer.bbox.max.z - layer.bbox.min.z;
            let t = if height_nm > 0 {
                (z - layer.bbox.min.z) as f64 / height_nm as f64
            } else {
                1.0
            };

            let current_outer_radius =
                (1.0 - t) * bottom_outer_radius as f64 + t * top_outer_radius as f64;
            let current_inner_radius = current_outer_radius - plating_thickness as f64;

            let cap_thickness = 35_000;
            let is_in_top_cap = z >= layer.bbox.max.z - cap_thickness;
            let is_in_bottom_cap = z <= layer.bbox.min.z + cap_thickness;

            if is_in_top_cap {
                match top_cap {
                    CapType::None => {
                        if dist_sq > (current_outer_radius * current_outer_radius) as i64
                            || dist_sq < (current_inner_radius * current_inner_radius) as i64
                        {
                            return false;
                        }
                    }
                    CapType::Annular => {
                        if dist_sq > pad_radius * pad_radius
                            || dist_sq < (current_inner_radius * current_inner_radius) as i64
                        {
                            return false;
                        }
                    }
                    CapType::Solid => {
                        if dist_sq > pad_radius * pad_radius {
                            return false;
                        }
                    }
                }
            } else if is_in_bottom_cap {
                match bottom_cap {
                    CapType::None => {
                        if dist_sq > (current_outer_radius * current_outer_radius) as i64
                            || dist_sq < (current_inner_radius * current_inner_radius) as i64
                        {
                            return false;
                        }
                    }
                    CapType::Annular => {
                        if dist_sq > pad_radius * pad_radius
                            || dist_sq < (current_inner_radius * current_inner_radius) as i64
                        {
                            return false;
                        }
                    }
                    CapType::Solid => {
                        if dist_sq > pad_radius * pad_radius {
                            return false;
                        }
                    }
                }
            } else if dist_sq > (current_outer_radius * current_outer_radius) as i64
                || dist_sq < (current_inner_radius * current_inner_radius) as i64
            {
                return false;
            }
        }
        SubstrateLayerShape::Rect => {}
        SubstrateLayerShape::Circle { radius } => {
            let center_x = (layer.bbox.min.x + layer.bbox.max.x) / 2;
            let center_y = (layer.bbox.min.y + layer.bbox.max.y) / 2;
            let dx = x - center_x;
            let dy = y - center_y;
            if dx * dx + dy * dy > radius * radius {
                return false;
            }
        }
    }

    for cutout in &layer.cutouts {
        let bbox = &cutout.bbox;
        if x >= bbox.min.x
            && x <= bbox.max.x
            && y >= bbox.min.y
            && y <= bbox.max.y
            && z >= bbox.min.z
            && z <= bbox.max.z
        {
            match &cutout.shape {
                SubstrateLayerShape::Polygon { outer_contour, .. } => {
                    let center_x = (bbox.min.x + bbox.max.x) / 2;
                    let center_y = (bbox.min.y + bbox.max.y) / 2;
                    let px = x - center_x;
                    let py = y - center_y;

                    let mut min_x = i64::MAX;
                    let mut max_x = i64::MIN;
                    let mut min_y = i64::MAX;
                    let mut max_y = i64::MIN;
                    for p in outer_contour.iter() {
                        if p.x < min_x {
                            min_x = p.x;
                        }
                        if p.x > max_x {
                            max_x = p.x;
                        }
                        if p.y < min_y {
                            min_y = p.y;
                        }
                        if p.y > max_y {
                            max_y = p.y;
                        }
                    }

                    if px >= min_x && px <= max_x && py >= min_y && py <= max_y {
                        return false;
                    }
                }
                SubstrateLayerShape::Tube {
                    outer_diameter,
                    inner_diameter,
                    ..
                } => {
                    let center_x = (bbox.min.x + bbox.max.x) / 2;
                    let center_y = (bbox.min.y + bbox.max.y) / 2;
                    let dx = x - center_x;
                    let dy = y - center_y;
                    let outer_radius = *outer_diameter as i64 / 2;
                    let inner_radius = *inner_diameter as i64 / 2;
                    let dist_sq = dx * dx + dy * dy;
                    if dist_sq <= outer_radius * outer_radius
                        && dist_sq >= inner_radius * inner_radius
                    {
                        return false;
                    }
                }
                SubstrateLayerShape::Rect => {
                    return false;
                }
                SubstrateLayerShape::Circle { radius } => {
                    let center_x = (bbox.min.x + bbox.max.x) / 2;
                    let center_y = (bbox.min.y + bbox.max.y) / 2;
                    let dx = x - center_x;
                    let dy = y - center_y;
                    if dx * dx + dy * dy <= radius * radius {
                        return false;
                    }
                }
            }
        }
    }

    true
}
