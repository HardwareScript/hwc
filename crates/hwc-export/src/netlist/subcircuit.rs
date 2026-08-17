//! SPICE subcircuit generation from the typed parser AST.
//!
//! Converts `SubcircuitDefinition`/`SubcircuitElement` AST nodes into SPICE
//! `.subckt` / `.ends` cards, formatting expressions with proper units.

use hwc_parser::{
    BinaryOperator, Expression, SubcircuitDefinition, SubcircuitElement, UnaryOperator, Unit,
};
use hwc_types::UnitRegistry;

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
    unit_registry: &UnitRegistry,
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
            format_expression_for_spice(output, default, unit_registry)?;
        } else {
            output.push('1'); // SPICE requires a default
        }
    }

    output.push('\n');

    // Generate circuit elements
    for element in &subckt.elements {
        generate_spice_element(output, element, unit_registry)?;
    }

    // Generate .ends footer
    output.push_str(".ends ");
    output.push_str(&subckt.name.name);
    output.push('\n');

    Ok(())
}

/// Classification of SPICE circuit elements with their standardized card syntax.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpiceElementKind {
    Resistor,
    Capacitor,
    Inductor,
    VoltageSource,
    CurrentSource,
    Diode,
    Mosfet,
    Bjt,
    SubcircuitInstance,
}

impl SpiceElementKind {
    /// Parse element kind from AST element type identifier
    pub fn from_type_name(name: &str) -> Self {
        match name {
            "Resistor" | "R" => Self::Resistor,
            "Capacitor" | "C" => Self::Capacitor,
            "Inductor" | "L" => Self::Inductor,
            "VoltageSource" | "V" => Self::VoltageSource,
            "CurrentSource" | "I" => Self::CurrentSource,
            "Diode" | "D" => Self::Diode,
            "Mosfet" | "MOSFET" | "M" => Self::Mosfet,
            "Bjt" | "BJT" | "Q" => Self::Bjt,
            _ => Self::SubcircuitInstance,
        }
    }

    /// SPICE component prefix character
    pub fn prefix(&self) -> char {
        match self {
            Self::Resistor => 'R',
            Self::Capacitor => 'C',
            Self::Inductor => 'L',
            Self::VoltageSource => 'V',
            Self::CurrentSource => 'I',
            Self::Diode => 'D',
            Self::Mosfet => 'M',
            Self::Bjt => 'Q',
            Self::SubcircuitInstance => 'X',
        }
    }
}

/// Generate a single SPICE element from the typed AST
///
/// Uses the typed `SpiceElementKind` to format positional values and named parameters.
pub fn generate_spice_element(
    output: &mut String,
    element: &SubcircuitElement,
    unit_registry: &UnitRegistry,
) -> Result<(), String> {
    let kind = SpiceElementKind::from_type_name(&element.element_type);

    // Emit: <prefix><name> <nodes...>
    output.push(kind.prefix());
    output.push_str(&element.name);

    for node in &element.nodes {
        output.push(' ');
        output.push_str(&node.to_spice());
    }

    // Separate primary positional "value" from named parameters
    let (value_param, named_params): (Vec<_>, Vec<_>) = element
        .parameters
        .iter()
        .partition(|(name, _)| name.as_str() == "value");

    // Emit positional value first for passives (R, C, L)
    for (_, val) in value_param {
        output.push(' ');
        format_expression_for_spice(output, val, unit_registry)?;
    }

    // Emit named parameters (e.g. tc1=..., tc2=..., W=..., L=...)
    for (param_name, param_value) in named_params {
        output.push(' ');
        output.push_str(param_name);
        output.push('=');
        format_expression_for_spice(output, param_value, unit_registry)?;
    }

    output.push('\n');

    Ok(())
}

/// Format an expression for SPICE output
///
/// Converts HardwareScript expressions to SPICE-compatible format.
/// Handles units, arithmetic, and parameters.
///
/// **CRITICAL FIX (v0.2.1):**
/// When inside a math expression context (within { }), use PURE SCIENTIFIC NOTATION
/// without unit suffixes. SPICE parsers treat "350ohm" as a variable name, not a value.
/// Only emit unit suffixes in top-level parameter values (not inside expressions).
///
/// **DATA-DRIVEN UNIT CONVERSION:**
/// Uses UnitRegistry for all unit conversions - no hardcoded multipliers.
pub fn format_expression_for_spice(
    output: &mut String,
    expr: &Expression,
    unit_registry: &UnitRegistry,
) -> Result<(), String> {
    format_expression_for_spice_internal(output, expr, unit_registry, false)
}

fn format_expression_for_spice_internal(
    output: &mut String,
    expr: &Expression,
    unit_registry: &UnitRegistry,
    inside_math: bool,
) -> Result<(), String> {
    match expr {
        Expression::Literal { value, .. } => {
            output.push_str(&format!("{}", value));
        }
        Expression::FloatLiteral { value, .. } => {
            output.push_str(&format!("{}", value));
        }
        Expression::Measurement { value, unit, ..} => {
            if inside_math {
                // Inside math expressions: use PURE scientific notation (no unit suffixes)
                // Convert unit to its base SI value using UnitRegistry
                let unit_symbol = unit.to_symbol();
                let base_value = unit_registry
                    .to_base_si(*value, &unit_symbol)
                    .ok_or_else(|| {
                        format!(
                            "Cannot convert unit '{}' to base SI - not defined in unit registry",
                            unit_symbol
                        )
                    })?;
                output.push_str(&format!("{:.6e}", base_value));
            } else {
                // Top-level parameters: convert to base SI and use SPICE suffix
                // SPICE elements like R, C, L use positional value parameters where:
                // - The unit is IMPLIED by the element type (R=ohm, C=farad, L=henry)
                // - Only SI prefixes are used: f, p, n, u, m, k, meg, g
                // - NO full unit names like "ohm", "F", "H"
                let unit_symbol = unit.to_symbol();
                let base_value = unit_registry
                    .to_base_si(*value, &unit_symbol)
                    .ok_or_else(|| {
                        format!(
                            "Cannot convert unit '{}' to base SI - not defined in unit registry",
                            unit_symbol
                        )
                    })?;
                
                // Format with appropriate SPICE SI prefix
                output.push_str(&format_value_with_spice_prefix(base_value));
            }
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
            // CRITICAL: Only emit ONE level of braces, not nested {{...}}
            if !inside_math {
                output.push('{');
            }
            format_expression_for_spice_internal(output, left, unit_registry, true)?;
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
            format_expression_for_spice_internal(output, right, unit_registry, true)?;
            if !inside_math {
                output.push('}');
            }
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
            format_expression_for_spice_internal(output, operand, unit_registry, inside_math)?;
        }
        Expression::Grouped { expression, .. } => {
            output.push('(');
            format_expression_for_spice_internal(output, expression, unit_registry, inside_math)?;
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

/// Format a base SI value with appropriate SPICE prefix
///
/// SPICE uses single-letter prefixes: f, p, n, u, m, k, meg, g
/// This function converts a base SI value to the most readable format.
///
/// Examples:
/// - 1e-15 → "1f"
/// - 1e-12 → "1p"
/// - 1e-9  → "1n"
/// - 1e-6  → "1u"
/// - 1e-3  → "1m"
/// - 1.0   → "1"
/// - 1e3   → "1k"
/// - 1e6   → "1meg"
/// - 1e9   → "1g"
fn format_value_with_spice_prefix(value: f64) -> String {
    let abs_val = value.abs();
    
    // Handle special cases
    if abs_val == 0.0 {
        return "0".to_string();
    }
    
    // Determine appropriate prefix based on magnitude
    let (scaled, suffix) = if abs_val >= 1e9 {
        (value / 1e9, "g")
    } else if abs_val >= 1e6 {
        (value / 1e6, "meg")
    } else if abs_val >= 1e3 {
        (value / 1e3, "k")
    } else if abs_val >= 1.0 {
        (value, "")
    } else if abs_val >= 1e-3 {
        (value / 1e-3, "m")
    } else if abs_val >= 1e-6 {
        (value / 1e-6, "u")
    } else if abs_val >= 1e-9 {
        (value / 1e-9, "n")
    } else if abs_val >= 1e-12 {
        (value / 1e-12, "p")
    } else {
        (value / 1e-15, "f")
    };
    
    // Format the scaled value, removing unnecessary decimal zeros
    let formatted = if scaled.fract() == 0.0 && scaled.abs() < 1e10 {
        format!("{:.0}", scaled)
    } else {
        format!("{:.6}", scaled).trim_end_matches('0').trim_end_matches('.').to_string()
    };
    
    format!("{}{}", formatted, suffix)
}
