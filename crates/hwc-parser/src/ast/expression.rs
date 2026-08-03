use super::Span;
use compact_str::CompactString;
use serde::{Deserialize, Serialize};

/// Mathematical expression that can be evaluated at compile time
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Expression {
    /// Integer literal: 42
    Literal { value: i64, span: Span },
    /// Float literal: 3.14 (v0.1.7)
    FloatLiteral { value: f64, span: Span },
    /// Measurement literal: 10mm, 5cm
    Measurement {
        value: f64,
        unit: super::Unit,
        span: Span,
    },
    /// Percentage literal: 50%, 25%
    Percentage { value: f64, span: Span },
    /// Variable reference: i, x, count
    Variable { name: CompactString, span: Span },
    /// Binary operation: a + b, x * 2, etc.
    Binary {
        left: Box<Expression>,
        operator: BinaryOperator,
        right: Box<Expression>,
        span: Span,
    },
    /// Unary operation: -x, +x
    Unary {
        operator: UnaryOperator,
        operand: Box<Expression>,
        span: Span,
    },
    /// Parenthesized expression: (x + 1)
    Grouped {
        expression: Box<Expression>,
        span: Span,
    },
    /// Anchor reference: ComponentName.edge (v0.1.6 Sprint 3.8)
    /// Allows mixing relative and absolute coordinates per axis
    /// Example: [x: GroundPlane.right + 1mm, y: 5mm, z: 2]
    AnchorReference {
        anchor: super::AnchorReference,
        edge: super::Edge,
        span: Span,
    },
    /// Coordinate literal as an expression (v0.2.0)
    /// Allows coordinate math: PMOS_Region.center - [200nm, 0nm]
    /// Example: at: anchor.center + [1mm, 2mm, 0mm]
    Coordinate {
        coord: Box<super::Coordinate>,
        span: Span,
    },
    /// Function call: sin(x), cos(angle), sqrt(value) (v0.2.1)
    FunctionCall {
        name: CompactString,
        arguments: Vec<Expression>,
        span: Span,
    },
}

/// Binary operators for expressions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BinaryOperator {
    // Arithmetic operators
    Add,      // +
    Subtract, // -
    Multiply, // *
    Divide,   // /
    Modulo,   // % (via 'mod' keyword)
    
    // Comparison operators (v0.2.1: for compile-time conditionals)
    Equal,              // == (requires double equals for comparison)
    NotEqual,           // !=
    LessThan,           // <
    GreaterThan,        // >
    LessThanOrEqual,    // <=
    GreaterThanOrEqual, // >=
    
    // Boolean operators (v0.2.1: for compile-time conditionals)
    And,  // and (logical AND)
    Or,   // or (logical OR)
}

/// Unary operators for expressions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnaryOperator {
    Negate, // - (arithmetic negation)
    Plus,   // + (arithmetic positive)
    Not,    // not (logical NOT) (v0.2.1)
}

impl Expression {
    /// Get the span of this expression
    pub fn span(&self) -> Span {
        match self {
            Expression::Literal { span, .. }
            | Expression::FloatLiteral { span, .. }
            | Expression::Measurement { span, .. }
            | Expression::Percentage { span, .. }
            | Expression::Variable { span, .. }
            | Expression::Binary { span, .. }
            | Expression::Unary { span, .. }
            | Expression::Grouped { span, .. }
            | Expression::AnchorReference { span, .. }
            | Expression::Coordinate { span, .. }
            | Expression::FunctionCall { span, .. } => *span,
        }
    }

    /// Check if this expression is a simple literal
    pub fn as_literal(&self) -> Option<i64> {
        match self {
            Expression::Literal { value, .. } => Some(*value),
            _ => None,
        }
    }

    /// Check if this expression is a simple variable
    pub fn as_variable(&self) -> Option<&str> {
        match self {
            Expression::Variable { name, .. } => Some(name.as_str()),
            _ => None,
        }
    }

    /// Check if this expression contains anchor references (needs constraint solving)
    pub fn contains_anchor_reference(&self) -> bool {
        match self {
            Expression::AnchorReference { .. } => true,
            Expression::Coordinate { coord, .. } => {
                // Check if the coordinate itself contains anchor references
                match coord.as_ref() {
                    super::Coordinate::Relative(_) => true,
                    super::Coordinate::Positional { x, y, z, .. } => {
                        x.contains_anchor_reference()
                            || y.contains_anchor_reference()
                            || z.contains_anchor_reference()
                    }
                    super::Coordinate::Declarative { x, y, z, .. } => {
                        x.contains_anchor_reference()
                            || y.contains_anchor_reference()
                            || z.contains_anchor_reference()
                    }
                }
            }
            Expression::Binary { left, right, .. } => {
                left.contains_anchor_reference() || right.contains_anchor_reference()
            }
            Expression::Unary { operand, .. } => operand.contains_anchor_reference(),
            Expression::Grouped { expression, .. } => expression.contains_anchor_reference(),
            Expression::FunctionCall { arguments, .. } => {
                arguments.iter().any(|arg| arg.contains_anchor_reference())
            }
            _ => false,
        }
    }
}

impl BinaryOperator {
    /// Get the precedence of this operator (higher = tighter binding)
    /// Precedence levels (from lowest to highest):
    /// 0: Boolean operators (or)
    /// 1: Boolean operators (and)
    /// 2: Comparison operators (==, !=, <, >, <=, >=)
    /// 3: Addition and subtraction (+, -)
    /// 4: Multiplication, division, modulo (*, /, mod)
    pub fn precedence(&self) -> u8 {
        match self {
            // Boolean OR (lowest precedence - evaluates last)
            BinaryOperator::Or => 0,
            // Boolean AND (higher than OR)
            BinaryOperator::And => 1,
            // Comparison operators
            BinaryOperator::Equal
            | BinaryOperator::NotEqual
            | BinaryOperator::LessThan
            | BinaryOperator::GreaterThan
            | BinaryOperator::LessThanOrEqual
            | BinaryOperator::GreaterThanOrEqual => 2,
            // Addition and subtraction
            BinaryOperator::Add | BinaryOperator::Subtract => 3,
            // Multiplication, division, modulo (highest precedence)
            BinaryOperator::Multiply | BinaryOperator::Divide | BinaryOperator::Modulo => 4,
        }
    }

    /// Apply this operator to two values
    /// Returns an error if the operation is invalid (overflow, division by zero, etc.)
    pub fn apply(&self, left: i64, right: i64) -> Result<i64, String> {
        match self {
            BinaryOperator::Add => left
                .checked_add(right)
                .ok_or("Integer overflow in addition"),
            BinaryOperator::Subtract => left
                .checked_sub(right)
                .ok_or("Integer overflow in subtraction"),
            BinaryOperator::Multiply => left
                .checked_mul(right)
                .ok_or("Integer overflow in multiplication"),
            BinaryOperator::Divide => {
                if right == 0 {
                    Err("Division by zero")
                } else {
                    Ok(left / right)
                }
            }
            BinaryOperator::Modulo => {
                if right == 0 {
                    Err("Modulo by zero")
                } else {
                    Ok(left % right)
                }
            }
            // Comparison operators return 1 for true, 0 for false
            BinaryOperator::Equal => Ok(if left == right { 1 } else { 0 }),
            BinaryOperator::NotEqual => Ok(if left != right { 1 } else { 0 }),
            BinaryOperator::LessThan => Ok(if left < right { 1 } else { 0 }),
            BinaryOperator::GreaterThan => Ok(if left > right { 1 } else { 0 }),
            BinaryOperator::LessThanOrEqual => Ok(if left <= right { 1 } else { 0 }),
            BinaryOperator::GreaterThanOrEqual => Ok(if left >= right { 1 } else { 0 }),
            // Boolean operators (treat non-zero as true, zero as false)
            BinaryOperator::And => Ok(if left != 0 && right != 0 { 1 } else { 0 }),
            BinaryOperator::Or => Ok(if left != 0 || right != 0 { 1 } else { 0 }),
        }
        .map_err(|s| s.to_string())
    }
}

impl UnaryOperator {
    /// Apply this operator to a value
    pub fn apply(&self, value: i64) -> Result<i64, String> {
        match self {
            UnaryOperator::Negate => value.checked_neg().ok_or("Integer overflow in negation"),
            UnaryOperator::Plus => Ok(value),
            UnaryOperator::Not => Ok(if value == 0 { 1 } else { 0 }), // Logical NOT: !0 = 1, !non-zero = 0
        }
        .map_err(|s| s.to_string())
    }
}

use rustc_hash::FxHashMap;
use std::fmt;

/// Result of evaluating an expression
///
/// ## Type System Invariant: Preserve Unit Information Throughout Compilation
///
/// The compiler maintains strict dimensional correctness by storing typed values:
/// - **`Value::Number`**: Dimensionless integers (loop indices, multipliers, counts)
/// - **`Value::Float`**: Dimensionless floating-point numbers (ratios, scaling factors)
/// - **`Value::Measurement`**: Physical quantities with explicit units (50µm, 200nm, 1mm)
/// - **`Value::Percentage`**: Relative positioning values (50%, 25%)
///
/// PDK constants like `pdk.edge_clearance` are stored as `Value::Measurement` with
/// their original units preserved. This ensures expressions like `pdk.edge_clearance + 200µm`
/// are evaluated with full dimensional analysis, preventing mathematically invalid operations
/// like adding bare scalars to physical distances.
///
/// Final conversion to absolute nanometer coordinates happens in `conversions.rs` via
/// `to_nanometers()`, maintaining a clean separation between the parser (AST/evaluation)
/// and the physical engine (coordinate resolution).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Value {
    /// Integer value (grid index or dimensionless number)
    Number(i64),
    /// Float value (multiplier or ratio) (v0.1.7)
    Float(f64),
    /// Physical measurement with unit
    Measurement { value: f64, unit: super::Unit },
    /// Percentage value (for relative positioning)
    Percentage(f64),
}

impl Value {
    /// Convert to float, supporting both Number and Float variants
    pub fn as_number(&self) -> Result<f64, String> {
        match self {
            Value::Number(n) => Ok(*n as f64),
            Value::Float(f) => Ok(*f),
            Value::Measurement { .. } => Err("Expected number but got measurement".into()),
            Value::Percentage(_) => Err("Expected number but got percentage".into()),
        }
    }

    /// Convert to integer, failing if this is a measurement or percentage
    pub fn as_integer(&self) -> Result<i64, String> {
        match self {
            Value::Number(n) => Ok(*n),
            Value::Float(f) => Ok(*f as i64),
            Value::Measurement { .. } => Err("Expected integer but got measurement".into()),
            Value::Percentage(_) => Err("Expected integer but got percentage".into()),
        }
    }

    /// Convert to nanometers
    /// For percentages, requires the reference dimension
    pub fn to_nanometers(&self) -> Result<i64, String> {
        match self {
            Value::Number(n) => Ok(*n), // Already a number, assume it's in nm if used as distance
            Value::Float(f) => Ok(*f as i64),
            Value::Measurement { value, unit } => match unit {
                super::Unit::Millimeter => Ok((value * 1_000_000.0) as i64),
                super::Unit::Centimeter => Ok((value * 10_000_000.0) as i64),
                super::Unit::Micrometer => Ok((value * 1_000.0) as i64),
                super::Unit::Nanometer => Ok(*value as i64),
                super::Unit::Picometer => Ok((*value * 0.001) as i64),
                _ => Err(format!("Cannot convert {:?} to nanometers", unit)),
            },
            Value::Percentage(_) => {
                Err("Cannot convert percentage to nanometers without reference dimension".into())
            }
        }
    }

    /// Convert to nanometers with a reference dimension (for percentages)
    pub fn to_nanometers_with_ref(&self, reference_nm: i64) -> Result<i64, String> {
        match self {
            Value::Number(n) => Ok(*n),
            Value::Float(f) => Ok(*f as i64),
            Value::Measurement { value, unit } => match unit {
                super::Unit::Millimeter => Ok((value * 1_000_000.0) as i64),
                super::Unit::Centimeter => Ok((value * 10_000_000.0) as i64),
                super::Unit::Micrometer => Ok((value * 1_000.0) as i64),
                super::Unit::Nanometer => Ok(*value as i64),
                super::Unit::Picometer => Ok((*value * 0.001) as i64),
                _ => Err(format!("Cannot convert {:?} to nanometers", unit)),
            },
            Value::Percentage(pct) => {
                // Convert percentage to nanometers: 50% of 100mm = 50mm
                Ok(((pct / 100.0) * reference_nm as f64) as i64)
            }
        }
    }

    /// Convert to picometers (i64) — the engine's internal coordinate representation.
    /// Maximum addressable range: +/-9,220 km.
    pub fn to_picometers(&self) -> Result<i64, String> {
        match self {
            Value::Number(n) => Ok(*n), // Already a number, assume pm if used as distance
            Value::Float(f) => Ok(*f as i64),
            Value::Measurement { value, unit } => match unit {
                super::Unit::Millimeter => Ok((value * 1_000_000.0) as i64),
                super::Unit::Centimeter => Ok((value * 10_000_000.0) as i64),
                super::Unit::Micrometer => Ok((value * 1_000.0) as i64),
                super::Unit::Nanometer => Ok((*value * 1_000.0) as i64),
                super::Unit::Picometer => Ok(*value as i64),
                _ => Err(format!("Cannot convert {:?} to picometers", unit)),
            },
            Value::Percentage(_) => {
                Err("Cannot convert percentage to picometers without reference dimension".into())
            }
        }
    }

    /// Convert to picometers with a reference dimension (for percentages)
    pub fn to_picometers_with_ref(&self, reference_pm: i64) -> Result<i64, String> {
        match self {
            Value::Number(n) => Ok(*n),
            Value::Float(f) => Ok(*f as i64),
            Value::Measurement { value, unit } => match unit {
                super::Unit::Millimeter => Ok((value * 1_000_000.0) as i64),
                super::Unit::Centimeter => Ok((value * 10_000_000.0) as i64),
                super::Unit::Micrometer => Ok((value * 1_000.0) as i64),
                super::Unit::Nanometer => Ok((*value * 1_000.0) as i64),
                super::Unit::Picometer => Ok(*value as i64),
                _ => Err(format!("Cannot convert {:?} to picometers", unit)),
            },
            Value::Percentage(pct) => Ok(((pct / 100.0) * reference_pm as f64) as i64),
        }
    }

    /// Check if this is a measurement
    pub fn is_measurement(&self) -> bool {
        matches!(self, Value::Measurement { .. })
    }

    /// Check if this is a percentage
    pub fn is_percentage(&self) -> bool {
        matches!(self, Value::Percentage(_))
    }

    /// Check if this is a measurement or percentage (valid for X/Y coordinates)
    pub fn is_physical_or_relative(&self) -> bool {
        matches!(self, Value::Measurement { .. } | Value::Percentage(_))
    }
}

/// Context for evaluating expressions with strongly-typed variable bindings
///
/// ## Architectural Principle: Preserve Unit Information Throughout Compilation
///
/// This context stores `Value` enums (not bare `i64`) to maintain dimensional correctness:
/// - **Value::Number**: Dimensionless scalars (loop counters, array indices, multipliers)
/// - **Value::Measurement**: Physical quantities with units (50µm, 200nm, pdk.edge_clearance)
/// - **Value::Percentage**: Relative positioning (50%, 25%)
///
/// This ensures the type system prevents mathematically invalid operations like
/// adding a bare scalar to a physical distance, and keeps unit metadata intact
/// throughout the entire compilation pipeline until final coordinate resolution.
pub type EvaluationContext = FxHashMap<CompactString, Value>;

impl Expression {
    /// Evaluate this expression to a concrete value (number, measurement, or percentage)
    /// Returns an error if the expression contains undefined variables or division by zero
    pub fn evaluate(&self, context: &EvaluationContext) -> Result<Value, String> {
        match self {
            Expression::Literal { value, .. } => Ok(Value::Number(*value)),
            Expression::FloatLiteral { value, .. } => Ok(Value::Float(*value)),
            Expression::Measurement { value, unit, .. } => Ok(Value::Measurement {
                value: *value,
                unit: unit.clone(),
            }),
            Expression::Percentage { value, .. } => Ok(Value::Percentage(*value)),
            Expression::Variable { name, .. } => {
                if name == "true" {
                    Ok(Value::Number(1))
                } else if name == "false" {
                    Ok(Value::Number(0))
                } else {
                    context
                        .get(name)
                        .cloned()
                        .ok_or_else(|| format!("Undefined variable '{}' in expression", name))
                }
            }
            Expression::Binary {
                left,
                operator,
                right,
                ..
            } => {
                let left_val = left.evaluate(context)?;
                let right_val = right.evaluate(context)?;

                // Handle arithmetic between different value types
                match (&left_val, &right_val) {
                    (Value::Number(l), Value::Number(r)) => {
                        // Both integers: normal integer arithmetic
                        operator.apply(*l, *r).map(Value::Number)
                    }
                    (Value::Float(l), Value::Float(r)) => {
                        // Both floats: normal float arithmetic
                        apply_op_f64(*l, *r, operator).map(Value::Float)
                    }
                    (Value::Number(l), Value::Float(r)) => {
                        // Integer op Float: promote to float
                        apply_op_f64(*l as f64, *r, operator).map(Value::Float)
                    }
                    (Value::Float(l), Value::Number(r)) => {
                        // Float op Integer: promote to float
                        apply_op_f64(*l, *r as f64, operator).map(Value::Float)
                    }
                    (
                        Value::Measurement { value: lv, unit: lu },
                        Value::Measurement { value: rv, unit: ru },
                    ) => {
                        // CLEAN ARCHITECTURE: Physics-Correct Math (Unit Normalization)
                        // Both operands are measurements with units preserved
                        
                        // For comparison operators, convert to same units and return boolean (0 or 1)
                        if matches!(operator, 
                            BinaryOperator::Equal | BinaryOperator::NotEqual |
                            BinaryOperator::LessThan | BinaryOperator::GreaterThan |
                            BinaryOperator::LessThanOrEqual | BinaryOperator::GreaterThanOrEqual
                        ) {
                            // Normalize both to nanometers for comparison
                            let l_nm = Value::Measurement { value: *lv, unit: lu.clone() }.to_nanometers()?;
                            let r_nm = Value::Measurement { value: *rv, unit: ru.clone() }.to_nanometers()?;
                            let result = operator.apply(l_nm, r_nm)?;
                            return Ok(Value::Number(result)); // Return boolean as Number (0 or 1)
                        }
                        
                        // For arithmetic operations
                        if lu == ru {
                            // Same units: safe to perform arithmetic
                            let result = apply_op_f64(*lv, *rv, operator)?;
                            Ok(Value::Measurement {
                                value: result,
                                unit: lu.clone(),
                            })
                        } else {
                            // Different units: normalize both to nanometers
                            let l_nm = Value::Measurement { value: *lv, unit: lu.clone() }.to_nanometers()?;
                            let r_nm = Value::Measurement { value: *rv, unit: ru.clone() }.to_nanometers()?;
                            let result_nm = operator.apply(l_nm, r_nm)?;
                            // Return result in nanometers (normalized unit)
                            Ok(Value::Measurement {
                                value: result_nm as f64,
                                unit: super::Unit::Nanometer,
                            })
                        }
                    }
                    (Value::Measurement { value: lv, unit: lu }, Value::Number(r)) => {
                        // ARCHITECTURAL ERROR: Mixing physical measurements with bare scalars
                        // This is mathematically invalid unless the scalar represents a multiplier
                        match operator {
                            BinaryOperator::Multiply | BinaryOperator::Divide => {
                                // Scaling operations are valid: 50µm * 2 = 100µm
                                let result = apply_op_f64(*lv, *r as f64, operator)?;
                                Ok(Value::Measurement {
                                    value: result,
                                    unit: lu.clone(),
                                })
                            }
                            _ => Err(format!(
                                "Cannot perform {:?} between measurement ({:?}) and dimensionless number ({}). \
                                 This operation is mathematically invalid. Did you mean to use a measurement unit?",
                                operator, lu, r
                            )),
                        }
                    }
                    (Value::Number(l), Value::Measurement { value: rv, unit: ru }) => {
                        // ARCHITECTURAL ERROR: Same as above, but reversed operands
                        match operator {
                            BinaryOperator::Multiply => {
                                // Scaling is valid: 2 * 50µm = 100µm
                                let result = apply_op_f64(*l as f64, *rv, operator)?;
                                Ok(Value::Measurement {
                                    value: result,
                                    unit: ru.clone(),
                                })
                            }
                            _ => Err(format!(
                                "Cannot perform {:?} between dimensionless number ({}) and measurement ({:?}). \
                                 This operation is mathematically invalid. Did you mean to use a measurement unit?",
                                operator, l, ru
                            )),
                        }
                    }
                    (Value::Measurement { value: lv, unit: lu }, Value::Float(r)) => {
                        // Measurement op Float: treat Float as a multiplier/divisor
                        match operator {
                            BinaryOperator::Multiply | BinaryOperator::Divide => {
                                let result = apply_op_f64(*lv, *r, operator)?;
                                Ok(Value::Measurement {
                                    value: result,
                                    unit: lu.clone(),
                                })
                            }
                            _ => Err(format!(
                                "Cannot perform {:?} between measurement ({:?}) and dimensionless float ({}). \
                                 This operation is mathematically invalid. Did you mean to use a measurement unit?",
                                operator, lu, r
                            )),
                        }
                    }
                    (Value::Float(l), Value::Measurement { value: rv, unit: ru }) => {
                        // Float op Measurement: treat Float as a multiplier
                        match operator {
                            BinaryOperator::Multiply => {
                                let result = apply_op_f64(*l, *rv, operator)?;
                                Ok(Value::Measurement {
                                    value: result,
                                    unit: ru.clone(),
                                })
                            }
                            _ => Err(format!(
                                "Cannot perform {:?} between dimensionless float ({}) and measurement ({:?}). \
                                 This operation is mathematically invalid. Did you mean to use a measurement unit?",
                                operator, l, ru
                            )),
                        }
                    }
                    (Value::Percentage(l), Value::Number(r)) => {
                        // Percentage op Number: apply to percentage value
                        let result = apply_op_f64(*l, *r as f64, operator)?;
                        Ok(Value::Percentage(result))
                    }
                    (Value::Percentage(l), Value::Float(r)) => {
                        // Percentage op Float: apply to percentage value
                        let result = apply_op_f64(*l, *r, operator)?;
                        Ok(Value::Percentage(result))
                    }
                    (Value::Number(l), Value::Percentage(r)) => {
                        // Number op Percentage: apply to percentage value
                        let result = apply_op_f64(*l as f64, *r, operator)?;
                        Ok(Value::Percentage(result))
                    }
                    (Value::Float(l), Value::Percentage(r)) => {
                        // Float op Percentage: apply to percentage value
                        let result = apply_op_f64(*l, *r, operator)?;
                        Ok(Value::Percentage(result))
                    }
                    (Value::Percentage(l), Value::Percentage(r)) => {
                        // Percentage op Percentage
                        let result = apply_op_f64(*l, *r, operator)?;
                        Ok(Value::Percentage(result))
                    }
                    // Mixed percentage and measurement operations
                    (Value::Percentage(_), Value::Measurement { .. }) |
                    (Value::Measurement { .. }, Value::Percentage(_)) => {
                        Err("Cannot perform arithmetic between percentages and measurements directly. Percentages must be resolved to physical units first.".into())
                    }
                }
            }
            Expression::Unary {
                operator, operand, ..
            } => {
                let operand_val = operand.evaluate(context)?;
                match operand_val {
                    Value::Number(n) => operator.apply(n).map(Value::Number),
                    Value::Float(f) => {
                        let result = match operator {
                            UnaryOperator::Negate => -f,
                            UnaryOperator::Plus => f,
                            UnaryOperator::Not => if f == 0.0 { 1.0 } else { 0.0 },
                        };
                        Ok(Value::Float(result))
                    }
                    Value::Measurement { value, unit } => {
                        match operator {
                            UnaryOperator::Negate => Ok(Value::Measurement {
                                value: -value,
                                unit,
                            }),
                            UnaryOperator::Plus => Ok(Value::Measurement { value, unit }),
                            UnaryOperator::Not => Err("Logical NOT cannot be applied to measurements. Use comparison operators instead.".into()),
                        }
                    }
                    Value::Percentage(pct) => {
                        match operator {
                            UnaryOperator::Negate => Ok(Value::Percentage(-pct)),
                            UnaryOperator::Plus => Ok(Value::Percentage(pct)),
                            UnaryOperator::Not => Err("Logical NOT cannot be applied to percentages. Use comparison operators instead.".into()),
                        }
                    }
                }
            }
            Expression::Grouped { expression, .. } => expression.evaluate(context),
            Expression::AnchorReference { .. } => {
                // Anchor references cannot be evaluated without the bounding box tracker
                // They must be resolved by the compiler's constraint solver
                Err("Anchor references require constraint solver context and cannot be evaluated in the parser. \
                     This expression should be evaluated by the compiler using evaluate_coordinate_with_anchors.".into())
            }
            Expression::Coordinate { .. } => {
                // Coordinate literals cannot be evaluated to a single value
                // They must be handled by the coordinate evaluation system
                Err("Coordinate literals cannot be evaluated to a single value. \
                     They must be resolved by the coordinate evaluation system.".into())
            }
            Expression::FunctionCall { name, arguments, span } => {
                // Evaluate function calls (sin, cos, tan, sqrt, etc.)
                evaluate_function_call(name, arguments, context, *span)
            }
        }
    }

    /// Evaluate with an empty context (no variables)
    pub fn evaluate_const(&self) -> Result<Value, String> {
        self.evaluate(&FxHashMap::default())
    }

    /// Try to evaluate as a constant (no variables)
    /// Returns None if the expression contains variables
    pub fn try_evaluate_const(&self) -> Option<Value> {
        self.evaluate_const().ok()
    }
}

/// Evaluate a function call expression
fn evaluate_function_call(
    name: &str,
    arguments: &[Expression],
    context: &EvaluationContext,
    _span: Span,
) -> Result<Value, String> {
    use std::f64::consts::PI;
    
    match name {
        // Trigonometric functions (expect radians)
        "sin" => {
            if arguments.len() != 1 {
                return Err(format!("sin() expects 1 argument, got {}", arguments.len()));
            }
            let arg = arguments[0].evaluate(context)?.as_number()?;
            Ok(Value::Float(arg.sin()))
        }
        "cos" => {
            if arguments.len() != 1 {
                return Err(format!("cos() expects 1 argument, got {}", arguments.len()));
            }
            let arg = arguments[0].evaluate(context)?.as_number()?;
            Ok(Value::Float(arg.cos()))
        }
        "tan" => {
            if arguments.len() != 1 {
                return Err(format!("tan() expects 1 argument, got {}", arguments.len()));
            }
            let arg = arguments[0].evaluate(context)?.as_number()?;
            Ok(Value::Float(arg.tan()))
        }
        "asin" => {
            if arguments.len() != 1 {
                return Err(format!("asin() expects 1 argument, got {}", arguments.len()));
            }
            let arg = arguments[0].evaluate(context)?.as_number()?;
            if arg < -1.0 || arg > 1.0 {
                return Err(format!("asin() argument must be in range [-1, 1], got {}", arg));
            }
            Ok(Value::Float(arg.asin()))
        }
        "acos" => {
            if arguments.len() != 1 {
                return Err(format!("acos() expects 1 argument, got {}", arguments.len()));
            }
            let arg = arguments[0].evaluate(context)?.as_number()?;
            if arg < -1.0 || arg > 1.0 {
                return Err(format!("acos() argument must be in range [-1, 1], got {}", arg));
            }
            Ok(Value::Float(arg.acos()))
        }
        "atan" => {
            if arguments.len() != 1 {
                return Err(format!("atan() expects 1 argument, got {}", arguments.len()));
            }
            let arg = arguments[0].evaluate(context)?.as_number()?;
            Ok(Value::Float(arg.atan()))
        }
        "atan2" => {
            if arguments.len() != 2 {
                return Err(format!("atan2() expects 2 arguments (y, x), got {}", arguments.len()));
            }
            let y = arguments[0].evaluate(context)?.as_number()?;
            let x = arguments[1].evaluate(context)?.as_number()?;
            Ok(Value::Float(y.atan2(x)))
        }
        
        // Mathematical functions
        "sqrt" => {
            if arguments.len() != 1 {
                return Err(format!("sqrt() expects 1 argument, got {}", arguments.len()));
            }
            let arg = arguments[0].evaluate(context)?.as_number()?;
            if arg < 0.0 {
                return Err(format!("sqrt() argument must be non-negative, got {}", arg));
            }
            Ok(Value::Float(arg.sqrt()))
        }
        "abs" => {
            if arguments.len() != 1 {
                return Err(format!("abs() expects 1 argument, got {}", arguments.len()));
            }
            let val = arguments[0].evaluate(context)?;
            match val {
                Value::Number(n) => Ok(Value::Number(n.abs())),
                Value::Float(f) => Ok(Value::Float(f.abs())),
                Value::Measurement { value, unit } => Ok(Value::Measurement { 
                    value: value.abs(), 
                    unit 
                }),
                Value::Percentage(p) => Ok(Value::Percentage(p.abs())),
            }
        }
        "pow" => {
            if arguments.len() != 2 {
                return Err(format!("pow() expects 2 arguments (base, exponent), got {}", arguments.len()));
            }
            let base = arguments[0].evaluate(context)?.as_number()?;
            let exp = arguments[1].evaluate(context)?.as_number()?;
            Ok(Value::Float(base.powf(exp)))
        }
        "exp" => {
            if arguments.len() != 1 {
                return Err(format!("exp() expects 1 argument, got {}", arguments.len()));
            }
            let arg = arguments[0].evaluate(context)?.as_number()?;
            Ok(Value::Float(arg.exp()))
        }
        "ln" => {
            if arguments.len() != 1 {
                return Err(format!("ln() expects 1 argument, got {}", arguments.len()));
            }
            let arg = arguments[0].evaluate(context)?.as_number()?;
            if arg <= 0.0 {
                return Err(format!("ln() argument must be positive, got {}", arg));
            }
            Ok(Value::Float(arg.ln()))
        }
        "log" | "log10" => {
            if arguments.len() != 1 {
                return Err(format!("log10() expects 1 argument, got {}", arguments.len()));
            }
            let arg = arguments[0].evaluate(context)?.as_number()?;
            if arg <= 0.0 {
                return Err(format!("log10() argument must be positive, got {}", arg));
            }
            Ok(Value::Float(arg.log10()))
        }
        "log2" => {
            if arguments.len() != 1 {
                return Err(format!("log2() expects 1 argument, got {}", arguments.len()));
            }
            let arg = arguments[0].evaluate(context)?.as_number()?;
            if arg <= 0.0 {
                return Err(format!("log2() argument must be positive, got {}", arg));
            }
            Ok(Value::Float(arg.log2()))
        }
        
        // Rounding functions
        "floor" => {
            if arguments.len() != 1 {
                return Err(format!("floor() expects 1 argument, got {}", arguments.len()));
            }
            let arg = arguments[0].evaluate(context)?.as_number()?;
            Ok(Value::Float(arg.floor()))
        }
        "ceil" => {
            if arguments.len() != 1 {
                return Err(format!("ceil() expects 1 argument, got {}", arguments.len()));
            }
            let arg = arguments[0].evaluate(context)?.as_number()?;
            Ok(Value::Float(arg.ceil()))
        }
        "round" => {
            if arguments.len() != 1 {
                return Err(format!("round() expects 1 argument, got {}", arguments.len()));
            }
            let arg = arguments[0].evaluate(context)?.as_number()?;
            Ok(Value::Float(arg.round()))
        }
        
        // Utility functions
        "min" => {
            if arguments.len() < 2 {
                return Err(format!("min() expects at least 2 arguments, got {}", arguments.len()));
            }
            let mut min = arguments[0].evaluate(context)?.as_number()?;
            for arg in &arguments[1..] {
                let val = arg.evaluate(context)?.as_number()?;
                if val < min {
                    min = val;
                }
            }
            Ok(Value::Float(min))
        }
        "max" => {
            if arguments.len() < 2 {
                return Err(format!("max() expects at least 2 arguments, got {}", arguments.len()));
            }
            let mut max = arguments[0].evaluate(context)?.as_number()?;
            for arg in &arguments[1..] {
                let val = arg.evaluate(context)?.as_number()?;
                if val > max {
                    max = val;
                }
            }
            Ok(Value::Float(max))
        }
        
        // Unit conversion helper (degrees to radians)
        "radians" | "rad" => {
            if arguments.len() != 1 {
                return Err(format!("radians() expects 1 argument (degrees), got {}", arguments.len()));
            }
            let degrees = arguments[0].evaluate(context)?.as_number()?;
            Ok(Value::Float(degrees * PI / 180.0))
        }
        "degrees" | "deg" => {
            if arguments.len() != 1 {
                return Err(format!("degrees() expects 1 argument (radians), got {}", arguments.len()));
            }
            let radians = arguments[0].evaluate(context)?.as_number()?;
            Ok(Value::Float(radians * 180.0 / PI))
        }
        
        _ => Err(format!("Unknown function '{}'. Available functions: sin, cos, tan, asin, acos, atan, atan2, sqrt, abs, pow, exp, ln, log10, log2, floor, ceil, round, min, max, radians, degrees", name))
    }
}

/// Helper function to apply binary operators to f64 values
fn apply_op_f64(left: f64, right: f64, operator: &BinaryOperator) -> Result<f64, String> {
    match operator {
        BinaryOperator::Add => Ok(left + right),
        BinaryOperator::Subtract => Ok(left - right),
        BinaryOperator::Multiply => Ok(left * right),
        BinaryOperator::Divide => {
            if right == 0.0 {
                Err("Division by zero".into())
            } else {
                Ok(left / right)
            }
        }
        BinaryOperator::Modulo => Err("Modulo not supported for floating point values".into()),
        // Comparison operators return 1.0 for true, 0.0 for false
        BinaryOperator::Equal => Ok(if (left - right).abs() < f64::EPSILON { 1.0 } else { 0.0 }),
        BinaryOperator::NotEqual => Ok(if (left - right).abs() >= f64::EPSILON { 1.0 } else { 0.0 }),
        BinaryOperator::LessThan => Ok(if left < right { 1.0 } else { 0.0 }),
        BinaryOperator::GreaterThan => Ok(if left > right { 1.0 } else { 0.0 }),
        BinaryOperator::LessThanOrEqual => Ok(if left <= right { 1.0 } else { 0.0 }),
        BinaryOperator::GreaterThanOrEqual => Ok(if left >= right { 1.0 } else { 0.0 }),
        // Boolean operators (treat non-zero as true, zero as false)
        BinaryOperator::And => Ok(if left != 0.0 && right != 0.0 { 1.0 } else { 0.0 }),
        BinaryOperator::Or => Ok(if left != 0.0 || right != 0.0 { 1.0 } else { 0.0 }),
    }
}

impl fmt::Display for Expression {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Expression::Literal { value, .. } => write!(f, "{}", value),
            Expression::FloatLiteral { value, .. } => write!(f, "{}", value),
            Expression::Measurement { value, unit, .. } => write!(f, "{}{:?}", value, unit),
            Expression::Percentage { value, .. } => write!(f, "{}%", value),
            Expression::Variable { name, .. } => write!(f, "{}", name),
            Expression::Binary {
                left,
                operator,
                right,
                ..
            } => {
                let op_str = match operator {
                    BinaryOperator::Add => "+",
                    BinaryOperator::Subtract => "-",
                    BinaryOperator::Multiply => "*",
                    BinaryOperator::Divide => "/",
                    BinaryOperator::Modulo => "%",
                    BinaryOperator::Equal => "==",
                    BinaryOperator::NotEqual => "!=",
                    BinaryOperator::LessThan => "<",
                    BinaryOperator::GreaterThan => ">",
                    BinaryOperator::LessThanOrEqual => "<=",
                    BinaryOperator::GreaterThanOrEqual => ">=",
                    BinaryOperator::And => "and",
                    BinaryOperator::Or => "or",
                };
                write!(f, "{} {} {}", left, op_str, right)
            }
            Expression::Unary {
                operator, operand, ..
            } => {
                let op_str = match operator {
                    UnaryOperator::Negate => "-",
                    UnaryOperator::Plus => "+",
                    UnaryOperator::Not => "not ",
                };
                write!(f, "{}{}", op_str, operand)
            }
            Expression::Grouped { expression, .. } => write!(f, "({})", expression),
            Expression::AnchorReference { anchor, edge, .. } => {
                let edge_str = match edge {
                    super::Edge::Left => "left",
                    super::Edge::Right => "right",
                    super::Edge::Top => "top",
                    super::Edge::Bottom => "bottom",
                    super::Edge::Front => "front",
                    super::Edge::Back => "back",
                    super::Edge::MinZ => "min_z",
                    super::Edge::MaxZ => "max_z",
                    super::Edge::TopLeft => "top_left",
                    super::Edge::TopRight => "top_right",
                    super::Edge::BottomLeft => "bottom_left",
                    super::Edge::BottomRight => "bottom_right",
                    super::Edge::Center => "center",
                    super::Edge::CenterX => "center_x",
                    super::Edge::CenterY => "center_y",
                    super::Edge::CenterZ => "center_z",
                };
                write!(f, "{}.{}", anchor.name, edge_str)
            }
            Expression::Coordinate { coord, .. } => {
                write!(f, "{:?}", coord) // Use debug format for coordinate
            }
            Expression::FunctionCall { name, arguments, .. } => {
                write!(f, "{}(", name)?;
                for (i, arg) in arguments.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", arg)?;
                }
                write!(f, ")")
            }
        }
    }
}

#[cfg(test)]
mod eval_tests {
    use super::*;

    #[test]
    fn test_evaluate_literal() {
        let expr = Expression::Literal {
            value: 42,
            span: Span::new(0, 0),
        };
        assert_eq!(expr.evaluate_const().unwrap(), Value::Number(42));
    }

    #[test]
    fn test_evaluate_addition() {
        let expr = Expression::Binary {
            left: Box::new(Expression::Literal {
                value: 10,
                span: Span::new(0, 0),
            }),
            operator: BinaryOperator::Add,
            right: Box::new(Expression::Literal {
                value: 5,
                span: Span::new(0, 0),
            }),
            span: Span::new(0, 0),
        };
        assert_eq!(expr.evaluate_const().unwrap(), Value::Number(15));
    }

    #[test]
    fn test_evaluate_with_variable() {
        let expr = Expression::Binary {
            left: Box::new(Expression::Literal {
                value: 20,
                span: Span::new(0, 0),
            }),
            operator: BinaryOperator::Add,
            right: Box::new(Expression::Binary {
                left: Box::new(Expression::Variable {
                    name: "i".into(),
                    span: Span::new(0, 0),
                }),
                operator: BinaryOperator::Multiply,
                right: Box::new(Expression::Literal {
                    value: 2,
                    span: Span::new(0, 0),
                }),
                span: Span::new(0, 0),
            }),
            span: Span::new(0, 0),
        };

        let mut context = FxHashMap::default();
        context.insert("i".into(), Value::Number(5));
        assert_eq!(expr.evaluate(&context).unwrap(), Value::Number(30)); // 20 + (5 * 2) = 30
    }

    #[test]
    fn test_evaluate_undefined_variable() {
        let expr = Expression::Variable {
            name: "x".into(),
            span: Span::new(0, 0),
        };
        assert!(expr.evaluate_const().is_err());
    }

    #[test]
    fn test_evaluate_division_by_zero() {
        let expr = Expression::Binary {
            left: Box::new(Expression::Literal {
                value: 10,
                span: Span::new(0, 0),
            }),
            operator: BinaryOperator::Divide,
            right: Box::new(Expression::Literal {
                value: 0,
                span: Span::new(0, 0),
            }),
            span: Span::new(0, 0),
        };
        assert!(expr.evaluate_const().is_err());
    }
}
