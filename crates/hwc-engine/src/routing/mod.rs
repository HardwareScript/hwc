//! HardwareScript v0.3.0 DOPHR Routing Subsystem
//! Data-Oriented Progressive Hierarchical Routing Engine

pub mod detailed;
pub mod dophr;
pub mod global;
pub mod track_assign;

pub use detailed::{
    ColorScheduler, ColorSet, DetailedSegment, DetailedTerminal, GuidedDetailedRouter,
};
pub use dophr::{DophrConfig, DophrEngine, DophrRoutingResult};
pub use global::{GCellVolume3D, GlobalPath, GlobalTerminal, PathFinderGlobalRouter, RoutingGuide, VolumetricTensor3D};
pub use track_assign::{NetInterval, Panel, PanelTrackAssigner, TrackAnchor};
