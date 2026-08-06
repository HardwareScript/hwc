//! Parameter Extraction Registry - Extensible Device Parameter Calculation
//!
//! This module provides a registry-based system for calculating physical parameters
//! from device geometry. Instead of hardcoding device types, extraction functions
//! are registered dynamically, following Hardware Script's data-driven philosophy.
//!
//! # Architecture
//!
//! 1. **Extraction Functions**: Pure functions that calculate parameters from geometry + materials
//! 2. **Registry**: Maps device names/patterns to extraction functions
//! 3. **Fallback**: Returns empty parameters if no extractor is registered
//!
//! # Example Usage
//!
//! ```rust
//! let mut registry = ParameterExtractionRegistry::new();
//! registry.register_standard_extractors();
//! 
//! let params = registry.extract("Capacitor", &terminal_pours, space)?;
//! // Returns: {"C": 0.35e-12}
//! ```

use compact_str::CompactString;
use hwc_engine::space::PourMetadata;
use hwc_engine::HardwareSpace;
use hwc_physics::geometry::BoundingBox;
use rustc_hash::FxHashMap;

/// Parameter extraction function signature
///
/// Takes terminal geometry and space context, returns calculated parameters.
pub type ExtractionFn = fn(
    &FxHashMap<CompactString, PourMetadata>,
    &HardwareSpace,
) -> Result<FxHashMap<CompactString, f64>, String>;

/// Registry for device parameter extraction functions
///
/// Maps device type names to extraction functions. This allows the compiler
/// to support new device types without modifying core extraction logic.
pub struct ParameterExtractionRegistry {
    /// Map from device type name to extraction function
    extractors: FxHashMap<String, ExtractionFn>,
}

impl ParameterExtractionRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            extractors: FxHashMap::default(),
        }
    }

    /// Register a parameter extraction function for a device type
    ///
    /// # Arguments
    /// * `device_type` - Device type name (e.g., "Capacitor", "Resistor")
    /// * `extractor` - Function that calculates parameters from geometry
    pub fn register(&mut self, device_type: impl Into<String>, extractor: ExtractionFn) {
        self.extractors.insert(device_type.into(), extractor);
    }

    /// Register all standard extraction functions
    ///
    /// This includes built-in support for common passive components:
    /// - Resistor: R = ρ(L/A)
    /// - Capacitor: C = ε₀εᵣ(A/d)
    pub fn register_standard_extractors(&mut self) {
        self.register("Resistor", extract_resistor_parameters);
        self.register("PolyResistor", extract_resistor_parameters);
        self.register("Capacitor", extract_capacitor_parameters);
    }

    /// Extract parameters for a device
    ///
    /// # Arguments
    /// * `device_type` - Device type name
    /// * `terminal_pours` - Geometry for each terminal
    /// * `space` - Hardware space context (for material properties)
    ///
    /// # Returns
    /// * `Ok(params)` - Successfully calculated parameters
    /// * `Err(msg)` - Extraction failed (geometry invalid, missing materials, etc.)
    ///
    /// If no extractor is registered for this device type, returns empty parameters.
    pub fn extract(
        &self,
        device_type: &str,
        terminal_pours: &FxHashMap<CompactString, PourMetadata>,
        space: &HardwareSpace,
    ) -> Result<FxHashMap<CompactString, f64>, String> {
        if let Some(extractor) = self.extractors.get(device_type) {
            extractor(terminal_pours, space)
        } else {
            // No extractor registered - return empty parameters
            // This allows devices without extraction to still be instantiated
            Ok(FxHashMap::default())
        }
    }
}

impl Default for ParameterExtractionRegistry {
    fn default() -> Self {
        let mut registry = Self::new();
        registry.register_standard_extractors();
        registry
    }
}

// ============================================================================
// Standard Extraction Functions
// ============================================================================

/// Extract capacitance from parallel-plate geometry: C = ε₀εᵣ(A/d)
fn extract_capacitor_parameters(
    terminal_pours: &FxHashMap<CompactString, PourMetadata>,
    space: &HardwareSpace,
) -> Result<FxHashMap<CompactString, f64>, String> {
    // Capacitor requires exactly 2 terminals
    if terminal_pours.len() != 2 {
        return Err(format!(
            "Capacitor must have exactly 2 terminals, found {}",
            terminal_pours.len()
        ));
    }

    let pours: Vec<_> = terminal_pours.values().collect();
    let pour1 = pours[0];
    let pour2 = pours[1];

    // Get bounding boxes
    let bbox1 = pour1
        .bbox
        .as_ref()
        .ok_or_else(|| "Terminal 1 has no bounding box".to_string())?;
    let bbox2 = pour2
        .bbox
        .as_ref()
        .ok_or_else(|| "Terminal 2 has no bounding box".to_string())?;

    // Calculate overlap area
    let overlap_area_nm2 = calculate_overlap_area(bbox1, bbox2)?;

    // Get Z positions and calculate actual dielectric thickness
    let z1 = pour1.z_bottom_nm;
    let z2 = pour2.z_bottom_nm;
    let z_min = z1.min(z2);
    let z_max = z1.max(z2);

    // Calculate dielectric thickness by summing only insulator layers between plates
    // Bug fix: Previous code used (z2 - z1) which included conductor layer thickness
    // Correct approach: Sum only the dielectric (insulator) layers in the Z range
    let mut dielectric_thickness_nm = 0.0;
    let mut dielectric_epsilon_r = 0.0;
    let mut found_dielectric = false;

    for layer in &space.stackup_layers {
        // Check if layer overlaps with the Z range between plates
        let layer_overlaps = layer.z_bottom < z_max && layer.z_top > z_min;
        
        if layer_overlaps {
            // Get material ID
            if let Some(material_id) = space.material_registry.get_id(&layer.material_name) {
                // Only count insulator layers for dielectric thickness
                if space.material_registry.is_insulator(material_id) {
                    // Calculate the overlapping portion of this layer
                    let overlap_start = layer.z_bottom.max(z_min);
                    let overlap_end = layer.z_top.min(z_max);
                    let overlap_thickness = (overlap_end - overlap_start) as f64;
                    
                    if overlap_thickness > 0.0 {
                        dielectric_thickness_nm += overlap_thickness;
                        
                        // Get permittivity for this dielectric layer
                        if let Some(props) = space.material_registry.get_physical_props(material_id) {
                            if let Some(epsilon_r) = props.get("relative_permittivity") {
                                // For multiple dielectric layers, use the first one found
                                // TODO: Implement series capacitance for multi-layer dielectrics
                                if !found_dielectric {
                                    dielectric_epsilon_r = epsilon_r;
                                    found_dielectric = true;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if dielectric_thickness_nm < 1.0 {
        return Err(format!(
            "No dielectric layer found between capacitor plates (Z={}nm to Z={}nm). \
             Capacitor requires an insulator material between conductor plates.",
            z_min, z_max
        ));
    }

    if !found_dielectric {
        return Err(format!(
            "Dielectric material between plates (Z={}nm to Z={}nm) missing 'relative_permittivity' property. \
             Add 'relative_permittivity: <value>' to the insulator material definition.",
            z_min, z_max
        ));
    }

    // Physics: C = ε₀ * εᵣ * (A / d)
    const EPSILON_0: f64 = 8.854e-12; // F/m (vacuum permittivity)

    // Convert units
    let area_m2 = overlap_area_nm2 * 1e-18; // nm² to m²
    let thickness_m = dielectric_thickness_nm * 1e-9; // nm to m

    let capacitance_f = EPSILON_0 * dielectric_epsilon_r * (area_m2 / thickness_m);

    let mut params = FxHashMap::default();
    params.insert("C".into(), capacitance_f);

    println!(
        "      ├─ C={:.2e}F (εᵣ={:.1}, A={:.0}nm², d={:.0}nm)",
        capacitance_f, dielectric_epsilon_r, overlap_area_nm2, dielectric_thickness_nm
    );

    Ok(params)
}

/// Extract resistance from rectangular geometry: R = ρ(L/A)
fn extract_resistor_parameters(
    terminal_pours: &FxHashMap<CompactString, PourMetadata>,
    space: &HardwareSpace,
) -> Result<FxHashMap<CompactString, f64>, String> {
    // Resistor requires exactly 2 terminals
    if terminal_pours.len() != 2 {
        return Err(format!(
            "Resistor must have exactly 2 terminals, found {}",
            terminal_pours.len()
        ));
    }

    let pours: Vec<_> = terminal_pours.values().collect();
    let pour1 = pours[0];
    let pour2 = pours[1];

    // Get material properties
    let material_id = space
        .material_registry
        .get_id(&pour1.material_name)
        .ok_or_else(|| format!("Material '{}' not found in registry", pour1.material_name))?;

    let material_props = space
        .material_registry
        .get_physical_props(material_id)
        .ok_or_else(|| {
            format!(
                "Material '{}' has no physical properties defined",
                pour1.material_name
            )
        })?;

    // Get required properties (strict - no defaults!)
    let resistivity = material_props
        .get("resistivity")
        .ok_or_else(|| {
            format!(
                "Material '{}' missing 'resistivity' property required for resistance calculation",
                pour1.material_name
            )
        })?;

    let thickness_nm = material_props
        .get("thickness")
        .ok_or_else(|| {
            format!(
                "Material '{}' missing 'thickness' property required for resistance calculation",
                pour1.material_name
            )
        })?;

    // Get bounding boxes
    let bbox1 = pour1
        .bbox
        .as_ref()
        .ok_or_else(|| "Terminal 1 has no bounding box".to_string())?;
    let bbox2 = pour2
        .bbox
        .as_ref()
        .ok_or_else(|| "Terminal 2 has no bounding box".to_string())?;

    // Calculate geometry
    let length_nm = calculate_resistor_length(bbox1, bbox2);  // Use edge-to-edge span, not center-to-center
    let width_nm = calculate_resistor_width(bbox1, bbox2);
    let cross_section_nm2 = width_nm * thickness_nm;

    // Convert to meters for physics calculation
    let length_m = length_nm * 1e-9;
    let cross_section_m2 = cross_section_nm2 * 1e-18;

    // Physics: R = ρ * (L / A)
    let resistance_ohms = resistivity * (length_m / cross_section_m2);

    let mut params = FxHashMap::default();
    params.insert("R".into(), resistance_ohms);
    params.insert("W".into(), width_nm / 1000.0); // Store in micrometers
    params.insert("L".into(), length_nm / 1000.0); // Store in micrometers

    println!(
        "      ├─ R={:.2}Ω (ρ={:.2e}Ω·m, L={:.1}um, W={:.1}um, t={:.0}nm)",
        resistance_ohms,
        resistivity,
        length_nm / 1000.0,
        width_nm / 1000.0,
        thickness_nm
    );

    Ok(params)
}

// ============================================================================
// Geometry Helper Functions
// ============================================================================

/// Calculate 2D overlap area between two bounding boxes
fn calculate_overlap_area(bbox1: &BoundingBox, bbox2: &BoundingBox) -> Result<f64, String> {
    // Find intersection rectangle
    let x_min = bbox1.min.x.max(bbox2.min.x);
    let x_max = bbox1.max.x.min(bbox2.max.x);
    let y_min = bbox1.min.y.max(bbox2.min.y);
    let y_max = bbox1.max.y.min(bbox2.max.y);

    if x_min >= x_max || y_min >= y_max {
        return Err("No overlap between device terminals".to_string());
    }

    let width = (x_max - x_min) as f64;
    let height = (y_max - y_min) as f64;

    Ok(width * height)
}

/// Calculate resistor body length (edge-to-edge span including both terminals)
///
/// For resistors, the length is the total span of the resistor body, not just
/// the distance between terminal centers. This function calculates the full
/// extent from the leftmost/topmost edge to the rightmost/bottommost edge.
fn calculate_resistor_length(bbox1: &BoundingBox, bbox2: &BoundingBox) -> f64 {
    // Find the overall bounding box that encompasses both terminals
    let min_x = bbox1.min.x.min(bbox2.min.x);
    let max_x = bbox1.max.x.max(bbox2.max.x);
    let min_y = bbox1.min.y.min(bbox2.min.y);
    let max_y = bbox1.max.y.max(bbox2.max.y);

    // Calculate length in primary axis (the longer dimension)
    let length_x = (max_x - min_x) as f64;
    let length_y = (max_y - min_y) as f64;

    // Use the longer dimension as the resistor length
    length_x.max(length_y)
}

/// Calculate resistor width (minimum of the two terminals)
fn calculate_resistor_width(bbox1: &BoundingBox, bbox2: &BoundingBox) -> f64 {
    let width1 = (bbox1.max.y - bbox1.min.y) as f64;
    let width2 = (bbox2.max.y - bbox2.min.y) as f64;
    width1.min(width2)
}

// ============================================================================
// Material Property Lookups
// ============================================================================
// (Functions removed - dielectric extraction now inline in extract_capacitor_parameters)
