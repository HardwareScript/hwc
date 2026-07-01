//! Component and substrate placement.
//!
//! This module provides functionality for placing hardware components, substrates,
//! pours, contacts, and modules. It has been refactored into
//! smaller, focused submodules for better maintainability.

mod array;
mod component;
mod contact;
pub mod context;
pub mod coordinate_evaluation; // Made public for anchor reference evaluation
pub mod helpers;
mod module;
mod plane;
mod pour;
mod substrate;

// Re-export public functions
pub use component::place_component;
pub use contact::place_contact;
pub use plane::place_plane;
pub use pour::place_pour;
pub use substrate::place_substrate;
