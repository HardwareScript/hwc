use hwc_engine::{geometry::BoundingBox, space::{ContactMetadata, KeepOutZone}};

use super::AutoViaInserter;

impl AutoViaInserter {
    pub(crate) fn is_colliding(
        &self,
        x_nm: i64,
        y_nm: i64,
        from_z_nm: i64,
        to_z_nm: i64,
        diameter_nm: i64,
        contacts: &[ContactMetadata],
        pending_vias: &[ContactMetadata],
        keep_out_zones: &[KeepOutZone],
        current_net: Option<&str>,
    ) -> bool {
        // v0.1.7: Layer 2 - Check Keep-Out Zones (KOZs)
        for koz in keep_out_zones {
            if !koz.allow_vias {
                // If it's a via-forbidden zone, check if this via falls inside it
                // We project the via as a point for simplicity, or we could use its bbox.
                if x_nm >= koz.bbox.min.x && x_nm <= koz.bbox.max.x &&
                   y_nm >= koz.bbox.min.y && y_nm <= koz.bbox.max.y &&
                   from_z_nm < koz.bbox.max.z && to_z_nm > koz.bbox.min.z {
                    
                    // Net exemption: if the KOZ exempts this net, it's allowed.
                    if let Some(net_name) = current_net {
                        if koz.exempted_nets.iter().any(|n| n == net_name) {
                            continue; // This net is allowed to have vias here (likely its own pins)
                        }
                    }
                    
                    return true;
                }
            }
        }

        for contact in contacts {
            if let Some(ref bbox) = contact.bbox {
                let other_net = contact.net.as_ref().map(|net| net.as_ref());
                if self.check_single_collision(
                    x_nm,
                    y_nm,
                    from_z_nm,
                    to_z_nm,
                    diameter_nm,
                    bbox,
                    contact.z_start_nm,
                    contact.z_end_nm,
                    current_net,
                    other_net,
                ) {
                    return true;
                }
            }
        }

        for via in pending_vias {
            if let Some(ref bbox) = via.bbox {
                let other_net = via.net.as_ref().map(|net| net.as_ref());
                if self.check_single_collision(
                    x_nm,
                    y_nm,
                    from_z_nm,
                    to_z_nm,
                    diameter_nm,
                    bbox,
                    via.z_start_nm,
                    via.z_end_nm,
                    current_net,
                    other_net,
                ) {
                    return true;
                }
            }
        }

        false
    }

    fn check_single_collision(
        &self,
        x_nm: i64,
        y_nm: i64,
        from_z_nm: i64,
        to_z_nm: i64,
        diameter_nm: i64,
        other_bbox: &BoundingBox,
        other_z_start: i64,
        other_z_end: i64,
        current_net: Option<&str>,
        other_net: Option<&str>,
    ) -> bool {
        let z_overlap = other_z_start < to_z_nm && from_z_nm < other_z_end;

        // 1. Calculate center coordinates
        let other_center_x = (other_bbox.min.x + other_bbox.max.x) / 2;
        let other_center_y = (other_bbox.min.y + other_bbox.max.y) / 2;

        // 2. Enforce Same-Net Deduplication
        if current_net.is_some() && current_net == other_net {
            if !z_overlap {
                return false; // Vertical stacking is permitted
            }

            // Exact coordinate match on the same net (duplicate check)
            if x_nm == other_center_x && y_nm == other_center_y {
                return true;
            }
        }

        if !z_overlap {
            return false;
        }

        // 3. Exact Profile Parameter Lookup
        // Instead of deriving diameter from the bounding box (which can introduce rounding errors),
        // we query the exact diameter defined in the active profile's via library.
        let other_diameter_nm = self.via_library
            .find_via_by_z_span(other_z_start, other_z_end)
            .map(|via| (via.diameter_mm * 1_000_000.0) as i64)
            .unwrap_or_else(|| {
                // Fallback to bounding box subtraction only if no via definition matches
                (other_bbox.max.x - other_bbox.min.x).min(other_bbox.max.y - other_bbox.min.y)
            });

        // 4. Pure Integer Clearance Comparison
        // Calculate distance between grid-snapped coordinate centers
        let dx = other_center_x - x_nm;
        let dy = other_center_y - y_nm;
        let center_dist_nm = ((dx * dx + dy * dy) as f64).sqrt() as i64;

        let other_radius = other_diameter_nm / 2;
        let this_radius = diameter_nm / 2;
        let drill_clearance_nm = center_dist_nm - other_radius - this_radius;

        // Direct, exact comparison. No magic adjustment values are needed
        // because both coordinates are locked to the integer voxel grid.
        drill_clearance_nm < self.min_spacing_nm
    }
}
