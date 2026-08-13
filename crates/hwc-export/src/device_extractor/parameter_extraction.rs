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

use compact_str::CompactString;
use hwc_engine::space::PourMetadata;
use hwc_engine::HardwareSpace;
use hwc_physics::geometry::BoundingBox;
use rustc_hash::FxHashMap;

/// Parameter extraction function signature
///
/// Takes terminal geometry and space context, returns calculated parameters.
///
/// Each terminal maps to ALL pours bound to it. Extractors must explicitly
/// decide how to handle multiple pours per terminal and fail loudly when the
/// binding is ambiguous.
pub type ExtractionFn = fn(
    &FxHashMap<CompactString, Vec<PourMetadata>>,
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
    /// - Resistor: R = ρ(L/A) + W/L for subcircuits
    /// - Capacitor: W/L geometry for foundry models
    /// - IdealCapacitor: C = ε₀εᵣ(A/d) for parallel-plate capacitors
    pub fn register_standard_extractors(&mut self) {
        self.register("Resistor", extract_resistor_parameters);
        self.register("PolyResistor", extract_resistor_parameters);
        self.register("Capacitor", extract_geometry_wl_parameters);        // Foundry MIM Cap (W, L)
        self.register("IdealCapacitor", extract_capacitor_parameters);     // Ideal Parallel Plate Cap (C, ESR)
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
        terminal_pours: &FxHashMap<CompactString, Vec<PourMetadata>>,
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
    terminal_pours: &FxHashMap<CompactString, Vec<PourMetadata>>,
    space: &HardwareSpace,
) -> Result<FxHashMap<CompactString, f64>, String> {
    // Capacitor requires exactly 2 terminals
    if terminal_pours.len() != 2 {
        return Err(format!(
            "Capacitor must have exactly 2 terminals, found {}",
            terminal_pours.len()
        ));
    }

    // Pair each terminal name with its pours so error messages never rely on
    // implicit keys()/values() index alignment.
    let entries: Vec<(&CompactString, &Vec<PourMetadata>)> = terminal_pours.iter().collect();

    // FAIL LOUDLY: No unwrap, explicit error for missing pours
    let (terminal_1, pour_vec_1) = *entries.first().ok_or_else(|| {
        "Internal error: terminal_pours has length 2 but index 0 is missing".to_string()
    })?;
    let (terminal_2, pour_vec_2) = *entries.get(1).ok_or_else(|| {
        "Internal error: terminal_pours has length 2 but index 1 is missing".to_string()
    })?;

    // FAIL LOUDLY: Each terminal must have at least one pour
    let pour1 = pour_vec_1.first().ok_or_else(|| {
        format!(
            "Capacitor terminal '{}' has zero pours bound. \
             Add 'device: C1.<terminal>' to a conductor pour.",
            terminal_1
        )
    })?;
    let pour2 = pour_vec_2.first().ok_or_else(|| {
        format!(
            "Capacitor terminal '{}' has zero pours bound. \
             Add 'device: C1.<terminal>' to a conductor pour.",
            terminal_2
        )
    })?;

    // FAIL LOUDLY: Multiple pours per terminal not supported for capacitors yet
    if pour_vec_1.len() > 1 {
        return Err(format!(
            "Capacitor terminal '{}' has {} pours bound. \
             Capacitors support only one pour per terminal. \
             Remove extra bindings or split into separate devices.",
            terminal_1,
            pour_vec_1.len()
        ));
    }
    if pour_vec_2.len() > 1 {
        return Err(format!(
            "Capacitor terminal '{}' has {} pours bound. \
             Capacitors support only one pour per terminal. \
             Remove extra bindings or split into separate devices.",
            terminal_2,
            pour_vec_2.len()
        ));
    }

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

/// Extract resistance from rectangular geometry using BindingPriority-based separation
///
/// **Resilient Architecture (v0.2.2)**: Uses BindingPriority to distinguish between:
/// - Channel pours (BindingPriority::Channel = 0): Primary resistive body (Polysilicon)
/// - Contact pours (BindingPriority::Contact = 100): Terminal connection overlays (TiSi2, Al)
///
/// This eliminates brittle pointer-equality hacks and works correctly for:
/// - Straight resistors with contact overlap
/// - L-shaped or serpentine resistors
/// - Multi-finger transistors
/// - Tapped resistors and voltage dividers
///
/// For SPICE parameter extraction:
/// - L_drawn: Full length of the Channel pour (for foundry models like sky130_fd_pr__res_high_po)
/// - L_effective: Actual unsilicided length after subtracting contact overlap (for physics calculation)
fn extract_resistor_parameters(
    terminal_pours: &FxHashMap<CompactString, Vec<PourMetadata>>,
    space: &HardwareSpace,
) -> Result<FxHashMap<CompactString, f64>, String> {
    use hwc_engine::space::BindingPriority;
    
    // FAIL LOUDLY: Resistor requires at least 2 terminals (A, B), may have more (BULK, etc.)
    if terminal_pours.len() < 2 {
        return Err(format!(
            "Resistor extraction requires at least 2 terminals (A, B), found {} terminals: {:?}",
            terminal_pours.len(),
            terminal_pours.keys().collect::<Vec<_>>()
        ));
    }

    // FAIL LOUDLY: Terminal A must exist
    let pours_a = terminal_pours.get("A").ok_or_else(|| {
        format!(
            "Resistor missing terminal A binding. \
             Found terminals: {:?}\n\
             Add 'device: R1.A' to a pour.",
            terminal_pours.keys().collect::<Vec<_>>()
        )
    })?;

    // FAIL LOUDLY: Terminal B must exist
    let pours_b = terminal_pours.get("B").ok_or_else(|| {
        format!(
            "Resistor missing terminal B binding. \
             Found terminals: {:?}\n\
             Add 'device: R1.B' to a pour.",
            terminal_pours.keys().collect::<Vec<_>>()
        )
    })?;

    // FAIL LOUDLY: Each terminal must have at least one pour
    if pours_a.is_empty() {
        return Err("Resistor terminal A has zero pours bound.".to_string());
    }
    if pours_b.is_empty() {
        return Err("Resistor terminal B has zero pours bound.".to_string());
    }

    // SERPENTINE RESISTOR SUPPORT (v0.2.3):
    // Collect ALL Channel pours bound to BOTH terminals A and B (intersection)
    // For serpentine resistors, all segments (Seg1..Seg5, Vert1..Vert4) are bound to both A and B
    // We need to sum their lengths to get the total resistive path length
    
    // Collect channel pours from terminal A
    let channel_pours_a: Vec<&PourMetadata> = pours_a
        .iter()
        .filter(|p| {
            p.device_binding
                .as_ref()
                .map(|b| b.priority == BindingPriority::Channel)
                .unwrap_or(false)
        })
        .collect();

    if channel_pours_a.is_empty() {
        return Err(format!(
            "Terminal A missing Channel pour bindings (BindingPriority::Channel).\n\
             \n\
             Found {} pours on terminal A, but none marked as Channel priority.\n\
             \n\
             Fix: Ensure your resistor body pour is bound to terminal A.",
            pours_a.len()
        ));
    }

    // Collect channel pours from terminal B
    let channel_pours_b: Vec<&PourMetadata> = pours_b
        .iter()
        .filter(|p| {
            p.device_binding
                .as_ref()
                .map(|b| b.priority == BindingPriority::Channel)
                .unwrap_or(false)
        })
        .collect();

    if channel_pours_b.is_empty() {
        return Err(format!(
            "Terminal B missing Channel pour bindings (BindingPriority::Channel).\n\
             \n\
             Found {} pours on terminal B, but none marked as Channel priority.\n\
             \n\
             Fix: Ensure your resistor body pour is bound to terminal B.",
            pours_b.len()
        ));
    }

    // Find common channel pours (bound to BOTH A and B)
    // For serpentine resistors, all segments should be in this intersection
    let mut common_channel_pours: Vec<&PourMetadata> = Vec::new();
    for pour_a in &channel_pours_a {
        if channel_pours_b.iter().any(|p| p.name == pour_a.name) {
            common_channel_pours.push(pour_a);
        }
    }

    if common_channel_pours.is_empty() {
        return Err(format!(
            "No common Channel pours found bound to BOTH terminals A and B.\n\
             \n\
             Terminal A has {} channel pours: {:?}\n\
             Terminal B has {} channel pours: {:?}\n\
             \n\
             Fix: For a resistor, channel pours must be bound to BOTH A and B.",
            channel_pours_a.len(),
            channel_pours_a.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
            channel_pours_b.len(),
            channel_pours_b.iter().map(|p| p.name.as_str()).collect::<Vec<_>>()
        ));
    }

    // Use the first pour as the reference for width and material
    let reference_pour = common_channel_pours[0];
    let reference_bbox = reference_pour.bbox.as_ref().ok_or_else(|| {
        format!(
            "Channel pour '{}' has no bounding box",
            reference_pour.name
        )
    })?;

    // Calculate TOTAL drawn length by summing all segment lengths
    // For simple resistors: 1 segment with its full length
    // For serpentine resistors: Sum of all horizontal + vertical segments
    let mut total_l_drawn_nm = 0.0;
    let mut segment_count = 0;
    
    for pour in &common_channel_pours {
        if let Some(bbox) = &pour.bbox {
            // For each segment, the "length" is the longer dimension (X or Y)
            let length_x = (bbox.max.x - bbox.min.x).abs() as f64;
            let length_y = (bbox.max.y - bbox.min.y).abs() as f64;
            
            // Take the maximum dimension as the segment length
            let segment_length = length_x.max(length_y);
            total_l_drawn_nm += segment_length;
            segment_count += 1;
            
            println!(
                "      [CHANNEL SEGMENT] '{}' ({}) - L={:.2}um x W={:.2}um",
                pour.name,
                pour.material_name,
                length_x / 1000.0,
                length_y / 1000.0
            );
        }
    }

    // Use the minimum dimension of the reference pour as the channel width
    // (For serpentine: all segments should have the same width)
    let reference_length_x = (reference_bbox.max.x - reference_bbox.min.x).abs() as f64;
    let reference_length_y = (reference_bbox.max.y - reference_bbox.min.y).abs() as f64;
    let w_drawn_nm = reference_length_x.min(reference_length_y);

    println!(
        "      [CHANNEL TOTAL] {} segments, Total L_drawn={:.2}um, W={:.2}um",
        segment_count,
        total_l_drawn_nm / 1000.0,
        w_drawn_nm / 1000.0
    );

    let l_drawn_nm = total_l_drawn_nm;

    // Calculate effective length by measuring contact overlap
    // For serpentine resistors with multiple segments, we need to find the 
    // overall bounding box of all channel segments to measure contact overlap
    let mut channel_min_x = i64::MAX;
    let mut channel_max_x = i64::MIN;
    let mut channel_min_y = i64::MAX;
    let mut channel_max_y = i64::MIN;
    
    for pour in &common_channel_pours {
        if let Some(bbox) = &pour.bbox {
            channel_min_x = channel_min_x.min(bbox.min.x);
            channel_max_x = channel_max_x.max(bbox.max.x);
            channel_min_y = channel_min_y.min(bbox.min.y);
            channel_max_y = channel_max_y.max(bbox.max.y);
        }
    }
    
    let mut overlap_head_nm: f64 = 0.0;
    let mut overlap_tail_nm: f64 = 0.0;

    // Find Contact pours on Terminal A (head side)
    for pour in pours_a {
        if let Some(binding) = &pour.device_binding {
            if binding.priority == BindingPriority::Contact {
                if let Some(bbox_contact) = &pour.bbox {
                    // Calculate overlap into the channel body from the left (head) side
                    let overlap = (bbox_contact.max.x.min(channel_max_x) 
                                 - channel_min_x.max(bbox_contact.min.x))
                                 .max(0) as f64;
                    if overlap > 0.0 && overlap < l_drawn_nm {
                        overlap_head_nm = overlap_head_nm.max(overlap);
                        println!(
                            "      [CONTACT A] '{}' ({}) overlaps channel by {:.2}um",
                            pour.name,
                            pour.material_name,
                            overlap / 1000.0
                        );
                    }
                }
            }
        }
    }

    // Find Contact pours on Terminal B (tail side)
    for pour in pours_b {
        if let Some(binding) = &pour.device_binding {
            if binding.priority == BindingPriority::Contact {
                if let Some(bbox_contact) = &pour.bbox {
                    // Calculate overlap into the channel body from the right (tail) side
                    let overlap = (channel_max_x.min(bbox_contact.max.x)
                                 - bbox_contact.min.x.max(channel_min_x))
                                 .max(0) as f64;
                    if overlap > 0.0 && overlap < l_drawn_nm {
                        overlap_tail_nm = overlap_tail_nm.max(overlap);
                        println!(
                            "      [CONTACT B] '{}' ({}) overlaps channel by {:.2}um",
                            pour.name,
                            pour.material_name,
                            overlap / 1000.0
                        );
                    }
                }
            }
        }
    }

    // Calculate effective resistive length (unsilicided region)
    let l_effective_nm = (l_drawn_nm - overlap_head_nm - overlap_tail_nm).max(0.0);

    if l_effective_nm <= 0.0 {
        return Err(format!(
            "Resistor effective length is zero or negative!\n\
             L_drawn = {:.2}um, overlap_head = {:.2}um, overlap_tail = {:.2}um\n\
             Contacts completely cover the resistive channel.",
            l_drawn_nm / 1000.0,
            overlap_head_nm / 1000.0,
            overlap_tail_nm / 1000.0
        ));
    }

    println!(
        "      [EFFECTIVE LENGTH] L_eff={:.2}um (L_drawn={:.2}um - head_overlap={:.2}um - tail_overlap={:.2}um)",
        l_effective_nm / 1000.0,
        l_drawn_nm / 1000.0,
        overlap_head_nm / 1000.0,
        overlap_tail_nm / 1000.0
    );

    // Get material properties for physics calculation
    let material_id = space
        .material_registry
        .get_id(&reference_pour.material_name)
        .ok_or_else(|| {
            format!(
                "Material '{}' not found in registry",
                reference_pour.material_name
            )
        })?;

    let props = space
        .material_registry
        .get_physical_props(material_id)
        .ok_or_else(|| {
            format!(
                "Material '{}' missing physical properties",
                reference_pour.material_name
            )
        })?;

    let resistivity = props.get("resistivity").ok_or_else(|| {
        format!(
            "Material '{}' missing 'resistivity' property.\n\
             Add 'resistivity: <value>' to the material properties block.",
            reference_pour.material_name
        )
    })?;

    // Get layer thickness from stackup
    let z_bottom = reference_pour.z_bottom_nm;
    let layer_thickness_nm = space
        .stackup_layers
        .iter()
        .find(|layer| z_bottom >= layer.z_bottom && z_bottom < layer.z_top)
        .map(|layer| (layer.z_top - layer.z_bottom) as f64)
        .ok_or_else(|| {
            format!(
                "Could not find stackup layer for pour '{}' at Z={}nm",
                reference_pour.name, z_bottom
            )
        })?;

    let cross_section_nm2 = w_drawn_nm * layer_thickness_nm;

    // Convert to meters for physics calculation
    let l_effective_m = l_effective_nm * 1e-9;
    let cross_section_m2 = cross_section_nm2 * 1e-18;

    // Calculate body resistance using L_effective: R_body = ρ * (L_eff / A)
    let r_body = resistivity * (l_effective_m / cross_section_m2);

    // Calculate contact resistance at head and tail (R_0)
    let r0_head = calculate_contact_resistance(reference_pour, space)?;
    let r0_tail = calculate_contact_resistance(reference_pour, space)?;

    // Total resistance includes body + both contact resistances
    let r_total = r_body + r0_head + r0_tail;

    let mut params = FxHashMap::default();
    params.insert("R".into(), r_total);
    params.insert("W".into(), w_drawn_nm / 1000.0); // Store in micrometers (for SPICE)
    params.insert("L".into(), l_drawn_nm / 1000.0); // Store drawn length in micrometers (for SPICE)
    params.insert("L_eff".into(), l_effective_nm / 1000.0); // Store effective length for physics

    // Log format distinguishes between extracted parameters and PDK-managed physics
    // to prevent engineers from confusing compiler-extracted body resistance with
    // the actual DC resistance in silicon (which includes PDK subcircuit contact resistance)
    println!(
        "      ├─ [EXTRACTED] R_body = {:.2}Ω (L_eff={:.2}um) | R_contact = {}",
        r_body, 
        l_effective_nm / 1000.0,
        if r0_head > 0.0 || r0_tail > 0.0 {
            format!("head={:.2}Ω, tail={:.2}Ω", r0_head, r0_tail)
        } else {
            "[PDK Subcircuit Managed]".to_string()
        }
    );
    println!(
        "      ├─ SPICE Parameters: W={:.2}um, L={:.2}um (drawn dimensions for foundry model)",
        w_drawn_nm / 1000.0,
        l_drawn_nm / 1000.0
    );
    println!(
        "      └─ Contact Overlap: head={:.2}um, tail={:.2}um (excluded from R_body calculation)",
        overlap_head_nm / 1000.0,
        overlap_tail_nm / 1000.0
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

/// Extract W and L parameters from terminal geometry for foundry PDK models
///
/// **FOUNDRY MODEL EXTRACTION (TYPE A in NETLIST-ARCHITECTURE.md)**
///
/// For devices with `spice_include` (foundry subcircuits), the compiler:
/// 1. Reads what parameters the subcircuit declares (parameters: [W, L])
/// 2. Measures the physical pour geometry (bounding box width and length)
/// 3. Passes W and L to the foundry SPICE model
/// 4. Does NOT calculate physics (C, R, etc.) - the foundry model handles that
///
/// This is the Zero-Magic approach:
/// - No string pattern matching ("Width", "LENGTH", "area")
/// - No physics formulas (C = εA/d is in the foundry model, not the compiler)
/// - Direct geometric measurement from user's explicit device bindings
///
/// Example:
/// ```
/// export subcircuit sky130_fd_pr__cap_mim:
///     terminals: [top, bottom]
///     parameters: [W = 10.0um, L = 10.0um]  # ← Declares W and L
///     spice_include: "path/to/model.spice"
/// ```
///
/// Compiler extracts: W = bbox.width, L = bbox.length
/// Output SPICE: `XC1 top bottom sky130_fd_pr__cap_mim W=10.0u L=10.0u`
fn extract_geometry_wl_parameters(
    terminal_pours: &FxHashMap<CompactString, Vec<PourMetadata>>,
    _space: &HardwareSpace,
) -> Result<FxHashMap<CompactString, f64>, String> {
    // For MIM capacitors: the active capacitance area is determined by the 
    // OVERLAPPING region = the smaller plate, NOT the larger plate with overhang.
    //
    // Strategy:
    // 1. Each terminal may have multiple pours (main plate + contact pads)
    // 2. Find the LARGEST pour per terminal (ignore small contact pads)
    // 3. EXCLUDE non-active terminals (b, body, bulk, BULK, sub, substrate)
    // 4. Use the SMALLER of the two active plates (this is the overlap area)
    //
    // Physics: C = C_area × A_overlap
    // Example: top plate = 10μm × 10μm, bottom = 11μm × 11μm, bulk = 1μm × 1μm
    //   - WRONG: Using 1μm × 1μm (bulk tie) → 98.7% capacitance error! 
    //   - WRONG: Using 11μm × 11μm → 20.6% capacitance error
    //   - CORRECT: Using 10μm × 10μm (the actual overlap area)
    
    // List of terminal names that are NOT active device plates/channels
    // These are substrate/bulk ties and should be excluded from geometry extraction
    const NON_ACTIVE_TERMINALS: &[&str] = &["b", "body", "bulk", "BULK", "sub", "substrate", "SUBSTRATE"];
    
    // Step 1: Find largest pour per terminal (EXCLUDING non-active terminals)
    let mut largest_per_terminal: FxHashMap<&CompactString, (&PourMetadata, f64)> = FxHashMap::default();
    
    for (terminal_name, pours) in terminal_pours {
        // Skip non-active terminals (bulk ties, substrate connections)
        if NON_ACTIVE_TERMINALS.contains(&terminal_name.as_str()) {
            println!(
                "      [SKIPPED] Terminal '{}' excluded from W/L extraction (non-active terminal)",
                terminal_name
            );
            continue;
        }
        
        let mut max_area = 0.0;
        let mut max_pour: Option<&PourMetadata> = None;
        
        for pour in pours {
            if let Some(bbox) = &pour.bbox {
                let width = (bbox.max.x - bbox.min.x) as f64;
                let length = (bbox.max.y - bbox.min.y) as f64;
                let area = width * length;
                
                if area > max_area {
                    max_area = area;
                    max_pour = Some(pour);
                }
            }
        }
        
        if let Some(pour) = max_pour {
            largest_per_terminal.insert(terminal_name, (pour, max_area));
            println!(
                "      [CANDIDATE] Terminal '{}': pour '{}' ({:.2}um x {:.2}um, area={:.2}um²)",
                terminal_name,
                pour.name,
                (pour.bbox.as_ref().unwrap().max.x - pour.bbox.as_ref().unwrap().min.x) as f64 / 1000.0,
                (pour.bbox.as_ref().unwrap().max.y - pour.bbox.as_ref().unwrap().min.y) as f64 / 1000.0,
                max_area / 1_000_000.0
            );
        }
    }
    
    if largest_per_terminal.is_empty() {
        return Err(
            "No ACTIVE terminal pours with bounding boxes found for geometry extraction.\n\
             All terminals were either non-active (b, body, bulk) or had no geometry.".to_string()
        );
    }
    
    // Step 2: Find the smallest of the active terminal plates (the limiting plate)
    let (pour, area) = largest_per_terminal
        .values()
        .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .ok_or_else(|| {
            "Could not determine smallest active terminal plate".to_string()
        })?;
    
    println!(
        "      [SELECTED] Using pour '{}' for W/L extraction (smallest active plate, area={:.2}um²)",
        pour.name,
        area / 1_000_000.0
    );
    
    let bbox = pour.bbox.as_ref().ok_or_else(|| {
        format!("Pour '{}' has no bounding box", pour.name)
    })?;
    
    // Extract width and length from bounding box
    // If terminal bounding boxes exist, length is the span between terminals A and B,
    // and width is the cross-sectional width. This ensures correct orientation for
    // devices rotated 90° or oriented vertically.
    let (width_nm, length_nm) = if let (Some(pours_a), Some(pours_b)) = (terminal_pours.get("A"), terminal_pours.get("B")) {
        if let (Some(pa), Some(pb)) = (pours_a.first(), pours_b.first()) {
            if let (Some(ba), Some(bb)) = (&pa.bbox, &pb.bbox) {
                (calculate_resistor_width(ba, bb), calculate_resistor_length(ba, bb))
            } else {
                ((bbox.max.y - bbox.min.y) as f64, (bbox.max.x - bbox.min.x) as f64)
            }
        } else {
            ((bbox.max.y - bbox.min.y) as f64, (bbox.max.x - bbox.min.x) as f64)
        }
    } else {
        ((bbox.max.y - bbox.min.y) as f64, (bbox.max.x - bbox.min.x) as f64)
    };
    
    let mut params = FxHashMap::default();
    params.insert("W".into(), width_nm / 1000.0); // Convert to micrometers
    params.insert("L".into(), length_nm / 1000.0); // Convert to micrometers
    
    println!(
        "      ├─ Geometry: W={:.2}um, L={:.2}um (from pour '{}')",
        width_nm / 1000.0,
        length_nm / 1000.0,
        pour.name
    );
    println!(
        "      └─ Foundry model will calculate C, R, and parasitics from W/L"
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
