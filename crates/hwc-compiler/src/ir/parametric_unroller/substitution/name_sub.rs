use super::super::expression::build_simple_expression_ast;
use super::expression_sub::substitute_in_expression;
use crate::ir::errors::IrError;
use compact_str::CompactString;
use hwc_parser::Expression;

pub fn substitute_in_net_name(
    net_name: &hwc_parser::NetName,
    variable: &str,
    value: usize,
) -> Result<hwc_parser::NetName, IrError> {
    if let Some(ref index_expr) = net_name.index {
        let substituted_index = substitute_in_expression(index_expr, variable, value)
            .unwrap_or_else(|_| index_expr.clone());

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

        Ok(hwc_parser::NetName::indexed(
            net_name.base.clone(),
            evaluated_index,
            net_name.span,
        ))
    } else {
        Ok(hwc_parser::NetName::simple(
            net_name.base.clone(),
            net_name.span,
        ))
    }
}

pub fn substitute_in_net_binding(
    binding: &hwc_parser::NetBinding,
    variable: &str,
    value: usize,
) -> Result<hwc_parser::NetBinding, IrError> {
    match binding {
        hwc_parser::NetBinding::Simple(net_name) => {
            let substituted = substitute_in_net_name_string(net_name, variable, value)?;
            Ok(hwc_parser::NetBinding::Simple(substituted))
        }
        hwc_parser::NetBinding::Conditional {
            condition,
            then_net,
            else_net,
        } => {
            let mut context = rustc_hash::FxHashMap::default();
            context.insert(variable.into(), hwc_parser::Value::Number(value as i64));

            let condition_result = condition.evaluate(&context).map_err(|e| {
                IrError::InvalidExpression(format!(
                    "Failed to evaluate conditional net binding: {}",
                    e
                ))
            })?;

            let is_true = match condition_result {
                hwc_parser::Value::Number(n) => n != 0,
                _ => {
                    return Err(IrError::InvalidExpression(
                        "Conditional expression must evaluate to a number".to_string(),
                    ))
                }
            };

            let selected_net = if is_true { then_net } else { else_net };
            let substituted = substitute_in_net_name_string(selected_net, variable, value)?;
            Ok(hwc_parser::NetBinding::Simple(substituted))
        }
    }
}

pub fn substitute_in_net_name_string(
    net_name: &str,
    variable: &str,
    value: usize,
) -> Result<CompactString, IrError> {
    if let Some(open_bracket) = net_name.find('[') {
        if let Some(close_bracket) = net_name.rfind(']') {
            let base_name = &net_name[..open_bracket];
            let index_str = &net_name[open_bracket + 1..close_bracket];

            let parsed_expr = build_simple_expression_ast(index_str)?;
            let substituted_expr = substitute_in_expression(&parsed_expr, variable, value)?;

            let evaluated_index = match substituted_expr.evaluate_const() {
                Ok(hwc_parser::Value::Number(n)) => {
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

            return Ok(format!("{}[{}]", base_name, evaluated_index).into());
        }
    }
    Ok(net_name.into())
}

pub fn substitute_in_component_name(
    name: &hwc_parser::ComponentName,
    variable: &str,
    value: usize,
) -> hwc_parser::ComponentName {
    // Handle template interpolation (v0.2.1)
    if let Some(ref template_parts) = name.template_parts {
        let mut substituted_parts = Vec::new();
        
        for part in template_parts {
            match part {
                hwc_parser::TemplateNamePart::Literal(lit) => {
                    substituted_parts.push(hwc_parser::TemplateNamePart::Literal(lit.clone()));
                }
                hwc_parser::TemplateNamePart::Expression(expr) => {
                    let substituted_expr = substitute_in_expression(expr, variable, value)
                        .unwrap_or_else(|_| expr.clone());
                    substituted_parts.push(hwc_parser::TemplateNamePart::Expression(substituted_expr));
                }
            }
        }
        
        return hwc_parser::ComponentName::template(substituted_parts, name.span);
    }
    
    // Handle array indexing
    if let Some(ref index_expr) = name.index {
        let substituted_index = substitute_in_expression(index_expr, variable, value)
            .unwrap_or_else(|_| index_expr.clone());
        return hwc_parser::ComponentName::indexed(name.base.clone(), substituted_index, name.span);
    }
    
    // Simple name - no substitution needed
    name.clone()
}
