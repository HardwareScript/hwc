//! Hole and via drilling methods for EntityGraph.

use crate::geometry::BoundingBox;
use crate::netlist::NetId;

use super::{EntityGraph, ViaHoleSpec};
use crate::geometry_router::substrate_types::SubstrateLayerType;

impl EntityGraph {
    /// Drill a hole through all substrate layers that intersect the given bbox.
    pub fn drill_hole(
        &mut self,
        hole_bbox: BoundingBox,
        diameter_nm: Option<i64>,
        _drill_net: NetId,
    ) {
        for layer in &mut self.substrate_layers {
            let z_intersects =
                |layer: &crate::geometry_router::substrate_types::SubstrateLayer| -> bool {
                    if layer.regions.is_empty() {
                        layer.bbox.min.z <= hole_bbox.max.z
                            && layer.bbox.max.z >= hole_bbox.min.z
                            && layer.bbox.min.x < hole_bbox.max.x
                            && layer.bbox.max.x > hole_bbox.min.x
                            && layer.bbox.min.y < hole_bbox.max.y
                            && layer.bbox.max.y > hole_bbox.min.y
                    } else {
                        layer.regions.iter().any(|r| {
                            r.min.z <= hole_bbox.max.z
                                && r.max.z >= hole_bbox.min.z
                                && r.min.x < hole_bbox.max.x
                                && r.max.x > hole_bbox.min.x
                                && r.min.y < hole_bbox.max.y
                                && r.max.y > hole_bbox.min.y
                        })
                    }
                };

            let should_drill = match layer.layer_type {
                SubstrateLayerType::Substrate => true,
                SubstrateLayerType::Pour => true,
                SubstrateLayerType::Contact => false,
            };

            if should_drill && z_intersects(layer) {
                if let Some(diameter) = diameter_nm {
                    layer.add_cylinder_cutout(hole_bbox, diameter);
                } else {
                    layer.add_cutout(hole_bbox);
                }
            }
        }
    }

    /// Drill a hole for a via, respecting net connectivity.
    ///
    /// v0.2.1: Solder mask / passivation layers are no longer special-cased here.
    /// Mask openings are generated at export time using Clipper2 Boolean operations.
    pub fn drill_via_hole(&mut self, spec: ViaHoleSpec) {
        let ViaHoleSpec {
            hole_bbox,
            diameter_nm,
            via_net,
            clearance_nm,
            is_tented: _,
            pad_diameter_nm: _,
        } = spec;
        for layer in &mut self.substrate_layers {
            let intersects = if layer.regions.is_empty() {
                let xy = layer.bbox.min.x < hole_bbox.max.x
                    && layer.bbox.max.x > hole_bbox.min.x
                    && layer.bbox.min.y < hole_bbox.max.y
                    && layer.bbox.max.y > hole_bbox.min.y;
                let z = layer.bbox.min.z <= hole_bbox.max.z && layer.bbox.max.z >= hole_bbox.min.z;
                xy && z
            } else {
                layer.regions.iter().any(|r| {
                    let xy = r.min.x < hole_bbox.max.x
                        && r.max.x > hole_bbox.min.x
                        && r.min.y < hole_bbox.max.y
                        && r.max.y > hole_bbox.min.y;
                    let z = r.min.z <= hole_bbox.max.z && r.max.z >= hole_bbox.min.z;
                    xy && z
                })
            };

            if intersects {
                match layer.layer_type {
                    SubstrateLayerType::Substrate => {
                        layer.add_cylinder_cutout(hole_bbox, diameter_nm);
                    }
                    SubstrateLayerType::Pour => {
                        let diameter = if layer.net == via_net {
                            diameter_nm
                        } else {
                            diameter_nm + 2 * clearance_nm
                        };
                        layer.add_cylinder_cutout(hole_bbox, diameter);
                    }
                    SubstrateLayerType::Contact => {}
                }
            }
        }
    }
}
