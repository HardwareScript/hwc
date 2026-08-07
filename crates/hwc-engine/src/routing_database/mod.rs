//! Hierarchical Routing Database
//!
//! Maintains clear separation between child-instance routes and parent-level
//! interconnects, enabling proper connectivity validation and error reporting
//! in hierarchical designs.
//!
//! # Architecture
//!
//! ```text
//! Parent Space (Inverter_Cell)
//! ├── Child Instance Routes (immutable after flattening)
//! │   ├── PMOS_Inst.VDD: [route segments in parent coords]
//! │   ├── PMOS_Inst.Out: [route segments in parent coords]
//! │   ├── NMOS_Inst.GND: [route segments in parent coords]
//! │   └── NMOS_Inst.Out: [route segments in parent coords]
//! └── Parent Interconnects (created by parent)
//!     ├── Out: PMOS_Inst.Out_Pad → NMOS_Inst.Out_Pad
//!     └── In: PMOS_Inst.Gate_Strip → NMOS_Inst.Gate_Strip
//! ```
//!
//! # Key Principles
//!
//! 1. **Immutable Child Routes**: Once a child space is flattened, its routes
//!    are transformed to parent coordinates and stored immutably.
//!
//! 2. **Parent Interconnects**: Routes created at the parent level to connect
//!    between child instances or to external ports.
//!
//! 3. **Provenance Tracking**: Every route knows its source (child instance
//!    or parent level) for debugging and error reporting.
//!
//! 4. **Lazy Merging**: Child and parent routes are only merged on-demand
//!    during connectivity validation - never stored merged.
//!
//! # Module Layout
//!
//! This module was split out of the former monolithic `routing_database.rs`:
//!
//! | Module          | Responsibility                                             |
//! |-----------------|------------------------------------------------------------|
//! | [`ids`]         | [`RouteId`] newtype and identifier generation              |
//! | [`provenance`]  | [`RouteSource`] and [`ProvenanceSegment`]                  |
//! | [`database`]    | [`HierarchicalRoutingDatabase`] storage + simple accessors |
//! | [`registration`]| Route registration entry points                            |
//! | [`connectivity`]| Unified provenance view used by validation                 |
//! | [`export`]      | Export to `TraceSegment` form with direct layer lineage    |
//! | [`analytic`]    | Rebuild of `space.analytic_routes` from the database       |
//! | [`validation`]  | Hierarchical connectivity validation                       |
//! | [`statistics`]  | [`RoutingStatistics`] reporting                            |
//! | [`errors`]      | [`ConnectivityError`] and its diagnostics rendering        |

mod analytic;
mod connectivity;
mod database;
mod errors;
mod export;
mod ids;
mod provenance;
mod registration;
mod statistics;
mod validation;

#[cfg(test)]
mod tests;

pub use database::HierarchicalRoutingDatabase;
pub use errors::ConnectivityError;
pub use ids::RouteId;
pub use provenance::{ProvenanceSegment, RouteSource};
pub use statistics::RoutingStatistics;
