//! Region placement and anchor registration (v0.2.0)
//!
//! Regions are logical floorplanning zones that can be used as anchors
//! for component placement. This module resolves region positions and
//! registers them in the bounding box tracker.

use crate::bounding_box_tracker::BoundingBoxTracker;
use crate::constraint_solver::ConstraintSolver;
use crate::ir::errors::IrError;
use compact_str::CompactString;
use hwc_engine::geometry::{BoundingBox, Point3D};
use hwc_parser::{OriginPoint, RegionDefinition};

/// Process a region declaration and register it as an anchor
pub fn register_region(
    region: &RegionDefinition,
    bbox_tracker: &mut BoundingBoxTracker,
    symbol_table: &crate::SymbolTable,
    eval_context: &hwc_parser::EvaluationContext,
    origin: OriginPoint,
    space_dimensions: &hwc_engine::Dimensions,
    stackup_manager: &crate::ir::stackup_manager::StackupManager,
    profile: Option<&hwc_parser::ProfileDefinition>,
) -> Result<(), IrError> {
    let region_name: CompactString = region.name.to_string().into();

    // Resolve the region's position
    let position = resolve_region_position(
        region,
        bbox_tracker,
        symbol_table,
        eval_context,
        origin,
        space_dimensions,
        stackup_manager,
        profile,
    )?;

    // Determine region size
    let (width, height, depth) = if let Some(boundary) = &region.boundary {
        // Explicit boundary provided
        resolve_region_boundary(boundary, symbol_table, eval_context)?
    } else {
        // No explicit boundary provided
        // 
        // ARCHITECTURAL DECISION (v0.2.0 - Single-Pass Paradigm):
        // We require explicit boundaries for regions whose geometric properties
        // (.center, .right, .top, etc.) are referenced during placement.
        //
        // This enforces:
        // 1. Zero-Magic Compliance: No implicit/guessed dimensions
        // 2. Single-Pass Determinism: All values known during first unrolling
        // 3. Declarative Clarity: Designer explicitly states dimensions via `let` variables
        //
        // Example of correct usage:
        //   let pad_w = 150um
        //   let pad_h = 150um
        //   region MyRegion:
        //       at: space.center
        //       boundary: [pad_w, pad_h]  # ← Explicit, single-pass resolvable
        //
        return Err(IrError::CoordinateResolutionFailed {
            coordinate_str: format!("region {}", region_name),
            reason: "Region boundary (width and height) must be explicitly specified.\n\n\
                    To fix: Add 'boundary: [width, height]' to the region declaration.\n\
                    Use local 'let' variables for dimensions shared between regions and components.\n\n\
                    Example:\n\
                      let pad_w = 150um\n\
                      let pad_h = 150um\n\
                      region EdgeRegionA:\n\
                          at: space.bottom_left + [50um, 50um]\n\
                          boundary: [pad_w, pad_h]".to_string(),
        });
    };

    // Create bounding box for the region
    let bbox = BoundingBox {
        min: position,
        max: Point3D {
            x: position.x + width,
            y: position.y + height,
            z: position.z + depth,
        },
    };

    eprintln!(
        "[REGION] '{}' bbox: ({}, {}, {}) to ({}, {}, {})",
        region_name,
        bbox.min.x, bbox.min.y, bbox.min.z,
        bbox.max.x, bbox.max.y, bbox.max.z
    );

    // Register the region as an anchor
    bbox_tracker.register(region_name, bbox, position);

    Ok(())
}

/// Resolve a region's position from its anchor and constraints
fn resolve_region_position(
    region: &RegionDefinition,
    bbox_tracker: &BoundingBoxTracker,
    symbol_table: &crate::SymbolTable,
    eval_context: &hwc_parser::EvaluationContext,
    origin: OriginPoint,
    space_dimensions: &hwc_engine::Dimensions,
    stackup_manager: &crate::ir::stackup_manager::StackupManager,
    profile: Option<&hwc_parser::ProfileDefinition>,
) -> Result<Point3D, IrError> {
    // First, check for relational constraints (right_of, align, etc.)
    if !region.constraints.is_empty() {
        // Convert RegionConstraints to a coordinate using similar logic to component constraints
        return resolve_region_from_constraints(
            region,
            bbox_tracker,
            symbol_table,
            eval_context,
            origin,
            space_dimensions,
            stackup_manager,
            profile,
        );
    }

    // Otherwise, use the anchor if provided
    if let Some(anchor) = &region.anchor {
        let intent = resolve_region_anchor(anchor, bbox_tracker, symbol_table, eval_context)?;
        
        // PlacementIntent already contains the resolved point - no need for coordinate_to_point
        return Ok(intent.point());
    }

    // No positioning info - default to origin (0, 0, 0)
    Ok(Point3D { x: 0, y: 0, z: 0 })
}

/// Resolve region position from relational constraints (right_of, align, etc.)
fn resolve_region_from_constraints(
    region: &RegionDefinition,
    bbox_tracker: &BoundingBoxTracker,
    symbol_table: &crate::SymbolTable,
    eval_context: &hwc_parser::EvaluationContext,
    origin: OriginPoint,
    space_dimensions: &hwc_engine::Dimensions,
    _stackup_manager: &crate::ir::stackup_manager::StackupManager,
    _profile: Option<&hwc_parser::ProfileDefinition>,
) -> Result<Point3D, IrError> {
    let mut x_nm: Option<i64> = None;
    let mut y_nm: Option<i64> = None;
    let z_nm: i64 = 0; // Regions are 2D, Z defaults to 0
    
    // PRE-RESOLVE REGION DIMENSIONS for all relational placement calculations
    // This is necessary because:
    // 1. Center alignment needs dimensions to calculate center-to-center offset
    // 2. Directional constraints (right_of, left_of, above, below) need dimensions for:
    //    - Y-axis centering when using X-directionals
    //    - X-axis centering when using Y-directionals
    // 3. Edge-to-edge spacing semantics require knowing the region's size
    //
    // If boundary is not specified, we MUST fail for relational placement
    let needs_dimensions = !region.constraints.is_empty();
    
    let (region_width, region_height) = if needs_dimensions {
        if let Some(boundary) = &region.boundary {
            let (w, h, _d) = resolve_region_boundary(boundary, symbol_table, eval_context)?;
            (w, h)
        } else {
            return Err(IrError::CoordinateResolutionFailed {
                coordinate_str: format!("region {}", region.name),
                reason: "Relational placement constraints require explicit region boundary (width and height). Please specify a boundary for the region.".to_string(),
            });
        }
    } else {
        // No constraints, dimensions not needed
        (0, 0)
    };
    
    // Derive origin-direction multipliers. These drive the unified formula lookup.
    let (x_multiplier, y_multiplier) = match origin.xy {
        hwc_parser::OriginXY::BL => (1i64, 1i64),
        hwc_parser::OriginXY::TL => (1, -1),
        hwc_parser::OriginXY::BR => (-1, 1),
        hwc_parser::OriginXY::TR => (-1, -1),
    };

    use crate::ir::relational_resolver::{
        RelationalPlacementFormula, SpatialRelation, target_bbox_to_user_ranges,
    };

    for constraint in &region.constraints {
        // Get target region's bounding box
        let target_name = CompactString::from(constraint.target.as_str());
        let target_bbox = bbox_tracker.get(&target_name)
            .ok_or_else(|| IrError::CoordinateResolutionFailed {
                coordinate_str: format!("region {} constraint", region.name),
                reason: format!("Target region '{}' not found", constraint.target),
            })?;

        // Convert the stored physical bbox into user-space ranges.
        // This is the single source of truth for directional math and prevents
        // the double-coordinate-conversion bug (computing in physical space then
        // calling coordinate_to_point which flips again for non-BL origins).
        let (tx_min, tx_max, ty_min, ty_max) =
            target_bbox_to_user_ranges(target_bbox, space_dimensions, origin.xy);
        
        // Evaluate spacing expression if present
        let spacing_nm = if let Some(spacing_expr) = &constraint.spacing {
            // Evaluate the expression with the evaluation context (supports pdk.* variables)
            match spacing_expr.evaluate(eval_context) {
                Ok(hwc_parser::Value::Measurement { value, unit }) => {
                    // Convert measurement to nm based on unit using a lookup table
                    let multiplier: f64 = match unit {
                        hwc_parser::Unit::Nanometer  => 1.0,
                        hwc_parser::Unit::Micrometer => 1_000.0,
                        hwc_parser::Unit::Millimeter => 1_000_000.0,
                        hwc_parser::Unit::Centimeter => 10_000_000.0,
                        _ => {
                            return Err(IrError::CoordinateResolutionFailed {
                                coordinate_str: format!("{:?}", spacing_expr),
                                reason: format!("Invalid unit for spacing: {:?}", unit),
                            });
                        }
                    };
                    (value * multiplier) as i64
                }
                Ok(hwc_parser::Value::Number(n)) => n, // Assume already in nm if unitless
                Ok(other) => {
                    return Err(IrError::CoordinateResolutionFailed {
                        coordinate_str: format!("{:?}", spacing_expr),
                        reason: format!("Spacing expression evaluated to {:?}, expected measurement", other),
                    });
                }
                Err(e) => {
                    return Err(IrError::CoordinateResolutionFailed {
                        coordinate_str: format!("{:?}", spacing_expr),
                        reason: e,
                    });
                }
            }
        } else {
            0
        };
        
        // Apply constraint based on type using the unified formula lookup.
        // All math is performed in USER SPACE (tx_min/tx_max/ty_min/ty_max).
        use hwc_parser::RegionConstraintType;
        match constraint.constraint_type {
            RegionConstraintType::RightOf => {
                let formula = RelationalPlacementFormula::get(
                    SpatialRelation::RightOf, x_multiplier, y_multiplier,
                );
                x_nm = Some(formula.resolve(tx_min, tx_max, spacing_nm, region_width));
                if y_nm.is_none() {
                    let formula_cy = RelationalPlacementFormula::get(
                        SpatialRelation::AlignY, x_multiplier, y_multiplier,
                    );
                    y_nm = Some(formula_cy.resolve(ty_min, ty_max, 0, region_height));
                }
            }
            RegionConstraintType::LeftOf => {
                let formula = RelationalPlacementFormula::get(
                    SpatialRelation::LeftOf, x_multiplier, y_multiplier,
                );
                x_nm = Some(formula.resolve(tx_min, tx_max, spacing_nm, region_width));
                if y_nm.is_none() {
                    let formula_cy = RelationalPlacementFormula::get(
                        SpatialRelation::AlignY, x_multiplier, y_multiplier,
                    );
                    y_nm = Some(formula_cy.resolve(ty_min, ty_max, 0, region_height));
                }
            }
            RegionConstraintType::Above => {
                // "above" means physically higher — start at target's high edge + spacing
                let formula = RelationalPlacementFormula::get(
                    SpatialRelation::Above, x_multiplier, y_multiplier,
                );
                y_nm = Some(formula.resolve(ty_min, ty_max, spacing_nm, region_height));
                if x_nm.is_none() {
                    let formula_cx = RelationalPlacementFormula::get(
                        SpatialRelation::AlignX, x_multiplier, y_multiplier,
                    );
                    x_nm = Some(formula_cx.resolve(tx_min, tx_max, 0, region_width));
                }
            }
            RegionConstraintType::Below => {
                let formula = RelationalPlacementFormula::get(
                    SpatialRelation::Below, x_multiplier, y_multiplier,
                );
                y_nm = Some(formula.resolve(ty_min, ty_max, spacing_nm, region_height));
                if x_nm.is_none() {
                    let formula_cx = RelationalPlacementFormula::get(
                        SpatialRelation::AlignX, x_multiplier, y_multiplier,
                    );
                    x_nm = Some(formula_cx.resolve(tx_min, tx_max, 0, region_width));
                }
            }
            RegionConstraintType::AlignX => {
                let formula = RelationalPlacementFormula::get(
                    SpatialRelation::AlignX, x_multiplier, y_multiplier,
                );
                x_nm = Some(formula.resolve(tx_min, tx_max, 0, region_width));
            }
            RegionConstraintType::AlignY => {
                let formula = RelationalPlacementFormula::get(
                    SpatialRelation::AlignY, x_multiplier, y_multiplier,
                );
                y_nm = Some(formula.resolve(ty_min, ty_max, 0, region_height));
            }
            RegionConstraintType::AlignLeft => {
                let formula = RelationalPlacementFormula::get(
                    SpatialRelation::AlignLeft, x_multiplier, y_multiplier,
                );
                x_nm = Some(formula.resolve(tx_min, tx_max, 0, region_width));
            }
            RegionConstraintType::AlignRight => {
                let formula = RelationalPlacementFormula::get(
                    SpatialRelation::AlignRight, x_multiplier, y_multiplier,
                );
                x_nm = Some(formula.resolve(tx_min, tx_max, 0, region_width));
            }
            RegionConstraintType::AlignTop => {
                let formula = RelationalPlacementFormula::get(
                    SpatialRelation::AlignTop, x_multiplier, y_multiplier,
                );
                y_nm = Some(formula.resolve(ty_min, ty_max, 0, region_height));
            }
            RegionConstraintType::AlignBottom => {
                let formula = RelationalPlacementFormula::get(
                    SpatialRelation::AlignBottom, x_multiplier, y_multiplier,
                );
                y_nm = Some(formula.resolve(ty_min, ty_max, 0, region_height));
            }
        }
    }
    
    // The resolved values are in USER SPACE (matching the declared origin).
    // Emit them directly as a Point3D — NO secondary coordinate_to_point call,
    // which would incorrectly apply the origin flip a second time.
    // The caller (register_region) constructs the BoundingBox directly from this point.
    Ok(Point3D {
        x: x_nm.unwrap_or(0),
        y: y_nm.unwrap_or(0),
        z: z_nm,
    })
}

/// Resolve a region anchor to a concrete placement intent
fn resolve_region_anchor(
    anchor: &hwc_parser::RegionAnchor,
    bbox_tracker: &BoundingBoxTracker,
    _symbol_table: &crate::SymbolTable,
    eval_context: &hwc_parser::EvaluationContext,
) -> Result<crate::ir::placement::intent::PlacementIntent, IrError> {
    let solver = ConstraintSolver::new(bbox_tracker, eval_context);
    
    match anchor {
        hwc_parser::RegionAnchor::Absolute(coord) => {
            solver.resolve_position(coord).map_err(|e| IrError::CoordinateResolutionFailed {
                coordinate_str: format!("{:?}", coord),
                reason: e.to_string(),
            })
        }
        hwc_parser::RegionAnchor::Expression(expr) => {
            // Expression-based positioning needs to be converted to a coordinate
            // Check if it's an anchor reference (like space.bottom_left)
            if let hwc_parser::Expression::AnchorReference { anchor: anchor_ref, edge, .. } = expr {
                // This is something like "space.bottom_left" - create a relative coordinate
                let coord = hwc_parser::Coordinate::Relative(hwc_parser::RelativePosition {
                    anchor: anchor_ref.clone(),
                    edge: *edge,
                    offset: hwc_parser::RelativeOffset::Vector {
                        x: hwc_parser::Expression::Literal { value: 0, span: expr.span() },
                        y: hwc_parser::Expression::Literal { value: 0, span: expr.span() },
                        z: hwc_parser::Expression::Literal { value: 0, span: expr.span() },
                    },
                    span: expr.span(),
                });
                return solver.resolve_position(&coord).map_err(|e| IrError::CoordinateResolutionFailed {
                    coordinate_str: format!("{:?}", expr),
                    reason: e.to_string(),
                });
            }
            
            Err(IrError::CoordinateResolutionFailed {
                coordinate_str: format!("{:?}", expr),
                reason: "Region anchor expressions must be anchor references or coordinates".to_string(),
            })
        }
        hwc_parser::RegionAnchor::Offset { base, operator, offset } => {
            // Offset-based positioning: base_expression +/- offset_coordinate
            // Example: space.bottom_left + [pdk.edge_clearance, pdk.edge_clearance]
            // Example: space.top_right - [pdk.edge_clearance + 200um, pdk.edge_clearance + 200um]
            
            // The base should be an anchor reference expression
            if let hwc_parser::Expression::AnchorReference { anchor: anchor_ref, edge, .. } = base {
                // Extract offset expressions from the coordinate
                let (offset_x, offset_y, offset_z) = match offset {
                    hwc_parser::Coordinate::Positional { x, y, z, .. } => (x.clone(), y.clone(), z.clone()),
                    hwc_parser::Coordinate::Declarative { x, y, z, .. } => (x.clone(), y.clone(), z.clone()),
                    hwc_parser::Coordinate::Relative(_) => {
                        return Err(IrError::CoordinateResolutionFailed {
                            coordinate_str: format!("{:?}", offset),
                            reason: "Offset coordinate cannot be relative".to_string(),
                        });
                    }
                };
                
                eprintln!("[REGION OFFSET] Region '{}', operator={:?}", 
                    anchor_ref.name, operator);
                eprintln!("[REGION OFFSET]   offset_x: {:?}", offset_x);
                eprintln!("[REGION OFFSET]   offset_y: {:?}", offset_y);
                
                // CLEAN ARCHITECTURE FIX: Use unary negation instead of `0 - expr`
                // 
                // The old approach `0 - expr` creates a dimensionally invalid operation when
                // expr is a physical measurement (you can't subtract a measurement from a scalar).
                //
                // The correct approach is to use unary negation `-(expr)`, which preserves
                // the units and simply flips the sign of the value.
                //
                // For subtraction (space.top_right - [200um, 200um]):
                // - We negate the offset values: -(200um) = -200um
                // - Then apply_offset will add -200um to base_point, achieving subtraction
                //
                // For addition (space.bottom_left + [200um, 200um]):
                // - Keep offset values positive
                // - apply_offset adds them directly
                let (final_offset_x, final_offset_y, final_offset_z) = match operator {
                    hwc_parser::BinaryOperator::Subtract => {
                        // Negate each offset expression using unary negation
                        let negate_expr = |expr: hwc_parser::Expression| -> hwc_parser::Expression {
                            hwc_parser::Expression::Unary {
                                operator: hwc_parser::UnaryOperator::Negate,
                                operand: Box::new(expr.clone()),
                                span: expr.span(),
                            }
                        };
                        (negate_expr(offset_x), negate_expr(offset_y), negate_expr(offset_z))
                    }
                    _ => {
                        // Addition or other operators - use offsets as-is
                        (offset_x, offset_y, offset_z)
                    }
                };
                
                eprintln!("[REGION OFFSET]   final_offset_x: {:?}", final_offset_x);
                eprintln!("[REGION OFFSET]   final_offset_y: {:?}", final_offset_y);
                
                // Create a relative coordinate from the base anchor + offset
                let relative_coord = hwc_parser::Coordinate::Relative(hwc_parser::RelativePosition {
                    anchor: anchor_ref.clone(),
                    edge: *edge,
                    offset: hwc_parser::RelativeOffset::Vector {
                        x: final_offset_x,
                        y: final_offset_y,
                        z: final_offset_z,
                    },
                    span: offset.span(),
                });
                
                // Now resolve the relative coordinate through the constraint solver
                let result = solver.resolve_position(&relative_coord).map_err(|e| IrError::CoordinateResolutionFailed {
                    coordinate_str: format!("{:?} {:?} {:?}", base, operator, offset),
                    reason: e.to_string(),
                });
                
                return result;
            }
            
            Err(IrError::CoordinateResolutionFailed {
                coordinate_str: format!("{:?} + offset", base),
                reason: "Region anchor offset base must be an anchor reference (like space.bottom_left)".to_string(),
            })
        }
    }
}

/// Resolve explicit region boundary dimensions
fn resolve_region_boundary(
    boundary: &hwc_parser::RegionBoundary,
    _symbol_table: &crate::SymbolTable,
    eval_context: &hwc_parser::EvaluationContext,
) -> Result<(i64, i64, i64), IrError> {
    // Evaluate width expression
    let width_nm = match boundary.width.evaluate(eval_context) {
        Ok(hwc_parser::Value::Measurement { value, unit }) => {
            match unit {
                hwc_parser::Unit::Nanometer => value as i64,
                hwc_parser::Unit::Micrometer => (value * 1_000.0) as i64,
                hwc_parser::Unit::Millimeter => (value * 1_000_000.0) as i64,
                hwc_parser::Unit::Centimeter => (value * 10_000_000.0) as i64,
                _ => {
                    return Err(IrError::CoordinateResolutionFailed {
                        coordinate_str: format!("region boundary width: {:?}", boundary.width),
                        reason: format!("Invalid unit for width: {:?}", unit),
                    });
                }
            }
        }
        Ok(hwc_parser::Value::Number(n)) => n, // Assume nm if unitless
        Ok(other) => {
            return Err(IrError::CoordinateResolutionFailed {
                coordinate_str: format!("region boundary width: {:?}", boundary.width),
                reason: format!("Width expression evaluated to {:?}, expected measurement", other),
            });
        }
        Err(e) => {
            return Err(IrError::CoordinateResolutionFailed {
                coordinate_str: format!("region boundary width: {:?}", boundary.width),
                reason: e,
            });
        }
    };
    
    // Evaluate height expression
    let height_nm = match boundary.height.evaluate(eval_context) {
        Ok(hwc_parser::Value::Measurement { value, unit }) => {
            match unit {
                hwc_parser::Unit::Nanometer => value as i64,
                hwc_parser::Unit::Micrometer => (value * 1_000.0) as i64,
                hwc_parser::Unit::Millimeter => (value * 1_000_000.0) as i64,
                hwc_parser::Unit::Centimeter => (value * 10_000_000.0) as i64,
                _ => {
                    return Err(IrError::CoordinateResolutionFailed {
                        coordinate_str: format!("region boundary height: {:?}", boundary.height),
                        reason: format!("Invalid unit for height: {:?}", unit),
                    });
                }
            }
        }
        Ok(hwc_parser::Value::Number(n)) => n, // Assume nm if unitless
        Ok(other) => {
            return Err(IrError::CoordinateResolutionFailed {
                coordinate_str: format!("region boundary height: {:?}", boundary.height),
                reason: format!("Height expression evaluated to {:?}, expected measurement", other),
            });
        }
        Err(e) => {
            return Err(IrError::CoordinateResolutionFailed {
                coordinate_str: format!("region boundary height: {:?}", boundary.height),
                reason: e,
            });
        }
    };
    
    // Depth is 0 for 2D regions (they exist on layer planes)
    let depth_nm = 0;
    
    Ok((width_nm, height_nm, depth_nm))
}
