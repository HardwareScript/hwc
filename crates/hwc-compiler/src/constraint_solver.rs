//! Constraint Solver (Sprint 3, Task 3.1)
//!
//! Resolves relative positions to absolute coordinates.
//! Handles syntax like: `at M1.right + 1mm` or `at M1.top + [0.5mm, 1mm, 0mm]`
//!
//! ## Error Handling (v0.1.6, Deferred Work #3)
//! - Detects circular dependencies (M2 at M3.right, M3 at M2.left)
//! - Validates anchor existence before resolution
//! - Provides helpful error messages with suggestions

use compact_str::CompactString;
use hwc_engine::geometry::Point3D;
use hwc_parser::ast::{Coordinate, Expression, Measurement, RelativeOffset, RelativePosition};
use hwc_parser::{Unit, Value};

use crate::bounding_box_tracker::BoundingBoxTracker;
use crate::ir::placement::intent::PlacementIntent;
use crate::symbol_table::SymbolTable;

/// Constraint solver for relative positioning
///
/// v0.1.6 UNIVERSAL CONTEXT: The EvaluationContext (constants like PI, e, units)
/// is created ONCE at the space level and reused for all coordinate calculations.
/// This eliminates the "Initialization Storm" where we were rebuilding the constant
/// dictionary 24+ times for just 8 components.
pub struct ConstraintSolver<'a> {
    bbox_tracker: &'a BoundingBoxTracker,
    /// Tracks which entities are currently being resolved (for cycle detection)
    resolution_stack: std::cell::RefCell<Vec<CompactString>>,
    /// UNIVERSAL CONTEXT: Pre-built evaluation context with all constants (by reference)
    /// This is the "Physical Laws" dictionary - built once, used everywhere
    eval_context: &'a hwc_parser::EvaluationContext,
}

impl<'a> ConstraintSolver<'a> {
    /// Create a new constraint solver with a pre-built evaluation context.
    ///
    /// UNIVERSAL CONTEXT: The eval_context is built ONCE by the caller (program_to_space)
    /// and contains all constants from the symbol table. This eliminates the need to
    /// rebuild it for every coordinate calculation.
    ///
    /// Performance Impact:
    /// - Before: O(N × constants) where N = number of coordinate calculations
    /// - After: O(1 × constants) + O(N × HashMap_lookup)
    pub fn new(
        bbox_tracker: &'a BoundingBoxTracker,
        eval_context: &'a hwc_parser::EvaluationContext,
    ) -> Self {
        Self {
            bbox_tracker,
            resolution_stack: std::cell::RefCell::new(Vec::new()),
            eval_context,
        }
    }

    /// Build a universal evaluation context from the symbol table.
    ///
    /// **CRITICAL ARCHITECTURE NOTE:**
    /// This context stores strongly-typed `Value` enums to preserve unit information:
    /// 1. **Dimensionless constants** (PI, e, etc.) - stored as `Value::Number`
    /// 2. **PDK constants** (pdk.edge_clearance, etc.) - added later as `Value::Measurement`
    ///
    /// This method ONLY handles (1). PDK constants (2) are added by build_eval_context()
    /// in space_setup.rs after profile resolution.
    ///
    /// This should be called ONCE at the space level, then the context is passed
    /// to the ConstraintSolver constructor.
    ///
    /// PHYSICAL CORRECTNESS: The value of PI, e, or dimensionless constants does not
    /// change between the first transistor and the last one. Building this dictionary
    /// once reflects the stability of physical laws.
    pub fn build_eval_context(symbol_table: &SymbolTable) -> hwc_parser::EvaluationContext {
        let mut eval_context = hwc_parser::EvaluationContext::default();
        // Only add dimensionless constants (PI, e, etc.) as Value::Number
        // PDK constants with units will be added later as Value::Measurement
        for (name, value) in symbol_table.get_all_constants() {
            eval_context.insert(name, Value::Number(value as i64));
        }
        eval_context
    }

    /// Resolve a coordinate to absolute form with explicit placement semantics
    ///
    /// Returns `PlacementIntent` instead of `Coordinate` to preserve whether
    /// the resolved point represents a corner or center placement.
    ///
    /// # Errors
    /// - Returns error if anchor doesn't exist
    /// - Returns error if circular dependency detected
    /// - Returns error if offset units are invalid
    pub fn resolve_position(&self, coord: &Coordinate) -> Result<PlacementIntent, String> {
        match coord {
            // Assembly tier: absolute coordinates -> Corner intent
            Coordinate::Positional { .. } | Coordinate::Declarative { .. } => {
                let point = self.declarative_to_point(coord)?;
                Ok(PlacementIntent::Corner(point))
            }
            // Middle tier: relative positioning -> preserve edge semantics
            Coordinate::Relative(rel_pos) => self.resolve_relative_position(rel_pos),
        }
    }

    /// Convert an absolute coordinate (Positional/Declarative) to a Point3D.
    fn declarative_to_point(&self, coord: &Coordinate) -> Result<Point3D, String> {
        let (x_expr, y_expr, z_expr) = match coord {
            Coordinate::Positional { x, y, z, .. } | Coordinate::Declarative { x, y, z, .. } => {
                (x, y, z)
            }
            Coordinate::Relative(_) => {
                return Err("Cannot convert relative coordinate directly".into())
            }
        };
        let x_nm = self.expression_to_nm(x_expr)?;
        let y_nm = self.expression_to_nm(y_expr)?;
        let z_nm = self.expression_to_nm(z_expr)?;
        Ok(Point3D::new(x_nm, y_nm, z_nm))
    }

    /// Resolve a relative position with circular dependency detection
    ///
    /// **v0.1.6**: Supports `last` keyword for space-global component reference
    /// - `last` resolves to the most recently placed component in the space
    /// - This allows chaining across loop boundaries (God-Tier feature!)
    fn resolve_relative_position(&self, rel_pos: &RelativePosition) -> Result<PlacementIntent, String> {
        let anchor_name = &rel_pos.anchor.name;

        // Handle 'last' keyword - resolve to the most recently placed component
        let resolved_anchor_name: CompactString = if anchor_name == "last" {
            // O(1) lookup of last registered component
            self.bbox_tracker
                .last_registered()
                .ok_or_else(|| {
                    "Cannot use 'last' keyword: no components have been placed yet.\n\
                     \n\
                     To fix this:\n\
                     1. Place at least one component with absolute positioning first\n\
                     2. Then use 'last.edge' to chain subsequent components"
                        .to_string()
                })?
                .clone()
        } else {
            anchor_name.clone()
        };

        // Check for circular dependencies
        {
            let stack = self.resolution_stack.borrow();
            if stack.contains(&resolved_anchor_name) {
                return Err(self
                    .format_circular_dependency_error(&stack, &resolved_anchor_name)
                    .to_string());
            }
        }

        // Check if anchor exists
        let anchor_bbox = self
            .bbox_tracker
            .get(&resolved_anchor_name)
            .ok_or_else(|| self.format_nonexistent_anchor_error(&resolved_anchor_name))?;

        // Add to resolution stack for cycle detection
        self.resolution_stack
            .borrow_mut()
            .push(resolved_anchor_name.clone());

        // BUG FIX: Handle compound edges (TopLeft, TopRight, BottomLeft, BottomRight) correctly
        // These need to return corner points, not edge midpoints
        let edge_point = match rel_pos.edge {
            hwc_parser::Edge::Center => {
                // Calculate the center point of the bounding box
                Point3D::new(
                    (anchor_bbox.min.x + anchor_bbox.max.x) / 2,
                    (anchor_bbox.min.y + anchor_bbox.max.y) / 2,
                    (anchor_bbox.min.z + anchor_bbox.max.z) / 2,
                )
            }
            hwc_parser::Edge::CenterX => {
                // center_x is the midpoint of left and right edges
                Point3D::new(
                    (anchor_bbox.min.x + anchor_bbox.max.x) / 2,
                    anchor_bbox.min.y,
                    anchor_bbox.min.z,
                )
            }
            hwc_parser::Edge::CenterY => {
                // center_y is the midpoint of top and bottom edges
                Point3D::new(
                    anchor_bbox.min.x,
                    (anchor_bbox.min.y + anchor_bbox.max.y) / 2,
                    anchor_bbox.min.z,
                )
            }
            hwc_parser::Edge::CenterZ => {
                // center_z is the midpoint of min_z and max_z edges
                Point3D::new(
                    anchor_bbox.min.x,
                    anchor_bbox.min.y,
                    (anchor_bbox.min.z + anchor_bbox.max.z) / 2,
                )
            }
            hwc_parser::Edge::TopLeft => {
                Point3D::new(anchor_bbox.min.x, anchor_bbox.max.y, anchor_bbox.min.z)
            }
            hwc_parser::Edge::TopRight => {
                Point3D::new(anchor_bbox.max.x, anchor_bbox.max.y, anchor_bbox.min.z)
            }
            hwc_parser::Edge::BottomLeft => {
                Point3D::new(anchor_bbox.min.x, anchor_bbox.min.y, anchor_bbox.min.z)
            }
            hwc_parser::Edge::BottomRight => {
                Point3D::new(anchor_bbox.max.x, anchor_bbox.min.y, anchor_bbox.min.z)
            }
            _ => {
                // For simple edges (Left, Right, Top, Bottom, Front, Back, MinZ, MaxZ),
                // use the engine's edge_point which returns the midpoint of that face
                let engine_edge = self.convert_edge(rel_pos.edge);
                anchor_bbox.edge_point(engine_edge)
            }
        };

        // GAP1 DEBUG: Log edge point calculation
        // eprintln!($3"[DEBUG GAP1] Anchor '{}' edge {:?} → edge_point=({:.3}, {:.3}, {:.3})",
        // resolved_anchor_name,
        // rel_pos.edge,
        // edge_point.x as f64 / 1_000_000.0,
        // edge_point.y as f64 / 1_000_000.0,
        // edge_point.z as f64 / 1_000_000.0,
        // );

        let final_point = self.apply_offset(edge_point, &rel_pos.offset, rel_pos.edge)?;

        // GAP1 DEBUG: Log final resolved point
        // eprintln!($3"[DEBUG GAP1] After offset → final=({:.3}, {:.3}, {:.3})",
        // final_point.x as f64 / 1_000_000.0,
        // final_point.y as f64 / 1_000_000.0,
        // final_point.z as f64 / 1_000_000.0,
        // );

        // Remove from resolution stack
        self.resolution_stack.borrow_mut().pop();

        // NATIVE FIX: Preserve edge semantics in placement intent
        match rel_pos.edge {
            hwc_parser::Edge::Center | hwc_parser::Edge::CenterX | hwc_parser::Edge::CenterY | hwc_parser::Edge::CenterZ => Ok(PlacementIntent::Center(final_point)),
            _ => Ok(PlacementIntent::Corner(final_point)),
        }
    }

    /// Format a helpful error message for circular dependencies
    fn format_circular_dependency_error(
        &self,
        stack: &[CompactString],
        current: &str,
    ) -> CompactString {
        let mut chain = stack.to_vec();
        chain.push(current.into());

        let cycle_display = chain.join(" → ");

        format!(
            "Circular dependency detected in relative positioning:\n\
             \n\
             Dependency chain: {}\n\
             \n\
             Component '{}' is trying to position itself relative to '{}',\n\
             but '{}' (directly or indirectly) depends on '{}'.\n\
             \n\
             To fix this:\n\
             1. Use absolute positioning for at least one component\n\
             2. Break the circular chain by repositioning one component\n\
             3. Ensure dependency flow is unidirectional (e.g., left-to-right)",
            cycle_display,
            stack.last().unwrap_or(&"<unknown>".into()),
            current,
            current,
            stack.last().unwrap_or(&"<unknown>".into())
        )
        .into()
    }

    /// Format a helpful error message for nonexistent anchors
    fn format_nonexistent_anchor_error(&self, anchor_name: &str) -> CompactString {
        let available = self.bbox_tracker.all_names();

        if available.is_empty() {
            return format!(
                "Cannot resolve relative position: anchor '{}' not found.\n\
                 \n\
                 No components have been placed yet.\n\
                 \n\
                 To fix this:\n\
                 1. Ensure the anchor component is defined before this component\n\
                 2. Use absolute positioning for the first component\n\
                 3. Check for typos in the anchor name",
                anchor_name
            )
            .into();
        }

        // Find similar names (simple edit distance)
        let suggestions = self.find_similar_names(anchor_name, &available);

        let mut error = format!(
            "Cannot resolve relative position: anchor '{}' not found.\n\
             \n\
             Available anchors: {}",
            anchor_name,
            available
                .iter()
                .map(|s| format!("'{}'", s))
                .collect::<Vec<_>>()
                .join(", ")
        );

        if !suggestions.is_empty() {
            error.push_str(&format!(
                "\n\
                 \n\
                 Did you mean: {}?",
                suggestions
                    .iter()
                    .map(|s| format!("'{}'", s))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }

        error.push_str(
            "\n\
             \n\
             To fix this:\n\
             1. Check the spelling of the anchor name\n\
             2. Ensure the anchor component is defined before this component\n\
             3. Components must be placed in dependency order",
        );

        error.into()
    }

    /// Find similar names using simple string similarity
    fn find_similar_names(
        &self,
        target: &str,
        candidates: &[&CompactString],
    ) -> Vec<CompactString> {
        let mut similar = Vec::new();
        let target_lower = target.to_lowercase();

        for candidate in candidates {
            let candidate_lower = candidate.to_lowercase();

            // Check for substring match
            if candidate_lower.contains(target_lower.as_str())
                || target_lower.contains(candidate_lower.as_str())
            {
                similar.push((*candidate).clone());
                continue;
            }

            // Check for similar length and character overlap
            if (candidate.len() as i32 - target.len() as i32).abs() <= 2 {
                let common_chars = target_lower
                    .chars()
                    .filter(|c| candidate_lower.contains(*c))
                    .count();

                if common_chars >= target.len().min(candidate.len()) / 2 {
                    similar.push((*candidate).clone());
                }
            }
        }

        similar.into_iter().collect()
    }

    fn convert_edge(&self, edge: hwc_parser::Edge) -> hwc_engine::geometry::Edge {
        match edge {
            hwc_parser::Edge::Left | hwc_parser::Edge::TopLeft | hwc_parser::Edge::BottomLeft => {
                hwc_engine::geometry::Edge::Left
            }
            hwc_parser::Edge::Right | hwc_parser::Edge::TopRight | hwc_parser::Edge::BottomRight => {
                hwc_engine::geometry::Edge::Right
            }
            hwc_parser::Edge::Top => hwc_engine::geometry::Edge::Top,
            hwc_parser::Edge::Bottom => hwc_engine::geometry::Edge::Bottom,
            hwc_parser::Edge::Front => hwc_engine::geometry::Edge::Front,
            hwc_parser::Edge::Back => hwc_engine::geometry::Edge::Back,
            hwc_parser::Edge::MinZ => hwc_engine::geometry::Edge::MinZ,
            hwc_parser::Edge::MaxZ => hwc_engine::geometry::Edge::MaxZ,
            hwc_parser::Edge::Center => hwc_engine::geometry::Edge::Left,
            hwc_parser::Edge::CenterX => hwc_engine::geometry::Edge::Left,
            hwc_parser::Edge::CenterY => hwc_engine::geometry::Edge::Left,
            hwc_parser::Edge::CenterZ => hwc_engine::geometry::Edge::Left,
        }
    }

    fn apply_offset(
        &self,
        base_point: Point3D,
        offset: &RelativeOffset,
        edge: hwc_parser::Edge,  // Parser edge, not engine edge
    ) -> Result<Point3D, String> {
        eprintln!("[APPLY_OFFSET] base_point=({}, {}, {}), edge={:?}", 
            base_point.x, base_point.y, base_point.z, edge);
        
        match offset {
            RelativeOffset::Single(measurement) => {
                // GAP1 FIX: Coordinate Inheritance for Single-Direction Offsets
                //
                // When using `last.right + 2mm`, the user is only specifying movement
                // in ONE direction (X for right/left, Y for top/bottom, Z for front/back).
                //
                // Physical Reality: "Place this next to the last one" means:
                // - Move in the specified direction (e.g., +X for right)
                // - INHERIT the other coordinates from the anchor (Y and Z stays the same)
                //
                // Before this fix:
                // - Adder[0] at y: 5mm → Adder[1] at y: 40mm (WRONG - teleported!)
                //
                // After this fix:
                // - Adder[0] at y: 5mm → Adder[1] at y: 5mm (CORRECT - stayed in line)
                let offset_nm = self.measurement_to_nm(measurement)?;
                
                // Special case: Center edge with zero offset is valid (just use center point)
                if matches!(edge, hwc_parser::Edge::Center | hwc_parser::Edge::CenterX | hwc_parser::Edge::CenterY | hwc_parser::Edge::CenterZ) && offset_nm == 0 {
                    return Ok(base_point);
                }
                
                // But non-zero single offsets from center are invalid (which direction to move?)
                if matches!(edge, hwc_parser::Edge::Center | hwc_parser::Edge::CenterX | hwc_parser::Edge::CenterY | hwc_parser::Edge::CenterZ) {
                    return Err(
                        "Cannot use single-value offset from center anchor. \
                         Use vector offset like: space.center + [x, y, z]".to_string()
                    );
                }
                
                let engine_edge = self.convert_edge(edge);
                let (dx, dy, dz) = engine_edge.direction_vector();

                // Only apply offset in the edge's direction
                // The direction_vector returns (1,0,0) for right, (0,1,0) for top, etc.
                // Multiplying by the direction vector ensures we only move in that axis
                Ok(Point3D::new(
                    base_point.x + dx * offset_nm,
                    base_point.y + dy * offset_nm,
                    base_point.z + dz * offset_nm,
                ))
            }
            RelativeOffset::Vector { x, y, z } => {
                // Vector offsets are explicit - user specified all three dimensions
                // No inheritance needed here
                let dx_nm = self.expression_to_nm(x)?;
                let dy_nm = self.expression_to_nm(y)?;
                let dz_nm = self.expression_to_nm(z)?;
                
                eprintln!("[APPLY_OFFSET] Vector offset: dx={}, dy={}, dz={}", dx_nm, dy_nm, dz_nm);
                eprintln!("[APPLY_OFFSET] Result: ({}, {}, {})", 
                    base_point.x + dx_nm, base_point.y + dy_nm, base_point.z + dz_nm);
                
                Ok(Point3D::new(
                    base_point.x + dx_nm,
                    base_point.y + dy_nm,
                    base_point.z + dz_nm,
                ))
            }
        }
    }

    fn measurement_to_nm(&self, measurement: &Measurement) -> Result<i64, String> {
        let value = measurement.value;
        let nm = match &measurement.unit {
            Unit::Millimeter => (value * 1_000_000.0) as i64,
            Unit::Centimeter => (value * 10_000_000.0) as i64,
            Unit::Micrometer => (value * 1_000.0) as i64,
            Unit::Nanometer => value as i64,
            _ => {
                return Err(format!(
                "Invalid unit for position offset: {:?}. Expected distance unit (mm, cm, µm, nm)",
                measurement.unit
            ))
            }
        };
        Ok(nm)
    }

    /// Convert an expression to nanometers using the pre-built evaluation context.
    ///
    /// UNIVERSAL CONTEXT FIX: This function now uses self.eval_context instead of
    /// rebuilding it every time. This eliminates the "Initialization Storm" bottleneck.
    ///
    /// Performance:
    /// - Before: Rebuild 51-entry HashMap on every call (1,224 allocations for 8 components)
    /// - After: Single HashMap lookup (O(1) per call)
    fn expression_to_nm(&self, expr: &Expression) -> Result<i64, String> {
        // NATIVE FIX: Use the pre-built context instead of rebuilding it
        eprintln!("[EXPR_TO_NM DEBUG] Evaluating expression: {:?}", expr);
        eprintln!("[EXPR_TO_NM DEBUG] eval_context has {} entries", self.eval_context.len());
        
        let value = expr.evaluate(self.eval_context)?;
        
        eprintln!("[EXPR_TO_NM DEBUG] Expression evaluated to: {:?}", value);

        match value {
            Value::Number(n) => Ok(n),
            Value::Float(f) => Ok(f as i64),
            Value::Measurement { value, unit } => {
                let nm = match unit {
                    Unit::Millimeter => (value * 1_000_000.0) as i64,
                    Unit::Centimeter => (value * 10_000_000.0) as i64,
                    Unit::Micrometer => (value * 1_000.0) as i64,
                    Unit::Nanometer => value as i64,
                    _ => {
                        return Err(format!(
                        "Invalid unit for position: {:?}. Expected distance unit (mm, cm, µm, nm)",
                        unit
                    ))
                    }
                };
                Ok(nm)
            }
            Value::Percentage(p) => Err(format!("Position offset cannot be a percentage: {}%", p)),
        }
    }
}
