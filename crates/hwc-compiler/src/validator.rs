use compact_str::CompactString;
use hwc_diagnostics::DiagnosticCollector;
use hwc_parser::{
    ComponentPlacement, Coordinate, Definition, MaterialDefinition, Program, PropertyValue, Route,
    SpaceDefinition,
};
use miette::Diagnostic;
use rustc_hash::{FxHashMap, FxHashSet};
use thiserror::Error;

/// Required properties for Minimum Physical Viability (MPV) validation
const REQUIRED_MATERIAL_PROPERTIES: &[&str] = &[
    "resistivity",
    "thermal_conductivity",
    "density",
    "melting_point",
    "max_current_density",
];

#[derive(Default)]
pub struct Validator;

impl Validator {
    pub fn new() -> Self {
        Self
    }

    pub fn validate(
        &self,
        collector: &DiagnosticCollector,
        program: &Program,
        symbol_table: &crate::SymbolTable,
    ) {
        // Check for space definition
        let space = match program.definitions.iter().find_map(|def| match def {
            Definition::Space(s) => Some(s),
            _ => None,
        }) {
            Some(s) => s,
            None => {
                collector.report(ValidationError::MissingSpace);
                return;
            }
        };

        self.check_physical_z_in_coordinates(collector, space);

        // Check for overlapping components
        self.check_collisions(collector, &space.components(), space, symbol_table);

        // Check for unconnected pins
        self.check_connectivity(collector, space, program);
    }

    /// Validate materials for Minimum Physical Viability (MPV)
    /// This checks that all materials have the required physical properties
    pub fn validate_materials_mpv(&self, collector: &DiagnosticCollector, program: &Program) {
        // Collect all material definitions
        let materials: Vec<&MaterialDefinition> = program
            .definitions
            .iter()
            .filter_map(|def| match def {
                Definition::Material(mat) => Some(mat),
                _ => None,
            })
            .collect();

        if materials.is_empty() {
            collector.report(ValidationWarning {
                message: "No materials defined. Foundry validation requires material definitions."
                    .to_string()
                    .into(),
            });
            return;
        }

        // Validate each material
        for material in materials {
            let material_name = &material.name.name;

            // Build property map for quick lookup
            let mut property_map: FxHashMap<CompactString, &PropertyValue> = FxHashMap::default();
            for prop in &material.properties {
                property_map.insert(prop.key.clone(), &prop.value);
            }

            // Check for required properties
            let mut missing_properties = Vec::new();
            for required_prop in REQUIRED_MATERIAL_PROPERTIES {
                if !property_map.contains_key(*required_prop) {
                    missing_properties.push(*required_prop);
                }
            }

            if !missing_properties.is_empty() {
                collector.report(ValidationError::IncompleteMaterial {
                    material: material_name.to_string().into(),
                    missing: missing_properties.iter().map(|s| (*s).into()).collect(),
                });
            } else {
                // Validate property types and values
                self.validate_material_property_values(collector, material_name, &property_map);
            }

            if collector.should_stop() {
                return;
            }
        }
    }

    /// Validate materials for Minimum Physical Viability (MPV) from symbol table
    /// This version checks the merged materials (after property-level shadowing)
    pub fn validate_materials_mpv_from_symbol_table(
        &self,
        collector: &DiagnosticCollector,
        symbol_table: &crate::SymbolTable,
    ) {
        // Get all materials from symbol table (these are already merged)
        let materials = symbol_table.materials();

        if materials.is_empty() {
            collector.report(ValidationWarning {
                message: "No materials defined. Foundry validation requires material definitions."
                    .to_string()
                    .into(),
            });
            return;
        }

        // Validate each material
        for (material_name, material) in materials.iter() {
            // Build property map for quick lookup
            let mut property_map: FxHashMap<CompactString, &PropertyValue> = FxHashMap::default();
            for prop in &material.properties {
                property_map.insert(prop.key.clone(), &prop.value);
            }

            // Check for required properties
            let mut missing_properties = Vec::new();
            for required_prop in REQUIRED_MATERIAL_PROPERTIES {
                if !property_map.contains_key(*required_prop) {
                    missing_properties.push(*required_prop);
                }
            }

            if !missing_properties.is_empty() {
                collector.report(ValidationError::IncompleteMaterial {
                    material: material_name.to_string().into(),
                    missing: missing_properties.iter().map(|s| (*s).into()).collect(),
                });
            } else {
                // Validate property types and values
                self.validate_material_property_values(collector, material_name, &property_map);
            }

            if collector.should_stop() {
                return;
            }
        }
    }

    /// Validate that material properties have correct types and reasonable values
    fn validate_material_property_values(
        &self,
        collector: &DiagnosticCollector,
        material_name: &str,
        properties: &FxHashMap<CompactString, &PropertyValue>,
    ) {
        // Validate resistivity (Ω·m)
        if let Some(PropertyValue::Measurement(m)) = properties.get("resistivity") {
            if m.value <= 0.0 {
                collector.report(ValidationWarning {
                    message: format!(
                        "Material '{}': resistivity should be positive (got {})",
                        material_name, m.value
                    )
                    .into(),
                });
            }
        } else if let Some(PropertyValue::Number(value)) = properties.get("resistivity") {
            if *value <= 0.0 {
                collector.report(ValidationWarning {
                    message: format!(
                        "Material '{}': resistivity should be positive (got {})",
                        material_name, value
                    )
                    .into(),
                });
            }
        }

        // Validate thermal_conductivity (W/(m·K))
        if let Some(PropertyValue::Measurement(m)) = properties.get("thermal_conductivity") {
            if m.value <= 0.0 {
                collector.report(ValidationWarning {
                    message: format!(
                        "Material '{}': thermal_conductivity should be positive (got {})",
                        material_name, m.value
                    )
                    .into(),
                });
            }
        } else if let Some(PropertyValue::Number(value)) = properties.get("thermal_conductivity") {
            if *value <= 0.0 {
                collector.report(ValidationWarning {
                    message: format!(
                        "Material '{}': thermal_conductivity should be positive (got {})",
                        material_name, value
                    )
                    .into(),
                });
            }
        }

        // Validate density (kg/m³)
        if let Some(PropertyValue::Measurement(m)) = properties.get("density") {
            if m.value <= 0.0 {
                collector.report(ValidationWarning {
                    message: format!(
                        "Material '{}': density should be positive (got {})",
                        material_name, m.value
                    )
                    .into(),
                });
            }
        } else if let Some(PropertyValue::Number(value)) = properties.get("density") {
            if *value <= 0.0 {
                collector.report(ValidationWarning {
                    message: format!(
                        "Material '{}': density should be positive (got {})",
                        material_name, value
                    )
                    .into(),
                });
            }
        }

        // Validate melting_point (K)
        if let Some(PropertyValue::Measurement(m)) = properties.get("melting_point") {
            if m.value <= 0.0 {
                collector.report(ValidationWarning {
                    message: format!(
                        "Material '{}': melting_point should be positive (got {})",
                        material_name, m.value
                    )
                    .into(),
                });
            }
        } else if let Some(PropertyValue::Number(value)) = properties.get("melting_point") {
            if *value <= 0.0 {
                collector.report(ValidationWarning {
                    message: format!(
                        "Material '{}': melting_point should be positive (got {})",
                        material_name, value
                    )
                    .into(),
                });
            }
        }

        // Validate max_current_density (A/m²)
        if let Some(PropertyValue::Measurement(m)) = properties.get("max_current_density") {
            if m.value <= 0.0 {
                collector.report(ValidationWarning {
                    message: format!(
                        "Material '{}': max_current_density should be positive (got {})",
                        material_name, m.value
                    )
                    .into(),
                });
            }
        } else if let Some(PropertyValue::Number(value)) = properties.get("max_current_density") {
            if *value <= 0.0 {
                collector.report(ValidationWarning {
                    message: format!(
                        "Material '{}': max_current_density should be positive (got {})",
                        material_name, value
                    )
                    .into(),
                });
            }
        }
    }

    fn check_physical_z_in_coordinates(
        &self,
        collector: &DiagnosticCollector,
        space: &SpaceDefinition,
    ) {
        for component in &space.components() {
            if component.position.is_relative() {
                continue;
            }
            if !crate::ir::conversions::z_expr_is_physical(component.position.z()) {
                collector.report(ValidationError::DimensionlessZCoordinate);
            }
        }

        if let Some(substrate) = &space.substrate {
            for coord in [&substrate.from, &substrate.to] {
                if !coord.is_relative() && !crate::ir::conversions::z_expr_is_physical(coord.z()) {
                    collector.report(ValidationError::DimensionlessZCoordinate);
                }
            }
        }
    }

    /// Check for overlapping component bounding boxes
    fn check_collisions(
        &self,
        collector: &DiagnosticCollector,
        components: &[ComponentPlacement],
        space: &SpaceDefinition,
        symbol_table: &crate::SymbolTable,
    ) {
        // Get space dimensions for percentage calculations
        let dimensions = match space.dimensions.as_ref() {
            Some(d) => d,
            None => {
                collector.report(ValidationError::MissingSpace);
                return;
            }
        };

        // Convert dimensions to nanometers using symbol table (supports custom units!)
        let dimensions_nm = (
            symbol_table
                .measurement_to_nm(&dimensions.width)
                .unwrap_or(0),
            symbol_table
                .measurement_to_nm(&dimensions.height)
                .unwrap_or(0),
            symbol_table
                .measurement_to_nm(&dimensions.depth)
                .unwrap_or(0),
        );

        // Build bounding boxes for all components
        let mut bboxes: Vec<(String, BoundingBox)> = Vec::new();

        for component in components {
            let name = component
                .name
                .as_ref()
                .map(|n| n.to_string())
                .unwrap_or_else(|| component.component_type.to_string().into());
            let bbox = self.calculate_bounding_box(
                &component.position,
                dimensions_nm,
                symbol_table,
                hwc_parser::OriginZ::Bottom,
                1,
            );
            bboxes.push((name.to_string(), bbox));
        }

        // Check all pairs for overlap
        for i in 0..bboxes.len() {
            for j in (i + 1)..bboxes.len() {
                let (name1, bbox1) = &bboxes[i];
                let (name2, bbox2) = &bboxes[j];

                if bbox1.intersects(bbox2) {
                    collector.report(ValidationError::OverlappingComponents {
                        component1: name1.clone().into(),
                        component2: name2.clone().into(),
                        position1: bbox1.min,
                        position2: bbox2.min,
                    });
                }
            }
        }
    }

    /// Calculate bounding box for a component at given position
    /// Uses a default size since we don't have component definitions yet
    fn calculate_bounding_box(
        &self,
        position: &Coordinate,
        space_dimensions_nm: (i64, i64, i64),
        symbol_table: &crate::SymbolTable,
        origin_z: hwc_parser::OriginZ,
        grid_z_layers: usize,
    ) -> BoundingBox {
        // Default component size: 1mm in each dimension
        let voxel_size_z_nm = 1_000_000; // 1mm default when grid unknown at validate time
        let (x_nm, y_nm, z_nm) = self.coordinate_to_nm(
            position,
            symbol_table,
            voxel_size_z_nm,
            space_dimensions_nm,
            origin_z,
            grid_z_layers,
        );

        // Convert to grid-like coordinates for bounding box (divide by 1mm)
        let x = x_nm / 1_000_000;
        let y = y_nm / 1_000_000;
        let z = z_nm / 1_000_000;

        BoundingBox {
            min: (x, y, z),
            max: (x + 1, y + 1, z + 1),
        }
    }

    /// Convert coordinate to physical position in nanometers (Z must use physical units).
    fn coordinate_to_nm(
        &self,
        coord: &Coordinate,
        symbol_table: &crate::SymbolTable,
        _voxel_size_z_nm: i64,
        space_dimensions_nm: (i64, i64, i64),
        _origin_z: hwc_parser::OriginZ,
        _grid_z_layers: usize,
    ) -> (i64, i64, i64) {
        let (x_val, y_val, _) = coord
            .evaluate_const()
            .expect("Failed to evaluate coordinate expression");

        let x_nm = x_val
            .to_nanometers_with_ref(space_dimensions_nm.0)
            .expect("X must be a physical measurement or percentage");
        let y_nm = y_val
            .to_nanometers_with_ref(space_dimensions_nm.1)
            .expect("Y must be a physical measurement or percentage");

        let z_nm = if matches!(coord, Coordinate::Relative(_)) {
            0
        } else {
            crate::ir::conversions::evaluate_expression_to_nm(coord.z(), symbol_table)
                .unwrap_or(0)
        };

        (x_nm, y_nm, z_nm)
    }

    /// Check connectivity: ensure all pins are connected
    fn check_connectivity(
        &self,
        collector: &DiagnosticCollector,
        space: &SpaceDefinition,
        program: &Program,
    ) {
        // Build component registry
        let component_defs = self.build_component_registry(program);

        // Build pin registry: component_name.pin_name -> exists
        let mut all_pins: FxHashSet<CompactString> = FxHashSet::default();
        for component in &space.components() {
            let component_name = component.name.clone().unwrap_or_else(|| {
                // Create a simple ComponentName from the component type
                hwc_parser::ComponentName::simple(
                    component.component_type.to_string().into(),
                    component.component_type.span,
                )
            });

            // Get component definition to find pins
            if let Some(comp_def) = component_defs.get(component.component_type.as_str()) {
                for pin in &comp_def.pins {
                    all_pins.insert(format!("{}.{}", component_name, pin).into());
                }
            } else {
                // Component definition not found - add warning
                collector.report(ValidationWarning {
                    message: format!(
                        "Component type '{}' not defined, cannot validate pins",
                        component.component_type
                    )
                    .into(),
                });
            }
        }

        // Build connected pins set from routes
        let mut connected_pins: FxHashSet<CompactString> = FxHashSet::default();
        for route in &space.routes {
            let from_pin = format!("{}.{}", route.from.component, route.from.pin);
            let to_pin = format!("{}.{}", route.to.component, route.to.pin);

            connected_pins.insert(from_pin.clone().into());
            connected_pins.insert(to_pin.clone().into());

            // Validate that referenced pins exist
            if !all_pins.contains(from_pin.as_str()) {
                collector.report(ValidationError::UnconnectedPin {
                    pin: from_pin.into(),
                    reason: "Pin referenced in route but component not found".into(),
                });
            }
            if !all_pins.contains(to_pin.as_str()) {
                collector.report(ValidationError::UnconnectedPin {
                    pin: to_pin.into(),
                    reason: "Pin referenced in route but component not found".into(),
                });
            }
        }

        // Find unconnected pins
        for pin in &all_pins {
            if !connected_pins.contains(pin) {
                collector.report(ValidationWarning {
                    message: format!("Pin '{}' is not connected to any net", pin).into(),
                });
            }
        }

        // Check for multiple drivers on same net (electrical borrow checker)
        self.check_multiple_drivers(collector, &space.routes, &component_defs);
    }

    /// Build component definition registry from program
    fn build_component_registry<'a>(
        &self,
        program: &'a Program,
    ) -> FxHashMap<CompactString, &'a hwc_parser::ComponentDefinition> {
        let mut registry = FxHashMap::default();

        for def in &program.definitions {
            if let Definition::Component(comp_def) = def {
                registry.insert(comp_def.name.to_string().into(), comp_def);
            }
        }

        registry
    }

    /// Check for multiple output drivers on the same net (electrical borrow checker)
    fn check_multiple_drivers(
        &self,
        collector: &DiagnosticCollector,
        routes: &[Route],
        component_defs: &FxHashMap<CompactString, &hwc_parser::ComponentDefinition>,
    ) {
        // Build nets: group routes by connected pins
        let mut nets: FxHashMap<CompactString, Vec<CompactString>> = FxHashMap::default();

        for route in routes {
            let from_pin = format!("{}.{}", route.from.component, route.from.pin);
            let to_pin = format!("{}.{}", route.to.component, route.to.pin);

            // Find or create net
            let net_id = self.find_or_create_net(&from_pin, &to_pin, &mut nets);

            // Add both pins to the net
            nets.entry(net_id.clone())
                .or_default()
                .push(from_pin.clone().into());
            nets.entry(net_id).or_default().push(to_pin.clone().into());
        }

        // Check each net for multiple output drivers
        for (net_id, pins) in nets {
            let mut output_count = 0;
            let mut output_pins = Vec::new();

            for pin in pins {
                // Check if pin is an output (simplified - would need pin direction metadata)
                if self.is_output_pin(&pin, component_defs) {
                    output_count += 1;
                    output_pins.push(pin);
                }
            }

            if output_count > 1 {
                collector.report(ValidationWarning {
                    message: format!(
                        "Net '{}' has multiple output drivers: {}. This may cause a short circuit.",
                        net_id,
                        output_pins.join(", ")
                    )
                    .into(),
                });
            }
        }
    }

    /// Find or create net ID for two connected pins
    fn find_or_create_net(
        &self,
        pin1: &str,
        pin2: &str,
        nets: &mut FxHashMap<CompactString, Vec<CompactString>>,
    ) -> CompactString {
        // Simple net naming: use first pin as net ID
        // In a real implementation, this would use union-find for transitive connectivity
        for (net_id, pins) in nets.iter() {
            if pins.contains(&pin1.into()) || pins.contains(&pin2.into()) {
                return net_id.clone();
            }
        }

        // Create new net
        format!("net_{}", pin1).into()
    }

    /// Check if a pin is an output pin (simplified heuristic)
    fn is_output_pin(
        &self,
        pin: &str,
        _component_defs: &FxHashMap<CompactString, &hwc_parser::ComponentDefinition>,
    ) -> bool {
        // Simplified heuristic: pins named "out", "output", "vcc", "vdd" are outputs
        let pin_lower = pin.to_lowercase();
        pin_lower.contains("out")
            || pin_lower.contains("vcc")
            || pin_lower.contains("vdd")
            || pin_lower.contains("source")
    }
}

/// 3D Bounding box for collision detection
#[derive(Debug, Clone)]
struct BoundingBox {
    min: (i64, i64, i64), // (x, y, z) minimum corner
    max: (i64, i64, i64), // (x, y, z) maximum corner
}

impl BoundingBox {
    /// Check if this bounding box intersects with another
    fn intersects(&self, other: &BoundingBox) -> bool {
        // AABB (Axis-Aligned Bounding Box) intersection test
        // Two boxes intersect if they overlap on all three axes
        self.max.0 > other.min.0
            && self.min.0 < other.max.0
            && self.max.1 > other.min.1
            && self.min.1 < other.max.1
            && self.max.2 > other.min.2
            && self.min.2 < other.max.2
    }
}

#[derive(Debug, Error, Diagnostic)]
pub enum ValidationError {
    #[error("No space definition found")]
    MissingSpace,

    #[error(
        "Z coordinate must use physical units (e.g. z: 1.5mm), not a dimensionless layer index like z: 1"
    )]
    #[diagnostic(
        code(C26),
        url("https://docs.hw-script.org/errors/C26"),
        help(
            "Use physical Z with units (Assembly), or omit Z from coordinates and use layer: at statement level (High-Level)."
        )
    )]
    DimensionlessZCoordinate,

    #[error(
        "Components '{component1}' at {position1:?} and '{component2}' at {position2:?} overlap"
    )]
    OverlappingComponents {
        component1: CompactString,
        component2: CompactString,
        position1: (i64, i64, i64),
        position2: (i64, i64, i64),
    },

    #[error("Pin '{pin}' error: {reason}")]
    UnconnectedPin { pin: CompactString, reason: String },

    #[error("Material '{material}' is missing required properties: {}", missing.join(", "))]
    IncompleteMaterial {
        material: CompactString,
        missing: Vec<CompactString>,
    },
}

#[derive(Debug, Error, Diagnostic)]
#[error("{message}")]
#[diagnostic(severity(Warning))]
pub struct ValidationWarning {
    pub message: CompactString,
}
