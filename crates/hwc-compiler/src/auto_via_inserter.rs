//! Automatic via insertion for layer transitions.
//!
//! This module implements Sprint 3.3: Automatic Via Insertion.
//! It detects when a net transitions between layers and automatically
//! inserts vias at overlap points to maintain electrical connectivity.
//!
//! # Architecture
//!
//! 1. **Layer Transition Detection**: Scan all pours on a net to find Z-layer changes
//! 2. **Overlap Detection**: Find XY overlap regions between pours on different layers
//! 3. **Via Stamping**: Insert vias at overlap centers with appropriate enclosure
//!
//! # Example
//!
//! ```text
//! Net "VDD":
//!   - Pour1 on z:1 (Metal1) at [0mm, 0mm] to [5mm, 5mm]
//!   - Pour2 on z:2 (Metal2) at [3mm, 3mm] to [8mm, 8mm]
//!   
//! Overlap detected: [3mm, 3mm] to [5mm, 5mm]
//! Auto-insert via at center: [4mm, 4mm] spanning z:1 to z:2
//! ```

use compact_str::CompactString;
use hwc_engine::{geometry::BoundingBox, HardwareSpace, PourMetadata};
use hwc_parser::{ContactPlacement, Coordinate, Expression, Span, Unit};
use rustc_hash::FxHashMap;

/// Via type definition for standard via library.
///
/// Defines the properties of a via that connects two specific layers.
#[derive(Debug, Clone)]
pub struct ViaType {
    /// Via name (e.g., "Via12" for Metal1-to-Metal2)
    pub name: CompactString,

    /// Material to fill the via (e.g., "Copper", "Tungsten")
    pub material: CompactString,

    /// Starting layer index
    pub from_layer: usize,

    /// Ending layer index
    pub to_layer: usize,

    /// Via diameter in mm
    pub diameter_mm: f64,

    /// Minimum enclosure (overlap) required on each layer in mm
    pub min_enclosure_mm: f64,
}

impl ViaType {
    /// Create a standard via type.
    pub fn new(
        name: CompactString,
        material: CompactString,
        from_layer: usize,
        to_layer: usize,
        diameter_mm: f64,
        min_enclosure_mm: f64,
    ) -> Self {
        Self {
            name,
            material,
            from_layer,
            to_layer,
            diameter_mm,
            min_enclosure_mm,
        }
    }
}

/// Standard via library with common via types.
pub struct ViaLibrary {
    vias: Vec<ViaType>,
}

impl ViaLibrary {
    /// Create a new via library with standard via types.
    pub fn new_standard(fabrication: Option<&hwc_engine::constraint_manager::FabricationConstraints>) -> Self {
        let mut vias = Vec::new();

        // v0.1.7: Use fabrication constraints for default via dimensions (Limitation 7)
        let default_dia = fabrication.map(|f| f.default_via_diameter_nm as f64 / 1_000_000.0).unwrap_or(0.3);
        let min_enclosure = fabrication.map(|f| f.min_annular_ring_nm as f64 / 1_000_000.0).unwrap_or(0.15);

        // Standard vias for PCB (Metal1 to Metal2, Metal2 to Metal3, etc.)
        // Using conservative dimensions for manufacturability
        for layer in 1..=10 {
            vias.push(ViaType::new(
                format!("Via{}_{}", layer, layer + 1).into(),
                "Copper".into(),
                layer,
                layer + 1,
                default_dia,
                min_enclosure,
            ));
        }

        // Silicon vias (for IC design) - smaller dimensions
        vias.push(ViaType::new(
            "ContactPoly".into(),
            "Tungsten".into(),
            0,      // Poly layer
            1,      // Metal1
            0.001,  // 1um diameter
            0.0005, // 0.5um enclosure
        ));

        Self { vias }
    }

    /// Find the appropriate via type for a layer pair.
    pub fn find_via_for_layers(&self, from_layer: usize, to_layer: usize) -> Option<&ViaType> {
        // Normalize layer order (always from lower to higher)
        let (start, end) = if from_layer < to_layer {
            (from_layer, to_layer)
        } else {
            (to_layer, from_layer)
        };

        self.vias
            .iter()
            .find(|v| v.from_layer == start && v.to_layer == end)
    }
}

/// Layer transition information for a net.
#[derive(Debug, Clone)]
struct LayerTransition {
    net_name: CompactString,
    from_layer: usize,
    to_layer: usize,
    /// Physical Z (nm) at the bottom of the lower pour (via start)
    from_z_nm: i64,
    /// Physical Z (nm) at the top of the upper pour (via end)
    to_z_nm: i64,
    from_pour: CompactString,
    to_pour: CompactString,
    from_material: CompactString,
    to_material: CompactString,
    from_bbox: BoundingBox,
    to_bbox: BoundingBox,
}

/// Overlap region between two pours on different layers.
#[derive(Debug, Clone)]
struct OverlapRegion {
    bbox: BoundingBox,
    center_x_nm: i64,
    center_y_nm: i64,
}

/// Via array configuration for high-current nets.
#[derive(Debug, Clone)]
struct ViaArrayConfig {
    /// Number of vias in X direction
    cols: usize,
    /// Number of vias in Y direction
    rows: usize,
    /// Spacing between via centers in X direction (nm)
    pitch_x_nm: i64,
    /// Spacing between via centers in Y direction (nm)
    pitch_y_nm: i64,
    /// Starting X position for array (nm)
    start_x_nm: i64,
    /// Starting Y position for array (nm)
    start_y_nm: i64,
}

/// Automatic via inserter.
pub struct AutoViaInserter {
    via_library: ViaLibrary,
}

impl AutoViaInserter {
    /// Create a new automatic via inserter with standard via library.
    pub fn new() -> Self {
        Self {
            via_library: ViaLibrary::new_standard(None),
        }
    }

    /// Create a new automatic via inserter with fabrication constraints.
    pub fn new_with_constraints(fabrication: Option<&hwc_engine::constraint_manager::FabricationConstraints>) -> Self {
        Self {
            via_library: ViaLibrary::new_standard(fabrication),
        }
    }

    /// Insert vias automatically for all nets that transition between layers.
    ///
    /// This is the main entry point for automatic via insertion.
    /// It scans all pours, detects layer transitions, finds overlaps,
    /// and inserts vias (single or arrays) based on net classification.
    ///
    /// **Constraint-Driven Logic:**
    /// - Power/Ground nets with `min_spacing` → Via arrays (maximize conductivity)
    /// - Signal nets → Single via (minimize capacitance)
    ///
    /// # Arguments
    ///
    /// * `space` - The hardware space containing pours and contacts
    /// * `profile` - Optional profile definition with via spacing constraints
    ///
    /// # Returns
    ///
    /// Vector of ContactPlacement objects representing the auto-inserted vias
    pub fn insert_vias(
        &self,
        space: &HardwareSpace,
        profile: Option<&hwc_parser::ProfileDefinition>,
    ) -> Result<Vec<ContactPlacement>, String> {
        let mut inserted_vias = Vec::new();

        // Step 1: Group pours by net name
        let pours_by_net = self.group_pours_by_net(&space.pours);

        println!("\n🔌 Auto Via Insertion:");
        println!(
            "   ├─ Analyzing {} nets for layer transitions...",
            pours_by_net.len()
        );

        // Step 2: For each net, find layer transitions
        for (net_name, pours) in &pours_by_net {
            let transitions =
                self.find_layer_transitions(net_name, pours, space.voxel_size.z_nm);

            if transitions.is_empty() {
                continue;
            }

            // Determine net classification from space
            let is_power_or_ground = space
                .net_classifications
                .get(net_name.as_str())
                .map(|classification| matches!(classification, hwc_engine::space::NetClassification::Power | hwc_engine::space::NetClassification::Ground))
                .unwrap_or(false);

            println!(
                "   ├─ Net '{}' ({}): {} layer transition(s) detected",
                net_name,
                if is_power_or_ground { "power/ground" } else { "signal" },
                transitions.len()
            );

            // Step 3: For each transition, find overlap and insert via(s)
            for transition in transitions {
                match self.process_transition(&transition, profile, is_power_or_ground) {
                    Ok(vias) => {
                        if vias.len() > 1 {
                            println!(
                                "   │  ├─ Auto-inserted {} vias in array for power distribution",
                                vias.len()
                            );
                        } else if vias.len() == 1 {
                            println!(
                                "   │  ├─ Auto-inserted single via at ({:.3}mm, {:.3}mm) spanning z {:.3}mm to {:.3}mm",
                                self.coord_to_mm(&vias[0].position),
                                self.coord_to_mm(&vias[0].position),
                                transition.from_z_nm as f64 / 1_000_000.0,
                                transition.to_z_nm as f64 / 1_000_000.0,
                            );
                        }
                        inserted_vias.extend(vias);
                    }
                    Err(e) => {
                        println!(
                            "   │  ├─ ⚠️  Could not insert via for transition {} → {}: {}",
                            transition.from_pour, transition.to_pour, e
                        );
                    }
                }
            }
        }

        println!("   └─ Total vias inserted: {}", inserted_vias.len());

        Ok(inserted_vias)
    }

    /// Group pours by their net name.
    fn group_pours_by_net<'a>(
        &self,
        pours: &'a [PourMetadata],
    ) -> FxHashMap<CompactString, Vec<&'a PourMetadata>> {
        let mut by_net: FxHashMap<CompactString, Vec<&PourMetadata>> = FxHashMap::default();

        for pour in pours {
            if let Some(net_name) = &pour.net {
                by_net.entry(net_name.clone()).or_default().push(pour);
            }
        }

        by_net
    }

    /// Find all layer transitions for a net.
    ///
    /// A layer transition occurs when two pours on the same net are on different Z-layers.
    fn find_layer_transitions(
        &self,
        net_name: &str,
        pours: &[&PourMetadata],
        voxel_z_nm: i64,
    ) -> Vec<LayerTransition> {
        let mut transitions = Vec::new();

        // Compare each pair of pours
        for i in 0..pours.len() {
            for j in (i + 1)..pours.len() {
                let pour1 = pours[i];
                let pour2 = pours[j];

                // Check if they're on different Z elevations
                if pour1.z_bottom_nm != pour2.z_bottom_nm {
                    // Check if both have bounding boxes
                    if let (Some(bbox1), Some(bbox2)) = (&pour1.bbox, &pour2.bbox) {
                        let (lower_bbox, upper_bbox, lower_pour, upper_pour, lower_mat, upper_mat) =
                            if pour1.z_bottom_nm < pour2.z_bottom_nm {
                                (bbox1, bbox2, &pour1.name, &pour2.name, &pour1.material_name, &pour2.material_name)
                            } else {
                                (bbox2, bbox1, &pour2.name, &pour1.name, &pour2.material_name, &pour1.material_name)
                            };
                        let voxel_z = voxel_z_nm.max(1);
                        let from_layer = (lower_bbox.min.z / voxel_z) as usize;
                        let to_layer = (upper_bbox.max.z / voxel_z) as usize;
                        transitions.push(LayerTransition {
                            net_name: net_name.to_string().into(),
                            from_layer,
                            to_layer,
                            from_z_nm: lower_bbox.min.z,
                            to_z_nm: upper_bbox.max.z,
                            from_pour: lower_pour.clone(),
                            to_pour: upper_pour.clone(),
                            from_material: lower_mat.clone(),
                            to_material: upper_mat.clone(),
                            from_bbox: *lower_bbox,
                            to_bbox: *upper_bbox,
                        });
                    }
                }
            }
        }

        transitions
    }

    /// Process a single layer transition and insert via(s) based on net classification.
    ///
    /// **Constraint-Driven Decision:**
    /// - Power/Ground + min_spacing → Via array (fill overlap area)
    /// - Signal or no spacing → Single via (center of overlap)
    fn process_transition(
        &self,
        transition: &LayerTransition,
        profile: Option<&hwc_parser::ProfileDefinition>,
        is_power_or_ground: bool,
    ) -> Result<Vec<ContactPlacement>, String> {
        // Step 1: Find overlap region
        let overlap = self.find_overlap(&transition.from_bbox, &transition.to_bbox)?;

        // Step 2: Validate the transition/stack
        self.validate_via_stack(transition, &overlap)?;

        // Step 3: Determine if we should use via arrays
        // Logic: Power/Ground nets with min_spacing constraint get arrays
        let use_array = is_power_or_ground
            && profile
                .and_then(|p| p.via.as_ref())
                .and_then(|v| v.min_spacing.as_ref())
                .is_some();

        let profile_bridge_table = profile.map(crate::bridge_resolver::BridgeTable::from_profile);
        let bridge_stack = crate::bridge_resolver::resolve_bridge(
            &transition.from_material,
            &transition.to_material,
            profile_bridge_table.as_ref(),
            None, // stdlib_table
            None, // explicit override
        ).map_err(|e| e.to_string())?;

        // Step 4: Insert vias (single, stack, or array)
        if transition.to_layer - transition.from_layer > 1 {
            // Multi-layer via stack
            if use_array {
                self.insert_via_stack_array(transition, &overlap, profile, &bridge_stack)
            } else {
                self.insert_via_stack(transition, &overlap, &bridge_stack)
            }
        } else {
            // Single layer transition
            if use_array {
                self.insert_via_array(transition, &overlap, profile, &bridge_stack)
            } else {
                let via_type = self
                    .via_library
                    .find_via_for_layers(transition.from_layer, transition.to_layer)
                    .ok_or_else(|| {
                        format!(
                            "No via type found for layers {} to {}",
                            transition.from_layer, transition.to_layer
                        )
                    })?;

                Ok(vec![self.create_via_placement(
                    transition,
                    &overlap,
                    via_type,
                    transition.from_layer,
                    transition.to_layer,
                    &bridge_stack,
                )])
            }
        }
    }

    /// Validate a via stack before insertion.
    fn validate_via_stack(
        &self,
        transition: &LayerTransition,
        overlap: &OverlapRegion,
    ) -> Result<(), String> {
        let from = transition.from_layer;
        let to = transition.to_layer;

        for layer in from..to {
            let via_type = self
                .via_library
                .find_via_for_layers(layer, layer + 1)
                .ok_or_else(|| {
                    format!(
                        "Via stack validation failed: No via type found to connect z:{} to z:{}",
                        layer,
                        layer + 1
                    )
                })?;

            // Check enclosure requirements for this layer transition
            self.verify_enclosure(overlap, via_type).map_err(|e| {
                format!("Via stack enclosure error at z:{}->z:{}: {}", layer, layer + 1, e)
            })?;
        }

        Ok(())
    }

    /// Insert a stack of vias for transitions spanning multiple layers.
    /// Assumes validation has already passed.
    fn insert_via_stack(
        &self,
        transition: &LayerTransition,
        overlap: &OverlapRegion,
        bridge_stack: &crate::bridge_resolver::BridgeStack,
    ) -> Result<Vec<ContactPlacement>, String> {
        let mut stack = Vec::new();
        let from = transition.from_layer;
        let to = transition.to_layer;

        for layer in from..to {
            let via_type = self
                .via_library
                .find_via_for_layers(layer, layer + 1)
                .expect("Via type should have been validated");

            stack.push(self.create_via_placement(
                transition,
                overlap,
                via_type,
                layer,
                layer + 1,
                bridge_stack,
            ));
        }

        Ok(stack)
    }

    /// Insert a via array for a single layer transition.
    /// Used for high-current nets like power distribution.
    fn insert_via_array(
        &self,
        transition: &LayerTransition,
        overlap: &OverlapRegion,
        profile: Option<&hwc_parser::ProfileDefinition>,
        bridge_stack: &crate::bridge_resolver::BridgeStack,
    ) -> Result<Vec<ContactPlacement>, String> {
        let via_type = self
            .via_library
            .find_via_for_layers(transition.from_layer, transition.to_layer)
            .ok_or_else(|| {
                format!(
                    "No via type found for layers {} to {}",
                    transition.from_layer, transition.to_layer
                )
            })?;

        let array_config = self.calculate_via_array(overlap, via_type, profile)?;

        let mut vias = Vec::new();
        for row in 0..array_config.rows {
            for col in 0..array_config.cols {
                let x_nm = array_config.start_x_nm + (col as i64 * array_config.pitch_x_nm);
                let y_nm = array_config.start_y_nm + (row as i64 * array_config.pitch_y_nm);
                
                vias.push(self.create_via_placement_at(
                    transition,
                    via_type,
                    transition.from_layer,
                    transition.to_layer,
                    x_nm,
                    y_nm,
                    row,
                    col,
                    bridge_stack,
                ));
            }
        }

        Ok(vias)
    }

    /// Insert via stack arrays for multi-layer transitions.
    fn insert_via_stack_array(
        &self,
        transition: &LayerTransition,
        overlap: &OverlapRegion,
        profile: Option<&hwc_parser::ProfileDefinition>,
        bridge_stack: &crate::bridge_resolver::BridgeStack,
    ) -> Result<Vec<ContactPlacement>, String> {
        let mut all_vias = Vec::new();
        let from = transition.from_layer;
        let to = transition.to_layer;

        for layer in from..to {
            let via_type = self
                .via_library
                .find_via_for_layers(layer, layer + 1)
                .expect("Via type should have been validated");

            let array_config = self.calculate_via_array(overlap, via_type, profile)?;

            for row in 0..array_config.rows {
                for col in 0..array_config.cols {
                    let x_nm = array_config.start_x_nm + (col as i64 * array_config.pitch_x_nm);
                    let y_nm = array_config.start_y_nm + (row as i64 * array_config.pitch_y_nm);
                    
                    all_vias.push(self.create_via_placement_at(
                        transition,
                        via_type,
                        layer,
                        layer + 1,
                        x_nm,
                        y_nm,
                        row,
                        col,
                        bridge_stack,
                    ));
                }
            }
        }

        Ok(all_vias)
    }

    /// Calculate via array configuration based on overlap size and profile constraints.
    fn calculate_via_array(
        &self,
        overlap: &OverlapRegion,
        via_type: &ViaType,
        profile: Option<&hwc_parser::ProfileDefinition>,
    ) -> Result<ViaArrayConfig, String> {
        // Get spacing from profile or use default
        let spacing_mm = profile
            .and_then(|p| p.via.as_ref())
            .and_then(|v| v.min_spacing.as_ref())
            .map(|m| m.value)
            .unwrap_or(via_type.diameter_mm * 2.0); // Default: 2x diameter

        let spacing_nm = (spacing_mm * 1_000_000.0) as i64;

        // Calculate available space
        let overlap_width_nm = overlap.bbox.max.x - overlap.bbox.min.x;
        let overlap_height_nm = overlap.bbox.max.y - overlap.bbox.min.y;

        // Calculate how many vias fit
        let cols = ((overlap_width_nm as f64 / spacing_nm as f64).floor() as usize).max(1);
        let rows = ((overlap_height_nm as f64 / spacing_nm as f64).floor() as usize).max(1);

        // Calculate starting position to center the array
        let total_width_nm = (cols - 1) as i64 * spacing_nm;
        let total_height_nm = (rows - 1) as i64 * spacing_nm;
        
        let start_x_nm = overlap.center_x_nm - total_width_nm / 2;
        let start_y_nm = overlap.center_y_nm - total_height_nm / 2;

        Ok(ViaArrayConfig {
            cols,
            rows,
            pitch_x_nm: spacing_nm,
            pitch_y_nm: spacing_nm,
            start_x_nm,
            start_y_nm,
        })
    }

    /// Create a ContactPlacement for a specific via at exact coordinates.
    fn create_via_placement_at(
        &self,
        transition: &LayerTransition,
        via_type: &ViaType,
        _from_layer: usize,
        _to_layer: usize,
        x_nm: i64,
        y_nm: i64,
        row: usize,
        col: usize,
        bridge_stack: &crate::bridge_resolver::BridgeStack,
    ) -> ContactPlacement {
        let via_name = if row == 0 && col == 0 {
            format!(
                "AutoVia_{}_{}_{}",
                transition.net_name, transition.from_layer, transition.to_layer
            )
        } else {
            format!(
                "AutoVia_{}_{}_{}_r{}c{}",
                transition.net_name, transition.from_layer, transition.to_layer, row, col
            )
        };

        // Create a dummy span for auto-generated vias
        let span = Span::new(0, 0);

        // Convert nanometers to millimeters for the coordinate
        let x_mm = x_nm as f64 / 1_000_000.0;
        let y_mm = y_nm as f64 / 1_000_000.0;
        let z_mm = transition.from_z_nm as f64 / 1_000_000.0;

        ContactPlacement {
            material: bridge_stack.fill_material.clone(),
            name: Some(hwc_parser::ComponentName::simple(via_name.into(), span)),
            position: Coordinate::Declarative {
                x: Expression::Measurement {
                    value: x_mm,
                    unit: Unit::Millimeter,
                    span,
                },
                y: Expression::Measurement {
                    value: y_mm,
                    unit: Unit::Millimeter,
                    span,
                },
                z: Expression::Measurement {
                    value: z_mm,
                    unit: Unit::Millimeter,
                    span,
                },
                span,
            },
            from_elevation: crate::ir::stackup_manager::StackupManager::elevation_from_z_nm(
                transition.from_z_nm,
                span,
            ),
            to_elevation: crate::ir::stackup_manager::StackupManager::elevation_from_z_nm(
                transition.to_z_nm,
                span,
            ),
            net: Some(hwc_parser::NetName::simple(
                transition.net_name.clone(),
                span,
            )),
            diameter: Some(hwc_parser::Measurement {
                value: via_type.diameter_mm,
                unit: hwc_parser::Unit::Millimeter,
                span,
            }),
            annular_ring: None,
            caps: None,
            bridge: if bridge_stack.interface_material == bridge_stack.fill_material {
                None
            } else {
                Some(bridge_stack.interface_material.clone())
            },
            liner: None,
            liner_thickness: None,
            koz: None,
            span,
        }
    }

    /// Create a ContactPlacement for a specific via in a transition (single via, centered).
    fn create_via_placement(
        &self,
        transition: &LayerTransition,
        overlap: &OverlapRegion,
        via_type: &ViaType,
        from_layer: usize,
        to_layer: usize,
        bridge_stack: &crate::bridge_resolver::BridgeStack,
    ) -> ContactPlacement {
        self.create_via_placement_at(
            transition,
            via_type,
            from_layer,
            to_layer,
            overlap.center_x_nm,
            overlap.center_y_nm,
            0,
            0,
            bridge_stack,
        )
    }

    /// Find the XY overlap region between two bounding boxes.
    fn find_overlap(
        &self,
        bbox1: &BoundingBox,
        bbox2: &BoundingBox,
    ) -> Result<OverlapRegion, String> {
        // Calculate overlap in X dimension
        let overlap_min_x = bbox1.min.x.max(bbox2.min.x);
        let overlap_max_x = bbox1.max.x.min(bbox2.max.x);

        // Calculate overlap in Y dimension
        let overlap_min_y = bbox1.min.y.max(bbox2.min.y);
        let overlap_max_y = bbox1.max.y.min(bbox2.max.y);

        // Check if there's actual overlap
        if overlap_min_x >= overlap_max_x || overlap_min_y >= overlap_max_y {
            return Err("No XY overlap between pours".into());
        }

        // Calculate center point
        let center_x_nm = (overlap_min_x + overlap_max_x) / 2;
        let center_y_nm = (overlap_min_y + overlap_max_y) / 2;

        Ok(OverlapRegion {
            bbox: BoundingBox::new(
                hwc_engine::geometry::Point3D::new(overlap_min_x, overlap_min_y, 0),
                hwc_engine::geometry::Point3D::new(overlap_max_x, overlap_max_y, 0),
            ),
            center_x_nm,
            center_y_nm,
        })
    }

    /// Verify that the overlap region provides sufficient enclosure for the via.
    fn verify_enclosure(&self, overlap: &OverlapRegion, via_type: &ViaType) -> Result<(), String> {
        let overlap_width_nm = overlap.bbox.max.x - overlap.bbox.min.x;
        let overlap_height_nm = overlap.bbox.max.y - overlap.bbox.min.y;

        let required_size_nm =
            ((via_type.diameter_mm + 2.0 * via_type.min_enclosure_mm) * 1_000_000.0) as i64;

        if overlap_width_nm < required_size_nm || overlap_height_nm < required_size_nm {
            return Err(format!(
                "Overlap region too small for via. Required: {:.3}mm, Available: {:.3}mm x {:.3}mm",
                required_size_nm as f64 / 1_000_000.0,
                overlap_width_nm as f64 / 1_000_000.0,
                overlap_height_nm as f64 / 1_000_000.0
            ));
        }

        Ok(())
    }

    /// Helper to convert Coordinate to mm for display.
    fn coord_to_mm(&self, coord: &Coordinate) -> f64 {
        match coord {
            Coordinate::Declarative { x, .. } => match x {
                Expression::Measurement { value, .. } => *value,
                Expression::Literal { value, .. } => *value as f64,
                _ => 0.0,
            },
            _ => 0.0,
        }
    }
}

impl Default for AutoViaInserter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use hwc_engine::geometry::Point3D;

    #[test]
    fn test_via_library_creation() {
        let lib = ViaLibrary::new_standard(None);
        assert!(!lib.vias.is_empty());

        // Check that we have a via for Metal1-to-Metal2
        let via = lib.find_via_for_layers(1, 2);
        assert!(via.is_some());
        assert_eq!(via.unwrap().name, "Via1_2");
    }

    #[test]
    fn test_find_overlap_success() {
        let inserter = AutoViaInserter::new();

        let bbox1 = BoundingBox::new(
            Point3D::new(0, 0, 0),
            Point3D::new(5_000_000, 5_000_000, 1_000_000), // 5mm x 5mm
        );

        let bbox2 = BoundingBox::new(
            Point3D::new(3_000_000, 3_000_000, 1_000_000),
            Point3D::new(8_000_000, 8_000_000, 2_000_000), // 5mm x 5mm, offset
        );

        let overlap = inserter.find_overlap(&bbox1, &bbox2).unwrap();

        // Overlap should be [3mm, 3mm] to [5mm, 5mm]
        assert_eq!(overlap.bbox.min.x, 3_000_000);
        assert_eq!(overlap.bbox.min.y, 3_000_000);
        assert_eq!(overlap.bbox.max.x, 5_000_000);
        assert_eq!(overlap.bbox.max.y, 5_000_000);

        // Center should be at [4mm, 4mm]
        assert_eq!(overlap.center_x_nm, 4_000_000);
        assert_eq!(overlap.center_y_nm, 4_000_000);
    }

    #[test]
    fn test_find_overlap_no_overlap() {
        let inserter = AutoViaInserter::new();

        let bbox1 = BoundingBox::new(
            Point3D::new(0, 0, 0),
            Point3D::new(2_000_000, 2_000_000, 1_000_000),
        );

        let bbox2 = BoundingBox::new(
            Point3D::new(5_000_000, 5_000_000, 1_000_000),
            Point3D::new(8_000_000, 8_000_000, 2_000_000),
        );

        let result = inserter.find_overlap(&bbox1, &bbox2);
        assert!(result.is_err());
    }

    #[test]
    fn test_verify_enclosure_sufficient() {
        let inserter = AutoViaInserter::new();

        let overlap = OverlapRegion {
            bbox: BoundingBox::new(
                Point3D::new(0, 0, 0),
                Point3D::new(2_000_000, 2_000_000, 0), // 2mm x 2mm
            ),
            center_x_nm: 1_000_000,
            center_y_nm: 1_000_000,
        };

        let via_type = ViaType::new(
            "TestVia".into(),
            "Copper".into(),
            1,
            2,
            0.3,  // 300um diameter
            0.15, // 150um enclosure
        );

        // 2mm overlap is sufficient for 0.3mm via + 2*0.15mm enclosure = 0.6mm total
        let result = inserter.verify_enclosure(&overlap, &via_type);
        assert!(result.is_ok());
    }

    #[test]
    fn test_verify_enclosure_insufficient() {
        let inserter = AutoViaInserter::new();

        let overlap = OverlapRegion {
            bbox: BoundingBox::new(
                Point3D::new(0, 0, 0),
                Point3D::new(400_000, 400_000, 0), // 0.4mm x 0.4mm (too small)
            ),
            center_x_nm: 200_000,
            center_y_nm: 200_000,
        };

        let via_type = ViaType::new(
            "TestVia".into(),
            "Copper".into(),
            1,
            2,
            0.3,  // 300um diameter
            0.15, // 150um enclosure
        );

        // 0.4mm overlap is insufficient for 0.6mm required
        let result = inserter.verify_enclosure(&overlap, &via_type);
        assert!(result.is_err());
    }

    #[test]
    fn test_insert_via_stack() {
        let inserter = AutoViaInserter::new();

        let transition = LayerTransition {
            net_name: "VDD".into(),
            from_layer: 1,
            to_layer: 5,
            from_z_nm: 1_000_000,
            to_z_nm: 6_000_000,
            from_pour: "P1".into(),
            to_pour: "P2".into(),
            from_material: "Copper".into(),
            to_material: "Copper".into(),
            from_bbox: BoundingBox::new(Point3D::new(0, 0, 0), Point3D::new(5_000_000, 5_000_000, 0)),
            to_bbox: BoundingBox::new(Point3D::new(0, 0, 0), Point3D::new(5_000_000, 5_000_000, 0)),
        };

        let result = inserter.process_transition(&transition, None, false).unwrap();

        // Should have 4 vias: 1-2, 2-3, 3-4, 4-5
        assert_eq!(result.len(), 4);
        assert!(matches!(
            result[0].from_elevation,
            hwc_parser::Elevation::Physical { .. }
        ));
        assert!(matches!(
            result[0].to_elevation,
            hwc_parser::Elevation::Physical { .. }
        ));

        for via in &result {
            assert_eq!(via.net.as_ref().unwrap().to_string(), "VDD");
        }
    }
}