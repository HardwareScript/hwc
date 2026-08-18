//! Substrate layer management methods for EntityGraph.

use crate::geometry::BoundingBox;
use crate::geometry_router::spatial_index::IndexedSegment;
use crate::geometry_router::substrate_types::{
    ComponentMetadata, MaterialId, SubstrateLayer, SubstrateLayerShape, SubstrateLayerType,
};
use crate::netlist::NetId;

use super::{EntityGraph, TubeLayerSpec};

/// Configuration for checked substrate layer addition with clearance validation
pub struct SubstrateLayerConfig<'a> {
    pub material: MaterialId,
    pub net: NetId,
    pub bbox: BoundingBox,
    pub layer_type: SubstrateLayerType,
    pub min_clearance_nm: i64,
    pub device_binding: Option<(
        &'a compact_str::CompactString,
        &'a compact_str::CompactString,
    )>,
    pub pours: &'a [crate::space::PourMetadata],
}

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
    /// v0.2.3: Layer-aware clearance - only check conductors on the same Z-layer
    pub fn add_substrate_layer_checked(
        &mut self,
        config: SubstrateLayerConfig,
    ) -> Result<(), String> {
        if config.net != NetId::UNCONNECTED {
            for existing in self.substrate_layers.iter() {
                // Skip same-net layers (can overlap your own pours)
                if existing.net == NetId::UNCONNECTED || existing.net == config.net {
                    continue;
                }

                // v0.2.3: LAYER-AWARE CLEARANCE
                // Only check clearance for conductors on overlapping Z-ranges.
                // Layers separated by dielectric don't need lateral clearance.
                //
                // Example: metal1 at Z=0-20nm and metal2 at Z=70-100nm are separated
                // by 50nm of dielectric. They should NOT trigger clearance violations.
                //
                // Two ranges overlap if: !(a.max <= b.min || b.max <= a.min)
                // Simplified: a.max > b.min && b.max > a.min
                let z_overlap = config.bbox.max.z > existing.bbox.min.z
                    && existing.bbox.max.z > config.bbox.min.z;

                if !z_overlap {
                    // No Z-overlap = different layers, skip clearance check
                    eprintln!(
                        "[PLACEMENT] Skipping clearance check: Z-separated layers (new Z={}-{}nm, existing Z={}-{}nm)",
                        config.bbox.min.z, config.bbox.max.z, existing.bbox.min.z, existing.bbox.max.z
                    );
                    continue;
                }

                let distance = config.bbox.distance_to(&existing.bbox);
                if distance < config.min_clearance_nm {
                    return Err(format!(
                        "Clearance violation: Pour on net {} at {:?} is {}nm from net {} (required: {}nm)",
                        config.net.raw(), config.bbox, distance, existing.net.raw(), config.min_clearance_nm
                    ));
                }
            }
        }
        let mut layer =
            SubstrateLayer::new(config.material, config.net, config.bbox, config.layer_type);
        if let Some((dev_name, terminal)) = config.device_binding {
            layer.device_binding = Some((dev_name.to_string(), terminal.to_string()));
        }
        self.substrate_layers.push(layer);
        Ok(())
    }

    /// Get a reference to all substrate layers (includes masks and physical layers).
    /// 
    /// **WARNING**: This returns ALL layers including zero-thickness masks.
    /// For export operations (DXF, GDSII, GLB), use `get_physical_substrate_layers()` instead.
    pub fn get_substrate_layers(&self) -> &[SubstrateLayer] {
        &self.substrate_layers
    }

    /// Get a mutable reference to all substrate layers.
    pub fn get_substrate_layers_mut(&mut self) -> &mut Vec<SubstrateLayer> {
        &mut self.substrate_layers
    }

    /// Get an iterator over only physical substrate layers (excludes zero-thickness masks).
    /// 
    /// This is the CORRECT method for export operations (DXF, GDSII, GLB, STL).
    /// Mask materials are fabrication instructions and must never be exported as physical geometry.
    /// 
    /// **Architecture (v0.2.2 - Mask Filtering)**:
    /// - Masks are stored alongside physical layers in substrate_layers
    /// - This method filters them out at the boundary (export layer)
    /// - Makes invalid states unrepresentable: exporters can't accidentally export masks
    pub fn get_physical_substrate_layers<'a>(
        &'a self,
        material_registry: &'a crate::material::MaterialRegistry,
    ) -> impl Iterator<Item = &'a SubstrateLayer> + 'a {
        self.substrate_layers.iter().filter(move |layer| {
            // Filter out zero-thickness mask materials
            if let Some(category) = material_registry.get_category(layer.material) {
                !category.is_zero_thickness()
            } else {
                // If category lookup fails, include by default (fail-safe for legacy materials)
                true
            }
        })
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
                    let mut mat: MaterialId = seg.material_id;
                    if mat == 0 {
                        let mid_z = (bbox.min.z + bbox.max.z) / 2;
                        if let Some(matching) = self
                            .substrate_layers
                            .iter()
                            .find(|l| mid_z >= l.bbox.min.z && mid_z <= l.bbox.max.z)
                        {
                            mat = matching.material;
                        }
                    }
                    let layer = SubstrateLayer::new(
                        mat,
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
                    let mut mat: MaterialId = seg.material_id;
                    if mat == 0 {
                        let mid_z = (bbox.min.z + bbox.max.z) / 2;
                        if let Some(matching) = self
                            .substrate_layers
                            .iter()
                            .find(|l| mid_z >= l.bbox.min.z && mid_z <= l.bbox.max.z)
                        {
                            mat = matching.material;
                        }
                    }
                    let layer = SubstrateLayer::new(
                        mat,
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
                    let mut mat: MaterialId = seg.material_id;
                    if mat == 0 {
                        let mid_z = (bbox.min.z + bbox.max.z) / 2;
                        if let Some(matching) = self
                            .substrate_layers
                            .iter()
                            .find(|l| mid_z >= l.bbox.min.z && mid_z <= l.bbox.max.z)
                        {
                            mat = matching.material;
                        }
                    }
                    let layer = SubstrateLayer::new(
                        mat,
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
                    device_binding: layer.device_binding.as_ref().map(|(dev, term)| {
                        hwc_physics::connectivity::DeviceBinding {
                            device_name: dev.as_str().into(),
                            terminals: vec![term.as_str().into()], // v0.2.2: Wrap single terminal in Vec
                        }
                    }), // v0.2.2: Convert (String, String) to DeviceBinding
                }
            })
            .collect()
    }
}
