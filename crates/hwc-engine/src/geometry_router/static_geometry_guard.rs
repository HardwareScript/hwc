use std::fmt;

use rustc_hash::FxHashMap;

use crate::geometry::BoundingBox;
use crate::geometry_router::entity_graph::EntityGraph;
use crate::geometry_router::substrate_types::SubstrateLayerType;
use crate::netlist::NetlistArena;

#[derive(Debug, Clone)]
pub struct StaticViolation {
    pub net_a: compact_str::CompactString,
    pub net_b: compact_str::CompactString,
    pub material_a: crate::material::MaterialId,
    pub material_b: crate::material::MaterialId,
    pub bbox: BoundingBox,
    pub z_overlap: (i64, i64),
}

impl fmt::Display for StaticViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Static short: net '{}' overlaps net '{}' at ({:.3},{:.3},{:.3})-({:.3},{:.3},{:.3}) mm",
            self.net_a,
            self.net_b,
            self.bbox.min.x as f64 / 1_000_000.0,
            self.bbox.min.y as f64 / 1_000_000.0,
            self.bbox.min.z as f64 / 1_000_000.0,
            self.bbox.max.x as f64 / 1_000_000.0,
            self.bbox.max.y as f64 / 1_000_000.0,
            self.bbox.max.z as f64 / 1_000_000.0,
        )
    }
}

struct LayerEntry {
    net: u32,
    material: crate::material::MaterialId,
    bbox: BoundingBox,
}

fn z_overlap(a_min: i64, a_max: i64, b_min: i64, b_max: i64) -> Option<(i64, i64)> {
    let lo = a_min.max(b_min);
    let hi = a_max.min(b_max);
    if lo < hi {
        Some((lo, hi))
    } else {
        None
    }
}

fn xy_overlap(a: &BoundingBox, b: &BoundingBox) -> bool {
    a.min.x < b.max.x && a.max.x > b.min.x && a.min.y < b.max.y && a.max.y > b.min.y
}

pub fn check_static_shorts(
    entity_graph: &EntityGraph,
    netlist: &NetlistArena,
) -> Vec<StaticViolation> {
    let mut violations = Vec::new();

    let mut by_material: FxHashMap<crate::material::MaterialId, Vec<LayerEntry>> =
        FxHashMap::default();

    for layer in entity_graph.get_substrate_layers() {
        if layer.net == 0 {
            continue;
        }
        if !matches!(
            layer.layer_type,
            SubstrateLayerType::Pour | SubstrateLayerType::Contact
        ) {
            continue;
        }

        let mut bboxes = vec![layer.bbox];
        for region in &layer.regions {
            bboxes.push(*region);
        }

        for bbox in bboxes {
            by_material
                .entry(layer.material)
                .or_default()
                .push(LayerEntry {
                    net: layer.net,
                    material: layer.material,
                    bbox,
                });
        }
    }

    for (_mat_id, entries) in &by_material {
        for i in 0..entries.len() {
            for j in (i + 1)..entries.len() {
                let a = &entries[i];
                let b = &entries[j];

                if a.net == b.net {
                    continue;
                }

                if !xy_overlap(&a.bbox, &b.bbox) {
                    continue;
                }

                if let Some(z) = z_overlap(a.bbox.min.z, a.bbox.max.z, b.bbox.min.z, b.bbox.max.z) {
                    let intersect_min = BoundingBox {
                        min: crate::geometry::Point3D::new(
                            a.bbox.min.x.max(b.bbox.min.x),
                            a.bbox.min.y.max(b.bbox.min.y),
                            z.0,
                        ),
                        max: crate::geometry::Point3D::new(
                            a.bbox.max.x.min(b.bbox.max.x),
                            a.bbox.max.y.min(b.bbox.max.y),
                            z.1,
                        ),
                    };

                    let net_a_name = netlist
                        .get_net(crate::netlist::NetId::new(a.net))
                        .map(|n| n.name.clone())
                        .unwrap_or_else(|| {
                            let s = format!("net_{}", a.net);
                            compact_str::CompactString::from(s.as_str())
                        });

                    let net_b_name = netlist
                        .get_net(crate::netlist::NetId::new(b.net))
                        .map(|n| n.name.clone())
                        .unwrap_or_else(|| {
                            let s = format!("net_{}", b.net);
                            compact_str::CompactString::from(s.as_str())
                        });

                    violations.push(StaticViolation {
                        net_a: net_a_name,
                        net_b: net_b_name,
                        material_a: a.material,
                        material_b: b.material,
                        bbox: intersect_min,
                        z_overlap: z,
                    });
                }
            }
        }
    }

    violations
}
