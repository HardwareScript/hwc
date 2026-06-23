//! Integration test: Edge cases and boundary conditions.
//!
//! Tests extreme inputs, zero-length segments, overlapping segments,
//! coordinate boundaries, and other boundary conditions.

use hwc_engine::geometry::Point3D;
use hwc_engine::geometry_router::connectivity_check::verify_connectivity;
use hwc_engine::geometry_router::deterministic_export::{content_hash, export_dxf_deterministic};
use hwc_engine::geometry_router::geometry_refinement::RefinedContour;
use hwc_engine::geometry_router::gcell_sweep::{
    classify_overlap, compute_actual_clearance, find_overlaps, segment_bbox,
    sort_segments_by_morton, OverlapResult,
};
use hwc_engine::geometry_router::parasitic_extraction::{
    extract_parasitics, ExtractionParams,
};
use hwc_engine::geometry_router::route_decomposition::VirtualJunction;
use hwc_engine::geometry_router::spatial_index::DynamicSpatialIndex;
use hwc_engine::geometry_router::spatial_index::IndexedSegment;
use hwc_engine::geometry_router::gcell_sweep::BridgeTable;
use hwc_engine::material::{MaterialId, MaterialRegistry};
use hwc_engine::netlist::NetId;

// ---------------------------------------------------------------------------
// Empty Input Tests
// ---------------------------------------------------------------------------

#[test]
fn test_empty_segments_find_overlaps() {
    let overlaps = find_overlaps(&[]);
    assert!(overlaps.is_empty());
}

#[test]
fn test_empty_segments_spatial_index() {
    let index = DynamicSpatialIndex::new();
    assert!(index.is_empty());
    assert_eq!(index.len(), 0);

    let bbox = hwc_engine::geometry::BoundingBox::new(
        Point3D::new(0, 0, 0),
        Point3D::new(100_000_000, 100_000_000, 0),
    );
    let results = index.query_bbox(&bbox);
    assert!(results.is_empty());
}

#[test]
fn test_empty_segments_connectivity() {
    let result = verify_connectivity(&[], &[]);
    assert!(result.violations.is_empty());
    assert_eq!(result.nets_checked, 0);
    assert_eq!(result.pins_verified, 0);
}

#[test]
fn test_empty_segments_parasitic_extraction() {
    let params = ExtractionParams {
        freq_hz: 1.0e9,
        substrate_er: 4.5,
        substrate_height_m: 35.0e-6,
        trace_thickness_m: 35.0e-6,
        loss_tangent: 0.02,
    };
    let result = extract_parasitics(&[], &[], &params);
    assert!(result.traces.is_empty());
    assert!(result.spice_netlist.contains(".SUBCKT BOARD_PARASITICS"));
}

#[test]
fn test_empty_dxf_export() {
    let layer_names = std::collections::HashMap::new();
    let dxf1 = export_dxf_deterministic(&[], &layer_names);
    let dxf2 = export_dxf_deterministic(&[], &layer_names);
    assert_eq!(dxf1, dxf2);
    assert!(dxf1.contains("0\nEOF\n"));
}

// ---------------------------------------------------------------------------
// Single Segment Tests
// ---------------------------------------------------------------------------

#[test]
fn test_single_segment_no_overlaps() {
    let segs = vec![IndexedSegment {
        segment_id: 0,
        net_id: 1,
        width_nm: 200_000,
        start: Point3D::new(0, 0, 0),
        end: Point3D::new(10_000_000, 0, 0),
        layer: 0,
    }];

    let overlaps = find_overlaps(&segs);
    assert!(overlaps.is_empty());
}

#[test]
fn test_single_segment_spatial_index_query() {
    let seg = IndexedSegment {
        segment_id: 0,
        net_id: 1,
        width_nm: 200_000,
        start: Point3D::new(5_000_000, 5_000_000, 0),
        end: Point3D::new(10_000_000, 5_000_000, 0),
        layer: 0,
    };

    let mut index = DynamicSpatialIndex::new();
    index.insert(seg.clone());
    assert_eq!(index.len(), 1);

    // Query that contains the segment
    let bbox = hwc_engine::geometry::BoundingBox::new(
        Point3D::new(0, 0, 0),
        Point3D::new(20_000_000, 20_000_000, 0),
    );
    let results = index.query_bbox(&bbox);
    assert_eq!(results.len(), 1);

    // Query that doesn't contain the segment
    let bbox = hwc_engine::geometry::BoundingBox::new(
        Point3D::new(0, 0, 0),
        Point3D::new(1_000_000, 1_000_000, 0),
    );
    let results = index.query_bbox(&bbox);
    assert!(results.is_empty());
}

#[test]
fn test_single_segment_connectivity() {
    let segs = vec![IndexedSegment {
        segment_id: 0,
        net_id: 1,
        width_nm: 200_000,
        start: Point3D::new(0, 0, 0),
        end: Point3D::new(10_000_000, 0, 0),
        layer: 0,
    }];

    let result = verify_connectivity(&segs, &[]);
    // Single segment with 2 endpoints, same net, no violations
    assert!(result.violations.is_empty());
    assert_eq!(result.nets_checked, 1);
}

#[test]
fn test_single_segment_parasitic_extraction() {
    let segs = vec![IndexedSegment {
        segment_id: 0,
        net_id: 1,
        width_nm: 200_000,
        start: Point3D::new(0, 0, 0),
        end: Point3D::new(10_000_000, 0, 0),
        layer: 0,
    }];

    let params = ExtractionParams {
        freq_hz: 1.0e9,
        substrate_er: 4.5,
        substrate_height_m: 35.0e-6,
        trace_thickness_m: 35.0e-6,
        loss_tangent: 0.02,
    };

    let result = extract_parasitics(&segs, &[], &params);
    assert_eq!(result.traces.len(), 1);
    assert_eq!(result.traces[0].segments.len(), 1);
    assert!(result.traces[0].segments[0].length_m > 0.0);
}

// ---------------------------------------------------------------------------
// Two Overlapping Segments (Same Net)
// ---------------------------------------------------------------------------

#[test]
fn test_two_overlapping_same_net_no_violation() {
    let seg_a = IndexedSegment {
        segment_id: 0,
        net_id: 1,
        width_nm: 200_000,
        start: Point3D::new(0, 0, 0),
        end: Point3D::new(10_000_000, 0, 0),
        layer: 0,
    };
    let seg_b = IndexedSegment {
        segment_id: 1,
        net_id: 1,
        width_nm: 200_000,
        start: Point3D::new(5_000_000, 0, 0),
        end: Point3D::new(15_000_000, 0, 0),
        layer: 0,
    };

    // Same-net overlap should be classified as SameNet
    let result = classify_overlap(&seg_a, &seg_b, &[], 200_000, None, None, &MaterialRegistry::new(), &BridgeTable::default());
    match result {
        OverlapResult::SameNet { net_id, .. } => {
            assert_eq!(net_id, 1);
        }
        other => panic!("Expected SameNet, got {:?}", other),
    }
}

#[test]
fn test_two_overlapping_same_net_with_junction() {
    let seg_a = IndexedSegment {
        segment_id: 0,
        net_id: 1,
        width_nm: 200_000,
        start: Point3D::new(0, 0, 0),
        end: Point3D::new(10_000_000, 0, 0),
        layer: 0,
    };
    let seg_b = IndexedSegment {
        segment_id: 1,
        net_id: 1,
        width_nm: 200_000,
        start: Point3D::new(5_000_000, 0, 0),
        end: Point3D::new(15_000_000, 0, 0),
        layer: 0,
    };

    let junctions = vec![VirtualJunction {
        junction_id: 0,
        position: Point3D::new(7_500_000, 0, 0),
        connected_segments: vec![0, 1],
        net_id: NetId::new(1),
        capacitance_pf: 0.1,
        inductance_nh: 0.05,
    }];

    let result = classify_overlap(&seg_a, &seg_b, &junctions, 200_000, None, None, &MaterialRegistry::new(), &BridgeTable::default());
    match result {
        OverlapResult::SameNet {
            is_valid_junction, ..
        } => {
            assert!(is_valid_junction, "Junction at overlap point should be valid");
        }
        other => panic!("Expected SameNet with junction, got {:?}", other),
    }
}

// ---------------------------------------------------------------------------
// Two Overlapping Segments (Different Net) — should detect violation
// ---------------------------------------------------------------------------

#[test]
fn test_two_overlapping_different_net_violation() {
    let seg_a = IndexedSegment {
        segment_id: 0,
        net_id: 1,
        width_nm: 200_000,
        start: Point3D::new(0, 0, 0),
        end: Point3D::new(10_000_000, 0, 0),
        layer: 0,
    };
    let seg_b = IndexedSegment {
        segment_id: 1,
        net_id: 2,
        width_nm: 200_000,
        start: Point3D::new(5_000_000, 0, 0),
        end: Point3D::new(15_000_000, 0, 0),
        layer: 0,
    };

    let result = classify_overlap(&seg_a, &seg_b, &[], 200_000, None, None, &MaterialRegistry::new(), &BridgeTable::default());
    match result {
        OverlapResult::DifferentNet {
            net_a,
            net_b,
            required_clearance,
            ..
        } => {
            assert_eq!(net_a, 1);
            assert_eq!(net_b, 2);
            assert!(required_clearance > 0);
        }
        other => panic!("Expected DifferentNet, got {:?}", other),
    }
}

#[test]
fn test_crossing_different_net_segments() {
    let seg_h = IndexedSegment {
        segment_id: 0,
        net_id: 1,
        width_nm: 200_000,
        start: Point3D::new(-5_000_000, 0, 0),
        end: Point3D::new(5_000_000, 0, 0),
        layer: 0,
    };
    let seg_v = IndexedSegment {
        segment_id: 1,
        net_id: 2,
        width_nm: 200_000,
        start: Point3D::new(0, -5_000_000, 0),
        end: Point3D::new(0, 5_000_000, 0),
        layer: 0,
    };

    // Crossing segments: clearance is negative (overlap)
    let clearance = compute_actual_clearance(&seg_h, &seg_v);
    assert!(clearance < 0, "Crossing segments should have negative clearance");

    // Should detect AABB overlap
    let overlaps = find_overlaps(&[seg_h, seg_v]);
    assert!(!overlaps.is_empty(), "Crossing segments should produce overlap");
}

#[test]
fn test_different_net_short_detected() {
    // Two segments from different nets sharing an endpoint
    let shared = Point3D::new(5_000_000, 0, 0);
    let segs = vec![
        IndexedSegment {
            segment_id: 0,
            net_id: 1,
            width_nm: 200_000,
            start: Point3D::new(0, 0, 0),
            end: shared,
            layer: 0,
        },
        IndexedSegment {
            segment_id: 1,
            net_id: 2,
            width_nm: 200_000,
            start: Point3D::new(10_000_000, 0, 0),
            end: shared,
            layer: 0,
        },
    ];

    let result = verify_connectivity(&segs, &[]);
    let shorts = result
        .violations
        .iter()
        .filter(|v| matches!(v, hwc_engine::geometry_router::connectivity_check::ConnectivityViolation::UnwaivedShort { .. }))
        .count();
    assert_eq!(shorts, 1, "Should detect 1 short between nets 1 and 2");
}

// ---------------------------------------------------------------------------
// Coordinate Boundary Tests
// ---------------------------------------------------------------------------

#[test]
fn test_large_coordinates_no_overflow() {
    let large = i64::MAX / 4; // Stay well within bounds to avoid overflow in bbox computation
    let seg = IndexedSegment {
        segment_id: 0,
        net_id: 1,
        width_nm: 200_000,
        start: Point3D::new(large, large, 0),
        end: Point3D::new(large + 1_000_000, large, 0),
        layer: 0,
    };

    // Should not panic
    let bbox = segment_bbox(&seg);
    assert_eq!(bbox.min_x, large - 100_000);
    assert_eq!(bbox.max_x, large + 1_100_000);

    let mut index = DynamicSpatialIndex::new();
    index.insert(seg);
    assert_eq!(index.len(), 1);
}

#[test]
fn test_negative_coordinates() {
    let seg = IndexedSegment {
        segment_id: 0,
        net_id: 1,
        width_nm: 200_000,
        start: Point3D::new(-10_000_000, -10_000_000, 0),
        end: Point3D::new(10_000_000, 10_000_000, 0),
        layer: 0,
    };

    let bbox = segment_bbox(&seg);
    assert_eq!(bbox.min_x, -10_100_000);
    assert_eq!(bbox.min_y, -10_100_000);
    assert_eq!(bbox.max_x, 10_100_000);
    assert_eq!(bbox.max_y, 10_100_000);

    let mut index = DynamicSpatialIndex::new();
    index.insert(seg);
    assert_eq!(index.len(), 1);
}

#[test]
fn test_mixed_positive_negative_coordinates() {
    let segs = vec![
        IndexedSegment {
            segment_id: 0,
            net_id: 1,
            width_nm: 200_000,
            start: Point3D::new(-5_000_000, 0, 0),
            end: Point3D::new(5_000_000, 0, 0),
            layer: 0,
        },
        IndexedSegment {
            segment_id: 1,
            net_id: 2,
            width_nm: 200_000,
            start: Point3D::new(0, -5_000_000, 0),
            end: Point3D::new(0, 5_000_000, 0),
            layer: 0,
        },
    ];

    let overlaps = find_overlaps(&segs);
    assert!(!overlaps.is_empty(), "Crossing segments at origin should overlap");
}

// ---------------------------------------------------------------------------
// Very Small Segments (1nm length)
// ---------------------------------------------------------------------------

#[test]
fn test_one_nanometer_segment() {
    let seg = IndexedSegment {
        segment_id: 0,
        net_id: 1,
        width_nm: 200_000,
        start: Point3D::new(5_000_000, 5_000_000, 0),
        end: Point3D::new(5_000_001, 5_000_000, 0), // 1nm length
        layer: 0,
    };

    let bbox = segment_bbox(&seg);
    // Width is 200_000nm, so half-width is 100_000nm
    assert_eq!(bbox.min_x, 4_900_000);
    assert_eq!(bbox.max_x, 5_100_001);
    assert_eq!(bbox.min_y, 4_900_000);
    assert_eq!(bbox.max_y, 5_100_000);

    // Should not panic in any operation
    let mut index = DynamicSpatialIndex::new();
    index.insert(seg.clone());
    assert_eq!(index.len(), 1);

    let overlaps = find_overlaps(&[seg]);
    assert!(overlaps.is_empty(), "Single tiny segment should not overlap");
}

#[test]
fn test_two_tiny_segments_same_net() {
    let segs = vec![
        IndexedSegment {
            segment_id: 0,
            net_id: 1,
            width_nm: 1_000,
            start: Point3D::new(5_000_000, 5_000_000, 0),
            end: Point3D::new(5_000_001, 5_000_000, 0),
            layer: 0,
        },
        IndexedSegment {
            segment_id: 1,
            net_id: 1,
            width_nm: 1_000,
            start: Point3D::new(5_000_001, 5_000_000, 0),
            end: Point3D::new(5_000_002, 5_000_000, 0),
            layer: 0,
        },
    ];

    let result = verify_connectivity(&segs, &[]);
    assert!(
        result.violations.is_empty(),
        "Two tiny same-net segments sharing endpoint should be connected"
    );
}

#[test]
fn test_two_tiny_segments_different_net() {
    let segs = vec![
        IndexedSegment {
            segment_id: 0,
            net_id: 1,
            width_nm: 1_000,
            start: Point3D::new(5_000_000, 5_000_000, 0),
            end: Point3D::new(5_000_001, 5_000_000, 0),
            layer: 0,
        },
        IndexedSegment {
            segment_id: 1,
            net_id: 2,
            width_nm: 1_000,
            start: Point3D::new(5_000_000, 5_000_000, 0),
            end: Point3D::new(5_000_001, 5_000_000, 0),
            layer: 0,
        },
    ];

    // Two segments from different nets at the exact same position
    let result = verify_connectivity(&segs, &[]);
    let shorts = result
        .violations
        .iter()
        .filter(|v| matches!(v, hwc_engine::geometry_router::connectivity_check::ConnectivityViolation::UnwaivedShort { .. }))
        .count();
    assert!(shorts >= 1, "Co-located segments from different nets should be detected as short, got {shorts}");
}

// ---------------------------------------------------------------------------
// Zero-Width Segments
// ---------------------------------------------------------------------------

#[test]
fn test_zero_width_segment() {
    let seg = IndexedSegment {
        segment_id: 0,
        net_id: 1,
        width_nm: 0,
        start: Point3D::new(0, 0, 0),
        end: Point3D::new(10_000_000, 0, 0),
        layer: 0,
    };

    // Should not panic
    let bbox = segment_bbox(&seg);
    assert_eq!(bbox.min_x, 0);
    assert_eq!(bbox.max_x, 10_000_000);
    assert_eq!(bbox.min_y, 0);
    assert_eq!(bbox.max_y, 0);

    let mut index = DynamicSpatialIndex::new();
    index.insert(seg);
    assert_eq!(index.len(), 1);
}

#[test]
fn test_zero_width_segments_no_overlap_different_nets() {
    let segs = vec![
        IndexedSegment {
            segment_id: 0,
            net_id: 1,
            width_nm: 0,
            start: Point3D::new(0, 0, 0),
            end: Point3D::new(10_000_000, 0, 0),
            layer: 0,
        },
        IndexedSegment {
            segment_id: 1,
            net_id: 2,
            width_nm: 0,
            start: Point3D::new(0, 1_000_000, 0),
            end: Point3D::new(10_000_000, 1_000_000, 0),
            layer: 0,
        },
    ];

    // Zero-width parallel segments separated by 1mm should not overlap
    let overlaps = find_overlaps(&segs);
    assert!(overlaps.is_empty(), "Zero-width parallel segments 1mm apart should not overlap");
}

#[test]
fn test_zero_width_crossing_segments() {
    let segs = vec![
        IndexedSegment {
            segment_id: 0,
            net_id: 1,
            width_nm: 0,
            start: Point3D::new(-5_000_000, 0, 0),
            end: Point3D::new(5_000_000, 0, 0),
            layer: 0,
        },
        IndexedSegment {
            segment_id: 1,
            net_id: 2,
            width_nm: 0,
            start: Point3D::new(0, -5_000_000, 0),
            end: Point3D::new(0, 5_000_000, 0),
            layer: 0,
        },
    ];

    // Zero-width crossing segments: AABB overlap depends on bounding box computation
    // With zero width, the AABB is just the line, so they may or may not overlap
    // depending on the exact implementation. This test just verifies no panic.
    let _overlaps = find_overlaps(&segs);
}

// ---------------------------------------------------------------------------
// Parallel Close Segments (Clearance Boundary)
// ---------------------------------------------------------------------------

#[test]
fn test_parallel_segments_exactly_at_clearance() {
    let seg_a = IndexedSegment {
        segment_id: 0,
        net_id: 1,
        width_nm: 200_000,
        start: Point3D::new(0, 0, 0),
        end: Point3D::new(10_000_000, 0, 0),
        layer: 0,
    };
    // Center at y=400_000, half-width = 100_000
    // Edge-to-edge = 400_000 - 100_000 - 100_000 = 200_000
    let seg_b = IndexedSegment {
        segment_id: 1,
        net_id: 2,
        width_nm: 200_000,
        start: Point3D::new(0, 400_000, 0),
        end: Point3D::new(10_000_000, 400_000, 0),
        layer: 0,
    };

    let clearance = compute_actual_clearance(&seg_a, &seg_b);
    assert_eq!(clearance, 200_000, "Clearance should be exactly 200um");

    // With default clearance of 200_000, this is exactly at the boundary
    let result = classify_overlap(&seg_a, &seg_b, &[], 200_000, None, None, &MaterialRegistry::new(), &BridgeTable::default());
    match result {
        OverlapResult::DifferentNet { .. } => {
            // At exactly the clearance limit, this is a violation (strict <)
        }
        OverlapResult::NoOverlap => {
            // Or it could be NoOverlap if the check is <=
        }
        other => panic!("Expected DifferentNet or NoOverlap, got {:?}", other),
    }
}

#[test]
fn test_parallel_segments_within_clearance() {
    let seg_a = IndexedSegment {
        segment_id: 0,
        net_id: 1,
        width_nm: 200_000,
        start: Point3D::new(0, 0, 0),
        end: Point3D::new(10_000_000, 0, 0),
        layer: 0,
    };
    // Center at y=300_000, half-width = 100_000
    // Edge-to-edge = 300_000 - 100_000 - 100_000 = 100_000 (< 200_000 clearance)
    let seg_b = IndexedSegment {
        segment_id: 1,
        net_id: 2,
        width_nm: 200_000,
        start: Point3D::new(0, 300_000, 0),
        end: Point3D::new(10_000_000, 300_000, 0),
        layer: 0,
    };

    let clearance = compute_actual_clearance(&seg_a, &seg_b);
    assert_eq!(clearance, 100_000, "Clearance should be 100um");
    assert!(clearance < 200_000, "Should violate 200um clearance");

    let result = classify_overlap(&seg_a, &seg_b, &[], 200_000, None, None, &MaterialRegistry::new(), &BridgeTable::default());
    match result {
        OverlapResult::DifferentNet {
            required_clearance, ..
        } => {
            assert_eq!(required_clearance, 200_000);
        }
        other => panic!("Expected DifferentNet, got {:?}", other),
    }
}

#[test]
fn test_parallel_segments_outside_clearance() {
    let seg_a = IndexedSegment {
        segment_id: 0,
        net_id: 1,
        width_nm: 200_000,
        start: Point3D::new(0, 0, 0),
        end: Point3D::new(10_000_000, 0, 0),
        layer: 0,
    };
    // Center at y=600_000, half-width = 100_000
    // Edge-to-edge = 600_000 - 100_000 - 100_000 = 400_000 (> 200_000 clearance)
    let seg_b = IndexedSegment {
        segment_id: 1,
        net_id: 2,
        width_nm: 200_000,
        start: Point3D::new(0, 600_000, 0),
        end: Point3D::new(10_000_000, 600_000, 0),
        layer: 0,
    };

    let clearance = compute_actual_clearance(&seg_a, &seg_b);
    assert_eq!(clearance, 400_000, "Clearance should be 400um");
    assert!(clearance >= 200_000, "Should satisfy 200um clearance");

    let result = classify_overlap(&seg_a, &seg_b, &[], 200_000, None, None, &MaterialRegistry::new(), &BridgeTable::default());
    match result {
        OverlapResult::NoOverlap => {}
        other => panic!("Expected NoOverlap, got {:?}", other),
    }
}

// ---------------------------------------------------------------------------
// Multi-Layer Segments
// ---------------------------------------------------------------------------

#[test]
fn test_segments_on_different_layers_same_position() {
    let segs = vec![
        IndexedSegment {
            segment_id: 0,
            net_id: 1,
            width_nm: 200_000,
            start: Point3D::new(0, 0, 0),
            end: Point3D::new(10_000_000, 0, 0),
            layer: 0,
        },
        IndexedSegment {
            segment_id: 1,
            net_id: 2,
            width_nm: 200_000,
            start: Point3D::new(0, 0, 1_400_000),
            end: Point3D::new(10_000_000, 0, 1_400_000),
            layer: 1,
        },
    ];

    // Segments on different layers at the same XY position
    // The AABB overlap check is 2D, so they will overlap
    let overlaps = find_overlaps(&segs);
    assert!(!overlaps.is_empty(), "Segments on different layers at same XY should have AABB overlap");
}

// ---------------------------------------------------------------------------
// Diagonal Segments
// ---------------------------------------------------------------------------

#[test]
fn test_diagonal_segments_no_overlap() {
    let segs = vec![
        IndexedSegment {
            segment_id: 0,
            net_id: 1,
            width_nm: 100_000,
            start: Point3D::new(0, 0, 0),
            end: Point3D::new(10_000_000, 10_000_000, 0),
            layer: 0,
        },
        IndexedSegment {
            segment_id: 1,
            net_id: 2,
            width_nm: 100_000,
            start: Point3D::new(0, 10_000_000, 0),
            end: Point3D::new(10_000_000, 0, 0),
            layer: 0,
        },
    ];

    // Diagonal segments crossing at center should overlap in AABB
    let overlaps = find_overlaps(&segs);
    assert!(!overlaps.is_empty(), "Crossing diagonal segments should have AABB overlap");
}

// ---------------------------------------------------------------------------
// Morton Sort Edge Cases
// ---------------------------------------------------------------------------

#[test]
fn test_morton_sort_single_segment() {
    let mut segs = vec![IndexedSegment {
        segment_id: 0,
        net_id: 1,
        width_nm: 200_000,
        start: Point3D::new(5_000_000, 5_000_000, 0),
        end: Point3D::new(10_000_000, 5_000_000, 0),
        layer: 0,
    }];

    sort_segments_by_morton(&mut segs);
    assert_eq!(segs.len(), 1);
}

#[test]
fn test_morton_sort_identical_segments() {
    let make_seg = |id: usize| IndexedSegment {
        segment_id: id,
        net_id: 1,
        width_nm: 200_000,
        start: Point3D::new(5_000_000, 5_000_000, 0),
        end: Point3D::new(10_000_000, 5_000_000, 0),
        layer: 0,
    };

    let mut segs: Vec<IndexedSegment> = (0..10).map(make_seg).collect();
    let original = segs.clone();

    sort_segments_by_morton(&mut segs);
    // All segments have same center, so Morton codes are equal
    // Sort should be stable (preserve original order)
    assert_eq!(segs, original);
}

// ---------------------------------------------------------------------------
// Disconnected Pin Detection
// ---------------------------------------------------------------------------

#[test]
fn test_three_segments_two_disconnected() {
    let segs = vec![
        IndexedSegment {
            segment_id: 0,
            net_id: 1,
            width_nm: 200_000,
            start: Point3D::new(0, 0, 0),
            end: Point3D::new(5_000_000, 0, 0),
            layer: 0,
        },
        IndexedSegment {
            segment_id: 1,
            net_id: 1,
            width_nm: 200_000,
            start: Point3D::new(5_000_000, 0, 0),
            end: Point3D::new(10_000_000, 0, 0),
            layer: 0,
        },
        IndexedSegment {
            segment_id: 2,
            net_id: 1,
            width_nm: 200_000,
            start: Point3D::new(20_000_000, 0, 0),
            end: Point3D::new(25_000_000, 0, 0),
            layer: 0,
        },
    ];

    let result = verify_connectivity(&segs, &[]);
    let disconnected = result
        .violations
        .iter()
        .filter(|v| matches!(v, hwc_engine::geometry_router::connectivity_check::ConnectivityViolation::DisconnectedPin { .. }))
        .count();
    assert!(
        disconnected >= 2,
        "Should detect at least 2 disconnected pins (the isolated segment)"
    );
}

// ---------------------------------------------------------------------------
// DXF Export Edge Cases
// ---------------------------------------------------------------------------

#[test]
fn test_dxf_single_vertex_contour() {
    let contours = vec![RefinedContour {
        outer: vec![(1_000_000, 1_000_000)],
        holes: Vec::new(),
        area: 0,
    }];
    let layer_names = std::collections::HashMap::new();

    // Should not panic, just produce minimal output
    let dxf = export_dxf_deterministic(&contours, &layer_names);
    assert!(dxf.contains("0\nEOF\n"));
}

#[test]
fn test_dxf_two_vertex_contour() {
    let contours = vec![RefinedContour {
        outer: vec![(0, 0), (1_000_000, 1_000_000)],
        holes: Vec::new(),
        area: 0,
    }];
    let layer_names = std::collections::HashMap::new();

    let dxf = export_dxf_deterministic(&contours, &layer_names);
    assert!(dxf.contains("0\nEOF\n"));
}

#[test]
fn test_dxf_many_contours_deterministic() {
    let mut contours = Vec::new();
    for i in 0..100 {
        let x = (i as i64) * 100_000;
        contours.push(RefinedContour {
            outer: vec![(x, 0), (x + 50_000, 0), (x + 50_000, 50_000), (x, 50_000)],
            holes: Vec::new(),
            area: 2_500_000_000,
        });
    }

    let layer_names = std::collections::HashMap::new();
    let dxf1 = export_dxf_deterministic(&contours, &layer_names);
    let dxf2 = export_dxf_deterministic(&contours, &layer_names);
    assert_eq!(dxf1, dxf2);

    let hash = content_hash(dxf1.as_bytes());
    assert_eq!(hash, content_hash(dxf2.as_bytes()));
}
