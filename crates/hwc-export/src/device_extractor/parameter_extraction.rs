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
///
/// Also extracts ESR (Equivalent Series Resistance) from plate resistivity.
/// ESR = ρ_top × (t_top / A) + ρ_bottom × (t_bottom / A)
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
                        if let Some(props) = space.material_registry.get_physical_props(material_id)
                        {
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

    // Calculate ESR from plate resistivity (Phase 1a)
    // ESR = ρ_top × (t_top / A) + ρ_bottom × (t_bottom / A)
    let esr_top = calculate_plate_resistance(pour1, overlap_area_nm2, space)?;
    let esr_bottom = calculate_plate_resistance(pour2, overlap_area_nm2, space)?;
    let esr_total = esr_top + esr_bottom;

    let mut params = FxHashMap::default();
    params.insert("C".into(), capacitance_f);

    // Only include ESR if non-zero (allows backwards compatibility)
    if esr_total > 0.0 {
        params.insert("ESR".into(), esr_total);
    }

    println!(
        "      ├─ C={:.2e}F (εᵣ={:.1}, A={:.0}nm², d={:.0}nm)",
        capacitance_f, dielectric_epsilon_r, overlap_area_nm2, dielectric_thickness_nm
    );

    if esr_total > 0.0 {
        println!(
            "      ├─ ESR={:.2e}Ω (top={:.2e}Ω, bottom={:.2e}Ω)",
            esr_total, esr_top, esr_bottom
        );
    }

    Ok(params)
}

/// Calculate plate resistance for capacitor ESR
///
/// Plate resistance = ρ × (t / A) where:
/// - ρ = resistivity of plate material
/// - t = plate thickness (from stackup)
/// - A = overlap area
///
/// Returns 0.0 ONLY if resistivity is explicitly not a conductor (insulators have no ESR).
/// Errors if material properties are missing or incomplete.
fn calculate_plate_resistance(
    pour: &PourMetadata,
    overlap_area_nm2: f64,
    space: &HardwareSpace,
) -> Result<f64, String> {
    // Get material properties
    let material_id = space
        .material_registry
        .get_id(&pour.material_name)
        .ok_or_else(|| format!("Material '{}' not found in registry", pour.material_name))?;

    // Check if material is an insulator (insulators have no ESR)
    if space.material_registry.is_insulator(material_id) {
        return Ok(0.0); // Correct physics: insulators have no plate resistance
    }

    // For conductors, resistivity is REQUIRED
    let material_props = space
        .material_registry
        .get_physical_props(material_id)
        .ok_or_else(|| {
            format!(
                "Conductor material '{}' has no physical properties defined.\n\
             Add properties block to material definition with 'resistivity' field.",
                pour.material_name
            )
        })?;

    let resistivity = material_props.get("resistivity").ok_or_else(|| {
        format!(
            "Conductor material '{}' missing REQUIRED 'resistivity' property for ESR calculation.\n\
             \n\
             Add to material definition:\n\
             properties:\n    resistivity: <value>  # Ω·m",
            pour.material_name
        )
    })?;

    // Get plate thickness from stackup layer
    let z_bottom = pour.z_bottom_nm;
    let layer_thickness_nm = space
        .stackup_layers
        .iter()
        .find(|layer| z_bottom >= layer.z_bottom && z_bottom < layer.z_top)
        .map(|layer| (layer.z_top - layer.z_bottom) as f64)
        .ok_or_else(|| {
            format!(
                "Could not find stackup layer for capacitor plate '{}' at Z={}nm",
                pour.name, z_bottom
            )
        })?;

    // Convert to meters
    let thickness_m = layer_thickness_nm * 1e-9;
    let area_m2 = overlap_area_nm2 * 1e-18;

    // ESR = ρ × (t / A)
    let esr = resistivity * (thickness_m / area_m2);

    Ok(esr)
}

/// Extract resistance from rectangular geometry: R = ρ(L/A) + R_0_head + R_0_tail
///
/// Includes contact/interface resistance at head and tail terminals where via meets resistive material.
/// This is critical for IC resistors where contact resistance can be 10-20% of total resistance.
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

    // Get material properties for resistor body
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
    let resistivity = material_props.get("resistivity").ok_or_else(|| {
        format!(
            "Material '{}' missing 'resistivity' property required for resistance calculation",
            pour1.material_name
        )
    })?;

    // Get thickness from stackup layer (NOT from material properties!)
    let z_bottom = pour1.z_bottom_nm;
    let layer_thickness_nm = space
        .stackup_layers
        .iter()
        .find(|layer| z_bottom >= layer.z_bottom && z_bottom < layer.z_top)
        .map(|layer| (layer.z_top - layer.z_bottom) as f64)
        .ok_or_else(|| {
            format!(
                "Could not find stackup layer for pour '{}' at Z={}nm. \
                 Check that the pour's layer is defined in the profile stackup.",
                pour1.name, z_bottom
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

    // Calculate body geometry
    let length_nm = calculate_resistor_length(bbox1, bbox2);
    let width_nm = calculate_resistor_width(bbox1, bbox2);
    let cross_section_nm2 = width_nm * layer_thickness_nm;

    // Convert to meters for physics calculation
    let length_m = length_nm * 1e-9;
    let cross_section_m2 = cross_section_nm2 * 1e-18;

    // Calculate body resistance: R_body = ρ * (L / A)
    let r_body = resistivity * (length_m / cross_section_m2);

    // Calculate contact resistance at head and tail (R_0)
    // R_0 = ρ_interface × (t_interface / A_contact)
    // This is extracted from via contact geometry and interface material properties
    let r0_head = calculate_contact_resistance(pour1, space)?;
    let r0_tail = calculate_contact_resistance(pour2, space)?;

    // Total resistance includes body + both contact resistances
    let r_total = r_body + r0_head + r0_tail;

    let mut params = FxHashMap::default();
    params.insert("R".into(), r_total);
    params.insert("W".into(), width_nm / 1000.0); // Store in micrometers
    params.insert("L".into(), length_nm / 1000.0); // Store in micrometers

    println!(
        "      ├─ R={:.2}Ω (body={:.2}Ω, R0_head={:.2}Ω, R0_tail={:.2}Ω)",
        r_total, r_body, r0_head, r0_tail
    );
    println!(
        "      ├─ Geometry: ρ={:.2e}Ω·m, L={:.1}um, W={:.1}um, t={:.0}nm",
        resistivity,
        length_nm / 1000.0,
        width_nm / 1000.0,
        layer_thickness_nm
    );

    Ok(params)
}

/// Calculate contact/interface resistance for a terminal
///
/// When a via contacts a resistive material, there is interface resistance (R_0) at the contact.
/// This function looks for contacts connected to this pour and calculates R_0 from:
/// - Interface resistivity (from bridge material or silicide interface)
/// - Contact area (from via geometry)
///
/// Returns 0.0 if no contact found or no interface resistance data available.
fn calculate_contact_resistance(pour: &PourMetadata, space: &HardwareSpace) -> Result<f64, String> {
    // Find contacts that connect to this pour
    // A contact is relevant if it overlaps with the pour's bounding box and Z range
    let pour_bbox = pour.bbox.as_ref().ok_or_else(|| {
        format!(
            "Pour '{}' has no bounding box for contact resistance calculation",
            pour.name
        )
    })?;

    let pour_z = pour.z_bottom_nm;

    for contact in &space.contacts {
        // Get contact bbox
        let contact_bbox = match &contact.bbox {
            Some(bbox) => bbox,
            None => continue, // Skip contacts without bounding boxes
        };

        // Check if contact overlaps with this pour in X-Y plane
        let overlaps_xy = !(contact_bbox.min.x >= pour_bbox.max.x
            || contact_bbox.max.x <= pour_bbox.min.x
            || contact_bbox.min.y >= pour_bbox.max.y
            || contact_bbox.max.y <= pour_bbox.min.y);

        // Check if contact connects to this Z layer
        let overlaps_z = contact.z_start_nm <= pour_z && contact.z_end_nm >= pour_z;

        if overlaps_xy && overlaps_z {
            // Found a contact on this terminal
            // Calculate contact area from drill diameter
            if let Some(drill_diameter_nm) = contact.drill_diameter_nm {
                let contact_radius_nm = drill_diameter_nm as f64 / 2.0;
                let contact_area_nm2 = std::f64::consts::PI * contact_radius_nm * contact_radius_nm;
                let contact_area_m2 = contact_area_nm2 * 1e-18;

                // Look for interface/bridge material properties
                // The contact material's resistivity at the interface determines R_0
                let contact_material_id = space
                    .material_registry
                    .get_id(&contact.material_name)
                    .ok_or_else(|| {
                        format!("Contact material '{}' not found", contact.material_name)
                    })?;

                if let Some(contact_props) = space
                    .material_registry
                    .get_physical_props(contact_material_id)
                {
                    if let Some(interface_resistivity) = contact_props.get("interface_resistivity")
                    {
                        // Interface resistivity is defined - calculate R_0
                        // Assume interface thickness is contact diameter (conservative estimate)
                        let interface_thickness_m = drill_diameter_nm as f64 * 1e-9;

                        // R_0 = ρ_interface × (t / A)
                        let r0 = interface_resistivity * (interface_thickness_m / contact_area_m2);

                        return Ok(r0);
                    }
                }

                // No interface resistivity defined - return 0 (perfect contact)
                // This is NOT a fallback/default - it's the correct physical model when
                // the user hasn't defined interface resistance in their materials
                return Ok(0.0);
            }
        }
    }

    // No contact found on this terminal - return 0
    // This occurs for pours that don't have vias (e.g., direct metal connections)
    Ok(0.0)
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
