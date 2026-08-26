//! Module bounding box calculations.
//!
//! This module provides functions for calculating bounding boxes for module instances
//! from their declarations, used in hierarchical parallel routing.

use crate::geometry::{BoundingBox, Point3D};

/// Calculate the bounding box for a module instance from its declaration.
pub fn calculate_module_bounding_box(
    _module: &hwc_parser::ModuleDecl,
    manufacturing_grid_nm: i64,
    _arena: &hwc_parser::ast::arena::AstArena,
) -> BoundingBox {
    let default_size = 10_000_000; // 10mm
    let min = Point3D::new(0, 0, 0);
    let max = Point3D::new(default_size, default_size, manufacturing_grid_nm);

    BoundingBox::new(min, max)
}
