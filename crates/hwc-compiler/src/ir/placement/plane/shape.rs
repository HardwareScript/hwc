//! Shape dimension resolution for shape-based plane placement (v0.1.9
//! middle-level syntax).

use super::super::super::errors::IrError;
use crate::SymbolTable;
use hwc_parser::{EvaluationContext, Parameter, ParameterValue, ShapeInstance};

/// Resolve shape dimensions from a shape instance (v0.1.9 middle-level syntax).
///
/// Width and height are taken from the shape instance's keyword parameters
/// (`w`/`width`, `h`/`height`). v0.1.10 allows those parameters to be arbitrary
/// expressions (including variables), not just literal measurements.
///
/// If either dimension is still missing after processing the instance
/// parameters, the shape definition's CSG geometry AST is evaluated with the
/// supplied parameters and the resulting contour's bounding box is used to
/// fill in the gaps.
pub fn resolve_shape_dimensions(
    shape_inst: &ShapeInstance,
    symbol_table: &SymbolTable,
    eval_context: &EvaluationContext,
) -> Result<(i64, i64), IrError> {
    // Look up the shape definition to verify it exists
    let shape_def = symbol_table
        .get_shape(&shape_inst.shape_name)
        .ok_or_else(|| IrError::UndeclaredShape {
            shape: shape_inst.shape_name.clone(),
        })?;

//     eprintln!(
//         "[SHAPE DEBUG] Resolving dimensions for shape: {}",
//         shape_inst.shape_name
//     );
//     eprintln!(
//         "[SHAPE DEBUG] Parameters count: {}",
//         shape_inst.parameters.len()
//     );

    let mut eval_params: Vec<(String, i64)> = Vec::new();
    let mut width_nm = None;
    let mut height_nm = None;

    for param in &shape_inst.parameters {
        let Parameter::Keyword { name, value } = param;
//         eprintln!("[SHAPE DEBUG] Processing parameter: {} = {:?}", name, value);

        let value_nm = evaluate_parameter_nm(
            &shape_inst.shape_name,
            name,
            value,
            symbol_table,
            eval_context,
        )?;

        eval_params.push((name.to_string(), value_nm));

        match name.as_str() {
            "w" | "width" => {
//                 eprintln!("[SHAPE DEBUG] Setting width = {}nm", value_nm);
                width_nm = Some(value_nm);
            }
            "h" | "height" => {
//                 eprintln!("[SHAPE DEBUG] Setting height = {}nm", value_nm);
                height_nm = Some(value_nm);
            }
            _ => {}
        }
    }

    // Evaluate shape CSG geometry AST if dimensions are incomplete from the
    // explicit instance call parameters.
    if width_nm.is_none() || height_nm.is_none() {
        if let Some(ref csg_expr) = shape_def.csg {
            if let Some((evaluated_w, evaluated_h)) = evaluate_csg_extents(csg_expr, &eval_params) {
//                 eprintln!(
//                     "[SHAPE DEBUG] Evaluated CSG geometry dimensions: {}nm x {}nm",
//                     evaluated_w, evaluated_h
//                 );
                if width_nm.is_none() {
                    width_nm = Some(evaluated_w);
                }
                if height_nm.is_none() {
                    height_nm = Some(evaluated_h);
                }
            }
        }
    }

//     eprintln!(
//         "[SHAPE DEBUG] Final: width={:?}, height={:?}",
//         width_nm, height_nm
//     );

    match (width_nm, height_nm) {
        (Some(w), Some(h)) => {
//             eprintln!("[SHAPE DEBUG] Returning dimensions: {}nm x {}nm", w, h);
            Ok((w, h))
        }
        _ => Err(IrError::ShapeResolutionFailed {
            shape: shape_inst.shape_name.clone(),
            reason: format!(
                "Could not evaluate width and height for shape '{}' from its definition or instance parameters",
                shape_inst.shape_name
            ),
        }),
    }
}

/// Evaluate a single shape instance parameter down to nanometers.
fn evaluate_parameter_nm(
    shape_name: &str,
    param_name: &str,
    value: &ParameterValue,
    symbol_table: &SymbolTable,
    eval_context: &EvaluationContext,
) -> Result<i64, IrError> {
    match value {
        ParameterValue::Measurement(m) => {
            let pm = m
                .to_picometers_i64()
                .ok_or_else(|| IrError::ShapeResolutionFailed {
                    shape: shape_name.into(),
                    reason: format!("Parameter '{}' has non-distance unit", param_name),
                })?;
            let nm = pm / 1000; // Convert picometers to nanometers
//             eprintln!("[SHAPE DEBUG] Converted literal {}pm to {}nm", pm, nm);
            Ok(nm)
        }
        ParameterValue::Expression(expr) => {
            let nm =
                crate::ir::conversions::evaluate_expression_to_nm(expr, symbol_table, eval_context)
                    .map_err(|e| IrError::ShapeResolutionFailed {
                        shape: shape_name.into(),
                        reason: format!("Failed to evaluate parameter '{}': {}", param_name, e),
                    })?;
//             eprintln!("[SHAPE DEBUG] Evaluated expression to {}nm", nm);
            Ok(nm)
        }
        _ => Err(IrError::ShapeResolutionFailed {
            shape: shape_name.into(),
            reason: format!(
                "Parameter '{}' must be a Measurement or Expression",
                param_name
            ),
        }),
    }
}

/// Evaluate a shape's CSG expression and return the contour's `(width, height)`
/// extents in nanometers, or `None` when the contour is empty or degenerate.
fn evaluate_csg_extents(
    csg_expr: &hwc_parser::CsgExpression,
    eval_params: &[(String, i64)],
) -> Option<(i64, i64)> {
    let param_refs: Vec<(&str, i64)> = eval_params.iter().map(|(k, v)| (k.as_str(), *v)).collect();

    let contour =
        crate::via_resolver::library::csg_eval::evaluate_csg_expression(csg_expr, &param_refs);
    if contour.is_empty() {
        return None;
    }

    let min_x = contour.iter().map(|p| p.x).min()?;
    let max_x = contour.iter().map(|p| p.x).max()?;
    let min_y = contour.iter().map(|p| p.y).min()?;
    let max_y = contour.iter().map(|p| p.y).max()?;

    let evaluated_w = (max_x - min_x).abs();
    let evaluated_h = (max_y - min_y).abs();

    if evaluated_w > 0 && evaluated_h > 0 {
        Some((evaluated_w, evaluated_h))
    } else {
        None
    }
}
