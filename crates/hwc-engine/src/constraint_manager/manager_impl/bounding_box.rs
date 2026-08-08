//! Module bounding box calculations.
//!
//! This module provides functions for calculating bounding boxes for module instances
//! from their layout blocks, used in hierarchical parallel routing.

use crate::geometry::{BoundingBox, Point3D};

/// Calculate the bounding box for a module instance from its layout block.
///
/// This is Phase 1 of the Hierarchical Parallel Routing architecture (GAP3).
/// The bounding box defines the "Glass Box" domain for parallel routing.
///
/// # Arguments
/// * `layout` - The layout block containing component placements
/// * `resolution_nm` - The snapping resolution in nanometers
///
/// # Returns
/// A `BoundingBox` that encompasses all components in the layout
pub fn calculate_module_bounding_box(
    layout: &hwc_parser::ModuleLayoutBlock,
    resolution_nm: i64,
) -> BoundingBox {
    // Extract all placements from layout statements (flattening for loops and if statements)
    let placements = extract_placements_from_layout(&layout.statements);

    // If no placements, return a minimal bounding box at origin
    if placements.is_empty() {
        return BoundingBox::new(
            Point3D::new(0, 0, 0),
            Point3D::new(resolution_nm, resolution_nm, resolution_nm),
        );
    }

    // Initialize min/max with first placement position
    let first_placement = &placements[0];

    // For bounding box calculation, we need to convert coordinates to nanometers
    let (first_x_val, first_y_val, first_z_val) =
        first_placement.position.evaluate_const().unwrap_or((
            hwc_parser::Value::Number(0),
            hwc_parser::Value::Number(0),
            hwc_parser::Value::Number(0),
        ));

    // Convert to nanometers
    let first_x = first_x_val.to_nanometers().unwrap_or(0);
    let first_y = first_y_val.to_nanometers().unwrap_or(0);
    let first_z = first_z_val.as_integer().unwrap_or(0) * resolution_nm;

    let mut min_x = first_x;
    let mut max_x = first_x;
    let mut min_y = first_y;
    let mut max_y = first_y;
    let mut min_z = first_z;
    let mut max_z = first_z;

    // Find min/max coordinates across all placements
    for placement in &placements {
        let (x_val, y_val, z_val) = placement.position.evaluate_const().unwrap_or((
            hwc_parser::Value::Number(0),
            hwc_parser::Value::Number(0),
            hwc_parser::Value::Number(0),
        ));

        let x = x_val.to_nanometers().unwrap_or(0);
        let y = y_val.to_nanometers().unwrap_or(0);
        let z = z_val.as_integer().unwrap_or(0) * resolution_nm;

        min_x = min_x.min(x);
        max_x = max_x.max(x);
        min_y = min_y.min(y);
        max_y = max_y.max(y);
        min_z = min_z.min(z);
        max_z = max_z.max(z);
    }

    /// Helper function to extract all placements from layout statements
    /// This recursively flattens for loops and if statements to get all placements
    fn extract_placements_from_layout<'a>(
        statements: &'a [hwc_parser::LayoutStatement],
    ) -> Vec<&'a hwc_parser::ModuleInternalPlacement> {
        use hwc_parser::LayoutStatement;

        let mut placements = Vec::new();

        for statement in statements {
            match statement {
                LayoutStatement::Placement(p) => {
                    placements.push(*p);
                }
                LayoutStatement::For { body, .. } => {
                    // Recursively extract placements from for loop body
                    // Note: This doesn't evaluate the loop, just collects all placements
                    placements.extend(extract_placements_from_layout(body));
                }
                LayoutStatement::If {
                    then_body,
                    else_body,
                    ..
                } => {
                    // Collect placements from both branches
                    placements.extend(extract_placements_from_layout(then_body));
                    if let Some(else_statements) = else_body {
                        placements.extend(extract_placements_from_layout(else_statements));
                    }
                }
            }
        }

        placements
    }

    // All coordinates are already in nanometers
    let min = Point3D::new(min_x, min_y, min_z);
    let max = Point3D::new(max_x, max_y, max_z);

    // Add margin for component size (assume components are at least 1mm)
    let margin_nm = 1_000_000; // 1mm
    BoundingBox::new(min, max).expand(margin_nm)
}
