//! Conversion functions from AST definitions to runtime structures
//!
//! This module implements Phase 6.4 and 6.5:
//! - Profile → ConstraintSet conversion
//! - Material → MaterialDatabase conversion

mod error;
mod material_conversion;
mod physics_calc;
mod profile_conversion;
mod unit_conversion;

pub use error::ConversionError;
pub use material_conversion::populate_material_database;
pub use physics_calc::{
    calculate_clearance_nm, calculate_crosstalk_penalty, calculate_trace_width_nm,
    calculate_trace_width_nm_with_k,
};
pub use profile_conversion::profile_to_constraints;
