//! Routing Intent System
//!
//! Defines semantic routing intents that influence cost evaluation and
//! routing strategy selection. Intents are loaded from PDK profiles and
//! applied per-net or per-interface.
//!
//! This module was moved from `hwc-engine` to `hwc-materials` to allow
//! `ConstraintSet` to reference routing intents without circular dependencies.

use compact_str::CompactString;

/// A named routing intent with associated cost weights.
///
/// Intents encode designer policy about how a net should be routed.
/// The router uses intents to select cost evaluation parameters and
/// routing strategies.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct RoutingIntent {
    /// Human-readable intent name (e.g., "Clock", "PowerRail")
    pub name: CompactString,
    /// Whether this is on a timing-critical path
    pub is_critical_path: bool,
    /// Target impedance in milliohms (0 = no requirement)
    pub target_impedance_milliohms: u32,
    /// Cost weights override (None = use profile defaults)
    pub cost_weights: Option<IntentCostWeights>,
}

/// Cost weight overrides for a routing intent.
///
/// These values override the base `RoutingHeuristics` from the PDK profile.
/// All fields are optional — only specified fields override the defaults.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct IntentCostWeights {
    /// Base cost per grid step (base movement cost)
    pub base_cost: Option<i64>,
    /// Penalty for via transitions (layer changes)
    pub via_penalty: Option<i64>,
    /// Penalty for moving against preferred layer direction
    pub direction_penalty: Option<i64>,
    /// Penalty when clearance is tight
    pub tight_clearance_penalty: Option<i64>,
    /// Penalty for crosstalk risk
    pub crosstalk_penalty: Option<i64>,
    /// Penalty for impedance-controlled nets
    pub impedance_penalty: Option<i64>,
    /// Extreme penalty for crossing reference-plane voids
    pub reference_void_penalty: Option<i64>,
}

impl Default for RoutingIntent {
    fn default() -> Self {
        Self {
            name: CompactString::new("Default"),
            is_critical_path: false,
            target_impedance_milliohms: 0,
            cost_weights: None,
        }
    }
}

impl RoutingIntent {
    /// Create a new routing intent with a name.
    pub fn new(name: &str) -> Self {
        Self {
            name: CompactString::new(name),
            is_critical_path: false,
            target_impedance_milliohms: 0,
            cost_weights: None,
        }
    }

    /// Mark as critical path.
    pub fn with_critical_path(mut self, critical: bool) -> Self {
        self.is_critical_path = critical;
        self
    }

    /// Set target impedance.
    pub fn with_impedance(mut self, milliohms: u32) -> Self {
        self.target_impedance_milliohms = milliohms;
        self
    }

    /// Set cost weight overrides.
    pub fn with_cost_weights(mut self, weights: IntentCostWeights) -> Self {
        self.cost_weights = Some(weights);
        self
    }

    /// Create an intent from profile intent data.
    ///
    /// Converts parsed profile intent data to a `RoutingIntent` that can
    /// be used by the routing engine.
    pub fn from_profile_data(
        name: &str,
        routing_style: Option<&str>,
        cost_weights: Option<&IntentCostWeights>,
    ) -> Self {
        let mut intent = Self::new(name);

        // Map routing style to intent properties
        if let Some(style) = routing_style {
            match style {
                "straight" => {
                    intent.is_critical_path = true;
                }
                "manhattan" => {
                    // Default behavior, no special flags needed
                }
                "auto" => {
                    // Let the router decide
                }
                _ => {
                    // Unknown style, use defaults
                }
            }
        }

        // Clone cost weights if provided
        if let Some(pw) = cost_weights {
            intent.cost_weights = Some(pw.clone());
        }

        intent
    }

    /// Look up an intent by name from a list of known intents.
    pub fn lookup(name: &str, known_intents: &[RoutingIntent]) -> Option<RoutingIntent> {
        known_intents.iter().find(|i| i.name == name).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_intent() {
        let intent = RoutingIntent::default();
        assert_eq!(intent.name, "Default");
        assert!(!intent.is_critical_path);
        assert!(intent.cost_weights.is_none());
    }

    #[test]
    fn test_intent_lookup() {
        let intents = vec![RoutingIntent::new("Clock"), RoutingIntent::new("Power")];
        assert!(RoutingIntent::lookup("Clock", &intents).is_some());
        assert!(RoutingIntent::lookup("NonExistent", &intents).is_none());
    }

    #[test]
    fn test_from_profile_intent() {
        let cost_weights = IntentCostWeights {
            base_cost: None,
            via_penalty: Some(50_000),
            direction_penalty: Some(5),
            tight_clearance_penalty: None,
            crosstalk_penalty: Some(100),
            impedance_penalty: None,
            reference_void_penalty: None,
        };

        let intent =
            RoutingIntent::from_profile_data("Clock", Some("straight"), Some(&cost_weights));

        assert_eq!(intent.name, "Clock");
        assert!(intent.is_critical_path);
        let weights = intent.cost_weights.as_ref().expect("should have weights");
        assert_eq!(weights.via_penalty, Some(50_000));
        assert_eq!(weights.crosstalk_penalty, Some(100));
    }
}
