//! Component placement with collision detection and rotation support.
//!
//! This module handles placing components in the voxel grid with:
//! - Arbitrary rotation angles (using fixed-point trigonometry)
//! - Collision detection (bounding box + voxel-level)
//! - Voxel filling for component footprints
//! - Integration with NetlistArena for connectivity
//! - Automatic floorplanning (coarse-grid placement)
//! - Physical anchor system (Task B3)

mod anchor;
mod collision;
mod component_definition;
mod edge;
mod error;
mod floorplanner;
mod geometry;
mod placer;
mod substrate;
mod types;

// Re-export public API
pub use anchor::{Anchor, EdgePosition};
pub use component_definition::{
    bake_component_definition, BakedComponent, PadShape, PinDefinition,
};
pub use edge::Edge;
pub use error::{CollisionDetailedError, PlacementError};
pub use floorplanner::{ComponentPlacementRequest, FloorplanResult, Floorplanner};
pub use placer::ComponentPlacer;
pub use types::{DiagnosticReporter, PlacementParams, SymbolTableTrait};
