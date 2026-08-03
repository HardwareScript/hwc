//! Comptime Anchor Arithmetic Evaluator
//!
//! v0.2.1: Evaluates mathematical expressions over entity anchor properties.
//! All expressions evaluate to absolute picometer values (i64).
//!
//! The 5 Laws of Comptime Anchor Math:
//! 1. Immutability - No variable reassignment
//! 2. Acyclic DAG - No circular dependencies
//! 3. Physical Units - Type-safe dimensions
//! 4. No Runtime Flow - Comptime unrolling only
//! 5. Single-Pass - Evaluates ONCE to i64

use compact_str::CompactString;
use hwc_parser::{BinaryOperator, Edge, Expression, Span};
use rustc_hash::FxHashMap;

/// A placed entity with resolved bounding box for evaluation
#[derive(Debug, Clone)]
pub struct PlacedEntity {
    /// Minimum corner (left, bottom, min_z) in picometers
    pub min_x: i64,
    pub min_y: i64,
    pub min_z: i64,
    /// Maximum corner (right, top, max_z) in picometers
    pub max_x: i64,
    pub max_y: i64,
    pub max_z: i64,
}

/// Errors during anchor expression evaluation
#[derive(Debug, Clone)]
pub enum EvaluationError {
    /// Entity not yet placed (dependency not resolved)
    EntityNotPlaced {
        entity: CompactString,
        span: Span,
    },
    /// Unknown anchor property
    UnknownProperty {
        property: CompactString,
        span: Span,
    },
    /// Division by zero
    DivisionByZero { span: Span },
    /// Dimensional type mismatch (e.g., length + voltage)
    DimensionalMismatch {
        left_type: &'static str,
        right_type: &'static str,
        span: Span,
    },
}

impl std::fmt::Display for EvaluationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EvaluationError::EntityNotPlaced { entity, .. } => {
                write!(f, "Entity '{}' has not been placed yet", entity)
            }
            EvaluationError::UnknownProperty { property, .. } => {
                write!(f, "Unknown anchor property '{}'", property)
            }
            EvaluationError::DivisionByZero { .. } => {
                write!(f, "Division by zero in anchor expression")
            }
            EvaluationError::DimensionalMismatch {
                left_type,
                right_type,
                ..
            } => {
                write!(
                    f,
                    "Dimensional mismatch: cannot operate on {} and {}",
                    left_type, right_type
                )
            }
        }
    }
}

/// Evaluator for comptime anchor arithmetic expressions
pub struct AnchorEvaluator<'a> {
    placed_entities: &'a FxHashMap<CompactString, PlacedEntity>,
}

impl<'a> AnchorEvaluator<'a> {
    /// Create a new evaluator with already-placed entities
    pub fn new(placed_entities: &'a FxHashMap<CompactString, PlacedEntity>) -> Self {
        Self { placed_entities }
    }

    /// Resolve an anchor property to an absolute picometer value
    pub fn resolve_anchor_property(
        &self,
        entity_name: &str,
        edge: &Edge,
    ) -> Result<i64, EvaluationError> {
        let entity = self.placed_entities.get(entity_name).ok_or_else(|| {
            EvaluationError::EntityNotPlaced {
                entity: entity_name.into(),
                span: Span::new(0, 0),
            }
        })?;

        match edge {
            Edge::Left => Ok(entity.min_x),
            Edge::Right => Ok(entity.max_x),
            Edge::Bottom => Ok(entity.min_y),
            Edge::Top => Ok(entity.max_y),
            Edge::Front => Ok(entity.min_z),
            Edge::Back => Ok(entity.max_z),
            Edge::MinZ => Ok(entity.min_z),
            Edge::MaxZ => Ok(entity.max_z),
            Edge::CenterX => Ok((entity.min_x + entity.max_x) / 2),
            Edge::CenterY => Ok((entity.min_y + entity.max_y) / 2),
            Edge::CenterZ => Ok((entity.min_z + entity.max_z) / 2),
            Edge::Center => Ok((entity.min_x + entity.max_x) / 2), // Default to X
            Edge::TopLeft => Ok(entity.max_y),                      // Y component
            Edge::TopRight => Ok(entity.max_y),
            Edge::BottomLeft => Ok(entity.min_y),
            Edge::BottomRight => Ok(entity.min_y),
        }
    }

    /// Evaluate an expression to an absolute picometer value
    pub fn evaluate(&self, expr: &Expression) -> Result<i64, EvaluationError> {
        match expr {
            Expression::Literal { value, .. } => Ok(*value),

            Expression::FloatLiteral { value, .. } => Ok((*value * 1_000_000.0) as i64),

            Expression::Measurement {
                value, unit, span: _, ..
            } => {
                let pm = measurement_to_picometers(*value, unit);
                Ok(pm)
            }

            Expression::Percentage { value, span: _ } => {
                // Percentages are dimensionless; treat as picometers * value / 100
                Ok((*value * 10_000_000.0) as i64)
            }

            Expression::Variable { name, span } => {
                // Variables should have been substituted during unrolling
                Err(EvaluationError::EntityNotPlaced {
                    entity: name.clone(),
                    span: *span,
                })
            }

            Expression::Unary {
                operator,
                operand,
                span: _,
            } => {
                let val = self.evaluate(operand)?;
                match operator {
                    hwc_parser::UnaryOperator::Negate => Ok(-val),
                    hwc_parser::UnaryOperator::Plus => Ok(val),
                    hwc_parser::UnaryOperator::Not => Ok(if val == 0 { 1 } else { 0 }),
                }
            }

            Expression::Grouped { expression, .. } => self.evaluate(expression),

            Expression::AnchorReference { anchor, edge, .. } => {
                self.resolve_anchor_property(&anchor.name, edge)
            }

            Expression::Binary {
                left,
                operator,
                right,
                span: _,
            } => {
                let left_val = self.evaluate(left)?;
                let right_val = self.evaluate(right)?;

                match operator {
                    BinaryOperator::Add => Ok(left_val + right_val),
                    BinaryOperator::Subtract => Ok(left_val - right_val),
                    BinaryOperator::Multiply => Ok(left_val * right_val),
                    BinaryOperator::Divide => {
                        if right_val == 0 {
                            Err(EvaluationError::DivisionByZero { span: Span::new(0, 0) })
                        } else {
                            Ok(left_val / right_val)
                        }
                    }
                    BinaryOperator::Modulo => {
                        if right_val == 0 {
                            Err(EvaluationError::DivisionByZero { span: Span::new(0, 0) })
                        } else {
                            Ok(left_val % right_val)
                        }
                    }
                    // Comparison operators return 1 for true, 0 for false
                    BinaryOperator::Equal => Ok(if left_val == right_val { 1 } else { 0 }),
                    BinaryOperator::NotEqual => Ok(if left_val != right_val { 1 } else { 0 }),
                    BinaryOperator::LessThan => Ok(if left_val < right_val { 1 } else { 0 }),
                    BinaryOperator::GreaterThan => Ok(if left_val > right_val { 1 } else { 0 }),
                    BinaryOperator::LessThanOrEqual => Ok(if left_val <= right_val { 1 } else { 0 }),
                    BinaryOperator::GreaterThanOrEqual => Ok(if left_val >= right_val { 1 } else { 0 }),
                    // Boolean operators (treat non-zero as true)
                    BinaryOperator::And => Ok(if left_val != 0 && right_val != 0 { 1 } else { 0 }),
                    BinaryOperator::Or => Ok(if left_val != 0 || right_val != 0 { 1 } else { 0 }),
                }
            }

            Expression::Coordinate { coord: _, span: _ } => {
                // For coordinate expressions, evaluate the X component
                // This handles cases where a coordinate is used in an expression context
                Ok(0) // Placeholder - coordinates are handled separately
            }

            Expression::FunctionCall { name, arguments: _, span } => {
                // Function calls need evaluation context - delegate to expression evaluator
                let eval_context = hwc_parser::EvaluationContext::default();
                let result = expr.evaluate(&eval_context).map_err(|e| {
                    EvaluationError::EntityNotPlaced {
                        entity: format!("function '{}': {}", name, e).into(),
                        span: *span,
                    }
                })?;
                
                // Convert result to picometers
                result.to_picometers().map_err(|e| {
                    EvaluationError::EntityNotPlaced {
                        entity: format!("function result: {}", e).into(),
                        span: *span,
                    }
                })
            }
        }
    }
}

/// Convert a measurement value to picometers based on its unit
fn measurement_to_picometers(value: f64, unit: &hwc_parser::Unit) -> i64 {
    let pm = match unit {
        hwc_parser::Unit::Picometer => value,
        hwc_parser::Unit::Nanometer => value * 1_000.0,
        hwc_parser::Unit::Micrometer => value * 1_000_000.0,
        hwc_parser::Unit::Millimeter => value * 1_000_000_000.0,
        hwc_parser::Unit::Centimeter => value * 10_000_000_000.0,
        _ => value, // Default: assume picometers for custom/unknown units
    };
    pm as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use hwc_parser::Span;

    fn make_literal(val: i64) -> Expression {
        Expression::Literal {
            value: val,
            span: Span::new(0, 0),
        }
    }

    fn make_anchor(name: &str, edge: Edge) -> Expression {
        Expression::AnchorReference {
            anchor: hwc_parser::AnchorReference {
                name: name.into(),
                span: Span::new(0, 0),
            },
            edge,
            span: Span::new(0, 0),
        }
    }

    fn make_binary(left: Expression, op: BinaryOperator, right: Expression) -> Expression {
        Expression::Binary {
            left: Box::new(left),
            operator: op,
            right: Box::new(right),
            span: Span::new(0, 0),
        }
    }

    #[test]
    fn test_literal_evaluation() {
        let entities = FxHashMap::default();
        let evaluator = AnchorEvaluator::new(&entities);
        assert_eq!(evaluator.evaluate(&make_literal(42)).unwrap(), 42);
    }

    #[test]
    fn test_binary_addition() {
        let entities = FxHashMap::default();
        let evaluator = AnchorEvaluator::new(&entities);
        let expr = make_binary(make_literal(10), BinaryOperator::Add, make_literal(20));
        assert_eq!(evaluator.evaluate(&expr).unwrap(), 30);
    }

    #[test]
    fn test_anchor_property_resolution() {
        let mut entities = FxHashMap::default();
        entities.insert(
            "Pad_A".into(),
            PlacedEntity {
                min_x: 100_000_000,
                min_y: 100_000_000,
                min_z: 0,
                max_x: 200_000_000,
                max_y: 200_000_000,
                max_z: 0,
            },
        );

        let evaluator = AnchorEvaluator::new(&entities);

        // Pad_A.right = 200_000_000 pm
        assert_eq!(
            evaluator
                .resolve_anchor_property("Pad_A", &Edge::Right)
                .unwrap(),
            200_000_000
        );

        // Pad_A.left = 100_000_000 pm
        assert_eq!(
            evaluator
                .resolve_anchor_property("Pad_A", &Edge::Left)
                .unwrap(),
            100_000_000
        );

        // Pad_A.center_x = (100_000_000 + 200_000_000) / 2 = 150_000_000 pm
        assert_eq!(
            evaluator
                .resolve_anchor_property("Pad_A", &Edge::CenterX)
                .unwrap(),
            150_000_000
        );
    }

    #[test]
    fn test_circular_dependency_detection() {
        // This is tested at the graph level, not the evaluator level
        // The evaluator assumes topological order
    }
}
