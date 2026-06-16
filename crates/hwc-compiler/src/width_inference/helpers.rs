use hwc_parser::logic::*;

pub(crate) fn is_literal(expr: &LogicExpression) -> bool {
    matches!(
        expr,
        LogicExpression::Literal { .. } | LogicExpression::Boolean { .. }
    )
}

pub(crate) fn is_block_literal(block: &BlockOrExpr) -> bool {
    match block {
        BlockOrExpr::Expression(expr) => is_literal(expr),
        BlockOrExpr::Block(statements) => {
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
                        return false;
                    }
                }
            }
            false
        }
        BlockOrExpr::Pass(_) => false,
    }
}

pub(crate) fn unify_widths(
    width_a: usize,
    is_literal_a: bool,
    width_b: usize,
    is_literal_b: bool,
) -> Result<usize, String> {
    if width_a == width_b {
        return Ok(width_a);
    }

    if is_literal_a && width_a < width_b {
        return Ok(width_b);
    }

    if is_literal_b && width_b < width_a {
        return Ok(width_a);
    }

    if width_a < width_b {
        return Ok(width_b);
    }

    if width_b < width_a {
        return Ok(width_a);
    }

    Err(format!("{}-bit and {}-bit", width_a, width_b))
}
