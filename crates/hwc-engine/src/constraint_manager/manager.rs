//! Constraint Manager: Main orchestration for constraint generation.
//!
//! This module re-exports the modular implementation from manager_impl.
//! The actual implementation has been split into focused submodules for better maintainability.

// Re-export from modular implementation
pub use super::manager_impl::{ConstraintManager, SymbolTableTrait};
