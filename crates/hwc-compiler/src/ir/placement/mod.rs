//! Component and substrate placement.
//!
//! This module provides functionality for placing hardware components, substrates,
//! pours, contacts, modules, and regions. It has been refactored into
//! smaller, focused submodules for better maintainability.

mod array;
mod component;
mod contact;
pub mod context;
pub mod coordinate_evaluation; // Made public for anchor reference evaluation
pub mod helpers;
pub mod intent; // PlacementIntent: explicit semantic precision for placement
mod module;
mod plane; // Modular: see plane/mod.rs
mod pour;
mod region; // v0.2.0: Region floorplanning
mod space_instance; // v0.2.1: Hierarchical space composition
mod substrate;

// Re-export public functions
pub use component::place_component;
pub use contact::{place_contact, PlaceContactParams};
pub use intent::PlacementIntent;
pub use plane::place_plane;
pub use pour::place_pour;
pub use region::{register_region, RegisterRegionParams}; // v0.2.0: Region registration
pub use space_instance::instantiate_sub_space; // v0.2.1: Hierarchical space instantiation
pub use substrate::place_substrate;
