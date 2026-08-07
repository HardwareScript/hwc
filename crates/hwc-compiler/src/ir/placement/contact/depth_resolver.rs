//! Via depth resolution with material-aware penetration control (v0.2.1)
//!
//! This module evaluates depth expressions (percentages, absolute measurements)
//! and resolves material-specific penetration depths for via contacts.
//!
//! # Architecture
//!
//! Depth resolution follows a 3-tier lookup hierarchy:
//! 1. Per-instance override (contact property `contact_depth`)
//! 2. Material-specific PDK rule (profile `material_contact_depths`)
//! 3. Global PDK default (profile `contact_depth`)
//!
//! # Expression Types
//!
//! - **Percentage**: `50%` → 50% of layer thickness
//! - **Absolute**: `150nm` → exactly 150nm
//! - **0%**: Surface contact (no penetration)
//! - **100%**: Complete penetration through layer
//!
//! # Safety Bounds
//!
//! Optional `min_contact_depth` and `max_contact_depth` clamp the final result.

use crate::ir::errors::IrError;
use hwc_parser::{Expression, Unit};

/// Convert measurement to nanometers
fn convert_measurement_to_nm(value: f64, unit: &Unit) -> i64 {
    match unit {
        Unit::Millimeter => (value * 1_000_000.0) as i64,
        Unit::Centimeter => (value * 10_000_000.0) as i64,
        Unit::Micrometer => (value * 1_000.0) as i64,
        Unit::Nanometer => value as i64,
        Unit::Picometer => (value / 1000.0) as i64,
        _ => value as i64, // Fallback for unsupported units
    }
}

/// Depth specification from contact placement (per-instance override)
#[derive(Debug, Clone)]
pub enum DepthSpecification {
    /// Uniform depth for both layers: `contact_depth: 50%`
    Uniform(Expression),

    /// Asymmetric depths: `contact_depth: { lower: 75%, upper: 33% }`
    Asymmetric {
        lower: Expression,
        upper: Expression,
    },
}

/// Context for evaluating depth expressions
pub struct DepthEvaluationContext<'a> {
    /// Lower layer thickness in nanometers
    pub lower_layer_thickness_nm: i64,

    /// Upper layer thickness in nanometers
    pub upper_layer_thickness_nm: i64,

    /// Space resolution in nanometers (for minimal depths) - reserved for future use
    #[allow(dead_code)]
    pub resolution_nm: i64,

    /// Minimum allowed depth (safety bound)
    pub min_depth_nm: Option<i64>,

    /// Maximum allowed depth (safety bound)
    pub max_depth_nm: Option<i64>,

    /// Symbol table for expression evaluation - reserved for future use
    #[allow(dead_code)]
    pub symbol_table: &'a crate::SymbolTable,

    /// Evaluation context for expressions - reserved for future use
    #[allow(dead_code)]
    pub eval_context: &'a hwc_parser::EvaluationContext,
}

impl<'a> DepthEvaluationContext<'a> {
    /// Evaluate a depth expression for a specific layer thickness
    ///
    /// # Arguments
    /// * `expr` - The depth expression to evaluate
    /// * `layer_thickness_nm` - The thickness of the layer this depth applies to
    ///
    /// # Returns
    /// Depth in nanometers, clamped to safety bounds
    pub fn evaluate_for_layer(
        &self,
        expr: &Expression,
        layer_thickness_nm: i64,
    ) -> Result<i64, IrError> {
        let depth_nm = self.evaluate_expression_raw(expr, layer_thickness_nm)?;
        Ok(self.apply_safety_bounds(depth_nm))
    }

    /// Evaluate expression without applying safety bounds (internal)
    fn evaluate_expression_raw(
        &self,
        expr: &Expression,
        layer_thickness_nm: i64,
    ) -> Result<i64, IrError> {
        match expr {
            // Percentage: value is stored as raw number (50 for 50%)
            // Convert to decimal and multiply by layer thickness
            Expression::Percentage { value, .. } => {
                let depth = (layer_thickness_nm as f64 * (value / 100.0)).round() as i64;
                Ok(depth)
            }

            // Measurement (absolute)
            Expression::Measurement { value, unit, .. } => {
                let depth_nm = convert_measurement_to_nm(*value, unit);
                Ok(depth_nm)
            }

            // Integer literal (treated as nanometers)
            Expression::Literal { value, .. } => Ok(*value),

            // Float literal (treated as nanometers)
            Expression::FloatLiteral { value, .. } => Ok(value.round() as i64),

            // Binary operations
            Expression::Binary {
                left,
                operator,
                right,
                ..
            } => {
                let left_val = self.evaluate_expression_raw(left, layer_thickness_nm)?;
                let right_val = self.evaluate_expression_raw(right, layer_thickness_nm)?;

                match operator {
                    hwc_parser::BinaryOperator::Add => Ok(left_val + right_val),
                    hwc_parser::BinaryOperator::Subtract => Ok(left_val - right_val),
                    hwc_parser::BinaryOperator::Multiply => Ok(left_val * right_val),
                    hwc_parser::BinaryOperator::Divide => {
                        if right_val == 0 {
                            return Err(IrError::ExpressionEvaluation {
                                message: "Division by zero in depth expression".into(),
                            });
                        }
                        Ok(left_val / right_val)
                    }
                    _ => Err(IrError::ExpressionEvaluation {
                        message: format!(
                            "Unsupported operator in depth expression: {:?}",
                            operator
                        ),
                    }),
                }
            }

            _ => {
                // For complex expressions, try generic evaluation
                // This handles identifiers, property access, etc.
                match self.try_generic_evaluation(expr) {
                    Ok(val) => Ok(val),
                    Err(_) => Err(IrError::ExpressionEvaluation {
                        message: format!(
                            "Cannot evaluate depth expression: {:?}. \
                             Depth must be a percentage (50%), measurement (150nm), or simple arithmetic.",
                            expr
                        ),
                    }),
                }
            }
        }
    }

    /// Try to evaluate expression using generic evaluation context
    fn try_generic_evaluation(&self, _expr: &Expression) -> Result<i64, IrError> {
        // TODO: Integrate with existing expression evaluator
        // For now, return error to force explicit depth specifications
        Err(IrError::ExpressionEvaluation {
            message: "Complex expression evaluation not yet implemented for depth".into(),
        })
    }

    /// Apply safety bounds (min/max) to depth value
    fn apply_safety_bounds(&self, depth_nm: i64) -> i64 {
        let mut result = depth_nm;

        if let Some(min) = self.min_depth_nm {
            result = result.max(min);
        }

        if let Some(max) = self.max_depth_nm {
            result = result.min(max);
        }

        result
    }
}

/// Resolve via penetration depths for both lower and upper layers
///
/// # Lookup Hierarchy
///
/// 1. Check contact properties for per-instance override
/// 2. Check material-specific depths in PDK
/// 3. Use global PDK default
///
/// # Arguments
///
/// * `contact` - The contact placement
/// * `lower_layer_name` - Name of the lower layer
/// * `lower_layer_thickness_nm` - Thickness of lower layer in nm
/// * `lower_material` - Material of lower layer
/// * `upper_layer_name` - Name of the upper layer
/// * `upper_layer_thickness_nm` - Thickness of upper layer in nm
/// * `upper_material` - Material of upper layer
/// * `profile` - PDK profile definition
/// * `context` - Depth evaluation context
///
/// # Returns
///
/// `(lower_depth_nm, upper_depth_nm)` - Penetration depths in nanometers
pub fn resolve_contact_depths(
    contact: &hwc_parser::ContactPlacement,
    lower_layer_name: &str,
    lower_layer_thickness_nm: i64,
    lower_material: &str,
    upper_layer_name: &str,
    upper_layer_thickness_nm: i64,
    upper_material: &str,
    profile: &hwc_parser::ProfileDefinition,
    context: &DepthEvaluationContext,
) -> Result<(i64, i64), IrError> {
    // PRIORITY 1: Per-instance override
    if let Some(depth_prop) = get_contact_depth_property(contact) {
        println!(
            "[DEPTH_RESOLVER] Contact '{}': Using per-instance depth override",
            contact.name.base.as_str()
        );
        return evaluate_depth_specification(&depth_prop, context);
    }

    // PRIORITY 2: Material-specific depths from PDK
    if let Some(material_depths) = &profile
        .via
        .as_ref()
        .and_then(|v| v.material_contact_depths.as_ref())
    {
        // Look up lower layer material
        if let Some(lower_expr) = material_depths.get(lower_material) {
            println!(
                "[DEPTH_RESOLVER] Contact '{}': Using material-specific depth for lower layer '{}' ({})",
                contact.name.base.as_str(), lower_layer_name, lower_material
            );
            let lower_depth = context.evaluate_for_layer(lower_expr, lower_layer_thickness_nm)?;

            // Look up upper layer material
            let upper_depth = if let Some(upper_expr) = material_depths.get(upper_material) {
                println!(
                    "[DEPTH_RESOLVER] Contact '{}': Using material-specific depth for upper layer '{}' ({})",
                    contact.name.base.as_str(), upper_layer_name, upper_material
                );
                context.evaluate_for_layer(upper_expr, upper_layer_thickness_nm)?
            } else {
                // Upper material not in map, use global default
                println!(
                    "[DEPTH_RESOLVER] Contact '{}': Upper material '{}' not in map, using global default",
                    contact.name.base.as_str(), upper_material
                );
                let global_expr = profile
                    .via
                    .as_ref()
                    .ok_or_else(|| IrError::MissingAsicConstraint {
                        message: "Profile via constraints required".into(),
                        hint: "Add via: block to profile".into(),
                    })?
                    .contact_depth
                    .clone();
                context.evaluate_for_layer(&global_expr, upper_layer_thickness_nm)?
            };

            return Ok((lower_depth, upper_depth));
        }

        // Lower material not in map, check upper
        if let Some(upper_expr) = material_depths.get(upper_material) {
            println!(
                "[DEPTH_RESOLVER] Contact '{}': Using material-specific depth for upper layer '{}' ({}), global for lower",
                contact.name.base.as_str(), upper_layer_name, upper_material
            );
            let upper_depth = context.evaluate_for_layer(upper_expr, upper_layer_thickness_nm)?;

            // Use global default for lower
            let global_expr = profile
                .via
                .as_ref()
                .ok_or_else(|| IrError::MissingAsicConstraint {
                    message: "Profile via constraints required".into(),
                    hint: "Add via: block to profile".into(),
                })?
                .contact_depth
                .clone();
            let lower_depth = context.evaluate_for_layer(&global_expr, lower_layer_thickness_nm)?;

            return Ok((lower_depth, upper_depth));
        }
    }

    // PRIORITY 3: Global PDK default
    println!(
        "[DEPTH_RESOLVER] Contact '{}': Using global PDK default depth",
        contact.name.base.as_str()
    );
    let global_expr = profile.via.as_ref()
        .ok_or_else(|| IrError::MissingAsicConstraint {
            message: format!(
                "Contact '{}' requires profile via.contact_depth but none is defined",
                contact.name.base.as_str()
            ),
            hint: "Add 'contact_depth: 50%' or 'contact_depth: 150nm' to the 'via:' section of your profile.\nThis specifies how deep vias penetrate into conductive layers.".into(),
        })?
        .contact_depth.clone();

    let lower_depth = context.evaluate_for_layer(&global_expr, lower_layer_thickness_nm)?;
    let upper_depth = context.evaluate_for_layer(&global_expr, upper_layer_thickness_nm)?;

    Ok((lower_depth, upper_depth))
}

/// Extract contact_depth property from contact placement (if specified)
fn get_contact_depth_property(
    contact: &hwc_parser::ContactPlacement,
) -> Option<DepthSpecification> {
    // Check if contact has a "contact_depth" property
    if let Some(expr) = contact.properties.get("contact_depth") {
        // Simple uniform depth: contact_depth: 50%
        return Some(DepthSpecification::Uniform(expr.clone()));
    }

    // Check for asymmetric depth properties: contact_depth_lower and contact_depth_upper
    let lower = contact.properties.get("contact_depth_lower");
    let upper = contact.properties.get("contact_depth_upper");

    if let (Some(lower_expr), Some(upper_expr)) = (lower, upper) {
        return Some(DepthSpecification::Asymmetric {
            lower: lower_expr.clone(),
            upper: upper_expr.clone(),
        });
    }

    None
}

/// Evaluate a depth specification (uniform or asymmetric)
fn evaluate_depth_specification(
    spec: &DepthSpecification,
    context: &DepthEvaluationContext,
) -> Result<(i64, i64), IrError> {
    match spec {
        DepthSpecification::Uniform(expr) => {
            let lower_depth = context.evaluate_for_layer(expr, context.lower_layer_thickness_nm)?;
            let upper_depth = context.evaluate_for_layer(expr, context.upper_layer_thickness_nm)?;
            Ok((lower_depth, upper_depth))
        }
        DepthSpecification::Asymmetric { lower, upper } => {
            let lower_depth =
                context.evaluate_for_layer(lower, context.lower_layer_thickness_nm)?;
            let upper_depth =
                context.evaluate_for_layer(upper, context.upper_layer_thickness_nm)?;
            Ok((lower_depth, upper_depth))
        }
    }
}
