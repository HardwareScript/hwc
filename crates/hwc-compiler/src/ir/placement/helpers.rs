//! Helper functions for placement operations.

use super::super::errors::IrError;

/// Parse shape dimensions from shape string (v0.1.6)
/// Supports:
/// - "Rectangle(w, h, d)" -> (w, h, d)
/// - "Box(w, h, d)" -> (w, h, d)
/// - "Circle(dia)" -> (dia, dia, 0)
/// - "Circle(dia, d)" -> (dia, dia, d)
pub fn parse_rectangle_dimensions(shape_str: &str) -> Option<(i64, i64, i64)> {
    // Simple regex-free parsing
    if shape_str.starts_with("Rectangle(") && shape_str.ends_with(')') {
        let params = &shape_str[10..shape_str.len() - 1]; // Extract "w, h, d"
        let parts: Vec<&str> = params.split(',').map(|s| s.trim()).collect();
        if parts.len() != 3 {
            return None;
        }
        let width_nm = parse_measurement_to_nm(parts[0])?;
        let height_nm = parse_measurement_to_nm(parts[1])?;
        let depth_nm = parse_measurement_to_nm(parts[2])?;
        return Some((width_nm, height_nm, depth_nm));
    }

    if shape_str.starts_with("Box(") && shape_str.ends_with(')') {
        let params = &shape_str[4..shape_str.len() - 1]; // Extract "w, h, d"
        let parts: Vec<&str> = params.split(',').map(|s| s.trim()).collect();
        if parts.len() != 3 {
            return None;
        }
        let width_nm = parse_measurement_to_nm(parts[0])?;
        let height_nm = parse_measurement_to_nm(parts[1])?;
        let depth_nm = parse_measurement_to_nm(parts[2])?;
        return Some((width_nm, height_nm, depth_nm));
    }

    if shape_str.starts_with("Circle(") && shape_str.ends_with(')') {
        let params = &shape_str[7..shape_str.len() - 1]; // Extract "dia" or "dia, d"
        let parts: Vec<&str> = params.split(',').map(|s| s.trim()).collect();
        if parts.len() == 1 {
            let dia_nm = parse_measurement_to_nm(parts[0])?;
            return Some((dia_nm, dia_nm, 0));
        } else if parts.len() == 2 {
            let dia_nm = parse_measurement_to_nm(parts[0])?;
            let depth_nm = parse_measurement_to_nm(parts[1])?;
            return Some((dia_nm, dia_nm, depth_nm));
        }
    }

    None
}

/// Parse a measurement string to nanometers
/// Examples: "4mm" -> 4_000_000, "500um" -> 500_000
pub fn parse_measurement_to_nm(s: &str) -> Option<i64> {
    let s = s.trim();

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

    let value: f64 = num_str.parse().ok()?;

    let nm = match unit_str {
        "mm" => (value * 1_000_000.0) as i64,
        "um" | "µm" => (value * 1_000.0) as i64,
        "cm" => (value * 10_000_000.0) as i64,
        "nm" => value as i64,
        _ => return None,
    };

    Some(nm)
}

/// Add an offset to a coordinate (for array unrolling)
pub fn offset_coordinate(
    coord: &hwc_parser::Coordinate,
    offset_x_nm: i64,
    offset_y_nm: i64,
) -> Result<hwc_parser::Coordinate, IrError> {
    match coord {
        hwc_parser::Coordinate::Declarative { x, y, z, span } => {
            // Add offset to X and Y expressions
            let new_x = add_offset_to_expression(x, offset_x_nm)?;
            let new_y = add_offset_to_expression(y, offset_y_nm)?;

            Ok(hwc_parser::Coordinate::Declarative {
                x: new_x,
                y: new_y,
                z: z.clone(),
                span: *span,
            })
        }
        hwc_parser::Coordinate::Positional { .. } => Err(IrError::PlacementError(
            "Positional coordinates not supported for arrays (use declarative syntax)".into(),
        )),
        hwc_parser::Coordinate::Relative(_) => Err(IrError::PlacementError(
            "Relative coordinates not yet supported for arrays".into(),
        )),
    }
}

/// Add an offset (in nanometers) to an expression
pub fn add_offset_to_expression(
    expr: &hwc_parser::Expression,
    offset_nm: i64,
) -> Result<hwc_parser::Expression, IrError> {
    if offset_nm == 0 {
        return Ok(expr.clone());
    }

    // Convert offset to millimeters for the expression
    let offset_mm = offset_nm as f64 / 1_000_000.0;

    // Create an addition expression: original + offset
    Ok(hwc_parser::Expression::Binary {
        left: Box::new(expr.clone()),
        operator: hwc_parser::BinaryOperator::Add,
        right: Box::new(hwc_parser::Expression::Measurement {
            value: offset_mm,
            unit: hwc_parser::Unit::Millimeter,
            span: hwc_parser::Span { start: 0, end: 0 },
        }),
        span: hwc_parser::Span { start: 0, end: 0 },
    })
}

/// Helper function to extract all placements from layout statements
/// This recursively flattens for loops and if statements to get all placements
pub fn extract_placements_from_layout_statements(
    statements: &[hwc_parser::LayoutStatement],
) -> Vec<hwc_parser::ModuleInternalPlacement> {
    use hwc_parser::LayoutStatement;

    let mut placements = Vec::new();

    for statement in statements {
        match statement {
            LayoutStatement::Placement(p) => {
                placements.push(p.clone());
            }
            LayoutStatement::For { body, .. } => {
                // Recursively extract placements from for loop body
                // Note: This doesn't evaluate the loop, just collects all placements
                placements.extend(extract_placements_from_layout_statements(body));
            }
            LayoutStatement::If {
                then_body,
                else_body,
                ..
            } => {
                // Collect placements from both branches
                placements.extend(extract_placements_from_layout_statements(then_body));
                if let Some(else_statements) = else_body {
                    placements.extend(extract_placements_from_layout_statements(else_statements));
                }
            }
        }
    }

    placements
}
