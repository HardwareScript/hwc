//! Route identifiers used for provenance tracking.

/// Unique identifier for a route segment (for provenance tracking)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RouteId(u64);

impl RouteId {
    pub fn new(id: u64) -> Self {
        Self(id)
    }

    /// Raw numeric value of this identifier.
    pub fn value(self) -> u64 {
        self.0
    }
}
