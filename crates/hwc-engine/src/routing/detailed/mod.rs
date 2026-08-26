//! DOPHR Stage 3: Guided Detailed Routing & Lock-free 4-Coloring

pub mod color_scheduler;
pub mod guided_router;

pub use color_scheduler::{ColorScheduler, ColorSet, SpatialCell};
pub use guided_router::{DetailedSegment, DetailedTerminal, GuidedDetailedRouter, RoutingError};
