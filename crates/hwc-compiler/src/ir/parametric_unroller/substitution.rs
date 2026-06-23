//! Variable substitution in AST nodes
//!
//! Handles substitution of loop variables in all statement types:
//! - Components (names, positions, net bindings)
//! - Pours (names, nets, boundaries)
//! - Contacts (names, nets, positions)
//! - Routes (pin references, waypoints)

use super::expression::{build_simple_expression_ast, evaluate_anchor_index_expression};
use crate::ir::errors::IrError;
use compact_str::CompactString;
use hwc_parser::{
    ComponentName, ComponentPlacement, ContactPlacement, Expression, PlanePlacement, PourPlacement,
    Route,
};

pub use super::collision::format_net_name;

/// Unroll a component placement by substituting the loop variable
///
/// **v0.1.6**: The `last` keyword is NOT resolved here
/// - `last` remains as "last" in the anchor name
/// - Resolution happens during constraint solving via BoundingBoxTracker
///
/// **v0.1.6 Item #13**: Substitutes loop variables in net bindings
/// - Simple bindings: `a: A[i]` → `a: A[0]`, `a: A[1]`, etc.
/// - Conditional bindings: `carry_in: if i == 0 then CarryIn else Carry[i-1]`
pub fn unroll_component(
    component: &ComponentPlacement,
    variable: &str,
    value: usize,
) -> Result<ComponentPlacement, IrError> {
    // Substitute loop variable in component name
    let name = component
        .name
        .as_ref()
        .map(|n| substitute_in_component_name(n, variable, value));

    // Substitute loop variable in position
    // Note: 'last' keyword is preserved and will be resolved during constraint solving
    let position = substitute_in_coordinate(&component.position, variable, value)?;

    // Substitute loop variable in net bindings (v0.1.6 Item #13)
    let pin_net_bindings = component
        .pin_net_bindings
        .iter()
        .map(|(pin, binding)| {
            let substituted_binding = substitute_in_net_binding(binding, variable, value)?;
            Ok((pin.clone(), substituted_binding))
        })
        .collect::<Result<rustc_hash::FxHashMap<_, _>, IrError>>()?;

    // Substitute loop variable in elevation (v0.1.7)
    let elevation = if let Some(elevation) = &component.elevation {
        match elevation {
            hwc_parser::Elevation::Physical { start, end } => {
                Some(hwc_parser::Elevation::Physical {
                    start: substitute_in_expression(start, variable, value)?,
                    end: end
                        .as_ref()
                        .map(|e| substitute_in_expression(e, variable, value))
                        .transpose()?,
                })
            }
            hwc_parser::Elevation::Semantic(id) => {
                Some(hwc_parser::Elevation::Semantic(id.clone()))
            }
            hwc_parser::Elevation::Relative => Some(hwc_parser::Elevation::Relative),
        }
    } else {
        None
    };

    Ok(ComponentPlacement {
        component_type: component.component_type.clone(),
        parameters: component.parameters.clone(),
        name,
        position,
        rotation: component.rotation.clone(),
        elevation,
        mount: component.mount,
        standoff: component.standoff.clone(),
        array_config: component.array_config.clone(),
        pin_net_bindings,
        waivers: component.waivers.clone(), // v0.1.7: Preserve unified waivers
        span: component.span,
    })
}

/// Unroll a pour placement by substituting the loop variable
pub fn unroll_pour(
    pour: &PourPlacement,
    variable: &str,
    value: usize,
) -> Result<PourPlacement, IrError> {
    // Substitute loop variable in pour name
    let name = substitute_in_component_name(&pour.name, variable, value);

    // Substitute loop variable in net name (v0.1.6: NetName with optional array index)
    let net = pour
        .net
        .as_ref()
        .map(|n| substitute_in_net_name(n, variable, value))
        .transpose()?;

    // Substitute loop variable in boundary
    let boundary = if let Some(b) = &pour.boundary {
        match b {
            hwc_parser::PourBoundary::Rect(from, to) => {
                let from_sub = substitute_in_coordinate(from, variable, value)?;
                let to_sub = substitute_in_coordinate(to, variable, value)?;
                Some(hwc_parser::PourBoundary::Rect(
                    Box::new(from_sub),
                    Box::new(to_sub),
                ))
            }
            hwc_parser::PourBoundary::Circle { center, radius } => {
                let center_sub = substitute_in_coordinate(center, variable, value)?;
                let radius_sub = substitute_in_expression(radius, variable, value)?;
                Some(hwc_parser::PourBoundary::Circle {
                    center: Box::new(center_sub),
                    radius: radius_sub,
                })
            }
        }
    } else {
        None
    };

    let elevation = match &pour.elevation {
        hwc_parser::Elevation::Physical { start, end } => hwc_parser::Elevation::Physical {
            start: substitute_in_expression(start, variable, value)?,
            end: end
                .as_ref()
                .map(|e| substitute_in_expression(e, variable, value))
                .transpose()?,
        },
        hwc_parser::Elevation::Semantic(id) => hwc_parser::Elevation::Semantic(id.clone()),
        hwc_parser::Elevation::Relative => hwc_parser::Elevation::Relative,
    };

    let thickness = pour
        .thickness
        .as_ref()
        .map(|t| substitute_in_expression(t, variable, value))
        .transpose()?;

    Ok(PourPlacement {
        material: pour.material.clone(),
        name,
        elevation,
        thickness,
        boundary,
        net,
        device: pour.device.clone(),
        thermal_relief: pour.thermal_relief,
        waivers: pour.waivers.clone(), // v0.1.7: Preserve unified waivers
        span: pour.span,
    })
}

/// Unroll a plane placement by substituting the loop variable
pub fn unroll_plane(
    plane: &PlanePlacement,
    variable: &str,
    value: usize,
) -> Result<PlanePlacement, IrError> {
    let name = substitute_in_component_name(&plane.name, variable, value);

    let net = plane
        .net
        .as_ref()
        .map(|n| substitute_in_net_name(n, variable, value))
        .transpose()?;

    let from = plane
        .from
        .as_ref()
        .map(|c| substitute_in_coordinate(c, variable, value))
        .transpose()?;

    let to = plane
        .to
        .as_ref()
        .map(|c| substitute_in_coordinate(c, variable, value))
        .transpose()?;

    let elevation = match &plane.elevation {
        hwc_parser::Elevation::Physical { start, end } => hwc_parser::Elevation::Physical {
            start: substitute_in_expression(start, variable, value)?,
            end: end
                .as_ref()
                .map(|e| substitute_in_expression(e, variable, value))
                .transpose()?,
        },
        hwc_parser::Elevation::Semantic(id) => hwc_parser::Elevation::Semantic(id.clone()),
        hwc_parser::Elevation::Relative => hwc_parser::Elevation::Relative,
    };

    let thickness = plane
        .thickness
        .as_ref()
        .map(|t| substitute_in_expression(t, variable, value))
        .transpose()?;

    let cutouts = plane
        .cutouts
        .iter()
        .map(|cutout| match cutout {
            hwc_parser::CutoutShape::Rectangle { width, height, at } => {
                let width_sub = substitute_in_expression(width, variable, value)?;
                let height_sub = substitute_in_expression(height, variable, value)?;
                let at_sub = substitute_in_coordinate(at, variable, value)?;
                Ok(hwc_parser::CutoutShape::Rectangle {
                    width: width_sub,
                    height: height_sub,
                    at: at_sub,
                })
            }
            hwc_parser::CutoutShape::Circle { radius, at } => {
                let radius_sub = substitute_in_expression(radius, variable, value)?;
                let at_sub = substitute_in_coordinate(at, variable, value)?;
                Ok(hwc_parser::CutoutShape::Circle {
                    radius: radius_sub,
                    at: at_sub,
                })
            }
        })
        .collect::<Result<Vec<_>, IrError>>()?;

    Ok(PlanePlacement {
        material: plane.material.clone(),
        name,
        elevation,
        thickness,
        from,
        to,
        net,
        cutouts,
        span: plane.span,
    })
}

/// Unroll a contact placement by substituting the loop variable
pub fn unroll_contact(
    contact: &ContactPlacement,
    variable: &str,
    value: usize,
) -> Result<ContactPlacement, IrError> {
    // Substitute loop variable in contact name
    let name = contact
        .name
        .as_ref()
        .map(|n| substitute_in_component_name(n, variable, value));

    // Substitute loop variable in net name (v0.1.6: NetName with optional array index)
    let net = contact
        .net
        .as_ref()
        .map(|n| substitute_in_net_name(n, variable, value))
        .transpose()?;

    // Substitute loop variable in position
    let position = substitute_in_coordinate(&contact.position, variable, value)?;

    let from_elevation = match &contact.from_elevation {
        hwc_parser::Elevation::Physical { start, end } => hwc_parser::Elevation::Physical {
            start: substitute_in_expression(start, variable, value)?,
            end: end
                .as_ref()
                .map(|e| substitute_in_expression(e, variable, value))
                .transpose()?,
        },
        hwc_parser::Elevation::Semantic(id) => hwc_parser::Elevation::Semantic(id.clone()),
        hwc_parser::Elevation::Relative => hwc_parser::Elevation::Relative,
    };
    let to_elevation = match &contact.to_elevation {
        hwc_parser::Elevation::Physical { start, end } => hwc_parser::Elevation::Physical {
            start: substitute_in_expression(start, variable, value)?,
            end: end
                .as_ref()
                .map(|e| substitute_in_expression(e, variable, value))
                .transpose()?,
        },
        hwc_parser::Elevation::Semantic(id) => hwc_parser::Elevation::Semantic(id.clone()),
        hwc_parser::Elevation::Relative => hwc_parser::Elevation::Relative,
    };

    // Substitute loop variable in generic properties (v0.1.9)
    let mut properties = rustc_hash::FxHashMap::default();
    for (name, expr) in &contact.properties {
        properties.insert(
            name.clone(),
            substitute_in_expression(expr, variable, value)?,
        );
    }

    Ok(ContactPlacement {
        material: contact.material.clone(),
        name,
        position,
        from_elevation,
        to_elevation,
        net,
        properties,
        contour: contact.contour.clone(),
        span: contact.span,
    })
}

/// Unroll a route by substituting the loop variable (Sprint 3.10: Parametric Routing)
///
/// Enables automatic routing inside for loops:
/// ```hw
/// for i in 0..2:
///     route Adder[i].carry_out to Adder[i+1].carry_in
/// ```
pub fn unroll_route(route: &Route, variable: &str, value: usize) -> Result<Route, IrError> {
    // Substitute loop variable in pin references
    let from = substitute_in_pin_reference(&route.from, variable, value)?;
    let to = substitute_in_pin_reference(&route.to, variable, value)?;

    // Substitute loop variable in width
    let width = route
        .width
        .as_ref()
        .map(|w| substitute_in_expression(w, variable, value))
        .transpose()?;

    // Substitute loop variable in strategy params
    let mut strategy_params = Vec::new();
    for (name, expr) in &route.strategy_params {
        strategy_params.push((
            name.clone(),
            substitute_in_expression(expr, variable, value)?,
        ));
    }

    // Substitute loop variable in path
    let path = route
        .path
        .as_ref()
        .map(|p| {
            p.iter()
                .map(|wp| substitute_in_coordinate(wp, variable, value))
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?;

    Ok(Route {
        from,
        to,
        width,
        layer: route.layer.clone(),
        strategy: route.strategy.clone(),
        pattern: route.pattern.clone(),
        strategy_params,
        path,
        signal_group: route.signal_group.clone(),
        bridge: route.bridge.clone(),
        exit_escape: route.exit_escape.clone(),
        enter_escape: route.enter_escape.clone(),
        current_limit_ac: route.current_limit_ac.clone(),
        span: route.span,
    })
}

/// Substitute loop variable in anchor names with expression evaluation
///
/// Handles anchor references like:
/// - `Adder[i-1]` → `Adder[0]` (when i=1)
/// - `Component[i+1]` → `Component[3]` (when i=2)
/// - `Block[i*2]` → `Block[4]` (when i=2)
///
/// **Strategy**:
/// 1. Parse the anchor name to extract base name and index expression
/// 2. If index contains loop variable, substitute and evaluate
/// 3. Return fully resolved anchor name (e.g., "Adder[0]")
///
/// **Physical Reality Rule**: Anchor names must resolve to concrete identifiers
/// before constraint solving. No symbolic references allowed in the engine.
pub fn substitute_in_anchor_name(
    anchor_name: &str,
    variable: &str,
    value: usize,
) -> Result<CompactString, IrError> {
    // Check if anchor name contains array syntax: Name[expr]
    if let Some(open_bracket) = anchor_name.find('[') {
        if let Some(close_bracket) = anchor_name.rfind(']') {
            // Extract base name and index expression
            let base_name = &anchor_name[..open_bracket];
            let index_str = &anchor_name[open_bracket + 1..close_bracket];

            // Parse the index expression
            // For now, we'll use a simple parser for common cases
            let evaluated_index = evaluate_anchor_index_expression(index_str, variable, value)?;

            // Return fully resolved anchor name
            return Ok(format!("{}[{}]", base_name, evaluated_index).into());
        }
    }

    // No array syntax - return name as is (v0.1.6: no more legacy _i suffixing)
    Ok(anchor_name.into())
}

/// Substitute loop variable in a net name (v0.1.6 Sprint 3.4)
///
/// Handles NetName with optional array index:
/// - Simple: `VDD` → `VDD` (no substitution needed)
/// - Indexed: `Bus[i]` → `Bus[0]`, `Bus[1]`, etc.
/// - Math: `D[i+1]` → `D[1]`, `D[2]`, etc. (evaluates expression)
///
/// **CRITICAL SAFETY GUARDS**:
/// 1. **Negative Index Guard (P44)**: Hardware indices cannot be negative
/// 2. **Division by Zero**: Caught during evaluation, returns graceful error
///
/// **Physical Reality Rule**: In hardware, `Bus[-1]` is a physical impossibility.
/// Unlike software arrays, hardware indices must be >= 0.
pub fn substitute_in_net_name(
    net_name: &hwc_parser::NetName,
    variable: &str,
    value: usize,
) -> Result<hwc_parser::NetName, IrError> {
    // If the net name has an index expression, substitute the variable in it
    if let Some(ref index_expr) = net_name.index {
        let substituted_index = substitute_in_expression(index_expr, variable, value)
            .unwrap_or_else(|_| index_expr.clone());

        // CRITICAL: Evaluate the expression to get a concrete value
        // This handles cases like D[i+1] → D[1] (not D[0+1])
        let evaluated_index = match substituted_index.evaluate_const() {
            Ok(hwc_parser::Value::Number(n)) => {
                if n < 0 {
                    return Err(IrError::InvalidExpression(format!(
                        "Negative array index in net name: {}[{}]",
                        net_name.base, n
                    )));
                }

                Expression::Literal {
                    value: n,
                    span: substituted_index.span(),
                }
            }
            Err(eval_error) => {
                return Err(IrError::InvalidExpression(format!(
                    "Expression evaluation failed in net name: {}",
                    eval_error
                )));
            }
            _ => substituted_index,
        };

        Ok(hwc_parser::NetName::indexed(net_name.base.clone(), evaluated_index, net_name.span))
    } else {
        Ok(hwc_parser::NetName::simple(net_name.base.clone(), net_name.span))
    }
}

/// Substitute loop variable in a net binding (v0.1.6 Item #13)
///
/// Handles:
/// - Simple bindings: `a: A[i]` → `a: A[0]`
/// - Conditional bindings: `carry_in: if i == 0 then CarryIn else Carry[i-1]`
///
/// For conditional bindings, evaluates the condition and returns the appropriate net name.
pub fn substitute_in_net_binding(
    binding: &hwc_parser::NetBinding,
    variable: &str,
    value: usize,
) -> Result<hwc_parser::NetBinding, IrError> {
    match binding {
        hwc_parser::NetBinding::Simple(net_name) => {
            // Substitute loop variable in net name string
            // Handle both simple names and array syntax: A[i], Bus[i+1], etc.
            let substituted = substitute_in_net_name_string(net_name, variable, value)?;
            Ok(hwc_parser::NetBinding::Simple(substituted))
        }
        hwc_parser::NetBinding::Conditional {
            condition,
            then_net,
            else_net,
        } => {
            // Evaluate the condition with the loop variable
            let mut context = rustc_hash::FxHashMap::default();
            context.insert(variable.into(), value as i64);

            let condition_result = condition.evaluate(&context).map_err(|e| {
                IrError::InvalidExpression(format!(
                    "Failed to evaluate conditional net binding: {}",
                    e
                ))
            })?;

            // Check if condition is true (non-zero)
            let is_true = match condition_result {
                hwc_parser::Value::Number(n) => n != 0,
                _ => {
                    return Err(IrError::InvalidExpression(
                        "Conditional expression must evaluate to a number".to_string(),
                    ))
                }
            };

            // Select the appropriate net name and substitute variables
            let selected_net = if is_true { then_net } else { else_net };
            let substituted = substitute_in_net_name_string(selected_net, variable, value)?;

            // Return as a simple binding (condition already evaluated)
            Ok(hwc_parser::NetBinding::Simple(substituted))
        }
    }
}

/// Substitute loop variable in a net name string
///
/// **CRITICAL FIX (Sprint 3.9)**: Use AST-based expression evaluation, not string math
///
/// Handles:
/// - Simple names: `VDD` → `VDD`
/// - Array syntax: `A[i]` → `A[0]`, `Bus[i+1]` → `Bus[1]`
///
/// **The "Carry[11]" Bug Fix**:
/// - OLD: `Carry[i+1]` with i=1 → string replace → `Carry[1+1]` → string squash → `Carry[11]` ❌
/// - NEW: `Carry[i+1]` with i=1 → parse AST → evaluate 1+1=2 → `Carry[2]` ✅
///
/// **Physical Reality Rule**: Net indices are mathematical expressions, not strings.
/// Treating `i+1` as a string is "Software-style string math" that breaks carry chains.
pub fn substitute_in_net_name_string(
    net_name: &str,
    variable: &str,
    value: usize,
) -> Result<CompactString, IrError> {
    // Check if net name contains array syntax: Name[expr]
    if let Some(open_bracket) = net_name.find('[') {
        if let Some(close_bracket) = net_name.rfind(']') {
            // Extract base name and index expression
            let base_name = &net_name[..open_bracket];
            let index_str = &net_name[open_bracket + 1..close_bracket];

            // CRITICAL FIX: Build an Expression AST and evaluate it properly
            // This ensures mathematical integrity: i+1 evaluates to 2, not "11"
            let parsed_expr = build_simple_expression_ast(index_str)?;

            // Substitute the loop variable in the AST
            let substituted_expr = substitute_in_expression(&parsed_expr, variable, value)?;

            // Evaluate the expression to get a concrete integer
            let evaluated_index = match substituted_expr.evaluate_const() {
                Ok(hwc_parser::Value::Number(n)) => {
                    // SAFETY GUARD: Check for negative indices (P44: Physical Impossibility)
                    if n < 0 {
                        return Err(IrError::InvalidExpression(format!(
                            "Net index expression '{}' evaluates to negative value {} (when {}={}). \
                             Hardware indices cannot be negative.",
                            index_str, n, variable, value
                        )));
                    }
                    n as usize
                }
                Ok(_) => {
                    return Err(IrError::InvalidExpression(format!(
                        "Net index expression '{}' must evaluate to a number",
                        index_str
                    )));
                }
                Err(e) => {
                    return Err(IrError::InvalidExpression(format!(
                        "Failed to evaluate net index expression '{}': {}",
                        index_str, e
                    )));
                }
            };

            // Return fully resolved net name with evaluated index
            return Ok(format!("{}[{}]", base_name, evaluated_index).into());
        }
    }

    // No array syntax - return as is
    Ok(net_name.into())
}

/// Substitute loop variable in a pin reference (Sprint 3.10: Parametric Routing)
///
/// Handles pin references with parametric indices:
/// - `Adder[i].carry_out` → `Adder[0].carry_out` (when i=0)
/// - `Adder[i+1].carry_in` → `Adder[1].carry_in` (when i=0)
/// - `Bus[i-1].data` → `Bus[0].data` (when i=1)
pub fn substitute_in_pin_reference(
    pin_ref: &hwc_parser::PinReference,
    variable: &str,
    value: usize,
) -> Result<hwc_parser::PinReference, IrError> {
    // Substitute in component name (v0.1.6: no more legacy _i suffixing)
    let component = pin_ref.component.clone();

    // Substitute in component index expression (if present)
    let component_index = if let Some(ref expr) = pin_ref.component_index {
        Some(substitute_in_expression(expr, variable, value)?)
    } else {
        None
    };

    // Substitute in pin index expression (if present)
    let pin_index = if let Some(ref expr) = pin_ref.pin_index {
        Some(substitute_in_expression(expr, variable, value)?)
    } else {
        None
    };

    Ok(hwc_parser::PinReference {
        component,
        component_index,
        pin: pin_ref.pin.clone(),
        pin_index,
        span: pin_ref.span,
    })
}

/// Substitute loop variable in a coordinate
///
/// **v0.1.6**: The `last` keyword is preserved as-is
/// - If anchor is "last", it stays as "last"
/// - Resolution happens during constraint solving via BoundingBoxTracker
pub fn substitute_in_coordinate(
    coord: &hwc_parser::Coordinate,
    variable: &str,
    value: usize,
) -> Result<hwc_parser::Coordinate, IrError> {
    match coord {
        hwc_parser::Coordinate::Positional { x, y, z, span } => {
            let x_sub = substitute_in_expression(x, variable, value)?;
            let y_sub = substitute_in_expression(y, variable, value)?;
            let z_sub = substitute_in_expression(z, variable, value)?;
            Ok(hwc_parser::Coordinate::Positional {
                x: x_sub,
                y: y_sub,
                z: z_sub,
                span: *span,
            })
        }
        hwc_parser::Coordinate::Declarative { x, y, z, span } => {
            let x_sub = substitute_in_expression(x, variable, value)?;
            let y_sub = substitute_in_expression(y, variable, value)?;
            let z_sub = substitute_in_expression(z, variable, value)?;
            Ok(hwc_parser::Coordinate::Declarative {
                x: x_sub,
                y: y_sub,
                z: z_sub,
                span: *span,
            })
        }
        hwc_parser::Coordinate::Relative(rel_pos) => {
            // Preserve 'last' keyword as-is - it will be resolved during constraint solving
            // Only substitute loop variables in regular anchor names
            let anchor_name = if rel_pos.anchor.name == "last" {
                "last".into()
            } else {
                substitute_in_anchor_name(&rel_pos.anchor.name, variable, value)?
            };

            let anchor = hwc_parser::AnchorReference {
                name: anchor_name,
                span: rel_pos.anchor.span,
            };

            // Substitute in offset
            let offset = match &rel_pos.offset {
                hwc_parser::RelativeOffset::Single(measurement) => {
                    hwc_parser::RelativeOffset::Single(measurement.clone())
                }
                hwc_parser::RelativeOffset::Vector { x, y, z } => {
                    let x_sub = substitute_in_expression(x, variable, value)?;
                    let y_sub = substitute_in_expression(y, variable, value)?;
                    let z_sub = substitute_in_expression(z, variable, value)?;
                    hwc_parser::RelativeOffset::Vector {
                        x: x_sub,
                        y: y_sub,
                        z: z_sub,
                    }
                }
            };

            Ok(hwc_parser::Coordinate::Relative(
                hwc_parser::RelativePosition {
                    anchor,
                    edge: rel_pos.edge,
                    offset,
                    span: rel_pos.span,
                },
            ))
        }
    }
}

/// Substitute loop variable in an expression
pub fn substitute_in_expression(
    expr: &Expression,
    variable: &str,
    value: usize,
) -> Result<Expression, IrError> {
    match expr {
        Expression::Literal { value: v, span } => Ok(Expression::Literal {
            value: *v,
            span: *span,
        }),
        Expression::FloatLiteral { value: v, span } => Ok(Expression::FloatLiteral {
            value: *v,
            span: *span,
        }),
        Expression::Measurement {
            value: v,
            unit,
            span,
        } => Ok(Expression::Measurement {
            value: *v,
            unit: unit.clone(),
            span: *span,
        }),
        Expression::Percentage { value: v, span } => Ok(Expression::Percentage {
            value: *v,
            span: *span,
        }),
        Expression::Variable { name, span } => {
            // If this is the loop variable, replace it with the literal value
            if name == variable {
                Ok(Expression::Literal {
                    value: value as i64,
                    span: *span,
                })
            } else {
                Ok(Expression::Variable {
                    name: name.clone(),
                    span: *span,
                })
            }
        }
        Expression::Binary {
            left,
            operator,
            right,
            span,
        } => {
            let left_sub = substitute_in_expression(left, variable, value)?;
            let right_sub = substitute_in_expression(right, variable, value)?;
            Ok(Expression::Binary {
                left: Box::new(left_sub),
                operator: *operator,
                right: Box::new(right_sub),
                span: *span,
            })
        }
        Expression::Unary {
            operator,
            operand,
            span,
        } => {
            let operand_sub = substitute_in_expression(operand, variable, value)?;
            Ok(Expression::Unary {
                operator: *operator,
                operand: Box::new(operand_sub),
                span: *span,
            })
        }
        Expression::Grouped { expression, span } => {
            let expression_sub = substitute_in_expression(expression, variable, value)?;
            Ok(Expression::Grouped {
                expression: Box::new(expression_sub),
                span: *span,
            })
        }
        Expression::AnchorReference { anchor, edge, span } => {
            // v0.1.6: Anchor names can contain loop variables if they were parsed
            // as symbolic strings (e.g., "Adder[i]"). We must substitute them.
            let anchor_name = if anchor.name == "last" {
                "last".into()
            } else {
                substitute_in_anchor_name(&anchor.name, variable, value)?
            };

            Ok(Expression::AnchorReference {
                anchor: hwc_parser::AnchorReference {
                    name: anchor_name,
                    span: anchor.span,
                },
                edge: *edge,
                span: *span,
            })
        }
    }
}
/// Substitute loop variable in a component name (v0.1.6)
pub fn substitute_in_component_name(
    name: &ComponentName,
    variable: &str,
    value: usize,
) -> ComponentName {
    // If the name has an index expression, substitute the variable in it
    if let Some(ref index_expr) = name.index {
        let substituted_index = substitute_in_expression(index_expr, variable, value)
            .unwrap_or_else(|_| index_expr.clone());
        ComponentName::indexed(name.base.clone(), substituted_index, name.span)
    } else {
        // No index expression - return name as is (v0.1.6: no more legacy _i suffixing)
        name.clone()
    }
}
