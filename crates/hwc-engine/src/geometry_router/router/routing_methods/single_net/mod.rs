//! Point-to-point single-net routing methods for the `GeometryRouter`.
//!
//! This module was split out of the monolithic `single_net.rs` into focused,
//! single-responsibility submodules:
//!
//! - `spatial_index`: Obstacle/spatial index construction (`build_routing_spatial_index`)
//! - `route`: Primary point-to-point routing (`route_net`)
//! - `length_constraint`: Length-targeted / meandered routing (`route_net_with_length_constraint`)
//! - `legalizer`: Localized legalization fallback (`legalize_local_window`)
//! - `tap`: Hierarchical same-net tap routing (`route_with_tapping` / `route_net_direct`)

pub mod legalizer;
pub mod length_constraint;
pub mod route;
pub mod spatial_index;
pub mod tap;
