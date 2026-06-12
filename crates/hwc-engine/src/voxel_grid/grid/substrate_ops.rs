//! Substrate layer operations (v0.1.6 Sparse Architecture)

use super::core::VoxelGrid;
use crate::geometry::BoundingBox;
use crate::voxel_grid::chunk::{MaterialId, NetId};
use crate::voxel_grid::substrate_layers::{SubstrateLayer, SubstrateLayerType, Cutout, SubstrateLayerShape, TSVParams};
use compact_str::CompactString;
use smallvec::SmallVec;

impl VoxelGrid {
    /// Add a substrate layer to the grid.
    ///
    /// This is the God-Tier O(1) memory operation for substrates.
    /// Instead of allocating millions of chunks, we store just the bounding box.
    ///
    /// The three-step lookup in get_material() handles substrate layers efficiently:
    /// 1. Check voxels (for traces/components)
    /// 2. Check substrate_layers (for large wafers/pours)
    /// 3. Default to insulator (for empty space)
    ///
    /// # Arguments
    /// * `material` - Material ID (e.g., 1 for FR4)
    /// * `net` - Net ID (typically 0 for substrate)
    /// * `bbox` - Bounding box in nanometers
    ///
    /// # Example
    /// ```
    /// # use hwc_engine::{VoxelGrid, geometry::{BoundingBox, Point3D}, test_utils::test_voxel_size};
    /// let mut grid = VoxelGrid::new(2000, 2000, 4, test_voxel_size());
    /// let bbox = BoundingBox::new(
    ///     Point3D::new(0, 0, 0),
    ///     Point3D::new(20_000_000, 20_000_000, 2_000_000)
    /// );
    /// grid.add_substrate_layer(1, 0, bbox, SubstrateLayerType::Pour); // FR4 substrate
    /// ```
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

    /// Add a cylindrical substrate layer (v0.1.6).
    pub fn add_cylinder_substrate_layer(
        &mut self,
        material: MaterialId,
        net: NetId,
        bbox: BoundingBox,
        diameter: i64,
        segments: u32,
        koz_radius_nm: i64,
    ) {
        let mut layer = SubstrateLayer::new_cylinder(material, net, bbox, diameter, segments);
        layer.koz_radius_nm = koz_radius_nm;
        self.substrate_layers.push(layer);
    }

    /// Add a circular 2D substrate layer (circular pours).
    pub fn add_circle_substrate_layer(
        &mut self,
        material: MaterialId,
        net: NetId,
        bbox: BoundingBox,
        radius: i64,
    ) {
        let layer = SubstrateLayer::new_circle(material, net, bbox, radius);
        self.substrate_layers.push(layer);
    }

    /// Add a tube (plated hole) substrate layer (v0.1.7).
    pub fn add_tube_substrate_layer(
        &mut self,
        material: MaterialId,
        net: NetId,
        bbox: BoundingBox,
        outer_diameter: u32,
        inner_diameter: u32,
        pad_diameter: u32,
        segments: u32,
        top_cap: crate::voxel_grid::substrate_layers::CapType,
        bottom_cap: crate::voxel_grid::substrate_layers::CapType,
        bottom_outer_diameter: Option<u32>,
    ) {
        let layer = SubstrateLayer::new_tube(
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
        );
        self.substrate_layers.push(layer);
    }

    /// Add a substrate layer with cutouts (mounting holes, edge cuts, etc.).
    ///
    /// This is the God-Tier O(1) memory operation for substrates with holes.
    /// Memory usage: 32 bytes (base) + 24 bytes per cutout.
    ///
    /// # Arguments
    /// * `material` - Material ID (e.g., 1 for FR4)
    /// * `net` - Net ID (typically 0 for substrate)
    /// * `bbox` - Bounding box in nanometers
    /// * `cutouts` - Vector of bounding boxes defining holes
    ///
    /// # Example
    /// ```
    /// # use hwc_engine::{VoxelGrid, geometry::{BoundingBox, Point3D}, test_utils::test_voxel_size};
    /// let mut grid = VoxelGrid::new(2000, 2000, 4, test_voxel_size());
    /// let bbox = BoundingBox::new(
    ///     Point3D::new(0, 0, 0),
    ///     Point3D::new(20_000_000, 20_000_000, 2_000_000)
    /// );
    /// let cutout = BoundingBox::new(
    ///     Point3D::new(5_000_000, 5_000_000, 0),
    ///     Point3D::new(6_000_000, 6_000_000, 2_000_000)
    /// );
    /// grid.add_substrate_layer_with_cutouts(1, 0, bbox, vec![cutout], SubstrateLayerType::Pour); // FR4 with mounting hole
    /// ```
    pub fn add_substrate_layer_with_cutouts(
        &mut self,
        material: MaterialId,
        net: NetId,
        bbox: BoundingBox,
        cutouts: Vec<BoundingBox>,
        layer_type: SubstrateLayerType,
    ) {
        let cutouts_with_shape = cutouts
            .into_iter()
            .map(|b| Cutout {
                bbox: b,
                shape: SubstrateLayerShape::Rect,
            })
            .collect();
        let layer = SubstrateLayer::new_with_cutouts(
            material,
            net,
            bbox,
            cutouts_with_shape,
            layer_type,
        );
        self.substrate_layers.push(layer);
    }

    /// Add a TSV stack that spans across multiple silicon layers (v0.1.7).
    ///
    /// This is the coordination method for multi-die 3D integration.
    /// It ensures the TSV is correctly stamped, drilled, and registered across all layers.
    ///
    /// # Arguments
    /// * `center_x_nm`, `center_y_nm` - Center coordinates
    /// * `z_start_nm`, `z_end_nm` - Vertical span
    /// * `params` - TSV parameters (diameter, materials, KOZ)
    /// * `handle` - Net handle for the conductive core
    pub fn add_tsv_stack(
        &mut self,
        center_x_nm: i64,
        center_y_nm: i64,
        z_start_nm: i64,
        z_end_nm: i64,
        params: TSVParams,
        handle: crate::netlist::NetHandle,
    ) {
        // v0.1.7: Calculate KOZ clearance from multiplier
        let clearance_nm = ((params.koz_multiplier - 1.0) * params.diameter_nm as f32 / 2.0) as i64;

        // 1. Drill through all substrate layers in the path
        self.drill_tsv(
            center_x_nm,
            center_y_nm,
            z_start_nm,
            z_end_nm,
            params.diameter_nm,
            handle.raw(),
            clearance_nm,
        );

        // 2. Stamp the physical voxels (Liner -> Bridge -> Fill)
        self.stamp_tsv(
            center_x_nm,
            center_y_nm,
            z_start_nm,
            z_end_nm,
            params,
            handle,
        );

        // 3. Register the TSV as a SubstrateLayer (Contact) for export/analytic checks
        // We use the conductive core diameter for the SubstrateLayer
        let fill_radius_nm = params.diameter_nm / 2
            - params.stack.liner_thickness_nm
            - params.stack.bridge_thickness_nm;
        let fill_diameter_nm = fill_radius_nm * 2;

        let bbox = BoundingBox::new(
            crate::geometry::Point3D::new(
                center_x_nm - fill_diameter_nm / 2,
                center_y_nm - fill_diameter_nm / 2,
                z_start_nm,
            ),
            crate::geometry::Point3D::new(
                center_x_nm + fill_diameter_nm / 2,
                center_y_nm + fill_diameter_nm / 2,
                z_end_nm,
            ),
        );

        // Add the conductive core as a contact layer with KOZ
        self.add_cylinder_substrate_layer(
            params.stack.fill_material,
            handle.raw(),
            bbox,
            fill_diameter_nm,
            16,
            (params.diameter_nm as f32 * params.koz_multiplier / 2.0) as i64,
        );
    }

    /// Drill a hole for a TSV through all intersecting substrate layers (v0.1.7).
    pub fn drill_tsv(
        &mut self,
        center_x_nm: i64,
        center_y_nm: i64,
        z_start_nm: i64,
        z_end_nm: i64,
        diameter_nm: i64,
        net_id: NetId, // v0.1.7: Added net awareness
        clearance_nm: i64, // v0.1.7: Added clearance awareness
    ) {
        // v0.1.7 FIXED: Calculate bbox based on total clearance diameter
        // to ensure intersection tests include the anti-pad area.
        let drill_radius_nm = diameter_nm / 2 + clearance_nm;
        let bbox = BoundingBox::new(
            crate::geometry::Point3D::new(
                center_x_nm - drill_radius_nm,
                center_y_nm - drill_radius_nm,
                z_start_nm,
            ),
            crate::geometry::Point3D::new(
                center_x_nm + drill_radius_nm,
                center_y_nm + drill_radius_nm,
                z_end_nm,
            ),
        );
        // TSVs are internal, never tented. Pad diameter = drill + 2*annular_ring (use 0 default).
        self.drill_via_hole(bbox, diameter_nm, net_id, clearance_nm, false, diameter_nm, 75_000);
    }

    /// Get the number of substrate layers.
    pub fn substrate_layer_count(&self) -> usize {
        self.substrate_layers.len()
    }

    /// Add a drill hole (cutout) to all existing substrate layers that intersect it (Limitation 7).
    ///
    /// This is used for through-hole pins and mounting holes.
    /// Memory usage: O(N) where N is the number of substrate layers.
    pub fn drill_hole(&mut self, hole_bbox: BoundingBox, diameter_nm: Option<i64>, _drill_net: NetId) {
        // 1. Bit-level clearing (for router/collision)
        self.clear_voxels_in_bbox(&hole_bbox);

        // 2. Structural clearing (for analytic export)
        for layer in &mut self.substrate_layers {
            // v0.1.7 FIXED: Drill into both Pours (Copper) AND Substrates (FR4/Core)
            // Skip Contact layers (Vias/Tubes) to avoid self-drilling artifacts
            
            // v0.1.7 FIXED: Use inclusive Z-intersection to handle perfect adjacency.
            let xy_intersects = layer.bbox.min.x < hole_bbox.max.x
                && layer.bbox.max.x > hole_bbox.min.x
                && layer.bbox.min.y < hole_bbox.max.y
                && layer.bbox.max.y > hole_bbox.min.y;

            let z_intersects = layer.bbox.min.z <= hole_bbox.max.z
                && layer.bbox.max.z >= hole_bbox.min.z;

            let should_drill = match layer.layer_type {
                SubstrateLayerType::Substrate => true, // Always drill dielectric (insulators)
                SubstrateLayerType::SolderMask => true, // Drill solder mask for via openings
                SubstrateLayerType::Pour => {
                    // v0.1.7: Drill into copper pours to ensure vias work.
                    // This fixes the issue where manual routes block vias.
                    true
                },
                SubstrateLayerType::Contact => false, // Don't drill other vias
            };

            if should_drill && xy_intersects && z_intersects {
                if let Some(diameter) = diameter_nm {
                    layer.add_cylinder_cutout(hole_bbox, diameter);
                } else {
                    layer.add_cutout(hole_bbox);
                }
            }
        }
    }

    /// Drill a hole for a via, respecting net connectivity (v0.1.7).
    ///
    /// This is the "Auto-Drill" logic that prevents short circuits.
    /// It drills through all Substrate layers and any Pour layers that have a DIFFERENT net.
    /// v0.1.7: Added clearance_nm for different-net anti-pads.
    /// v0.1.7: Added is_tented, pad_diameter_nm, and solder_mask_expansion_nm for
    /// profile-driven solder mask openings (Zero Implicit Magic).
    pub fn drill_via_hole(
        &mut self,
        hole_bbox: BoundingBox,
        diameter_nm: i64,
        via_net: NetId,
        clearance_nm: i64,
        is_tented: bool,
        pad_diameter_nm: i64,
        solder_mask_expansion_nm: i64,
    ) {
        // 1. Structural clearing
        for layer in &mut self.substrate_layers {
            let xy_intersects = layer.bbox.min.x < hole_bbox.max.x
                && layer.bbox.max.x > hole_bbox.min.x
                && layer.bbox.min.y < hole_bbox.max.y
                && layer.bbox.max.y > hole_bbox.min.y;

            let z_intersects = layer.bbox.min.z <= hole_bbox.max.z
                && layer.bbox.max.z >= hole_bbox.min.z;

            if xy_intersects && z_intersects {
                match layer.layer_type {
                    SubstrateLayerType::Substrate => {
                        // Always drill dielectric with the actual via diameter
                        layer.add_cylinder_cutout(hole_bbox, diameter_nm);
                    },
                    SubstrateLayerType::SolderMask => {
                        // v0.1.7: Profile-driven solder mask opening (Zero Implicit Magic)
                        if !is_tented {
                            // Exposed via: cut opening in solder mask
                            // Formula: pad_diameter + 2 × solder_mask_expansion
                            let opening_diameter = pad_diameter_nm + 2 * solder_mask_expansion_nm;
                            // The cutout must span the full mask thickness so the export
                            // slicing logic can match it. Use the layer's own Z range.
                            let mask_cutout_bbox = crate::geometry::BoundingBox::new(
                                crate::geometry::Point3D::new(
                                    hole_bbox.min.x.max(layer.bbox.min.x),
                                    hole_bbox.min.y.max(layer.bbox.min.y),
                                    layer.bbox.min.z,
                                ),
                                crate::geometry::Point3D::new(
                                    hole_bbox.max.x.min(layer.bbox.max.x),
                                    hole_bbox.max.y.min(layer.bbox.max.y),
                                    layer.bbox.max.z,
                                ),
                            );
                            layer.add_cylinder_cutout(mask_cutout_bbox, opening_diameter);
                        }
                        // Tented: do nothing, mask stays intact
                    },
                    SubstrateLayerType::Pour => {
                        // v0.1.7: Always drill into copper pours (same net or different net) 
                        // to ensure the hole is clear. The unioning logic will preserve the pad
                        // around the hole.
                        let diameter = if layer.net == via_net {
                            diameter_nm
                        } else {
                            diameter_nm + 2 * clearance_nm
                        };
                        layer.add_cylinder_cutout(hole_bbox, diameter);
                    },
                    SubstrateLayerType::Contact => {
                        // Don't drill other vias (handled by spacing checks)
                    },
                }
            }
        }
    }

    /// Get a reference to all substrate layers.
    ///
    /// This is used by export systems to efficiently render substrates
    /// as bounding boxes instead of individual voxels.
    ///
    /// # Returns
    /// Slice of substrate layers
    pub fn get_substrate_layers(&self) -> &[SubstrateLayer] {
        &self.substrate_layers
    }

    /// Get a mutable reference to all substrate layers.
    pub fn get_substrate_layers_mut(&mut self) -> &mut Vec<SubstrateLayer> {
        &mut self.substrate_layers
    }

    /// Add component metadata to the grid (GOD-TIER SPARSE ARCHITECTURE).
    ///
    /// This is the O(1) memory operation for components.
    /// Instead of filling millions of voxels (Density Bomb), we store just the bounding box.
    ///
    /// # Arguments
    /// * `bbox` - Bounding box in nanometers
    /// * `material` - Material ID (e.g., 5 for Ceramic)
    /// * `name` - Component name (e.g., "R1", "Q1")
    /// * `component_type` - Component type (e.g., "Resistor")
    /// * `blocked_z_ranges` - (v0.1.7) Layer-Aware KOZ: Z-ranges this component blocks.
    ///   Empty = full 3D keepout (legacy). Non-empty = only block these Z-ranges,
    ///   allowing pours/traces on other layers to pass through.
    ///
    /// # Example
    /// ```
    /// # use hwc_engine::{VoxelGrid, geometry::{BoundingBox, Point3D}, test_utils::test_voxel_size};
    /// # use smallvec::SmallVec;
    /// let mut grid = VoxelGrid::new(2000, 2000, 4, test_voxel_size(), 0);
    /// let bbox = BoundingBox::new(
    ///     Point3D::new(1_000_000, 1_000_000, 0),
    ///     Point3D::new(6_000_000, 3_000_000, 1_000_000)
    /// );
    /// grid.add_component_metadata(bbox, 5, "R1".into(), "Resistor".into(), SmallVec::new()); // Ceramic resistor
    /// ```
    pub fn add_component_metadata(
        &mut self,
        bbox: BoundingBox,
        material: MaterialId,
        name: CompactString,
        component_type: CompactString,
        blocked_z_ranges: SmallVec<[(i64, i64); 2]>,
    ) {
        use crate::voxel_grid::ComponentMetadata;
        let mut component = ComponentMetadata::new(material, bbox, name, component_type);
        component.blocked_z_ranges = blocked_z_ranges;
        self.component_metadata.push(component);
    }

    /// Get the number of components.
    pub fn component_count(&self) -> usize {
        self.component_metadata.len()
    }

    /// Get an iterator over component metadata.
    ///
    /// This allows external code to inspect component bounding boxes
    /// without exposing the internal Vec directly.
    pub fn component_metadata_iter(
        &self,
    ) -> impl Iterator<Item = &crate::voxel_grid::ComponentMetadata> {
        self.component_metadata.iter()
    }

    /// Add a component pin for physical continuity validation (v0.1.6 Sprint 3).
    ///
    /// Registers a pin at an absolute position in the design. Pins are used by
    /// the P43 validator to detect floating conductors - conductive geometry
    /// that has no component pins touching it.
    ///
    /// # Arguments
    /// * `x_nm` - X coordinate in nanometers (absolute)
    /// * `y_nm` - Y coordinate in nanometers (absolute)
    /// * `z_nm` - Z coordinate in nanometers (absolute)
    /// * `component_name` - Component instance name (e.g., "M1")
    /// * `pin_name` - Pin name within the component (e.g., "gate")
    /// * `net` - Optional net assignment (e.g., Some("VIN"))
    ///
    /// # Example
    /// ```
    /// # use hwc_engine::{VoxelGrid, test_utils::test_voxel_size};
    /// let mut grid = VoxelGrid::new(2000, 2000, 4, test_voxel_size(), 0);
    /// grid.add_component_pin(
    ///     1_000_000,  // x: 1mm
    ///     2_000_000,  // y: 2mm
    ///     0,          // z: 0mm
    ///     "M1".into(),
    ///     "gate".into(),
    ///     Some("VIN".into())
    /// );
    /// ```
    pub fn add_component_pin(
        &mut self,
        x_nm: i64,
        y_nm: i64,
        z_nm: i64,
        component_name: CompactString,
        pin_name: CompactString,
        net: Option<CompactString>,
    ) {
        use crate::voxel_grid::ComponentPin;
        let pin = ComponentPin::new(x_nm, y_nm, z_nm, component_name, pin_name, net);
        self.component_pins.push(pin);
    }

    /// Get the number of component pins.
    pub fn component_pin_count(&self) -> usize {
        self.component_pins.len()
    }

    /// Get a reference to all component pins.
    ///
    /// This is used by the physical continuity validator to check if each
    /// conductive island has at least one pin touching it (P43 check).
    ///
    /// # Returns
    /// Slice of component pins
    pub fn get_component_pins(&self) -> &[crate::voxel_grid::ComponentPin] {
        &self.component_pins
    }

    /// Check if a point (in nanometers) intersects any component keepout zone (KOZ).
    ///
    /// This is used by the router to avoid routing through components.
    /// v0.1.7 Layer-Aware: This now respects `blocked_z_ranges`, allowing traces
    /// to pass under/over components if they are outside the blocked ranges.
    ///
    /// # Arguments
    /// * `x_nm` - X coordinate in nanometers
    /// * `y_nm` - Y coordinate in nanometers
    /// * `z_nm` - Z coordinate in nanometers
    ///
    /// # Returns
    /// `Some(component_name)` if point is inside a component KOZ, `None` otherwise
    pub fn point_in_component(&self, x_nm: i64, y_nm: i64, z_nm: i64) -> Option<CompactString> {
        for component in &self.component_metadata {
            if component.is_in_koz(x_nm, y_nm, z_nm) {
                return Some(component.name.clone());
            }
        }
        None
    }

    /// Check if a point (in nanometers) is at a component pin location.
    ///
    /// This is used by the router to allow routing TO component pins (endpoints)
    /// while blocking routing THROUGH components.
    ///
    /// **Tolerance**: Checks if point is within 1 voxel of any pin position.
    /// This handles floating-point rounding and snapping differences.
    ///
    /// # Arguments
    /// * `x_nm` - X coordinate in nanometers
    /// * `y_nm` - Y coordinate in nanometers
    /// * `z_nm` - Z coordinate in nanometers
    /// * `tolerance_nm` - Tolerance in nanometers (typically 1 voxel size)
    ///
    /// # Returns
    /// `true` if point is at a pin location, `false` otherwise
    ///
    /// # Example
    /// ```
    /// # use hwc_engine::{VoxelGrid, test_utils::test_voxel_size};
    /// let mut grid = VoxelGrid::new(2000, 2000, 4, test_voxel_size(), 0);
    /// grid.add_component_pin(
    ///     1_000_000,  // x: 1mm
    ///     2_000_000,  // y: 2mm
    ///     0,          // z: 0mm
    ///     "M1".into(),
    ///     "gate".into(),
    ///     Some("VIN".into())
    /// );
    ///
    /// // Exact pin location
    /// assert!(grid.is_at_component_pin(1_000_000, 2_000_000, 0, 100_000));
    ///
    /// // Within tolerance
    /// assert!(grid.is_at_component_pin(1_050_000, 2_050_000, 0, 100_000));
    ///
    /// // Outside tolerance
    /// assert!(!grid.is_at_component_pin(2_000_000, 3_000_000, 0, 100_000));
    /// ```
    pub fn is_at_component_pin(&self, x_nm: i64, y_nm: i64, z_nm: i64, tolerance_nm: i64) -> bool {
        for pin in &self.component_pins {
            let dx = (pin.x_nm - x_nm).abs();
            let dy = (pin.y_nm - y_nm).abs();
            let dz = (pin.z_nm - z_nm).abs();

            if dx <= tolerance_nm && dy <= tolerance_nm && dz <= tolerance_nm {
                return true;
            }
        }
        false
    }
}
