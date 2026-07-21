//! Connection Interface Routing (CIR) — Core Module
//!
//! Implements the semantic abstraction layer above the existing port escape
//! and routing infrastructure for multi-interface component footprints,
//! capability tracking, and routing intent abstraction.
//!
//! Reference: `Docs/v0.1.9/Connection-Interface-Routing.md`

mod access_region;
mod capability;
mod geometry;
pub mod testing;
mod types;

pub use access_region::AccessRegion;
pub use capability::InterfaceCapability;
pub use geometry::InterfaceGeometry;
pub use physical::{PhysicalInterface, PhysicalInterfaceParams};
pub use testing::DefaultRoutingDatabase;
pub use types::{DerivedConstraint, InterfaceId, Normal2D, Orientation, RoutingDatabase};

mod physical;
