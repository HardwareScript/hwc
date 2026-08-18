//! Scene Graph for 3D Visualization
//!
//! The scene graph is an intermediate representation between Hardware IR and 3D export formats.
//! It handles material colors, mesh generation, and provides a unified interface for GLB export.
//!
//! ## Module Structure
//!
//! - `types`: Core data structures (Color, MaterialNode, Vertex, Face, MeshNode, BoxParams)
//! - `materials`: Material handling and color parsing
//! - `geometry`: Geometric algorithms (Douglas-Peucker, perpendicular distance, etc.)
//! - `mesh_generation`: Basic mesh generation utilities (boxes, components)
//! - `ribbon`: Extruded ribbon and path-based mesh generation
//! - `substrate`: Substrate layer processing and net-aware clustering
//! - `exporters`: Export format implementations (GLB)
//! - `scene_graph_impl`: Main SceneGraph struct and high-level API

pub mod exporters;
pub mod geometry;
pub mod materials;
pub mod mesh_generation;
pub mod procedural;
pub mod ribbon;
pub mod scene_graph_impl;
pub mod substrate;
pub mod trace_geometry; // v0.2.1: Proper segment-level trace geometry engine
pub mod types;
pub mod unified_geometry; // v0.2.2: Single source of truth for copper contours

// Re-export main types and functions for backward compatibility
pub use materials::SceneGraphError;
pub use scene_graph_impl::SceneGraph;
pub use types::{BoxParams, Color, Face, MaterialNode, MeshNode, Vertex};
pub use unified_geometry::{generate_copper_contours, LayerId, PhysicalLayer};
