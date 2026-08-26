//! Type-safe arena allocation using u32 indices (zero dependencies)
//!
//! This module provides a custom IndexVec implementation that enables:
//! - Compile-time type safety (can't mix ComponentId with RouteId)
//! - 4-byte indices (vs 8-byte pointers on 64-bit systems)
//! - Zero lifetimes (no 'ast pollution)
//! - Native thread safety (Copy + Send + Sync)
//! - Salsa compatibility ('static types)
//!
//! # Architecture
//!
//! All AST nodes are stored in contiguous Vec<T> arrays within AstArena.
//! References use lightweight u32 indices instead of pointers or lifetimes.

mod ast_arena;
mod core;
mod id_types;

// Re-export all public types
pub use ast_arena::{AstArena, AstArenaOffsets};
pub use core::{Idx, IndexVec};
pub use id_types::*;
