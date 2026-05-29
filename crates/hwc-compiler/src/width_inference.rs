//! Bit-Width Inference and Validation for Logic Synthesis
//!
//! Tracks and validates bit widths of all expressions and assignments.
//! Ensures type safety at the hardware level.
//!
//! Gap 5.14: Smart Width Unification - Literals automatically coerce to match wire widths

use crate::span_utils::span_to_source_span;
use compact_str::CompactString;
use hwc_parser::logic::*;
use miette::{Diagnostic, SourceSpan};
use rustc_hash::FxHashMap;
use thiserror::Error;

/// Checks if a given expression resolves to a hardcoded literal
fn is_literal(expr: &LogicExpression) -> bool {
    matches!(
        expr,
        LogicExpression::Literal { .. } | LogicExpression::Boolean { .. }
    )
}

/// Checks if a BlockOrExpr resolves to a literal
fn is_block_literal(block: &BlockOrExpr) -> bool {
    match block {
        BlockOrExpr::Expression(expr) => is_literal(expr),
        BlockOrExpr::Block(statements) => {
            // A block is a literal if its last value-producing statement is a literal
            for statement in statements.iter().rev() {
                match statement {
                    LogicStatement::Expression(expr) => {
                        return is_literal(expr);
                    }
                    LogicStatement::Let { .. } => continue,
                    LogicStatement::Assignment { expression, .. } => {
                        return is_literal(expression);
                    }
                    LogicStatement::If { .. } => {
                        return false; // If statements are not literals
                    }
                }
            }
            false
        }
        BlockOrExpr::Pass(_) => false,
    }
}

/// Unifies two bit-widths, allowing literals to coerce to the larger width.
/// This implements Gap 5.14: Bottom-Up Width Inference with contextual coercion.
///
/// Rules:
/// - If both widths match, return that width
/// - If one side is a literal and smaller, extend it to match the other
/// - If one side is a non-literal wire and smaller, zero-extend it to match the other
/// - If a literal is LARGER than a wire, fail (prevents accidental truncation)
fn unify_widths(
    width_a: usize,
    is_literal_a: bool,
    width_b: usize,
    is_literal_b: bool,
) -> Result<usize, String> {
    if width_a == width_b {
        return Ok(width_a);
    }

    // If A is a literal and is smaller, let it expand to match B
    if is_literal_a && width_a < width_b {
        return Ok(width_b);
    }

    // If B is a literal and is smaller, let it expand to match A
    if is_literal_b && width_b < width_a {
        return Ok(width_a);
    }

    // Hardware automatic width extension: smaller operand is zero-extended
    // This is standard behavior in hardware - a 1-bit signal added to an 8-bit
    // signal is automatically zero-extended to 8 bits
    if width_a < width_b {
        return Ok(width_b);
    }

    if width_b < width_a {
        return Ok(width_a);
    }

    // Should never reach here since we've covered all cases
    Err(format!("{}-bit and {}-bit", width_a, width_b))
}

/// Errors that can occur during width inference
#[derive(Error, Debug, Clone, Diagnostic)]
pub enum WidthError {
    #[error(
        "Width mismatch: Cannot assign {src_width}-bit value to {dst_width}-bit wire '{name}'"
    )]
    #[diagnostic(
        code(L02),
        url("https://docs.hw-script.org/errors/L02"),
        help("Use slicing to truncate: {name}[{dst_width_minus_1}..0]\nOr extend the destination: let {name}[{src_width}] = ...")
    )]
    WidthMismatch {
        #[label("{src_width}-bit value assigned to {dst_width}-bit wire")]
        span: SourceSpan,
        name: CompactString,
        src_width: usize,
        dst_width: usize,
        dst_width_minus_1: usize,
    },

    #[error("Cannot infer width for variable '{name}'")]
    #[diagnostic(
        code(L02),
        url("https://docs.hw-script.org/errors/L02"),
        help("Specify the width explicitly: let {name}[8] = ...")
    )]
    CannotInferWidth {
        #[label("width cannot be inferred")]
        span: SourceSpan,
        name: CompactString,
    },

    #[error("Width not specified for wire '{name}'")]
    #[diagnostic(
        code(L02),
        url("https://docs.hw-script.org/errors/L02"),
        help("Add bit width: let {name}[8] = ...")
    )]
    WidthNotSpecified {
        #[label("width not specified")]
        span: SourceSpan,
        name: CompactString,
    },

    #[error("Invalid bit slice [{high}..{low}] for {width}-bit wire '{name}'")]
    #[diagnostic(
        code(L02),
        url("https://docs.hw-script.org/errors/L02"),
        help("Valid range is [0..{width_minus_1}]")
    )]
    InvalidSlice {
        #[label("invalid slice range")]
        span: SourceSpan,
        name: CompactString,
        high: usize,
        low: usize,
        width: usize,
        width_minus_1: usize,
    },

    #[error(
        "Operand width mismatch: {left_width}-bit and {right_width}-bit values in {operation}"
    )]
    #[diagnostic(
        code(L02),
        url("https://docs.hw-script.org/errors/L02"),
        help("Ensure both operands have the same bit width, or use explicit casting")
    )]
    OperandWidthMismatch {
        #[label("{left_width}-bit operand")]
        left_span: Option<SourceSpan>,
        #[label("{right_width}-bit operand")]
        right_span: Option<SourceSpan>,
        operation: CompactString,
        left_width: usize,
        right_width: usize,
    },
}

/// Warnings that can occur during width inference
#[derive(Error, Debug, Clone, Diagnostic)]
#[diagnostic(severity(Warning))]
pub enum WidthWarning {
    #[error(
        "Implicit truncation: {src_width}-bit expression assigned to {dst_width}-bit wire '{name}'"
    )]
    #[diagnostic(
        code(L10),
        url("https://docs.hw-script.org/errors/L10"),
        help("The upper {truncated_bits} bit(s) will be discarded. Use explicit slicing to make this clear: {name} = expr[{dst_width_minus_1}..0]")
    )]
    ImplicitTruncation {
        #[label("{src_width}-bit value truncated to {dst_width} bits")]
        span: SourceSpan,
        name: CompactString,
        src_width: usize,
        dst_width: usize,
        dst_width_minus_1: usize,
        truncated_bits: usize,
    },
}

/// Result of width validation - can be Ok, Ok with warning, or Error
pub enum WidthValidationResult {
    /// Validation passed without issues
    Ok,
    /// Validation passed but with a warning
    Warning(WidthWarning),
    /// Validation failed with an error
    Error(WidthError),
}

/// Width inference engine
pub struct WidthInference<'a> {
    /// Map of variable name to bit width
    widths: FxHashMap<CompactString, usize>,
    /// Map of variable name to type name (for struct/enum variables)
    types: FxHashMap<CompactString, String>,
    /// Reference to symbol table for type lookups
    symbol_table: &'a crate::symbol_table::SymbolTable,
}

impl<'a> WidthInference<'a> {
    /// Create a new width inference engine
    pub fn new(symbol_table: &'a crate::symbol_table::SymbolTable) -> Self {
        Self {
            widths: FxHashMap::default(),
            types: FxHashMap::default(),
            symbol_table,
        }
    }

    /// Register a variable with a known width
    pub fn register_width(&mut self, name: CompactString, width: usize) {
        self.widths.insert(name, width);
    }

    /// Register a variable with a known type
    pub fn register_type(&mut self, name: CompactString, type_name: CompactString) {
        self.types.insert(name, type_name.to_string());
    }

    /// Get the width of a variable
    pub fn get_width(&self, name: &str) -> Option<usize> {
        self.widths.get(name).copied()
    }

    /// Get the type of a variable
    pub fn get_type(&self, name: &str) -> Option<&str> {
        self.types.get(name).map(|s| s.as_str())
    }

    /// Helper to extract span from any expression
    fn get_expression_span(expr: &LogicExpression) -> SourceSpan {
        let span = match expr {
            LogicExpression::Variable { span, .. } => span,
            LogicExpression::Literal { span, .. } => span,
            LogicExpression::Boolean { span, .. } => span,
            LogicExpression::Binary { span, .. } => span,
            LogicExpression::Unary { span, .. } => span,
            LogicExpression::ArrayAccess { span, .. } => span,
            LogicExpression::If { span, .. } => span,
            LogicExpression::Match { span, .. } => span,
            LogicExpression::Grouped { span, .. } => span,
            LogicExpression::Bundle { span, .. } => span,
            LogicExpression::RegisterInit { span, .. } => span,
            LogicExpression::Cast { span, .. } => span,
            LogicExpression::FieldAccess { span, .. } => span,
        };
        span_to_source_span(span)
    }

    /// Infer the width of an expression
    pub fn infer_expression_width(&self, expr: &LogicExpression) -> Result<usize, WidthError> {
        match expr {
            LogicExpression::Variable { name, span, .. } => self
                .widths
                .get(name)
                .copied()
                .ok_or_else(|| WidthError::CannotInferWidth {
                    span: span_to_source_span(span),
                    name: name.clone(),
                }),

            LogicExpression::Literal { value, .. } => {
                // Calculate minimum bits needed to represent the value
                if *value == 0 {
                    Ok(1) // Zero needs at least 1 bit
                } else {
                    let abs_value = value.unsigned_abs();
                    let bits = (64 - abs_value.leading_zeros()) as usize;
                    Ok(bits.max(1))
                }
            }

            LogicExpression::Boolean { .. } => {
                // Boolean is 1 bit
                Ok(1)
            }

            LogicExpression::Binary {
                left,
                operator,
                right,
                ..
            } => self.infer_binary_width(left, operator, right),

            LogicExpression::Unary { operand, .. } => {
                // Unary NOT preserves the width of the operand
                self.infer_expression_width(operand)
            }

            LogicExpression::ArrayAccess {
                base, range, span, ..
            } => {
                match range {
                    Range::Single(_) => Ok(1), // Single bit access
                    Range::Slice { high, low } => {
                        // Slice width is (high - low + 1)
                        if high >= low {
                            Ok(high - low + 1)
                        } else {
                            // Get base name for error message
                            let base_name = match base.as_ref() {
                                LogicExpression::Variable { name, .. } => name.clone(),
                                _ => "expression".into(),
                            };
                            Err(WidthError::InvalidSlice {
                                span: span_to_source_span(span),
                                name: base_name,
                                high: *high,
                                low: *low,
                                width: 0, // Unknown at this point
                                width_minus_1: 0,
                            })
                        }
                    }
                }
            }

            LogicExpression::If {
                then_expr,
                else_expr,
                span,
                ..
            } => {
                // Both branches must have compatible widths (with literal coercion)
                let then_width = self.infer_block_or_expr_width(then_expr)?;
                let else_width = self.infer_block_or_expr_width(else_expr)?;

                let is_lit_then = is_block_literal(then_expr);
                let is_lit_else = is_block_literal(else_expr);

                match unify_widths(then_width, is_lit_then, else_width, is_lit_else) {
                    Ok(unified_width) => Ok(unified_width),
                    Err(_) => Err(WidthError::OperandWidthMismatch {
                        left_span: Some(span_to_source_span(span)),
                        right_span: None,
                        operation: "if/else".into(),
                        left_width: then_width,
                        right_width: else_width,
                    }),
                }
            }

            LogicExpression::Match { arms, span, .. } => {
                // All arms must have compatible widths (with literal coercion)
                if arms.is_empty() {
                    return Err(WidthError::CannotInferWidth {
                        span: span_to_source_span(span),
                        name: "empty match".into(),
                    });
                }

                let first_width = self.infer_block_or_expr_width(&arms[0].body)?;
                let is_first_lit = is_block_literal(&arms[0].body);
                let mut unified_width = first_width;
                let mut any_non_literal = !is_first_lit;

                for arm in &arms[1..] {
                    let arm_width = self.infer_block_or_expr_width(&arm.body)?;
                    let is_arm_lit = is_block_literal(&arm.body);

                    if !is_arm_lit {
                        any_non_literal = true;
                    }

                    match unify_widths(unified_width, !any_non_literal, arm_width, is_arm_lit) {
                        Ok(new_width) => {
                            unified_width = new_width;
                        }
                        Err(_) => {
                            return Err(WidthError::OperandWidthMismatch {
                                left_span: Some(span_to_source_span(span)),
                                right_span: Some(span_to_source_span(&arm.span)),
                                operation: "match".into(),
                                left_width: unified_width,
                                right_width: arm_width,
                            });
                        }
                    }
                }

                Ok(unified_width)
            }

            LogicExpression::Grouped { expression, .. } => self.infer_expression_width(expression),

            LogicExpression::Bundle { items, .. } => {
                // Bundle width is sum of all item widths
                let mut total_width = 0;
                for item in items {
                    match item {
                        BundleItem::Expression(expr) => {
                            total_width += self.infer_expression_width(expr)?;
                        }
                        BundleItem::Duplication { value, count, .. } => {
                            let value_width = self.infer_expression_width(value)?;
                            total_width += value_width * count;
                        }
                    }
                }
                Ok(total_width)
            }

            LogicExpression::RegisterInit { init, .. } => {
                // Register width is determined by init value
                self.infer_expression_width(init)
            }

            LogicExpression::Cast {
                target_type, span, ..
            } => {
                // Cast reinterprets bits as a different type
                // The width is determined by the target type
                if let Ok(struct_def) = self.symbol_table.get_struct(target_type) {
                    // Calculate total width by summing all field widths
                    let total_width: usize = struct_def.fields.iter().map(|f| f.width).sum();
                    Ok(total_width)
                } else if let Ok(enum_def) = self.symbol_table.get_enum(target_type) {
                    // Enum width is the minimum bits needed to represent all variants
                    let num_variants = enum_def.variants.len();
                    let width = if num_variants <= 1 {
                        1
                    } else {
                        (num_variants as f64).log2().ceil() as usize
                    };
                    Ok(width)
                } else {
                    Err(WidthError::CannotInferWidth {
                        span: span_to_source_span(span),
                        name: format!("cast to unknown type '{}'", target_type).into(),
                    })
                }
            }

            LogicExpression::FieldAccess {
                base, field, span, ..
            } => {
                // Field access width is determined by the field's type
                // First, determine what type the base expression is
                match base.as_ref() {
                    LogicExpression::Variable { name, .. } => {
                        // Check if this is an enum variant access (e.g., State.IDLE)
                        if let Ok(enum_def) = self.symbol_table.get_enum(name) {
                            // Verify the variant exists
                            if enum_def.variants.iter().any(|v| v.name == *field) {
                                // Enum width is the minimum bits needed to represent all variants
                                let num_variants = enum_def.variants.len();
                                let width = if num_variants <= 1 {
                                    1
                                } else {
                                    (num_variants as f64).log2().ceil() as usize
                                };
                                return Ok(width);
                            } else {
                                return Err(WidthError::CannotInferWidth {
                                    span: span_to_source_span(span),
                                    name: format!(
                                        "variant '{}' not found in enum '{}'",
                                        field, name
                                    )
                                    .into(),
                                });
                            }
                        }

                        // Check if we have type information for this variable
                        if let Some(type_name) = self.get_type(name) {
                            // Look up the type in the symbol table
                            if let Ok(struct_def) = self.symbol_table.get_struct(type_name) {
                                // Find the field in the struct
                                if let Some(field_def) =
                                    struct_def.fields.iter().find(|f| f.name == *field)
                                {
                                    return Ok(field_def.width);
                                } else {
                                    return Err(WidthError::CannotInferWidth {
                                        span: span_to_source_span(span),
                                        name: format!(
                                            "field '{}' not found in struct '{}'",
                                            field, type_name
                                        )
                                        .into(),
                                    });
                                }
                            } else {
                                return Err(WidthError::CannotInferWidth {
                                    span: span_to_source_span(span),
                                    name: format!("type '{}' is not a struct", type_name).into(),
                                });
                            }
                        }

                        // Try to look up as a direct struct type name (for backward compatibility)
                        if let Ok(struct_def) = self.symbol_table.get_struct(name) {
                            if let Some(field_def) =
                                struct_def.fields.iter().find(|f| f.name == *field)
                            {
                                return Ok(field_def.width);
                            }
                        }

                        // No type information available
                        Err(WidthError::CannotInferWidth {
                            span: span_to_source_span(span),
                            name: format!("cannot infer width of field access '{}.{}' - variable type unknown", name, field).into(),
                        })
                    }
                    LogicExpression::Cast { target_type, .. } => {
                        // The base is a cast, so we know the type
                        if let Ok(struct_def) = self.symbol_table.get_struct(target_type) {
                            if let Some(field_def) =
                                struct_def.fields.iter().find(|f| f.name == *field)
                            {
                                Ok(field_def.width)
                            } else {
                                Err(WidthError::CannotInferWidth {
                                    span: span_to_source_span(span),
                                    name: format!(
                                        "field '{}' not found in struct '{}'",
                                        field, target_type
                                    )
                                    .into(),
                                })
                            }
                        } else {
                            Err(WidthError::CannotInferWidth {
                                span: span_to_source_span(span),
                                name: format!("cast target '{}' is not a struct", target_type)
                                    .into(),
                            })
                        }
                    }
                    _ => {
                        // For other base expressions, we can't determine the type yet
                        Err(WidthError::CannotInferWidth {
                            span: span_to_source_span(span),
                            name: "cannot infer width of field access on complex expression"
                                .to_string()
                                .into(),
                        })
                    }
                }
            }
        }
    }

    /// Infer width of a BlockOrExpr
    fn infer_block_or_expr_width(&self, block_or_expr: &BlockOrExpr) -> Result<usize, WidthError> {
        match block_or_expr {
            BlockOrExpr::Expression(expr) => self.infer_expression_width(expr),
            BlockOrExpr::Pass(_) => Ok(0), // Pass has no value
            BlockOrExpr::Block(statements) => {
                // For blocks, find the last expression that produces a value
                // In Hardware Script, blocks evaluate to their last expression
                if statements.is_empty() {
                    return Ok(0); // Empty block has no value
                }

                // Find the last statement that produces a value
                for statement in statements.iter().rev() {
                    match statement {
                        LogicStatement::Expression(expr) => {
                            // Bare expression is the tail value!
                            return self.infer_expression_width(expr);
                        }
                        LogicStatement::Let { .. } => {
                            // Let statements don't produce values for the block
                            continue;
                        }
                        LogicStatement::Assignment { expression, .. } => {
                            // Assignments produce the value being assigned
                            return self.infer_expression_width(expression);
                        }
                        LogicStatement::If {
                            then_block,
                            else_block,
                            ..
                        } => {
                            // If statements can produce values
                            let then_width = self.infer_block_or_expr_width(then_block)?;
                            if let Some(else_blk) = else_block {
                                let else_width = self.infer_block_or_expr_width(else_blk)?;

                                // Use width unification to allow automatic extension
                                let is_lit_then = is_block_literal(then_block);
                                let is_lit_else = is_block_literal(else_blk);

                                match unify_widths(then_width, is_lit_then, else_width, is_lit_else)
                                {
                                    Ok(unified_width) => return Ok(unified_width),
                                    Err(_) => {
                                        return Err(WidthError::OperandWidthMismatch {
                                            left_span: None,
                                            right_span: None,
                                            operation: "if/else block".into(),
                                            left_width: then_width,
                                            right_width: else_width,
                                        });
                                    }
                                }
                            }
                            return Ok(then_width);
                        }
                    }
                }

                // If no value-producing statement found, block has no value
                Ok(0)
            }
        }
    }

    /// Infer width of a binary operation
    fn infer_binary_width(
        &self,
        left: &LogicExpression,
        operator: &LogicOperator,
        right: &LogicExpression,
    ) -> Result<usize, WidthError> {
        let left_width = self.infer_expression_width(left)?;
        let right_width = self.infer_expression_width(right)?;

        let is_lit_left = is_literal(left);
        let is_lit_right = is_literal(right);

        match operator {
            // Arithmetic operations
            LogicOperator::Add | LogicOperator::Subtract => {
                // Unify operand widths first, then add 1 for carry/borrow
                match unify_widths(left_width, is_lit_left, right_width, is_lit_right) {
                    Ok(unified_width) => Ok(unified_width + 1),
                    Err(_) => Err(WidthError::OperandWidthMismatch {
                        left_span: Some(Self::get_expression_span(left)),
                        right_span: Some(Self::get_expression_span(right)),
                        operation: format!("{:?}", operator).into(),
                        left_width,
                        right_width,
                    }),
                }
            }

            LogicOperator::Multiply => {
                // Result is left + right bits
                Ok(left_width + right_width)
            }

            LogicOperator::Divide => {
                // Result is left bits (quotient)
                Ok(left_width)
            }

            LogicOperator::Modulo => {
                // Result is right bits (remainder is always less than divisor)
                Ok(right_width)
            }

            // Bitwise operations - require matching widths
            LogicOperator::BitwiseAnd | LogicOperator::BitwiseOr | LogicOperator::BitwiseXor => {
                // Use smart unification to allow literals to coerce
                match unify_widths(left_width, is_lit_left, right_width, is_lit_right) {
                    Ok(unified_width) => Ok(unified_width),
                    Err(_) => Err(WidthError::OperandWidthMismatch {
                        left_span: Some(Self::get_expression_span(left)),
                        right_span: Some(Self::get_expression_span(right)),
                        operation: format!("{:?}", operator).into(),
                        left_width,
                        right_width,
                    }),
                }
            }

            // Shift operations
            LogicOperator::ShiftLeft | LogicOperator::ShiftRight => {
                // Result width is left operand width
                Ok(left_width)
            }

            // Comparison operations
            LogicOperator::Equal
            | LogicOperator::NotEqual
            | LogicOperator::LessThan
            | LogicOperator::GreaterThan
            | LogicOperator::LessThanOrEqual
            | LogicOperator::GreaterThanOrEqual => {
                // Comparison result is always 1 bit (boolean)
                Ok(1)
            }
        }
    }

    /// Validate an assignment
    /// Returns Ok for valid assignments, Warning for explicit truncation, Error for invalid assignments
    pub fn validate_assignment(
        &self,
        target_name: &str,
        target_width: usize,
        expr: &LogicExpression,
        explicit_width: bool,
    ) -> WidthValidationResult {
        let expr_width = match self.infer_expression_width(expr) {
            Ok(w) => w,
            Err(e) => return WidthValidationResult::Error(e),
        };

        // Allow exact match
        if expr_width == target_width {
            return WidthValidationResult::Ok;
        }

        // Allow literals and smaller values to be zero-extended to target width
        // This is standard hardware behavior
        if expr_width < target_width {
            // Check if the expression is a literal or can be safely extended
            if is_literal(expr) {
                return WidthValidationResult::Ok; // Literals can be zero-extended
            }
            // Non-literals can also be zero-extended in hardware
            return WidthValidationResult::Ok;
        }

        // Expression is wider than target (requires truncation)
        // Extract span from expression
        let span = Self::get_expression_span(expr);

        // If the user explicitly specified the width, allow truncation but warn
        if explicit_width {
            return WidthValidationResult::Warning(WidthWarning::ImplicitTruncation {
                span,
                name: target_name.to_string().into(),
                src_width: expr_width,
                dst_width: target_width,
                dst_width_minus_1: target_width.saturating_sub(1),
                truncated_bits: expr_width - target_width,
            });
        }

        // No explicit width specified - this is an error
        WidthValidationResult::Error(WidthError::WidthMismatch {
            span,
            name: target_name.to_string().into(),
            src_width: expr_width,
            dst_width: target_width,
            dst_width_minus_1: target_width.saturating_sub(1),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbol_table::SymbolTable;

    use hwc_parser::Span;

    #[test]
    fn test_literal_width() {
        let symbol_table = SymbolTable::new();
        let inference = WidthInference::new(&symbol_table);

        let expr = LogicExpression::Literal {
            value: 0xFF,
            span: Span::new(0, 0),
        };

        assert_eq!(inference.infer_expression_width(&expr).unwrap(), 8);
    }

    #[test]
    fn test_variable_width() {
        let symbol_table = SymbolTable::new();
        let mut inference = WidthInference::new(&symbol_table);
        inference.register_width("x".into(), 16);

        let expr = LogicExpression::Variable {
            name: "x".into(),
            span: Span::new(0, 0),
        };

        assert_eq!(inference.infer_expression_width(&expr).unwrap(), 16);
    }

    #[test]
    fn test_add_width() {
        let symbol_table = SymbolTable::new();
        let mut inference = WidthInference::new(&symbol_table);
        inference.register_width("a".into(), 8);
        inference.register_width("b".into(), 8);

        let expr = LogicExpression::Binary {
            left: Box::new(LogicExpression::Variable {
                name: "a".into(),
                span: Span::new(0, 0),
            }),
            operator: LogicOperator::Add,
            right: Box::new(LogicExpression::Variable {
                name: "b".into(),
                span: Span::new(0, 0),
            }),
            span: Span::new(0, 0),
        };

        // 8-bit + 8-bit = 9-bit (with carry)
        assert_eq!(inference.infer_expression_width(&expr).unwrap(), 9);
    }

    #[test]
    fn test_slice_width() {
        let symbol_table = SymbolTable::new();
        let inference = WidthInference::new(&symbol_table);

        let expr = LogicExpression::ArrayAccess {
            base: Box::new(LogicExpression::Variable {
                name: "bus".into(),
                span: Span::new(0, 0),
            }),
            range: Range::Slice { high: 7, low: 0 },
            span: Span::new(0, 0),
        };

        assert_eq!(inference.infer_expression_width(&expr).unwrap(), 8);
    }

    #[test]
    fn test_width_mismatch() {
        let symbol_table = SymbolTable::new();
        let mut inference = WidthInference::new(&symbol_table);
        inference.register_width("x".into(), 8);

        let expr = LogicExpression::Variable {
            name: "x".into(),
            span: Span::new(0, 0),
        };

        let result = inference.validate_assignment("out", 4, &expr, false);
        assert!(matches!(result, WidthValidationResult::Error(_)));
    }
}
