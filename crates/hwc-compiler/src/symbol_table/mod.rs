//! Symbol Table for Two-Pass Compilation
//!
//! The Symbol Table stores all definitions (materials, profiles, components, etc.)
//! during Pass 1, then resolves references during Pass 2 when assembling the space.
//!
//! This enables forward references and better error messages with span tracking.

mod definition;
mod error;
mod layer;
mod registration;
mod resolution;
mod traits;
mod utils;

pub use definition::Definition;
pub use error::SymbolError;
pub use layer::{SymbolLayer, SymbolTable};
pub use utils::expand_pin_declarations;
