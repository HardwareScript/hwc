//! Core GeometryRouter implementation

mod config;
mod engine;
mod entity_graph;
mod initialization;
mod minkowski;
mod types;


pub use types::{CopperPour, GeometryRouter, RouteSpaceRequest, RouterConfig};

// Re-export specific methods or traits if needed
