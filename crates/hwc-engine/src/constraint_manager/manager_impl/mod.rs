//! Modular implementation of the Constraint Manager.
//!
//! This module contains the refactored implementation of the constraint manager,
//! split into focused submodules for better maintainability.

pub mod bounding_box;
pub mod constraint_generation;
pub mod domain;
pub mod electrical_analysis;
pub mod impedance;
pub mod layer_assignment;
pub mod manager;
pub mod net_classification;
pub mod symbol_table;

// Re-export the main types and traits
pub use manager::ConstraintManager;
pub use symbol_table::SymbolTableTrait;
