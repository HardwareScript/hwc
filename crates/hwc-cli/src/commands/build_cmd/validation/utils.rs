use hwc_engine::HardwareSpace;

/// Convert engine metadata to physics format
pub fn convert_metadata_to_physics(
    space: &HardwareSpace,
) -> (
    Vec<hwc_physics::connectivity::PourMetadata>,
    Vec<hwc_physics::connectivity::ContactMetadata>,
    Vec<hwc_physics::connectivity::SubstrateLayerMetadata>,
) {
    let physics_pours: Vec<hwc_physics::connectivity::PourMetadata> = space
        .pours
        .iter()
        .map(|pour| hwc_physics::connectivity::PourMetadata {
            name: pour.name.clone(),
            material_name: pour.material_name.clone(),
            net: pour.net.clone(),
            area_nm2: pour.area_nm2,
            bbox: pour
                .bbox
                .as_ref()
                .map(|bbox| hwc_physics::connectivity::BoundingBox {
                    min_x: bbox.min.x,
                    min_y: bbox.min.y,
                    min_z: bbox.min.z,
                    max_x: bbox.max.x,
                    max_y: bbox.max.y,
                    max_z: bbox.max.z,
                }),
        })
        .collect();

    let physics_contacts: Vec<hwc_physics::connectivity::ContactMetadata> = space
        .contacts
        .iter()
        .map(|contact| hwc_physics::connectivity::ContactMetadata {
            name: contact.name.clone(),
            material_name: contact.material_name.clone(),
            net: contact.net.clone(),
            bbox: contact
                .bbox
                .as_ref()
                .map(|bbox| hwc_physics::connectivity::BoundingBox {
                    min_x: bbox.min.x,
                    min_y: bbox.min.y,
                    min_z: bbox.min.z,
                    max_x: bbox.max.x,
                    max_y: bbox.max.y,
                    max_z: bbox.max.z,
                }),
        })
        .collect();

    let physics_substrate_layers: Vec<hwc_physics::connectivity::SubstrateLayerMetadata> = space
        .voxel_grid
        .get_substrate_layers()
        .iter()
        .map(|layer| {
            let net_name = if layer.net != 0 {
                space
                    .netlist
                    .get_net(hwc_engine::netlist::NetId::new(layer.net))
                    .map(|net_data| net_data.name.clone())
            } else {
                None
            };

            let shape = match layer.shape {
                hwc_engine::voxel_grid::SubstrateLayerShape::Rect => {
                    hwc_physics::connectivity::SubstrateLayerShapeMetadata::Rect
                }
                hwc_engine::voxel_grid::SubstrateLayerShape::Polygon {
                    ref outer_contour, ..
                } => hwc_physics::connectivity::SubstrateLayerShapeMetadata::Polygon {
                    outer_contour: outer_contour.iter().map(|p| (p.x, p.y)).collect(),
                },
                hwc_engine::voxel_grid::SubstrateLayerShape::Tube {
                    outer_diameter,
                    inner_diameter,
                    ..
                } => hwc_physics::connectivity::SubstrateLayerShapeMetadata::Tube {
                    outer_diameter,
                    inner_diameter,
                },
                hwc_engine::voxel_grid::SubstrateLayerShape::Circle { .. } => {
                    hwc_physics::connectivity::SubstrateLayerShapeMetadata::Rect
                }
            };

            let cutouts = layer
                .cutouts
                .iter()
                .map(|c| hwc_physics::connectivity::CutoutMetadata {
                    bbox: hwc_physics::connectivity::BoundingBox {
                        min_x: c.bbox.min.x,
                        min_y: c.bbox.min.y,
                        min_z: c.bbox.min.z,
                        max_x: c.bbox.max.x,
                        max_y: c.bbox.max.y,
                        max_z: c.bbox.max.z,
                    },
                    shape: match c.shape {
                        hwc_engine::voxel_grid::SubstrateLayerShape::Rect => {
                            hwc_physics::connectivity::SubstrateLayerShapeMetadata::Rect
                        }
                        hwc_engine::voxel_grid::SubstrateLayerShape::Polygon {
                            ref outer_contour,
                            ..
                        } => hwc_physics::connectivity::SubstrateLayerShapeMetadata::Polygon {
                            outer_contour: outer_contour.iter().map(|p| (p.x, p.y)).collect(),
                        },
                        hwc_engine::voxel_grid::SubstrateLayerShape::Tube {
                            outer_diameter,
                            inner_diameter,
                            ..
                        } => hwc_physics::connectivity::SubstrateLayerShapeMetadata::Tube {
                            outer_diameter,
                            inner_diameter,
                        },
                        hwc_engine::voxel_grid::SubstrateLayerShape::Circle { .. } => {
                            hwc_physics::connectivity::SubstrateLayerShapeMetadata::Rect
                        }
                    },
                })
                .collect();

            hwc_physics::connectivity::SubstrateLayerMetadata {
                material: layer.material,
                net: layer.net,
                net_name,
                bbox: hwc_physics::connectivity::BoundingBox {
                    min_x: layer.bbox.min.x,
                    min_y: layer.bbox.min.y,
                    min_z: layer.bbox.min.z,
                    max_x: layer.bbox.max.x,
                    max_y: layer.bbox.max.y,
                    max_z: layer.bbox.max.z,
                },
                shape,
                cutouts,
            }
        })
        .collect();

    (physics_pours, physics_contacts, physics_substrate_layers)
}
