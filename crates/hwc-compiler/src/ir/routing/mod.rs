//! Routing module for trace routing between pins.

mod automatic;
mod global;
mod helpers;
mod manual;

pub use automatic::route_automatic;
pub use global::AutoRouter;
pub use helpers::{
    collect_existing_nets, get_pin_positions, needs_automatic_routing, register_net_for_route,
};
pub use manual::route_manual;

use super::errors::IrError;
use hwc_engine::HardwareSpace;

/// Route a trace between pins.
///
/// Automatically selects between automatic A* routing or manual waypoint routing
/// based on whether waypoints are provided.
pub fn route_trace(
    space: &mut HardwareSpace,
    route: &hwc_parser::Route,
    origin: hwc_parser::OriginPoint,
    symbol_table: &crate::SymbolTable,
    eval_context: &hwc_parser::EvaluationContext, // UNIVERSAL CONTEXT
    stackup_manager: &crate::ir::stackup_manager::StackupManager,
    profile: Option<&hwc_parser::ProfileDefinition>,
) -> Result<(), IrError> {
    if needs_automatic_routing(route) {
        route_automatic(space, route, symbol_table, stackup_manager)
    } else {
        route_manual(
            space,
            route,
            origin,
            symbol_table,
            eval_context,
            stackup_manager,
            profile,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_needs_automatic_routing_with_path() {
        let route = hwc_parser::Route {
            from: hwc_parser::PinReference {
                component: "Power".into(),
                component_index: None,
                pin: "Plus".into(),
                pin_index: None,
                span: hwc_parser::Span::new(0, 0),
            },
            to: hwc_parser::PinReference {
                component: "Light".into(),
                component_index: None,
                pin: "Anode".into(),
                pin_index: None,
                span: hwc_parser::Span::new(0, 0),
            },
            width: None,
            strategy: None,
            strategy_params: vec![],
            path: Some(vec![hwc_parser::Coordinate::Positional {
                x: hwc_parser::Expression::Literal {
                    value: 1,
                    span: hwc_parser::Span::new(0, 1),
                },
                y: hwc_parser::Expression::Literal {
                    value: 15,
                    span: hwc_parser::Span::new(0, 2),
                },
                z: hwc_parser::Expression::Literal {
                    value: 15,
                    span: hwc_parser::Span::new(0, 2),
                },
                span: hwc_parser::Span::new(0, 0),
            }]),
            signal_group: None,
            bridge: None,
            enter_escape: None,
            exit_escape: None,
            span: hwc_parser::Span::new(0, 0),
        };

        assert!(!needs_automatic_routing(&route));
    }

    #[test]
    fn test_needs_automatic_routing_without_path() {
        let route = hwc_parser::Route {
            from: hwc_parser::PinReference {
                component: "Power".into(),
                component_index: None,
                pin: "Plus".into(),
                pin_index: None,
                span: hwc_parser::Span::new(0, 0),
            },
            to: hwc_parser::PinReference {
                component: "Light".into(),
                component_index: None,
                pin: "Anode".into(),
                pin_index: None,
                span: hwc_parser::Span::new(0, 0),
            },
            width: None,
            strategy: None,
            strategy_params: vec![],
            path: None,
            signal_group: None,
            bridge: None,
            enter_escape: None,
            exit_escape: None,
            span: hwc_parser::Span::new(0, 0),
        };

        assert!(needs_automatic_routing(&route));
    }
}
