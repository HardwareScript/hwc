//! Component definition types and parsing.

use crate::geometry::Point3D;
use compact_str::CompactString;

use super::error::PlacementError;
use super::types::SymbolTableTrait;

/// Component definition (footprint, pins, material).
///
/// SEMANTIC BAKING: This struct stores PRE-PARSED integers (nanometers).
/// The parsing happens ONCE during registration, not during placement.
#[derive(Debug, Clone)]
#[allow(dead_code)] // Fields used in tests and future phases
pub(super) struct ComponentDefinition {
    pub name: CompactString,
    pub footprint: Footprint,
    pub pins: Vec<PinDefinition>,
    /// Material name for the component body (e.g., "Body", "Polysilicon", "Silicon_N")
    /// This gets dynamically registered in the MaterialRegistry during placement
    pub material_name: CompactString,
}

/// Baked component definition with pre-parsed dimensions.
///
/// PERFORMANCE: This is the "Resolved Template" - all strings have been
/// converted to integers during registration. Placement is now pure math.
///
/// Registration Phase: Parse "Rectangle(8mm, 4mm, 0.5mm)" → BakedComponent { width_nm: 8_000_000, ... }
/// Placement Phase: Just read the integers and do addition (no parsing!)
#[derive(Debug, Clone)]
pub struct BakedComponent {
    pub name: CompactString,
    pub width_nm: i64,
    pub height_nm: i64,
    pub depth_nm: i64,
    pub pins: Vec<PinDefinition>,
    pub material_name: CompactString,
}

/// Component footprint shape.
#[derive(Debug, Clone)]
pub(crate) enum Footprint {
    Rectangle {
        width_nm: i64,
        height_nm: i64,
        depth_nm: i64,
    },
    // Future: Cylinder, Custom, etc.
}

/// Pad shape for component pins.
#[derive(Debug, Clone)]
pub enum PadShape {
    /// Circular pad with diameter
    Circle { diameter_nm: i64 },
    /// Rectangular pad with width and height
    Rectangle { width_nm: i64, height_nm: i64 },
    /// Obround (rounded rectangle) - stadium shape
    Obround { width_nm: i64, height_nm: i64 },
    /// Custom polygon defined by points (relative to pad center)
    Polygon { points: Vec<Point3D> },
    /// Rounded rectangle with corner radius
    RoundedRect {
        width_nm: i64,
        height_nm: i64,
        corner_radius_nm: i64,
    },
}

impl PadShape {
    /// Get the bounding box dimensions for this pad shape
    pub fn bounding_box(&self) -> (i64, i64) {
        match self {
            PadShape::Circle { diameter_nm } => (*diameter_nm, *diameter_nm),
            PadShape::Rectangle {
                width_nm,
                height_nm,
            } => (*width_nm, *height_nm),
            PadShape::Obround {
                width_nm,
                height_nm,
            } => (*width_nm, *height_nm),
            PadShape::RoundedRect {
                width_nm,
                height_nm,
                ..
            } => (*width_nm, *height_nm),
            PadShape::Polygon { points } => {
                if points.is_empty() {
                    return (0, 0);
                }
                let min_x = points.iter().map(|p| p.x).min().unwrap_or(0);
                let max_x = points.iter().map(|p| p.x).max().unwrap_or(0);
                let min_y = points.iter().map(|p| p.y).min().unwrap_or(0);
                let max_y = points.iter().map(|p| p.y).max().unwrap_or(0);
                (max_x - min_x, max_y - min_y)
            }
        }
    }
}

/// Pin definition with local offset and pad shape.
#[derive(Debug, Clone)]
pub struct PinDefinition {
    pub name: CompactString,
    pub local_offset: Point3D,
    pub pad_shape: PadShape,
}

/// Load component definition from Symbol Table.
///
/// Converts the parser's ComponentDefinition AST into the engine's
/// internal ComponentDefinition format for placement.
///
/// ⚠️ PERFORMANCE WARNING: This function parses strings every time it's called.
/// For loops with N iterations, this causes N×parsing_cost overhead.
/// Use `bake_component_definition` during registration instead.
pub(super) fn load_component_definition<S: SymbolTableTrait>(
    component_type: &str,
    symbol_table: &S,
) -> Result<ComponentDefinition, PlacementError> {
    // Get component from Symbol Table
    let component_ast = symbol_table.get_component(component_type).map_err(|_| {
        PlacementError::UnknownComponent {
            component_type: component_type.into(),
        }
    })?;

    // Extract layout information
    let layout = component_ast
        .layout
        .as_ref()
        .ok_or_else(|| PlacementError::MissingLayout {
            component: component_type.into(),
        })?;

    // Parse footprint from layout
    let footprint = parse_footprint(layout, symbol_table)?;

    // Convert pin positions to engine format (needs footprint for center calculation)
    let pins = convert_pin_positions(&component_ast.pins, layout, &footprint, symbol_table)?;

    // Extract material name from component definition
    // For Stage 1 Silicon: NMOS component -> "Component" (internal material)
    // For PCB: Use metadata value if specified, otherwise "Component"
    let material_name = component_ast
        .metadata
        .as_ref()
        .and_then(|meta| meta.value.clone())
        .unwrap_or_else(|| "Component".into());

    Ok(ComponentDefinition {
        name: component_ast.name.to_string().into(),
        footprint,
        pins,
        material_name,
    })
}

/// Bake a component definition into pre-parsed integers.
///
/// SEMANTIC BAKING: This is the Native Fix for the "Late String Parsing" bottleneck.
/// Call this ONCE during registration, then use the baked result during placement.
///
/// Performance Impact:
/// - Before: O(N × parsing_cost) where N = number of component instances
/// - After: O(1 × parsing_cost) + O(N × integer_addition)
///
/// Example:
/// - Input: "Rectangle(8mm, 4mm, 0.5mm)"
/// - Output: BakedComponent { width_nm: 8_000_000, height_nm: 4_000_000, depth_nm: 500_000 }
pub fn bake_component_definition<S: SymbolTableTrait>(
    component_type: &str,
    symbol_table: &S,
) -> Result<BakedComponent, PlacementError> {
    // Parse the component definition once
    let definition = load_component_definition(component_type, symbol_table)?;

    // Extract dimensions from footprint
    let (width_nm, height_nm, depth_nm) = match definition.footprint {
        Footprint::Rectangle {
            width_nm,
            height_nm,
            depth_nm,
        } => (width_nm, height_nm, depth_nm),
    };

    Ok(BakedComponent {
        name: definition.name,
        width_nm,
        height_nm,
        depth_nm,
        pins: definition.pins,
        material_name: definition.material_name,
    })
}

/// Parse footprint from layout block.
fn parse_footprint<S: SymbolTableTrait>(
    layout: &hwc_parser::LayoutBlock,
    symbol_table: &S,
) -> Result<Footprint, PlacementError> {
    // Extract shape string (e.g., "Rectangle(2.0mm, 1.25mm, 0.5mm)")
    let shape_str = layout.shape.as_ref().ok_or(PlacementError::MissingShape)?;

    // Parse Rectangle(width, height, depth)
    if let Some(rect_params) = shape_str.strip_prefix("Rectangle(") {
        let params = rect_params.trim_end_matches(')');
        let parts: Vec<&str> = params.split(',').map(|s| s.trim()).collect();

        if parts.len() != 3 {
            return Err(PlacementError::InvalidShape {
                shape: shape_str.to_string(),
            });
        }

        // Parse measurements (e.g., "2.0mm" -> 2_000_000 nm)
        let width_nm = parse_measurement_to_nm(parts[0], symbol_table)?;
        let height_nm = parse_measurement_to_nm(parts[1], symbol_table)?;
        let depth_nm = parse_measurement_to_nm(parts[2], symbol_table)?;

        Ok(Footprint::Rectangle {
            width_nm,
            height_nm,
            depth_nm,
        })
    } else {
        Err(PlacementError::UnsupportedShape {
            shape: shape_str.to_string(),
        })
    }
}

/// Parse measurement string to nanometers using the symbol table's unit system.
///
/// This properly delegates to the lexer to tokenize the measurement, then uses
/// the symbol table to resolve custom units, ensuring full support for:
/// - Built-in units (mm, cm, um, nm, m)
/// - Stdlib units (from primitives/units.hw)
/// - User-defined units (from imported libraries or local definitions)
///
/// Parse a measurement string to nanometers WITHOUT invoking the lexer.
///
/// PERFORMANCE FIX: This function was invoking the full lexer for tiny strings like "8mm",
/// causing 24+ lexer invocations per build (3 per component × 8 components).
/// Now uses simple string parsing - 100× faster.
///
/// Examples: "4mm" -> 4_000_000, "500um" -> 500_000
fn parse_measurement_to_nm<S: SymbolTableTrait>(
    measurement: &str,
    symbol_table: &S,
) -> Result<i64, PlacementError> {
    let s = measurement.trim();

    // Find where the number ends and unit begins
    let mut num_end = 0;
    for (i, c) in s.char_indices() {
        if c.is_ascii_digit() || c == '.' || c == '-' {
            num_end = i + 1;
        } else {
            break;
        }
    }

    let num_str = &s[..num_end];
    let unit_str = &s[num_end..];

    let value: f64 = num_str
        .parse()
        .map_err(|_| PlacementError::InvalidMeasurement {
            measurement: measurement.into(),
        })?;

    // Convert to nanometers based on unit
    // PERFORMANCE: Fast path for common units (mm, µm, cm, nm) to avoid symbol table lookup.
    // These hardcoded values MUST match stdlib/primitives/units.hw definitions.
    // Fallback to symbol table for custom/rare units.
    let nm = match unit_str {
        // Fast path: Common distance units (matches stdlib definitions)
        "mm" => (value * 1_000_000.0) as i64, // 1mm = 1e-3m = 1,000,000nm
        "um" | "µm" => (value * 1_000.0) as i64, // 1µm = 1e-6m = 1,000nm
        "cm" => (value * 10_000_000.0) as i64, // 1cm = 1e-2m = 10,000,000nm
        "nm" => value as i64,                 // 1nm = 1e-9m = 1nm

        // Slow path: Custom/rare units via symbol table lookup
        _ => {
            if let Some(unit_def) = symbol_table.resolve_unit_symbol(unit_str) {
                let multiplier = unit_def.multiplier.unwrap_or(1.0);
                // multiplier is relative to meters, convert to nanometers
                (value * multiplier * 1_000_000_000.0) as i64
            } else {
                return Err(PlacementError::InvalidMeasurement {
                    measurement: measurement.into(),
                });
            }
        }
    };

    Ok(nm)
}

/// Convert pin positions from parser format to engine format.
///
/// COORDINATE SYSTEM: Top-Left Anchor
/// - Pin positions in layout block are relative to component's top-left-front corner [0, 0, 0]
/// - Component position in netlist is ALSO the top-left-front corner
/// - No conversion needed - pins stay as direct offsets from anchor
/// - Pin at [1.5mm, 0mm, 0mm] means "1.5mm right of top-left corner"
fn convert_pin_positions<S: SymbolTableTrait>(
    pin_names: &[CompactString],
    layout: &hwc_parser::LayoutBlock,
    _footprint: &Footprint, // No longer needed - no center calculation
    symbol_table: &S,
) -> Result<Vec<PinDefinition>, PlacementError> {
    let mut pins = Vec::new();

    for pin_name in pin_names {
        if let Some(pin_pos) = layout.pin_positions.get(pin_name.as_str()) {
            // Convert from mm to nm - keep as direct offset from top-left anchor
            let x_nm = (pin_pos.x * 1_000_000.0) as i64;
            let y_nm = (pin_pos.y * 1_000_000.0) as i64;
            // If Z is not specified, default to 0 (top surface of component)
            // For PCB components, this is where traces connect
            let z_nm = pin_pos.z.map(|z| (z * 1_000_000.0) as i64).unwrap_or(0);

            // Parse pad shape if specified, otherwise default to small circle
            let pad_shape = if let Some(shape_str) = layout.pad_shapes.get(pin_name.as_str()) {
                parse_pad_shape(shape_str, symbol_table)?
            } else {
                // Default: 0.5mm diameter circular pad
                PadShape::Circle {
                    diameter_nm: 500_000,
                }
            };

            // No adjustment needed - pins are already top-left relative
            // Absolute pin position = component_anchor + pin_offset (simple addition)
            pins.push(PinDefinition {
                name: pin_name.clone(),
                local_offset: Point3D::new(x_nm, y_nm, z_nm),
                pad_shape,
            });
        } else {
            // Pin declared but no position - use top-left corner as default
            pins.push(PinDefinition {
                name: pin_name.clone(),
                local_offset: Point3D::new(0, 0, 0),
                pad_shape: PadShape::Circle {
                    diameter_nm: 500_000,
                },
            });
        }
    }

    Ok(pins)
}

/// Parse pad shape from string (e.g., "Circle(0.5mm)", "Rectangle(1mm, 0.8mm)")
fn parse_pad_shape<S: SymbolTableTrait>(
    shape_str: &str,
    symbol_table: &S,
) -> Result<PadShape, PlacementError> {
    let shape_str = shape_str.trim();

    if let Some(params) = shape_str.strip_prefix("Circle(") {
        let diameter_str = params.trim_end_matches(')').trim();
        let diameter_nm = parse_measurement_to_nm(diameter_str, symbol_table)?;
        Ok(PadShape::Circle { diameter_nm })
    } else if let Some(params) = shape_str.strip_prefix("Rectangle(") {
        let params = params.trim_end_matches(')');
        let parts: Vec<&str> = params.split(',').map(|s| s.trim()).collect();
        if parts.len() != 2 {
            return Err(PlacementError::InvalidShape {
                shape: shape_str.into(),
            });
        }
        let width_nm = parse_measurement_to_nm(parts[0], symbol_table)?;
        let height_nm = parse_measurement_to_nm(parts[1], symbol_table)?;
        Ok(PadShape::Rectangle {
            width_nm,
            height_nm,
        })
    } else if let Some(params) = shape_str.strip_prefix("Obround(") {
        let params = params.trim_end_matches(')');
        let parts: Vec<&str> = params.split(',').map(|s| s.trim()).collect();
        if parts.len() != 2 {
            return Err(PlacementError::InvalidShape {
                shape: shape_str.into(),
            });
        }
        let width_nm = parse_measurement_to_nm(parts[0], symbol_table)?;
        let height_nm = parse_measurement_to_nm(parts[1], symbol_table)?;
        Ok(PadShape::Obround {
            width_nm,
            height_nm,
        })
    } else if let Some(params) = shape_str.strip_prefix("RoundedRect(") {
        let params = params.trim_end_matches(')');
        let parts: Vec<&str> = params.split(',').map(|s| s.trim()).collect();
        if parts.len() != 3 {
            return Err(PlacementError::InvalidShape {
                shape: shape_str.into(),
            });
        }
        let width_nm = parse_measurement_to_nm(parts[0], symbol_table)?;
        let height_nm = parse_measurement_to_nm(parts[1], symbol_table)?;
        let corner_radius_nm = parse_measurement_to_nm(parts[2], symbol_table)?;
        Ok(PadShape::RoundedRect {
            width_nm,
            height_nm,
            corner_radius_nm,
        })
    } else if let Some(params) = shape_str.strip_prefix("Polygon(") {
        // Polygon points separated by commas: Polygon(0mm,0mm, 1mm,0mm, 1mm,1mm)
        // Each pair of values is x,y
        let params = params.trim_end_matches(')');
        let values: Vec<&str> = params.split(',').map(|s| s.trim()).collect();

        if !values.len().is_multiple_of(2) {
            return Err(PlacementError::InvalidShape {
                shape: shape_str.into(),
            });
        }

        let mut points = Vec::new();
        for i in (0..values.len()).step_by(2) {
            let x_nm = parse_measurement_to_nm(values[i], symbol_table)?;
            let y_nm = parse_measurement_to_nm(values[i + 1], symbol_table)?;
            points.push(Point3D::new(x_nm, y_nm, 0));
        }

        Ok(PadShape::Polygon { points })
    } else {
        Err(PlacementError::UnsupportedShape {
            shape: shape_str.into(),
        })
    }
}
