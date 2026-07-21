//! Routing Intent System
//!
//! Re-exports `RoutingIntent` and `IntentCostWeights` from `hwc-materials`.
//! The canonical definitions live in `hwc_materials::routing_intent` to
//! allow `ConstraintSet` to reference them without circular dependencies.

pub use hwc_materials::routing_intent::{IntentCostWeights, RoutingIntent};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reexported_types() {
        let intent = RoutingIntent::new("Clock");
        assert_eq!(intent.name, "Clock");
    }

    #[test]
    fn test_cost_weights_default() {
        let weights = IntentCostWeights::default();
        assert!(weights.via_penalty.is_none());
    }
}
