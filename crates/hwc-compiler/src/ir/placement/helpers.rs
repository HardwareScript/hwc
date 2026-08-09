//! Helper functions for placement operations.

use super::super::errors::IrError;

/// Parse shape dimensions from shape string (v0.1.6)
/// Supports:
/// - "Rectangle(w, h, d)" -> (w, h, d)
/// - "Box(w, h, d)" -> (w, h, d)
/// - "Circle(dia)" -> (dia, dia, 0)
/// - "Circle(dia, d)" -> (dia, dia, d)
pub fn parse_rectangle_dimensions(
    shape_str: &str,
    symbol_table: &crate::SymbolTable,
) -> Option<(i64, i64, i64)> {
    // Simple regex-free parsing
    if shape_str.starts_with("Rectangle(") && shape_str.ends_with(')') {
        let params = &shape_str[10..shape_str.len() - 1]; // Extract "w, h, d"
        let parts: Vec<&str> = params.split(',').map(|s| s.trim()).collect();
        if parts.len() != 3 {
            return None;
        }
        let width_nm = parse_measurement_to_nm(parts[0], symbol_table)?;
        let height_nm = parse_measurement_to_nm(parts[1], symbol_table)?;
        let depth_nm = parse_measurement_to_nm(parts[2], symbol_table)?;
        return Some((width_nm, height_nm, depth_nm));
    }

    if shape_str.starts_with("Box(") && shape_str.ends_with(')') {
        let params = &shape_str[4..shape_str.len() - 1]; // Extract "w, h, d"
        let parts: Vec<&str> = params.split(',').map(|s| s.trim()).collect();
        if parts.len() != 3 {
            return None;
        }
        let width_nm = parse_measurement_to_nm(parts[0], symbol_table)?;
        let height_nm = parse_measurement_to_nm(parts[1], symbol_table)?;
        let depth_nm = parse_measurement_to_nm(parts[2], symbol_table)?;
        return Some((width_nm, height_nm, depth_nm));
    }

    if shape_str.starts_with("Circle(") && shape_str.ends_with(')') {
        let params = &shape_str[7..shape_str.len() - 1]; // Extract "dia" or "dia, d"
        let parts: Vec<&str> = params.split(',').map(|s| s.trim()).collect();
        if parts.len() == 1 {
            let dia_nm = parse_measurement_to_nm(parts[0], symbol_table)?;
            return Some((dia_nm, dia_nm, 0));
        } else if parts.len() == 2 {
            let dia_nm = parse_measurement_to_nm(parts[0], symbol_table)?;
            let depth_nm = parse_measurement_to_nm(parts[1], symbol_table)?;
            return Some((dia_nm, dia_nm, depth_nm));
        }
    }

    None
}

/// Resolve parameterized shape string by substituting parameter values
/// For example: "Rectangle(w, h, 0nm)" with parameters {w: 600nm, h: 600nm}
/// becomes "Rectangle(600nm, 600nm, 0nm)"
/// v0.1.10: Now supports expressions (including variables) in parameters
pub fn resolve_parameterized_shape(
    shape_str: &str,
    parameters: &[hwc_parser::Parameter],
    symbol_table: &crate::SymbolTable,
    eval_context: &hwc_parser::EvaluationContext,
) -> Option<String> {
    use hwc_parser::ParameterValue;

    // Build a map of parameter names to their resolved values
    let mut param_map = std::collections::HashMap::new();

    for param in parameters {
        let hwc_parser::Parameter::Keyword { name, value } = param;
        let resolved_value = match value {
            ParameterValue::Measurement(m) => {
                let nm = symbol_table.measurement_to_nm(m).ok()?;
                // Convert back to a string with nm suffix
                format!("{}nm", nm)
            }
            ParameterValue::Expression(expr) => {
                // Evaluate the expression using the evaluation context (supports variables!)
                let nm = crate::ir::conversions::evaluate_expression_to_nm(
                    expr,
                    symbol_table,
                    eval_context,
                )
                .ok()?;
                format!("{}nm", nm)
            }
            ParameterValue::String(s) => s.to_string(),
            ParameterValue::Number(n) => n.to_string(),
        };
        param_map.insert(name.as_str(), resolved_value);
    }

    // If no parameters, return original
    if param_map.is_empty() {
        return Some(shape_str.to_string());
    }

    // Simple string substitution for shape parameters
    // This handles patterns like "Rectangle(w, h, depth)" or "Circle(diameter)"
    let mut result = shape_str.to_string();

    // Extract the part inside parentheses
    if let Some(start_idx) = result.find('(') {
        if let Some(end_idx) = result.rfind(')') {
            let prefix = &result[..start_idx + 1];
            let params_str = &result[start_idx + 1..end_idx];
            let suffix = &result[end_idx..];

            // Split by commas and substitute each parameter
            let parts: Vec<&str> = params_str.split(',').map(|s| s.trim()).collect();
            let mut resolved_parts = Vec::new();

            for part in parts {
                // Check if this part is a parameter name
                if let Some(value) = param_map.get(part) {
                    resolved_parts.push(value.clone());
                } else {
                    // Keep as-is (probably already a literal value)
                    resolved_parts.push(part.to_string());
                }
            }

            result = format!("{}{}{}", prefix, resolved_parts.join(", "), suffix);
        }
    }

    Some(result)
}

/// Parse a measurement string to nanometers via the SymbolTable.
/// Delegates to the canonical `SymbolTable::measurement_to_nm()` for unit resolution,
/// supporting both built-in units and custom/user-defined units.
pub fn parse_measurement_to_nm(s: &str, symbol_table: &crate::SymbolTable) -> Option<i64> {
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

    // Build a Measurement and delegate to symbol table for canonical conversion
    let unit = match unit_str {
        "mm" => hwc_parser::Unit::Millimeter,
        "um" | "µm" => hwc_parser::Unit::Micrometer,
        "cm" => hwc_parser::Unit::Centimeter,
        "nm" => hwc_parser::Unit::Nanometer,
        _ => hwc_parser::Unit::Custom(unit_str.into()),
    };

    let measurement = hwc_parser::Measurement {
        value,
        unit,
        span: hwc_parser::Span { start: 0, end: 0 },
    };
    symbol_table.measurement_to_nm(&measurement).ok()
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
        hwc_parser::Coordinate::Positional { .. } => Err(IrError::PlacementConstraint {
            message: "Positional coordinates not supported for arrays (use declarative syntax)"
                .into(),
            component: "array".into(),
        }),
        hwc_parser::Coordinate::Relative(_) => Err(IrError::PlacementConstraint {
            message: "Relative coordinates not yet supported for arrays".into(),
            component: "array".into(),
        }),
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
) -> Vec<hwc_parser::ast::arena::ModuleInternalId> {
    use hwc_parser::LayoutStatement;

    let mut placement_ids = Vec::new();

    for statement in statements {
        match statement {
            LayoutStatement::Placement(id) => {
                placement_ids.push(*id);
            }
            LayoutStatement::For { body, .. } => {
                placement_ids.extend(extract_placements_from_layout_statements(body));
            }
            LayoutStatement::If {
                then_body,
                else_body,
                ..
            } => {
                placement_ids.extend(extract_placements_from_layout_statements(then_body));
                if let Some(else_statements) = else_body {
                    placement_ids
                        .extend(extract_placements_from_layout_statements(else_statements));
                }
            }
        }
    }

    placement_ids
}
