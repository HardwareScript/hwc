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

// Submodules
mod checker;
mod clearance;
mod error;
mod parallel;
mod thermal;
mod trace_width;
mod types;
mod via_checks; // Task 4.2: DRC Engine

// Re-export public API
pub use checker::DesignRuleChecker;
pub use clearance::validate_clearances;
pub use error::{report_to_errors, violation_to_error, DrcError};
pub use parallel::validate_physics_parallel;
pub use thermal::validate_current_density;
pub use trace_width::validate_trace_widths;
pub use types::{DrcReport, DrcViolation};
pub use via_checks::{
    validate_drill_to_drill_clearance, validate_via_diameters_analytic,
    validate_via_enclosure_analytic,
}; // Task 4.2
