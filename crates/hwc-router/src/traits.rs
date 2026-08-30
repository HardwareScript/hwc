//! Universal Router Engine Trait and Diagnostic Errors

use crate::types::{PinAccessMap, RoutedOutput};
use hwc_engine::EntityGraph;
use miette::Diagnostic;
use thiserror::Error;

#[derive(Error, Diagnostic, Debug)]
pub enum RoutingError {
    #[error("Routing failed: Pin '{pin}' of component '{component}' has no legal access points (PAA starved)")]
    #[diagnostic(
        code(R01),
        help("Check standard-cell placement spacing or reduce adjacent blockage density.")
    )]
    PinAccessStarvation { component: String, pin: String },

    #[error("Routing failed: Unresolvable congestion in G-Cell ({x}, {y}, layer: {layer}) on Net '{net}'")]
    #[diagnostic(
        code(R02),
        help("Increase track pitch, spread macro placement, or add routing layers.")
    )]
    UnresolvableCongestion {
        x: usize,
        y: usize,
        layer: u8,
        net: String,
    },

    #[error("Freeze-Silicon ECO Violation: Base silicon layer '{layer}' has {mutation_count} illegal mutations")]
    #[diagnostic(
        code(R03),
        help("Freeze-Silicon ECO mode strictly requires base layers (1-20) to remain untouched.")
    )]
    FreezeSiliconViolation {
        layer: String,
        mutation_count: usize,
    },

    #[error("External Router Plugin Error: {message}")]
    #[diagnostic(code(R99))]
    PluginFailure { message: String },
}

/// Contextual design payload passed into the routing engine.
pub struct RoutingTask<'a> {
    pub entity_graph: &'a EntityGraph,
    pub stackup: &'a hwc_engine::stackup::StackupManager,
    pub pin_access_map: &'a PinAccessMap,
}

/// The Universal Router Engine Trait.
pub trait RouterEngine: Send + Sync {
    /// Name identifier of the active routing backend.
    fn name(&self) -> &'static str;

    /// Executes end-to-end physical synthesis and emits verified geometry.
    fn route(&mut self, task: &RoutingTask) -> Result<RoutedOutput, RoutingError>;
}
