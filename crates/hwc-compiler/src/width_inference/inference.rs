use crate::span_utils::span_to_source_span;
use compact_str::CompactString;
use hwc_parser::logic::*;
use miette::SourceSpan;
use rustc_hash::FxHashMap;

use super::errors::{WidthError, WidthValidationResult, WidthWarning};
use super::helpers::{is_block_literal, is_literal, unify_widths};

pub struct WidthInference<'a> {
    widths: FxHashMap<CompactString, usize>,
    types: FxHashMap<CompactString, String>,
    symbol_table: &'a crate::symbol_table::SymbolTable,
}

impl<'a> WidthInference<'a> {
    pub fn new(symbol_table: &'a crate::symbol_table::SymbolTable) -> Self {
        Self {
            widths: FxHashMap::default(),
            types: FxHashMap::default(),
            symbol_table,
        }
    }

    pub fn register_width(&mut self, name: CompactString, width: usize) {
        self.widths.insert(name, width);
    }

    pub fn register_type(&mut self, name: CompactString, type_name: CompactString) {
        self.types.insert(name, type_name.to_string());
    }

    pub fn get_width(&self, name: &str) -> Option<usize> {
        self.widths.get(name).copied()
    }

    pub fn get_type(&self, name: &str) -> Option<&str> {
        self.types.get(name).map(|s| s.as_str())
    }

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
                if *value == 0 {
                    Ok(1)
                } else {
                    let abs_value = value.unsigned_abs();
                    let bits = (64 - abs_value.leading_zeros()) as usize;
                    Ok(bits.max(1))
                }
            }

            LogicExpression::Boolean { .. } => Ok(1),

            LogicExpression::Binary {
                left,
                operator,
                right,
                ..
            } => self.infer_binary_width(left, operator, right),

            LogicExpression::Unary { operand, .. } => self.infer_expression_width(operand),

            LogicExpression::ArrayAccess {
                base, range, span, ..
            } => match range {
                Range::Single(_) => Ok(1),
                Range::Slice { high, low } => {
                    if high >= low {
                        Ok(high - low + 1)
                    } else {
                        let base_name = match base.as_ref() {
                            LogicExpression::Variable { name, .. } => name.clone(),
                            _ => "expression".into(),
                        };
                        Err(WidthError::InvalidSlice {
                            span: span_to_source_span(span),
                            name: base_name,
                            high: *high,
                            low: *low,
                            width: 0,
                            width_minus_1: 0,
                        })
                    }
                }
            },

            LogicExpression::If {
                then_expr,
                else_expr,
                span,
                ..
            } => {
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

            LogicExpression::RegisterInit { init, .. } => self.infer_expression_width(init),

            LogicExpression::Cast {
                target_type, span, ..
            } => {
                if let Ok(struct_def) = self.symbol_table.get_struct(target_type) {
                    let total_width: usize = struct_def.fields.iter().map(|f| f.width).sum();
                    Ok(total_width)
                } else if let Ok(enum_def) = self.symbol_table.get_enum(target_type) {
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
            } => match base.as_ref() {
                LogicExpression::Variable { name, .. } => {
                    if let Ok(enum_def) = self.symbol_table.get_enum(name) {
                        if enum_def.variants.iter().any(|v| v.name == *field) {
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
                                name: format!("variant '{}' not found in enum '{}'", field, name)
                                    .into(),
                            });
                        }
                    }

                    if let Some(type_name) = self.get_type(name) {
                        if let Ok(struct_def) = self.symbol_table.get_struct(type_name) {
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

                    if let Ok(struct_def) = self.symbol_table.get_struct(name) {
                        if let Some(field_def) = struct_def.fields.iter().find(|f| f.name == *field)
                        {
                            return Ok(field_def.width);
                        }
                    }

                    Err(WidthError::CannotInferWidth {
                        span: span_to_source_span(span),
                        name: format!(
                            "cannot infer width of field access '{}.{}' - variable type unknown",
                            name, field
                        )
                        .into(),
                    })
                }
                LogicExpression::Cast { target_type, .. } => {
                    if let Ok(struct_def) = self.symbol_table.get_struct(target_type) {
                        if let Some(field_def) = struct_def.fields.iter().find(|f| f.name == *field)
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
                            name: format!("cast target '{}' is not a struct", target_type).into(),
                        })
                    }
                }
                _ => Err(WidthError::CannotInferWidth {
                    span: span_to_source_span(span),
                    name: "cannot infer width of field access on complex expression"
                        .to_string()
                        .into(),
                }),
            },
        }
    }

    fn infer_block_or_expr_width(&self, block_or_expr: &BlockOrExpr) -> Result<usize, WidthError> {
        match block_or_expr {
            BlockOrExpr::Expression(expr) => self.infer_expression_width(expr),
            BlockOrExpr::Pass(_) => Ok(0),
            BlockOrExpr::Block(statements) => {
                if statements.is_empty() {
                    return Ok(0);
                }

                for statement in statements.iter().rev() {
                    match statement {
                        LogicStatement::Expression(expr) => {
                            return self.infer_expression_width(expr);
                        }
                        LogicStatement::Let { .. } => {
                            continue;
                        }
                        LogicStatement::Assignment { expression, .. } => {
                            return self.infer_expression_width(expression);
                        }
                        LogicStatement::If {
                            then_block,
                            else_block,
                            ..
                        } => {
                            let then_width = self.infer_block_or_expr_width(then_block)?;
                            if let Some(else_blk) = else_block {
                                let else_width = self.infer_block_or_expr_width(else_blk)?;

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

                Ok(0)
            }
        }
    }

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
            LogicOperator::Add | LogicOperator::Subtract => {
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

            LogicOperator::Multiply => Ok(left_width + right_width),

            LogicOperator::Divide => Ok(left_width),

            LogicOperator::Modulo => Ok(right_width),

            LogicOperator::BitwiseAnd | LogicOperator::BitwiseOr | LogicOperator::BitwiseXor => {
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

            LogicOperator::ShiftLeft | LogicOperator::ShiftRight => Ok(left_width),

            LogicOperator::Equal
            | LogicOperator::NotEqual
            | LogicOperator::LessThan
            | LogicOperator::GreaterThan
            | LogicOperator::LessThanOrEqual
            | LogicOperator::GreaterThanOrEqual => Ok(1),
        }
    }

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

        if expr_width == target_width {
            return WidthValidationResult::Ok;
        }

        if expr_width < target_width {
            if is_literal(expr) {
                return WidthValidationResult::Ok;
            }
            return WidthValidationResult::Ok;
        }

        let span = Self::get_expression_span(expr);

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

        WidthValidationResult::Error(WidthError::WidthMismatch {
            span,
            name: target_name.to_string().into(),
            src_width: expr_width,
            dst_width: target_width,
            dst_width_minus_1: target_width.saturating_sub(1),
        })
    }
}
