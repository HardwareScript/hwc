use hwc_engine::{
    geometry::BoundingBox,
    space::{ContactMetadata, KeepOutZone},
};

use super::{AutoViaInserter, ViaLocation};

impl AutoViaInserter {
    pub(crate) fn is_colliding(
        &self,
        location: &ViaLocation,
        contacts: &[ContactMetadata],
        pending_vias: &[ContactMetadata],
        keep_out_zones: &[KeepOutZone],
        current_net: Option<&str>,
    ) -> bool {
        // v0.1.7: Layer 2 - Check Keep-Out Zones (KOZs)
        for koz in keep_out_zones {
            if !koz.allow_vias
                && location.x_nm >= koz.bbox.min.x
                && location.x_nm <= koz.bbox.max.x
                && location.y_nm >= koz.bbox.min.y
                && location.y_nm <= koz.bbox.max.y
                && location.from_z_nm < koz.bbox.max.z
                && location.to_z_nm > koz.bbox.min.z
            {
                if let Some(net_name) = current_net {
                    if koz.exempted_nets.iter().any(|n| n == net_name) {
                        continue;
                    }
                }

                return true;
            }
        }

        for contact in contacts {
            if let Some(ref bbox) = contact.bbox {
                let other_net = contact.net.as_ref().map(|net| net.as_ref());
                if self.check_single_collision(
                    location,
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
                    location,
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
        location: &ViaLocation,
        other_bbox: &BoundingBox,
        other_z_start: i64,
        other_z_end: i64,
        current_net: Option<&str>,
        other_net: Option<&str>,
    ) -> bool {
        let z_overlap = other_z_start < location.to_z_nm && location.from_z_nm < other_z_end;

        let other_center_x = (other_bbox.min.x + other_bbox.max.x) / 2;
        let other_center_y = (other_bbox.min.y + other_bbox.max.y) / 2;

        if current_net.is_some() && current_net == other_net {
            if !z_overlap {
                return false;
            }

            if location.x_nm == other_center_x && location.y_nm == other_center_y {
                let other_is_via = self
                    .via_library
                    .find_via_by_z_span(other_z_start, other_z_end)
                    .is_some();
                return other_is_via;
            }

            return false;
        }

        if !z_overlap {
            return false;
        }

        let other_diameter_nm = self
            .via_library
            .find_via_by_z_span(other_z_start, other_z_end)
            .map(|via| (via.diameter_mm * 1_000_000.0) as i64)
            .unwrap_or_else(|| {
                (other_bbox.max.x - other_bbox.min.x).min(other_bbox.max.y - other_bbox.min.y)
            });

        let dx = other_center_x - location.x_nm;
        let dy = other_center_y - location.y_nm;
        let center_dist_nm = ((dx * dx + dy * dy) as f64).sqrt() as i64;

        let other_radius = other_diameter_nm / 2;
        let this_radius = location.diameter_nm / 2;
        let drill_clearance_nm = center_dist_nm - other_radius - this_radius;

        drill_clearance_nm < self.min_spacing_nm
    }
}
