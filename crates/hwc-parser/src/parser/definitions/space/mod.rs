//! Space definition parsing (dimensions, grid, origin, components, routes)
//!
//! This module is organized into logical submodules:
//! - `core`: Main space definition parsing
//! - `dimensions`: Dimensions, grid, and origin parsing
//! - `placements`: Component, pour, polygon, and contact placements
//! - `layout`: Module layout blocks and statements
//! - `device`: Device binding and net declarations
//! - `loops`: For loop parsing for parametric unrolling
//! - `region`: Region floorplanning (v0.2.0)

mod core;
mod device;
mod dimensions;
mod layout;
mod loops;
mod placements;
mod region;
