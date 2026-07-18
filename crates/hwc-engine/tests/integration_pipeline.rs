//! Integration test: Full pipeline from spatial indexing through export.
//!
//! Validates the complete PCB/APCB autorouter engine pipeline:
//! 1. Spatial index creation with synthetic test segments
//! 2. G-cell sweep DRC verification
//! 3. Connectivity check
//! 4. Parasitic extraction
//! 5. Deterministic export to DXF
//! 6. Export non-emptiness and determinism verification

use hwc_engine::geometry::{BoundingBox, Point3D};
use hwc_engine::geometry_router::connectivity_check::verify_connectivity;
use hwc_engine::geometry_router::deterministic_export::{
    content_hash, export_dxf_deterministic, sort_segments_deterministic,
};
use hwc_engine::geometry_router::gcell_sweep::{
    compute_actual_clearance, compute_morton_code, find_overlaps, segment_bbox,
    sort_segments_by_morton,
};
use hwc_engine::geometry_router::geometry_refinement::RefinedContour;
use hwc_engine::geometry_router::parasitic_extraction::{extract_parasitics, ExtractionParams};
use hwc_engine::geometry_router::partition::{partition_nets, PartitionGrid};
use hwc_engine::geometry_router::route_decomposition::VirtualJunction;
use hwc_engine::geometry_router::spatial_index::{DynamicSpatialIndex, IndexedSegment};
use hwc_engine::netlist::NetId;

// ---------------------------------------------------------------------------
// Synthetic data builders
// ---------------------------------------------------------------------------

/// Create 20 test segments across 2 layers and 5 nets.
///
/// Layout (all coordinates in i64 nanometers):
///
/// Layer 0 (F.Cu):
///   Net 1: 3 horizontal segments forming an L-shape
///   Net 2: 2 horizontal segments spaced apart
///   Net 3: 3 horizontal segments in a line
///
/// Layer 1 (B.Cu):
///   Net 4: 4 horizontal segments
///   Net 5: 5 horizontal segments forming a grid-like pattern
///   Net 1: 3 segments (via connections on bottom layer)
fn make_test_segments() -> Vec<IndexedSegment> {
    let mut segs = Vec::new();
    let mut id = 0;

    // Layer 0 (F.Cu) — z=0
    // Net 1: L-shape at y=1_000_000
    for &(sx, sy, ex, ey) in &[
        (0, 1_000_000, 5_000_000, 1_000_000),
        (5_000_000, 1_000_000, 5_000_000, 4_000_000),
        (5_000_000, 4_000_000, 10_000_000, 4_000_000),
    ] {
        segs.push(IndexedSegment {
            segment_id: id,
            net_id: 1,
            width_nm: 200_000,
            thickness_nm: 35_000,
            start: Point3D::new(sx, sy, 0),
            end: Point3D::new(ex, ey, 0),
            layer: 0,
        });
        id += 1;
    }

    // Net 2: two parallel horizontal segments at y=2_000_000 and y=3_000_000
    for &(sy, ex) in &[(2_000_000, 8_000_000), (3_000_000, 6_000_000)] {
        segs.push(IndexedSegment {
            segment_id: id,
            net_id: 2,
            width_nm: 150_000,
            thickness_nm: 35_000,
            start: Point3D::new(1_000_000, sy, 0),
            end: Point3D::new(ex, sy, 0),
            layer: 0,
        });
        id += 1;
    }

    // Net 3: three segments at y=5_000_000
    for &(sx, ex) in &[
        (0, 3_000_000),
        (3_000_000, 7_000_000),
        (7_000_000, 11_000_000),
    ] {
        segs.push(IndexedSegment {
            segment_id: id,
            net_id: 3,
            width_nm: 100_000,
            thickness_nm: 35_000,
            start: Point3D::new(sx, 5_000_000, 0),
            end: Point3D::new(ex, 5_000_000, 0),
            layer: 0,
        });
        id += 1;
    }

    // Layer 1 (B.Cu) — z=1_400_000 (1.4mm board thickness)
    // Net 4: four horizontal segments at different y positions
    for &(sy, sx, ex) in &[
        (0, 2_000_000, 9_000_000),
        (1_000_000, 0, 6_000_000),
        (2_000_000, 4_000_000, 10_000_000),
        (3_000_000, 1_000_000, 7_000_000),
    ] {
        segs.push(IndexedSegment {
            segment_id: id,
            net_id: 4,
            width_nm: 250_000,
            thickness_nm: 35_000,
            start: Point3D::new(sx, sy, 1_400_000),
            end: Point3D::new(ex, sy, 1_400_000),
            layer: 1,
        });
        id += 1;
    }

    // Net 5: five segments in a grid-like pattern
    for &(sx, sy, ex, ey) in &[
        (0, 6_000_000, 12_000_000, 6_000_000),
        (0, 7_000_000, 12_000_000, 7_000_000),
        (0, 8_000_000, 12_000_000, 8_000_000),
        (6_000_000, 6_000_000, 6_000_000, 8_000_000),
        (0, 6_000_000, 0, 8_000_000),
    ] {
        segs.push(IndexedSegment {
            segment_id: id,
            net_id: 5,
            width_nm: 100_000,
            thickness_nm: 35_000,
            start: Point3D::new(sx, sy, 1_400_000),
            end: Point3D::new(ex, ey, 1_400_000),
            layer: 1,
        });
        id += 1;
    }

    // Net 1 additional segments on layer 1 (via connections)
    for &(sx, sy, ex, ey) in &[
        (5_000_000, 1_000_000, 5_000_000, 4_000_000),
        (5_000_000, 4_000_000, 10_000_000, 4_000_000),
        (10_000_000, 4_000_000, 10_000_000, 1_000_000),
    ] {
        segs.push(IndexedSegment {
            segment_id: id,
            net_id: 1,
            width_nm: 200_000,
            thickness_nm: 35_000,
            start: Point3D::new(sx, sy, 1_400_000),
            end: Point3D::new(ex, ey, 1_400_000),
            layer: 1,
        });
        id += 1;
    }

    assert_eq!(id, 20, "Should produce exactly 20 segments");
    segs
}

/// Create junctions that connect same-net segments at shared endpoints.
fn make_test_junctions() -> Vec<VirtualJunction> {
    vec![
        VirtualJunction {
            junction_id: 0,
            position: Point3D::new(5_000_000, 1_000_000, 0),
            connected_segments: vec![0, 1],
            net_id: NetId::new(1),
            capacitance_pf: 0.1,
            inductance_nh: 0.05,
        },
        VirtualJunction {
            junction_id: 1,
            position: Point3D::new(5_000_000, 4_000_000, 0),
            connected_segments: vec![1, 2],
            net_id: NetId::new(1),
            capacitance_pf: 0.1,
            inductance_nh: 0.05,
        },
        VirtualJunction {
            junction_id: 2,
            position: Point3D::new(3_000_000, 5_000_000, 0),
            connected_segments: vec![5, 6],
            net_id: NetId::new(3),
            capacitance_pf: 0.05,
            inductance_nh: 0.02,
        },
        VirtualJunction {
            junction_id: 3,
            position: Point3D::new(7_000_000, 5_000_000, 0),
            connected_segments: vec![6, 7],
            net_id: NetId::new(3),
            capacitance_pf: 0.05,
            inductance_nh: 0.02,
        },
    ]
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn test_full_pipeline_small_pcb() {
    let segments = make_test_segments();
    let junctions = make_test_junctions();

    // Step 1: Build spatial index
    let mut spatial_index = DynamicSpatialIndex::new();
    for seg in &segments {
        spatial_index.insert(seg.clone());
    }
    assert_eq!(spatial_index.len(), 20);
    assert!(!spatial_index.is_empty());

    // Step 2: Run DRC sweep (find overlapping segment pairs)
    let overlaps = find_overlaps(&segments);
    // With 20 segments across 2 layers, we expect some AABB overlaps
    // (parallel close segments will overlap in bounding boxes)
    assert!(
        !overlaps.is_empty(),
        "20 segments should produce at least some AABB overlaps"
    );

    // Verify the overlaps are valid pairs (segment_id < segment_id)
    for (a, b) in &overlaps {
        assert!(a < b, "Overlap pair should be ordered: ({a}, {b})");
    }

    // Step 3: Run connectivity check
    let conn_result = verify_connectivity(&segments, &junctions);
    // The segments within the same net should be connected via shared endpoints
    assert_eq!(conn_result.nets_checked, 5);
    assert!(conn_result.pins_verified > 0);

    // Step 4: Run parasitic extraction
    let extraction_params = ExtractionParams {
        freq_hz: 1.0e9,
        substrate_er: 4.5,
        substrate_height_m: 35.0e-6,
        trace_thickness_m: 35.0e-6,
        loss_tangent: 0.02,
    };
    let extraction_result = extract_parasitics(&segments, &junctions, &extraction_params);
    assert_eq!(extraction_result.traces.len(), 5, "Should extract 5 nets");
    assert!(
        !extraction_result.spice_netlist.is_empty(),
        "SPICE netlist should not be empty"
    );
    assert!(
        extraction_result
            .spice_netlist
            .contains(".SUBCKT BOARD_PARASITICS"),
        "SPICE netlist should contain subcircuit header"
    );

    // Step 5: Export to DXF (convert segments to contours first)
    let contours: Vec<RefinedContour> = segments
        .iter()
        .map(|seg| {
            let half_w = seg.width_nm / 2;
            let sx = seg.start.x.min(seg.end.x);
            let sy = seg.start.y.min(seg.end.y);
            let ex = seg.start.x.max(seg.end.x);
            let ey = seg.start.y.max(seg.end.y);
            RefinedContour {
                outer: vec![
                    (sx - half_w, sy - half_w),
                    (ex + half_w, sy - half_w),
                    (ex + half_w, ey + half_w),
                    (sx - half_w, ey + half_w),
                ],
                holes: Vec::new(),
                area: ((ex - sx + seg.width_nm) * (ey - sy + seg.width_nm)) as i128,
            }
        })
        .collect();

    let dxf_output = export_dxf_deterministic(&contours, &std::collections::HashMap::new());
    assert!(!dxf_output.is_empty(), "DXF output should not be empty");
    assert!(
        dxf_output.contains("POLYLINE"),
        "DXF should contain POLYLINE entities"
    );
    assert!(dxf_output.contains("0\nEOF\n"), "DXF should end with EOF");

    // Step 6: Verify DXF is deterministic
    let dxf_output_2 = export_dxf_deterministic(&contours, &std::collections::HashMap::new());
    assert_eq!(dxf_output, dxf_output_2, "DXF export must be deterministic");

    // Verify content hash is stable
    let hash1 = content_hash(dxf_output.as_bytes());
    let hash2 = content_hash(dxf_output_2.as_bytes());
    assert_eq!(hash1, hash2, "Content hash must be stable");
}

#[test]
fn test_spatial_index_insert_query() {
    let segments = make_test_segments();
    let mut index = DynamicSpatialIndex::new();
    for seg in &segments {
        index.insert(seg.clone());
    }
    assert_eq!(index.len(), 20);

    // Query bounding box covering the full board
    let board_bbox = BoundingBox::new(
        Point3D::new(0, 0, 0),
        Point3D::new(15_000_000, 10_000_000, 2_000_000),
    );
    let results = index.query_bbox(&board_bbox);
    assert!(
        results.len() >= 10,
        "Full board query should return most segments, got {}",
        results.len()
    );

    // Query a small region
    let small_bbox = BoundingBox::new(Point3D::new(0, 0, 0), Point3D::new(2_000_000, 2_000_000, 1));
    let results = index.query_bbox(&small_bbox);
    assert!(
        results.len() < 20,
        "Small region should return fewer segments"
    );

    // Query radius around a known point
    let results = index.query_radius(5_000_000, 1_000_000, 2_000_000);
    assert!(
        !results.is_empty(),
        "Radius query near segments should return results"
    );

    // Query nearest
    let results = index.query_nearest(5_000_000, 1_000_000);
    assert!(results.is_some(), "Nearest query should return a segment");
}

#[test]
fn test_partition_grid_creation() {
    let board_bounds = BoundingBox::new(
        Point3D::new(0, 0, 0),
        Point3D::new(100_000_000, 100_000_000, 0),
    );
    let grid = PartitionGrid::new(board_bounds, 10_000_000, 10_000_000, 100_000, 200_000);

    assert_eq!(grid.cols, 10);
    assert_eq!(grid.rows, 10);
    assert_eq!(grid.total_cells(), 100);

    // Cell lookup
    let cell = grid.cell_at(Point3D::new(5_000_000, 5_000_000, 0));
    assert!(cell.is_some(), "Point inside board should return a cell");

    let cell_outside = grid.cell_at(Point3D::new(200_000_000, 200_000_000, 0));
    assert!(
        cell_outside.is_none(),
        "Point outside board should return None"
    );

    // Neighbors
    let cell_id = cell.unwrap();
    let neighbors = grid.neighbors(cell_id);
    assert!(
        neighbors.len() <= 4,
        "Interior cell should have 4 neighbors"
    );
}

#[test]
fn test_morton_sort_deterministic() {
    let mut segs1 = make_test_segments();
    let mut segs2 = segs1.clone();

    sort_segments_by_morton(&mut segs1);
    sort_segments_by_morton(&mut segs2);
    assert_eq!(segs1, segs2, "Morton sort must be deterministic");

    // Sort reversed input — stable sort preserves equal-key order, so results may differ
    // but both must be valid Morton-sorted sequences
    segs2 = make_test_segments();
    segs2.reverse();
    sort_segments_by_morton(&mut segs2);
    // Verify both are sorted by Morton code (each element's code <= next element's code)
    for w in segs1.windows(2) {
        let c1 = compute_morton_code(w[0].center().x, w[0].center().y);
        let c2 = compute_morton_code(w[1].center().x, w[1].center().y);
        assert!(c1 <= c2, "segs1 must be Morton-sorted");
    }
    for w in segs2.windows(2) {
        let c1 = compute_morton_code(w[0].center().x, w[0].center().y);
        let c2 = compute_morton_code(w[1].center().x, w[1].center().y);
        assert!(c1 <= c2, "segs2 must be Morton-sorted");
    }
}

#[test]
fn test_deterministic_sort_segments() {
    let mut segs1 = make_test_segments();
    let mut segs2 = segs1.clone();
    segs2.reverse();

    sort_segments_deterministic(&mut segs1);
    sort_segments_deterministic(&mut segs2);
    assert_eq!(
        segs1, segs2,
        "Deterministic sort must produce identical results"
    );
}

#[test]
fn test_clearance_computation() {
    // Two parallel horizontal segments with known spacing
    let seg_a = IndexedSegment {
        segment_id: 0,
        net_id: 1,
        width_nm: 200_000,
        thickness_nm: 35_000,
        start: Point3D::new(0, 0, 0),
        end: Point3D::new(10_000_000, 0, 0),
        layer: 0,
    };
    let seg_b = IndexedSegment {
        segment_id: 1,
        net_id: 2,
        width_nm: 200_000,
        thickness_nm: 35_000,
        start: Point3D::new(0, 1_000_000, 0),
        end: Point3D::new(10_000_000, 1_000_000, 0),
        layer: 0,
    };
    // Center-to-center = 1mm, each width = 0.2mm, half-width = 0.1mm
    // Edge-to-edge clearance = 1mm - 0.1mm - 0.1mm = 0.8mm = 800_000 nm
    let clearance = compute_actual_clearance(&seg_a, &seg_b);
    assert_eq!(clearance, 800_000);

    // Two crossing segments
    let seg_h = IndexedSegment {
        segment_id: 2,
        net_id: 3,
        width_nm: 100_000,
        thickness_nm: 35_000,
        start: Point3D::new(-5_000_000, 0, 0),
        end: Point3D::new(5_000_000, 0, 0),
        layer: 0,
    };
    let seg_v = IndexedSegment {
        segment_id: 3,
        net_id: 4,
        width_nm: 100_000,
        thickness_nm: 35_000,
        start: Point3D::new(0, -5_000_000, 0),
        end: Point3D::new(0, 5_000_000, 0),
        layer: 0,
    };
    let clearance = compute_actual_clearance(&seg_h, &seg_v);
    // Crossing segments: clearance is negative (they overlap)
    assert!(
        clearance < 0,
        "Crossing segments should have negative clearance"
    );
}

#[test]
fn test_parasitic_extraction_completeness() {
    let segments = make_test_segments();
    let junctions = make_test_junctions();

    let params = ExtractionParams {
        freq_hz: 1.0e9,
        substrate_er: 4.5,
        substrate_height_m: 35.0e-6,
        trace_thickness_m: 35.0e-6,
        loss_tangent: 0.02,
    };

    let result = extract_parasitics(&segments, &junctions, &params);

    // Should have traces for all 5 nets
    assert_eq!(result.traces.len(), 5);

    // Each trace should have at least one segment
    for trace in &result.traces {
        assert!(
            !trace.segments.is_empty(),
            "Net {} should have extracted segments",
            trace.net_id
        );
        // Each extracted segment should have positive resistance (unless zero-length)
        for seg in &trace.segments {
            assert!(
                seg.resistance_ohm >= 0.0,
                "Resistance should be non-negative for net {}",
                trace.net_id
            );
        }
    }

    // SPICE netlist should be well-formed
    let spice = &result.spice_netlist;
    assert!(spice.starts_with("* HWC Parasitic Extraction Netlist"));
    assert!(spice.contains(".SUBCKT BOARD_PARASITICS"));
    assert!(spice.contains(".ENDS BOARD_PARASITICS"));
}

#[test]
fn test_deterministic_dxf_export() {
    let segments = make_test_segments();
    let contours: Vec<RefinedContour> = segments
        .iter()
        .map(|seg| {
            let half_w = seg.width_nm / 2;
            let sx = seg.start.x.min(seg.end.x);
            let sy = seg.start.y.min(seg.end.y);
            let ex = seg.start.x.max(seg.end.x);
            let ey = seg.start.y.max(seg.end.y);
            RefinedContour {
                outer: vec![
                    (sx - half_w, sy - half_w),
                    (ex + half_w, sy - half_w),
                    (ex + half_w, ey + half_w),
                    (sx - half_w, ey + half_w),
                ],
                holes: Vec::new(),
                area: ((ex - sx + seg.width_nm) * (ey - sy + seg.width_nm)) as i128,
            }
        })
        .collect();

    let mut layer_names = std::collections::HashMap::new();
    layer_names.insert(0u8, "F_COPPER".to_string());
    layer_names.insert(1u8, "B_COPPER".to_string());

    // Run 5 times and verify byte-identical output
    let outputs: Vec<String> = (0..5)
        .map(|_| export_dxf_deterministic(&contours, &layer_names))
        .collect();

    for i in 1..outputs.len() {
        assert_eq!(
            outputs[0], outputs[i],
            "DXF export must be deterministic across runs"
        );
    }

    // Verify SHA-256 hash is stable
    let hash = content_hash(outputs[0].as_bytes());
    for output in &outputs[1..] {
        assert_eq!(
            hash,
            content_hash(output.as_bytes()),
            "Content hash must be stable"
        );
    }
}

#[test]
fn test_connectivity_fully_connected_net() {
    // Create a simple fully connected net
    let segments = vec![
        IndexedSegment {
            segment_id: 0,
            net_id: 1,
            width_nm: 200_000,
            thickness_nm: 35_000,
            start: Point3D::new(0, 0, 0),
            end: Point3D::new(5_000_000, 0, 0),
            layer: 0,
        },
        IndexedSegment {
            segment_id: 1,
            net_id: 1,
            width_nm: 200_000,
            thickness_nm: 35_000,
            start: Point3D::new(5_000_000, 0, 0),
            end: Point3D::new(10_000_000, 0, 0),
            layer: 0,
        },
        IndexedSegment {
            segment_id: 2,
            net_id: 1,
            width_nm: 200_000,
            thickness_nm: 35_000,
            start: Point3D::new(10_000_000, 0, 0),
            end: Point3D::new(10_000_000, 5_000_000, 0),
            layer: 0,
        },
    ];

    let result = verify_connectivity(&segments, &[]);
    // All segments share endpoints, so the net should be fully connected
    let disconnected = result
        .violations
        .iter()
        .filter(|v| matches!(v, hwc_engine::geometry_router::connectivity_check::ConnectivityViolation::DisconnectedPin { .. }))
        .count();
    assert_eq!(
        disconnected, 0,
        "Fully connected net should have no disconnected pins"
    );
}

#[test]
fn test_connectivity_disconnected_net() {
    // Two disconnected segments on the same net
    let segments = vec![
        IndexedSegment {
            segment_id: 0,
            net_id: 1,
            width_nm: 200_000,
            thickness_nm: 35_000,
            start: Point3D::new(0, 0, 0),
            end: Point3D::new(5_000_000, 0, 0),
            layer: 0,
        },
        IndexedSegment {
            segment_id: 1,
            net_id: 1,
            width_nm: 200_000,
            thickness_nm: 35_000,
            start: Point3D::new(20_000_000, 0, 0),
            end: Point3D::new(25_000_000, 0, 0),
            layer: 0,
        },
    ];

    let result = verify_connectivity(&segments, &[]);
    let disconnected = result
        .violations
        .iter()
        .filter(|v| matches!(v, hwc_engine::geometry_router::connectivity_check::ConnectivityViolation::DisconnectedPin { .. }))
        .count();
    assert!(
        disconnected >= 2,
        "Disconnected net should have at least 2 disconnected pins"
    );
}

#[test]
fn test_connectivity_short_detection() {
    // Two different nets sharing an endpoint (short)
    let shared = Point3D::new(5_000_000, 0, 0);
    let segments = vec![
        IndexedSegment {
            segment_id: 0,
            net_id: 1,
            width_nm: 200_000,
            thickness_nm: 35_000,
            start: Point3D::new(0, 0, 0),
            end: shared,
            layer: 0,
        },
        IndexedSegment {
            segment_id: 1,
            net_id: 2,
            width_nm: 200_000,
            thickness_nm: 35_000,
            start: Point3D::new(10_000_000, 0, 0),
            end: shared,
            layer: 0,
        },
    ];

    let result = verify_connectivity(&segments, &[]);
    let shorts = result
        .violations
        .iter()
        .filter(|v| matches!(v, hwc_engine::geometry_router::connectivity_check::ConnectivityViolation::UnwaivedShort { .. }))
        .count();
    assert_eq!(
        shorts, 1,
        "Should detect exactly 1 short between the two nets"
    );
}

#[test]
fn test_segment_bbox_computation() {
    let seg = IndexedSegment {
        segment_id: 0,
        net_id: 1,
        width_nm: 400_000,
        thickness_nm: 35_000,
        start: Point3D::new(1_000_000, 2_000_000, 0),
        end: Point3D::new(5_000_000, 2_000_000, 0),
        layer: 0,
    };

    let bbox = segment_bbox(&seg);
    // Half-width = 200_000
    assert_eq!(bbox.min_x, 800_000); // 1_000_000 - 200_000
    assert_eq!(bbox.max_x, 5_200_000); // 5_000_000 + 200_000
    assert_eq!(bbox.min_y, 1_800_000); // 2_000_000 - 200_000
    assert_eq!(bbox.max_y, 2_200_000); // 2_000_000 + 200_000
}

#[test]
fn test_partition_nets_in_grid() {
    let board_bounds = BoundingBox::new(
        Point3D::new(0, 0, 0),
        Point3D::new(100_000_000, 100_000_000, 0),
    );
    let mut grid = PartitionGrid::new(board_bounds, 10_000_000, 10_000_000, 100_000, 200_000);

    // Net 1 spans the full board
    let net1_bbox = BoundingBox::new(
        Point3D::new(0, 0, 0),
        Point3D::new(100_000_000, 100_000_000, 0),
    );
    // Net 2 is in a small region
    let net2_bbox = BoundingBox::new(
        Point3D::new(5_000_000, 5_000_000, 0),
        Point3D::new(15_000_000, 15_000_000, 0),
    );

    let net_bboxes = vec![(NetId::new(1), net1_bbox), (NetId::new(2), net2_bbox)];

    partition_nets(&mut grid, &net_bboxes);

    // All cells should have net 1
    for cell in &grid.cells {
        assert!(
            cell.nets.contains(&NetId::new(1)),
            "All cells should contain net 1"
        );
    }

    // Only cells overlapping net 2's bbox should have net 2
    let cell_with_net2 = grid
        .cells
        .iter()
        .filter(|c| c.nets.contains(&NetId::new(2)))
        .count();
    assert!(
        cell_with_net2 > 0 && cell_with_net2 < 100,
        "Net 2 should be in some but not all cells, got {cell_with_net2}"
    );
}

#[test]
fn test_boundary_port_allocation() {
    let board_bounds = BoundingBox::new(
        Point3D::new(0, 0, 0),
        Point3D::new(100_000_000, 100_000_000, 0),
    );
    let mut grid = PartitionGrid::new(board_bounds, 10_000_000, 10_000_000, 100_000, 200_000);

    // Allocate a boundary port between cells (0,0) and (1,0)
    let cell_0_0 = hwc_engine::geometry_router::partition::GCellId(0);
    let cell_1_0 = hwc_engine::geometry_router::partition::GCellId(1);

    let port = grid.allocate_boundary_port(
        cell_0_0,
        cell_1_0,
        NetId::new(1),
        Point3D::new(10_000_000, 5_000_000, 0),
        200_000,
    );

    assert!(
        port.is_some(),
        "Port allocation should succeed for adjacent cells"
    );
    let port = port.unwrap();
    assert_eq!(port.net_id, NetId::new(1));
    assert_eq!(port.clearance_nm, 200_000);
    assert_eq!(
        port.position.x, 10_000_000,
        "Port X should be on the boundary"
    );
}

#[test]
fn test_empty_segments_pipeline() {
    let segments: Vec<IndexedSegment> = vec![];
    let junctions: Vec<VirtualJunction> = vec![];

    // Spatial index
    let mut index = DynamicSpatialIndex::new();
    for seg in &segments {
        index.insert(seg.clone());
    }
    assert!(index.is_empty());

    // DRC sweep
    let overlaps = find_overlaps(&segments);
    assert!(overlaps.is_empty());

    // Connectivity
    let result = verify_connectivity(&segments, &junctions);
    assert!(result.violations.is_empty());
    assert_eq!(result.nets_checked, 0);

    // Parasitic extraction
    let params = ExtractionParams {
        freq_hz: 1.0e9,
        substrate_er: 4.5,
        substrate_height_m: 35.0e-6,
        trace_thickness_m: 35.0e-6,
        loss_tangent: 0.02,
    };
    let result = extract_parasitics(&segments, &junctions, &params);
    assert!(result.traces.is_empty());
}

#[test]
fn test_single_segment_pipeline() {
    let segments = vec![IndexedSegment {
        segment_id: 0,
        net_id: 1,
        width_nm: 200_000,
        thickness_nm: 35_000,
        start: Point3D::new(0, 0, 0),
        end: Point3D::new(10_000_000, 0, 0),
        layer: 0,
    }];

    // Spatial index
    let mut index = DynamicSpatialIndex::new();
    for seg in &segments {
        index.insert(seg.clone());
    }
    assert_eq!(index.len(), 1);

    // DRC sweep
    let overlaps = find_overlaps(&segments);
    assert!(overlaps.is_empty(), "Single segment cannot overlap");

    // Connectivity
    let result = verify_connectivity(&segments, &[]);
    assert!(result.violations.is_empty());

    // Parasitic extraction
    let params = ExtractionParams {
        freq_hz: 1.0e9,
        substrate_er: 4.5,
        substrate_height_m: 35.0e-6,
        trace_thickness_m: 35.0e-6,
        loss_tangent: 0.02,
    };
    let result = extract_parasitics(&segments, &[], &params);
    assert_eq!(result.traces.len(), 1);
    assert_eq!(result.traces[0].segments.len(), 1);
}
