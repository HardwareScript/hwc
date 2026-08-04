//! Substrate layer management methods for EntityGraph.

use crate::geometry::BoundingBox;
use crate::geometry_router::spatial_index::IndexedSegment;
use crate::geometry_router::substrate_types::{
    ComponentMetadata, MaterialId, SubstrateLayer, SubstrateLayerShape, SubstrateLayerType,
};
use crate::netlist::NetId;

use super::{EntityGraph, TubeLayerSpec};

impl EntityGraph {
    /// Add a substrate layer with optional clearance validation.
    pub fn add_substrate_layer(
        &mut self,
        material: MaterialId,
        net: NetId,
        bbox: BoundingBox,
        layer_type: SubstrateLayerType,
    ) {
        let layer = SubstrateLayer::new(material, net, bbox, layer_type);
        self.substrate_layers.push(layer);
    }

    /// Add a substrate layer with clearance validation (v0.1.9).
    /// v0.2.1: Added device terminal exemption for capacitors and other vertically-stacked devices.
    pub fn add_substrate_layer_checked(
        &mut self,
        material: MaterialId,
        net: NetId,
        bbox: BoundingBox,
        layer_type: SubstrateLayerType,
        min_clearance_nm: i64,
        device_binding: Option<(&compact_str::CompactString, &compact_str::CompactString)>, // (device_name, terminal)
        pours: &[crate::space::PourMetadata],
    ) -> Result<(), String> {
        if net != NetId::UNCONNECTED {
            for (idx, existing) in self.substrate_layers.iter().enumerate() {
                if existing.net == NetId::UNCONNECTED || existing.net == net {
                    continue;
                }
                
                // DEVICE TERMINAL EXEMPTION (v0.2.1): If both pours belong to same device instance,
                // skip clearance check (intentional overlap for capacitors, transistors, etc.)
                if let Some((dev_name, _terminal)) = device_binding {
                    // Check if existing layer has device binding to same device
                    // The substrate layer index maps to pour index since they're added in order
                    if idx < pours.len() {
                        if let Some(ref existing_binding) = pours[idx].device_binding {
                            if existing_binding.device_name == *dev_name {
                                // Same device instance - allow overlap (e.g., capacitor plates)
                                continue;
                            }
                        }
                    }
                }
                
                let distance = bbox.distance_to(&existing.bbox);
                if distance < min_clearance_nm {
                    return Err(format!(
                        "Clearance violation: Pour on net {} at {:?} is {}nm from net {} (required: {}nm)",
                        net.raw(), bbox, distance, existing.net.raw(), min_clearance_nm
                    ));
                }
            }
        }
        let mut layer = SubstrateLayer::new(material, net, bbox, layer_type);
        if let Some((dev_name, terminal)) = device_binding {
            layer.device_binding = Some((dev_name.to_string(), terminal.to_string()));
        }
        self.substrate_layers.push(layer);
        Ok(())
    }

    /// Get a reference to all substrate layers.
    pub fn get_substrate_layers(&self) -> &[SubstrateLayer] {
        &self.substrate_layers
    }

    /// Get a mutable reference to all substrate layers.
    pub fn get_substrate_layers_mut(&mut self) -> &mut Vec<SubstrateLayer> {
        &mut self.substrate_layers
    }

    /// Add component metadata.
    pub fn add_component_metadata(
        &mut self,
        bbox: BoundingBox,
        material: MaterialId,
        name: compact_str::CompactString,
        component_type: compact_str::CompactString,
        blocked_z_ranges: smallvec::SmallVec<[(i64, i64); 2]>,
    ) {
        let mut component = ComponentMetadata::new(material, bbox, name, component_type);
        component.blocked_z_ranges = blocked_z_ranges;
        self.component_metadata.push(component);
    }

    /// Get all elements (pours and routes) for a specific net across all layers.
    pub fn get_all_elements_for_net(&self, net_id: NetId) -> Vec<SubstrateLayer> {
        let mut elements = Vec::new();

        for layer in &self.substrate_layers {
            if layer.net == net_id {
                elements.push(layer.clone());
            }
        }

        for (seg_net_id, segments) in &self.routed_segments {
            if *seg_net_id == net_id {
                for seg in segments {
                    let bbox = BoundingBox::new(seg.start, seg.end);
                    let layer = SubstrateLayer::new(
                        seg.material_id,
                        net_id,
                        bbox,
                        SubstrateLayerType::Pour,
                    );
                    elements.push(layer);
                }
            }
        }

        elements
    }

    /// Get only routable surfaces (pours, traces) for a specific net, excluding vias/contacts.
    /// 
    /// This method is used by the ViaResolver to identify layer transitions that need bridging.
    /// Vias/contacts are bridges themselves and should not trigger via insertion.
    pub fn get_pours_for_net(&self, net_id: NetId) -> Vec<SubstrateLayer> {
        let mut elements = Vec::new();

        // Only include substrate layers that are Pour type (not Contact)
        for layer in &self.substrate_layers {
            if layer.net == net_id && layer.layer_type == SubstrateLayerType::Pour {
                elements.push(layer.clone());
            }
        }

        // Routed segments are always Pour type
        for (seg_net_id, segments) in &self.routed_segments {
            if *seg_net_id == net_id {
                for seg in segments {
                    let bbox = BoundingBox::new(seg.start, seg.end);
                    let layer = SubstrateLayer::new(
                        seg.material_id,
                        net_id,
                        bbox,
                        SubstrateLayerType::Pour,
                    );
                    elements.push(layer);
                }
            }
        }

        elements
    }

    /// Get elements (pours and routes) for a specific net on a specific layer.
    pub fn get_elements_for_net_on_layer(
        &self,
        net_id: NetId,
        _layer_idx: usize,
    ) -> Vec<SubstrateLayer> {
        let mut elements = Vec::new();

        for layer in &self.substrate_layers {
            if layer.net == net_id {
                elements.push(layer.clone());
            }
        }

        for (seg_net_id, segments) in &self.routed_segments {
            if *seg_net_id == net_id {
                for seg in segments {
                    let bbox = BoundingBox::new(seg.start, seg.end);
                    let layer = SubstrateLayer::new(
                        seg.material_id,
                        net_id,
                        bbox,
                        SubstrateLayerType::Pour,
                    );
                    elements.push(layer);
                }
            }
        }

        elements
    }

    /// Add a cylindrical substrate layer (e.g. via pad).
    pub fn add_cylinder_substrate_layer(
        &mut self,
        material: MaterialId,
        net: NetId,
        bbox: BoundingBox,
        diameter_nm: i64,
        _segments: u32,
        _rotation_deg: i64,
    ) {
        let mut layer = SubstrateLayer::new(material, net, bbox, SubstrateLayerType::Contact);
        layer.shape = SubstrateLayerShape::Circle {
            radius: diameter_nm / 2,
        };
        self.substrate_layers.push(layer);
    }

    /// Add a tube substrate layer (e.g. plated through-hole wall).
    pub fn add_tube_substrate_layer(&mut self, spec: TubeLayerSpec) {
        let TubeLayerSpec {
            material,
            net,
            bbox,
            outer_diameter,
            inner_diameter,
            pad_diameter,
            segments,
            top_cap,
            bottom_cap,
            bottom_outer_diameter,
        } = spec;
        let mut layer = SubstrateLayer::new(material, net, bbox, SubstrateLayerType::Contact);
        layer.shape = SubstrateLayerShape::Tube {
            outer_diameter,
            inner_diameter,
            pad_diameter,
            segments,
            top_cap,
            bottom_cap,
            bottom_outer_diameter,
        };
        self.substrate_layers.push(layer);
    }

    /// Add a polygonal substrate layer.
    pub fn add_polygon_substrate_layer(
        &mut self,
        material: MaterialId,
        net: NetId,
        bbox: BoundingBox,
        polygon: crate::geometry::Polygon,
    ) {
        let mut outer_contour = clipper2_rust::Path64::new();
        for p in &polygon.points {
            outer_contour.push(clipper2_rust::Point64::new(p.x, p.y));
        }

        let mut layer = SubstrateLayer::new(material, net, bbox, SubstrateLayerType::Contact);
        layer.shape = SubstrateLayerShape::Polygon {
            outer_contour,
            holes: clipper2_rust::Paths64::new(),
            segments: 32,
        };
        self.substrate_layers.push(layer);
    }

    /// Add a circular substrate layer (alias for add_cylinder_substrate_layer).
    pub fn add_circle_substrate_layer(
        &mut self,
        material: MaterialId,
        net: NetId,
        bbox: BoundingBox,
        radius_nm: i64,
    ) {
        self.add_cylinder_substrate_layer(material, net, bbox, radius_nm * 2, 32, 0);
    }

    /// Add a TSV (Through Silicon Via) stack.
    pub fn add_tsv_stack(
        &mut self,
        material: MaterialId,
        net: NetId,
        bbox: BoundingBox,
        outer_diameter: u32,
        inner_diameter: u32,
        circle_segments: u32,
    ) {
        self.add_tube_substrate_layer(
            TubeLayerSpec::builder(material, net, bbox, circle_segments)
                .outer_diameter(outer_diameter)
                .inner_diameter(inner_diameter)
                .pad_diameter(outer_diameter)
                .top_cap(crate::geometry_router::substrate_types::CapType::Solid)
                .bottom_cap(crate::geometry_router::substrate_types::CapType::Solid)
                .build(),
        );
    }

    /// Copy component metadata and pins from another EntityGraph.
    pub fn copy_metadata_from(&mut self, other: &EntityGraph) {
        self.component_metadata = other.component_metadata.clone();
        self.component_pins = other.component_pins.clone();
        self.substrate_layers = other.substrate_layers.clone();
        self.routed_segments = other.routed_segments.clone();
    }

    /// Convert substrate layers into IndexedSegments for spatial index insertion.
    pub fn get_substrate_layers_as_segments(&self) -> Vec<IndexedSegment> {
        self.substrate_layers
            .iter()
            .enumerate()
            .map(|(i, layer)| {
                let mut bboxes = vec![layer.bbox];
                for region in &layer.regions {
                    bboxes.push(*region);
                }
                let combined = bboxes.iter().fold(layer.bbox, |acc, b| acc.union(b));
                IndexedSegment {
                    source: hwc_physics::spatial_index::SpatialEntitySource::SubstrateLayer {
                        index: i,
                    },
                    segment_id: i,
                    net_id: layer.net,
                    width_nm: combined.max.x - combined.min.x,
                    thickness_nm: combined.max.z - combined.min.z,
                    start: combined.min,
                    end: combined.max,
                    layer: combined.min.z,
                }
            })
            .collect()
    }
}
