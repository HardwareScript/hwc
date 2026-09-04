//! Design Rule Check: Validates routed design against physics constraints.
//!
//! This module implements Phase 3 of the 3-Phase Routing Sub-Pipeline.
//! It validates the final routed design to ensure it meets all physics
//! requirements (clearance, trace width, current density, impedance).
//!
//! **Documentation References**:
//! - `Docs/v0.1.3/ROUTING-AND-PHYSICS.md` (lines 800-1000, DRC validation)
//! - `Docs/v0.1.3/COMPILER-INTERNALS.md` (lines 800-900, Layer 4 Physics IR)
//!
//! # Module Organization
//!
//! This module is organized into logical submodules:
//!
//! - `types` - Core data structures (DrcViolation, DrcReport)
//! - `clearance` - Clearance validation logic
//! - `trace_width` - Trace width validation logic
//! - `thermal` - Current density validation logic
//! - `via_checks` - Via diameter and enclosure validation (Task 4.2)
//! - `parallel` - Parallel validation orchestration using Rayon
//! - `checker` - Main DesignRuleChecker struct
//! - `error` - DrcError types with miette integration

mod checker;
mod clearance;
mod crosstalk; // v0.3.0: Signal integrity validation
mod die_boundary; // Die/board boundary overflow check
mod electromigration; // P21: Electromigration validation
mod error;
mod junction; // P46: Junction breakdown validation
mod layer_pair; // Generic Data-Driven 2D Layer-Pair DRC Evaluator
mod min_area; // Gap 2: Minimum area validation (CMP peeling prevention)
mod parallel;
mod short_circuit;
mod tap_proximity;
mod thermal; // P22: Thermal rise validation
mod trace_width;
mod types;
mod via_checks; // Task 4.2: DRC Engine

pub use checker::DesignRuleChecker;
pub use clearance::validate_clearances;
pub use crosstalk::validate_crosstalk; // v0.3.0
pub use die_boundary::validate_die_boundary;
pub use electromigration::validate_electromigration; // P21
pub use error::{report_to_errors, violation_to_error, DrcError};
pub use junction::validate_junction_breakdown; // P46
pub use layer_pair::validate_layer_pair_rules;
pub use min_area::validate_min_area; // Gap 2: Minimum area validation
pub use parallel::validate_physics_parallel;
pub use short_circuit::validate_planar_shorts;
pub use tap_proximity::validate_tap_proximity;
pub use thermal::validate_thermal_rise; // P22
pub use trace_width::validate_trace_widths;
pub use types::{DrcReport, DrcViolation};
pub use via_checks::{
    validate_drill_to_drill_clearance, validate_layer_specific_via_enclosure,
    validate_via_diameters_analytic, validate_via_enclosure_analytic,
}; // Task 4.2
