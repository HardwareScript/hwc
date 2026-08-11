//! SPICE subcircuit generation from the typed parser AST.
//!
//! Converts `SubcircuitDefinition`/`SubcircuitElement` AST nodes into SPICE
//! `.subckt` / `.ends` cards, formatting expressions with proper units.

use hwc_parser::{
    BinaryOperator, Expression, SubcircuitDefinition, SubcircuitElement, UnaryOperator, Unit,
};

/// Generate SPICE subcircuit from typed AST
///
/// Converts the compositional SubcircuitDefinition into SPICE netlist format.
/// This replaces the old raw string body emission with proper code generation.
///
/// v0.2.1: Supports `spice_include` for foundry model trust mode.
/// When `spice_include` is present, emits `.include` directive instead of
/// generating inline elements, deferring physics to the foundry model.
pub fn generate_spice_subcircuit(
    output: &mut String,
    subckt: &SubcircuitDefinition,
) -> Result<(), String> {
    // If spice_include is present, emit .include directive only (foundry trust mode)
    if let Some(ref model_path) = subckt.spice_include {
        output.push_str(&format!(
            "* Subcircuit '{}' uses foundry model\n",
            subckt.name.name
        ));
        output.push_str(&format!(".include \"{}\"\n", model_path));
        return Ok(());
    }

    // Otherwise, generate inline .subckt definition
    // Generate .subckt header
    output.push_str(".subckt ");
    output.push_str(&subckt.name.name);

    // Add terminals
    for terminal in &subckt.terminals {
        output.push(' ');
        output.push_str(terminal);
    }

    // Add parameters with defaults
    for param in &subckt.parameters {
        output.push(' ');
        output.push_str(&param.name);
        output.push('=');

        // Format default value
        if let Some(ref default) = param.default_value {
            format_expression_for_spice(output, default)?;
        } else {
            output.push('1'); // SPICE requires a default
        }
    }

    output.push('\n');

    // Generate circuit elements
    for element in &subckt.elements {
        generate_spice_element(output, element)?;
    }

    // Generate .ends footer
    output.push_str(".ends ");
    output.push_str(&subckt.name.name);
    output.push('\n');

    Ok(())
}

/// Generate a single SPICE element from the typed AST
///
/// Uses the element_type to determine the SPICE prefix and format.
/// This is compositional - it doesn't hardcode which types are valid.
pub fn generate_spice_element(
    output: &mut String,
    element: &SubcircuitElement,
) -> Result<(), String> {
    // Determine SPICE prefix from element type
    let prefix = match element.element_type.chars().next() {
        Some('R') => 'R', // Resistor
        Some('C') => 'C', // Capacitor
        Some('L') => 'L', // Inductor
        Some('V') => 'V', // Voltage source
        Some('I') => 'I', // Current source
        Some('M') => 'M', // MOSFET
        Some('X') => 'X', // Subcircuit instance
        Some('D') => 'D', // Diode
        Some('Q') => 'Q', // BJT
        _ => {
            // Default: use first character of element type
            element.element_type.chars().next().unwrap_or('X')
        }
    };

    // Emit: <prefix><name> <nodes...> <params...>
    output.push(prefix);
    output.push_str(&element.name);

    // Add nodes
    for node in &element.nodes {
        output.push(' ');
        output.push_str(&node.to_spice());
    }

    // Add parameters
    for (param_name, param_value) in &element.parameters {
        output.push(' ');

        // For simple "value" parameter, emit value directly
        // For named parameters, emit name=value
        if param_name == "value" {
            format_expression_for_spice(output, param_value)?;
        } else {
            output.push_str(param_name);
            output.push('=');
            format_expression_for_spice(output, param_value)?;
        }
    }

    output.push('\n');

    Ok(())
}

/// Format an expression for SPICE output
///
/// Converts HardwareScript expressions to SPICE-compatible format.
/// Handles units, arithmetic, and parameters.
pub fn format_expression_for_spice(output: &mut String, expr: &Expression) -> Result<(), String> {
    match expr {
        Expression::Literal { value, .. } => {
            output.push_str(&format!("{}", value));
        }
        Expression::FloatLiteral { value, .. } => {
            output.push_str(&format!("{}", value));
        }
        Expression::Measurement { value, unit, .. } => {
            // Convert to SPICE units
            let spice_value = convert_to_spice_units(*value, unit)?;
            output.push_str(&spice_value);
        }
        Expression::Variable { name, .. } => {
            // Parameter reference - emit as-is
            output.push_str(name);
        }
        Expression::Binary {
            left,
            operator,
            right,
            ..
        } => {
            // SPICE uses curly braces for expressions: {L / W}
            output.push('{');
            format_expression_for_spice(output, left)?;
            output.push(' ');
            output.push_str(match operator {
                BinaryOperator::Add => "+",
                BinaryOperator::Subtract => "-",
                BinaryOperator::Multiply => "*",
                BinaryOperator::Divide => "/",
                _ => {
                    return Err(format!(
                        "Unsupported operator {:?} in SPICE expression",
                        operator
                    ))
                }
            });
            output.push(' ');
            format_expression_for_spice(output, right)?;
            output.push('}');
        }
        Expression::Unary {
            operator, operand, ..
        } => {
            match operator {
                UnaryOperator::Negate => output.push('-'),
                UnaryOperator::Plus => output.push('+'),
                _ => {
                    return Err(format!(
                        "Unsupported unary operator {:?} in SPICE",
                        operator
                    ))
                }
            }
            format_expression_for_spice(output, operand)?;
        }
        Expression::Grouped { expression, .. } => {
            output.push('(');
            format_expression_for_spice(output, expression)?;
            output.push(')');
        }
        _ => {
            return Err(format!(
                "Unsupported expression type in SPICE generation: {:?}",
                expr
            ));
        }
    }

    Ok(())
}

/// Convert measurement to SPICE units
///
/// Uses the Unit's to_spice_suffix() method instead of hardcoded pattern matching.
/// This fails loudly if a unit doesn't have a SPICE representation.
pub fn convert_to_spice_units(value: f64, unit: &Unit) -> Result<String, String> {
    let suffix = unit.to_spice_suffix()?;
    Ok(format!("{}{}", value, suffix))
}
