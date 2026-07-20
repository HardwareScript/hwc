use super::expression_sub::{substitute_in_expression, substitute_in_route_endpoint};
use crate::ir::errors::IrError;
use hwc_parser::Route;

pub fn unroll_route(route: &Route, variable: &str, value: usize) -> Result<Route, IrError> {
    let from = substitute_in_route_endpoint(&route.from, variable, value)?;
    let to = substitute_in_route_endpoint(&route.to, variable, value)?;

    let width = route
        .width
        .as_ref()
        .map(|w| substitute_in_expression(w, variable, value))
        .transpose()?;

    let mut strategy_params = Vec::new();
    for (name, expr) in &route.strategy_params {
        strategy_params.push((
            name.clone(),
            substitute_in_expression(expr, variable, value)?,
        ));
    }

    let path = route
        .path
        .as_ref()
        .map(|p| {
            p.iter()
                .map(|wp| super::expression_sub::substitute_in_coordinate(wp, variable, value))
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
