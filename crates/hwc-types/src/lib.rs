//! Core type definitions shared across Hardware Script compiler crates.
//!
//! This crate contains fundamental types that need to be shared across
//! multiple crates without creating circular dependencies.

/// Strongly-typed net ID (newtype wrapper around u32).
///
/// Zero memory overhead - compiles to a raw u32.
/// Provides compile-time safety for net identification across the codebase.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
pub struct NetId(pub u32);

impl NetId {
    /// Semantic constant for unconnected/keepout zones.
    /// Components and pours with this net ID block all routing.
    pub const UNCONNECTED: NetId = NetId(0);

    /// Create a new net ID.
    #[inline]
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    /// Get the raw ID value.
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }

    /// Check if this is an unconnected/keepout zone.
    #[inline]
    pub const fn is_unconnected(self) -> bool {
        self.0 == 0
    }
}
