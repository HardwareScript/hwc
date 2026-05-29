//! Component definition parsing (metadata, pins, layout, electrical, render)
//!
//! This module is organized into logical submodules:
//! - `main`: Main component definition parsing and error recovery
//! - `metadata`: Component metadata parsing
//! - `pins`: Pin declaration parsing
//! - `layout`: Layout block parsing (shape, pin_positions, pad_shapes, internal_pours)
//! - `electrical`: Electrical properties parsing
//! - `render`: Render block parsing
//! - `internal_pour`: Internal pour parsing for component-relative geometry

mod electrical;
mod internal_pour;
mod layout;
mod main;
mod metadata;
mod pins;
mod render;
