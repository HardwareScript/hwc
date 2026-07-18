//! Integration test: Determinism validation across multiple runs.
//!
//! Verifies that the pipeline produces bit-identical results when run
//! multiple times with the same input data.

use hwc_engine::geometry::Point3D;
use hwc_engine::geometry_router::deterministic_export::{
    content_hash, export_csv_bom_deterministic, export_dxf_deterministic,
    export_spice_deterministic, sort_contours_deterministic, sort_segments_deterministic,
    sort_traces_deterministic, verify_export_deterministic,
};
use hwc_engine::geometry_router::export_isolation::{BomEntry, SpiceParams};
use hwc_engine::geometry_router::gcell_sweep::find_overlaps;
use hwc_engine::geometry_router::geometry_refinement::{refine_layer, RefinedContour};
use hwc_engine::geometry_router::spatial_index::IndexedSegment;

// ---------------------------------------------------------------------------
// Synthetic data
// ---------------------------------------------------------------------------

fn make_determinism_test_segments() -> Vec<IndexedSegment> {
    let mut segs = Vec::new();

    // Net 1: horizontal trace
    segs.push(IndexedSegment {
        segment_id: 0,
        net_id: 1,
        width_nm: 200_000,
        thickness_nm: 35_000,
        start: Point3D::new(0, 0, 0),
        end: Point3D::new(5_000_000, 0, 0),
        layer: 0,
    });

    // Net 2: vertical trace crossing net 1
    segs.push(IndexedSegment {
        segment_id: 1,
        net_id: 2,
        width_nm: 150_000,
        thickness_nm: 35_000,
        start: Point3D::new(2_500_000, -2_000_000, 0),
        end: Point3D::new(2_500_000, 2_000_000, 0),
        layer: 0,
    });

    // Net 3: diagonal-ish (L-shaped via two segments)
    segs.push(IndexedSegment {
        segment_id: 2,
        net_id: 3,
        width_nm: 100_000,
        thickness_nm: 35_000,
        start: Point3D::new(0, 3_000_000, 0),
        end: Point3D::new(4_000_000, 3_000_000, 0),
        layer: 0,
    });
    segs.push(IndexedSegment {
        segment_id: 3,
        net_id: 3,
        width_nm: 100_000,
        thickness_nm: 35_000,
        start: Point3D::new(4_000_000, 3_000_000, 0),
        end: Point3D::new(4_000_000, 7_000_000, 0),
        layer: 0,
    });

    // Net 4: on layer 1
    segs.push(IndexedSegment {
        segment_id: 4,
        net_id: 4,
        width_nm: 250_000,
        thickness_nm: 35_000,
        start: Point3D::new(1_000_000, 1_000_000, 1_400_000),
        end: Point3D::new(6_000_000, 1_000_000, 1_400_000),
        layer: 1,
    });

    // Net 5: parallel to net 4
    segs.push(IndexedSegment {
        segment_id: 5,
        net_id: 5,
        width_nm: 250_000,
        thickness_nm: 35_000,
        start: Point3D::new(1_000_000, 2_000_000, 1_400_000),
        end: Point3D::new(6_000_000, 2_000_000, 1_400_000),
        layer: 1,
    });

    segs
}

fn make_test_contours() -> Vec<RefinedContour> {
    vec![
        RefinedContour {
            outer: vec![
                (0, 0),
                (2_000_000, 0),
                (2_000_000, 2_000_000),
                (0, 2_000_000),
            ],
            holes: Vec::new(),
            area: 4_000_000_000_000,
        },
        RefinedContour {
            outer: vec![
                (5_000_000, 5_000_000),
                (7_000_000, 5_000_000),
                (7_000_000, 7_000_000),
                (5_000_000, 7_000_000),
            ],
            holes: Vec::new(),
            area: 4_000_000_000_000,
        },
        RefinedContour {
            outer: vec![
                (10_000_000, 0),
                (12_000_000, 0),
                (12_000_000, 3_000_000),
                (10_000_000, 3_000_000),
            ],
            holes: vec![vec![
                (10_500_000, 500_000),
                (11_500_000, 500_000),
                (11_500_000, 2_500_000),
                (10_500_000, 2_500_000),
            ]],
            area: 6_000_000_000_000 - 1_000_000_000_000,
        },
    ]
}

// ---------------------------------------------------------------------------
// Tests: DXF Export Determinism
// ---------------------------------------------------------------------------

#[test]
fn test_dxf_export_byte_identical_10_runs() {
    let contours = make_test_contours();
    let layer_names = std::collections::HashMap::new();

    let outputs: Vec<String> = (0..10)
        .map(|_| export_dxf_deterministic(&contours, &layer_names))
        .collect();

    let reference = &outputs[0];
    for (i, output) in outputs.iter().enumerate().skip(1) {
        assert_eq!(reference, output, "DXF output differs on run {i}");
    }

    let hash = content_hash(reference.as_bytes());
    for (i, output) in outputs.iter().enumerate().skip(1) {
        assert_eq!(
            hash,
            content_hash(output.as_bytes()),
            "Hash differs on run {i}"
        );
    }
}

#[test]
fn test_dxf_export_with_layer_names_deterministic() {
    let contours = make_test_contours();
    let mut layer_names = std::collections::HashMap::new();
    layer_names.insert(0u8, "F_COPPER".to_string());
    layer_names.insert(1u8, "B_COPPER".to_string());
    layer_names.insert(2u8, "INNER1".to_string());

    let dxf1 = export_dxf_deterministic(&contours, &layer_names);
    let dxf2 = export_dxf_deterministic(&contours, &layer_names);
    assert_eq!(dxf1, dxf2);
    assert!(dxf1.contains("F_COPPER"));
    assert!(dxf1.contains("B_COPPER"));
    assert!(dxf1.contains("INNER1"));
}

#[test]
fn test_dxf_export_deterministic_function() {
    let contours = make_test_contours();
    let layer_names = std::collections::HashMap::new();

    let result = verify_export_deterministic(|| {
        export_dxf_deterministic(&contours, &layer_names).into_bytes()
    });
    assert!(result, "DXF export must pass deterministic verification");
}

// ---------------------------------------------------------------------------
// Tests: SPICE Export Determinism
// ---------------------------------------------------------------------------

#[test]
fn test_spice_export_byte_identical_10_runs() {
    let traces = vec![
        (3, vec![(500_000, 0), (500_000, 2_000_000)]),
        (1, vec![(0, 0), (1_000_000, 0), (1_000_000, 1_000_000)]),
        (2, vec![(0, 0), (500_000, 0), (500_000, 1_000_000)]),
        (5, vec![(0, 5_000_000), (10_000_000, 5_000_000)]),
        (4, vec![(2_000_000, 0), (2_000_000, 3_000_000)]),
    ];
    let params = SpiceParams {
        substrate_er: 4.5,
        trace_thickness_m: 35e-6,
    };

    let outputs: Vec<String> = (0..10)
        .map(|_| export_spice_deterministic(&traces, &params))
        .collect();

    let reference = &outputs[0];
    for (i, output) in outputs.iter().enumerate().skip(1) {
        assert_eq!(reference, output, "SPICE output differs on run {i}");
    }
}

#[test]
fn test_spice_export_sorted_by_net_id() {
    let traces = vec![
        (5, vec![(0, 0), (1, 0)]),
        (2, vec![(0, 0), (1, 0)]),
        (1, vec![(0, 0), (1, 0)]),
        (4, vec![(0, 0), (1, 0)]),
        (3, vec![(0, 0), (1, 0)]),
    ];
    let params = SpiceParams {
        substrate_er: 4.5,
        trace_thickness_m: 35e-6,
    };

    let spice = export_spice_deterministic(&traces, &params);

    // Verify R elements appear in net_id order
    let r1_pos = spice.find("R1  ").unwrap_or(0);
    let r2_pos = spice.find("R2  ").unwrap_or(0);
    let r3_pos = spice.find("R3  ").unwrap_or(0);
    let r4_pos = spice.find("R4  ").unwrap_or(0);
    let r5_pos = spice.find("R5  ").unwrap_or(0);

    assert!(r1_pos < r2_pos, "R1 before R2");
    assert!(r2_pos < r3_pos, "R2 before R3");
    assert!(r3_pos < r4_pos, "R3 before R4");
    assert!(r4_pos < r5_pos, "R4 before R5");
}

#[test]
fn test_spice_export_coupling_sorted() {
    let traces = vec![
        (3, vec![(0, 0), (1, 0)]),
        (1, vec![(0, 0), (1, 0)]),
        (2, vec![(0, 0), (1, 0)]),
    ];
    let params = SpiceParams {
        substrate_er: 4.5,
        trace_thickness_m: 35e-6,
    };

    let spice = export_spice_deterministic(&traces, &params);

    let k12 = spice.find("K_coupling_1_2").unwrap_or(0);
    let k13 = spice.find("K_coupling_1_3").unwrap_or(0);
    let k23 = spice.find("K_coupling_2_3").unwrap_or(0);

    assert!(k12 < k13, "K_1_2 before K_1_3");
    assert!(k13 < k23, "K_1_3 before K_2_3");
}

#[test]
fn test_spice_export_deterministic_function() {
    let traces = vec![
        (1, vec![(0, 0), (1_000_000, 0)]),
        (2, vec![(0, 0), (0, 1_000_000)]),
    ];
    let params = SpiceParams {
        substrate_er: 4.5,
        trace_thickness_m: 35e-6,
    };

    let result =
        verify_export_deterministic(|| export_spice_deterministic(&traces, &params).into_bytes());
    assert!(result, "SPICE export must pass deterministic verification");
}

// ---------------------------------------------------------------------------
// Tests: CSV BOM Export Determinism
// ---------------------------------------------------------------------------

#[test]
fn test_csv_bom_byte_identical_10_runs() {
    let entries = vec![
        BomEntry {
            ref_des: "R1".to_string(),
            value: "10k".to_string(),
            footprint: "0402".to_string(),
            quantity: 1,
        },
        BomEntry {
            ref_des: "C1".to_string(),
            value: "100nF".to_string(),
            footprint: "0603".to_string(),
            quantity: 5,
        },
        BomEntry {
            ref_des: "U1".to_string(),
            value: "STM32F103".to_string(),
            footprint: "LQFP48".to_string(),
            quantity: 1,
        },
    ];

    let outputs: Vec<String> = (0..10)
        .map(|_| export_csv_bom_deterministic(&mut entries.clone()))
        .collect();

    let reference = &outputs[0];
    for (i, output) in outputs.iter().enumerate().skip(1) {
        assert_eq!(reference, output, "CSV BOM output differs on run {i}");
    }
}

#[test]
fn test_csv_bom_sorted_deterministic() {
    let entries = vec![
        BomEntry {
            ref_des: "Z1".to_string(),
            value: "10k".to_string(),
            footprint: "0402".to_string(),
            quantity: 1,
        },
        BomEntry {
            ref_des: "A1".to_string(),
            value: "10k".to_string(),
            footprint: "0402".to_string(),
            quantity: 1,
        },
        BomEntry {
            ref_des: "M1".to_string(),
            value: "10k".to_string(),
            footprint: "0402".to_string(),
            quantity: 1,
        },
    ];

    let csv = export_csv_bom_deterministic(&mut entries.clone());
    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(lines[0], "Reference,Value,Footprint,Quantity");
    assert!(lines[1].starts_with("A1,"));
    assert!(lines[2].starts_with("M1,"));
    assert!(lines[3].starts_with("Z1,"));
}

#[test]
fn test_csv_bom_deterministic_function() {
    let entries = vec![BomEntry {
        ref_des: "R1".to_string(),
        value: "10k".to_string(),
        footprint: "0402".to_string(),
        quantity: 1,
    }];

    let result = verify_export_deterministic(|| {
        export_csv_bom_deterministic(&mut entries.clone()).into_bytes()
    });
    assert!(
        result,
        "CSV BOM export must pass deterministic verification"
    );
}

// ---------------------------------------------------------------------------
// Tests: Content Hash Determinism
// ---------------------------------------------------------------------------

#[test]
fn test_content_hash_deterministic_across_runs() {
    let data = b"test data for determinism validation";
    let hashes: Vec<[u8; 32]> = (0..100).map(|_| content_hash(data)).collect();

    for (i, hash) in hashes.iter().enumerate().skip(1) {
        assert_eq!(hashes[0], *hash, "Content hash differs on iteration {i}");
    }
}

#[test]
fn test_content_hash_different_inputs() {
    let data1 = b"input A";
    let data2 = b"input B";
    let hash1 = content_hash(data1);
    let hash2 = content_hash(data2);
    assert_ne!(
        hash1, hash2,
        "Different inputs must produce different hashes"
    );
}

#[test]
fn test_content_hash_empty_input() {
    let hash1 = content_hash(b"");
    let hash2 = content_hash(b"");
    assert_eq!(hash1, hash2, "Empty input hash must be deterministic");
}

// ---------------------------------------------------------------------------
// Tests: Sorting Determinism
// ---------------------------------------------------------------------------

#[test]
fn test_sort_segments_deterministic_idempotent() {
    let mut segs = make_determinism_test_segments();

    sort_segments_deterministic(&mut segs);
    let after_first = segs.clone();

    sort_segments_deterministic(&mut segs);
    assert_eq!(after_first, segs, "Deterministic sort must be idempotent");
}

#[test]
fn test_sort_contours_deterministic_idempotent() {
    let mut contours: Vec<Vec<(i64, i64)>> = vec![
        vec![(500, 500), (600, 500), (600, 600), (500, 600)],
        vec![(100, 100), (200, 100), (200, 200), (100, 200)],
        vec![(300, 300), (400, 300), (400, 400), (300, 400)],
    ];

    sort_contours_deterministic(&mut contours);
    let after_first = contours.clone();

    sort_contours_deterministic(&mut contours);
    assert_eq!(after_first, contours, "Contour sort must be idempotent");
}

#[test]
fn test_sort_traces_deterministic_idempotent() {
    let mut traces = vec![
        (3, vec![(0, 0), (1, 0)]),
        (1, vec![(0, 0), (1, 0)]),
        (2, vec![(0, 0), (1, 0)]),
    ];

    sort_traces_deterministic(&mut traces);
    let after_first = traces.clone();

    sort_traces_deterministic(&mut traces);
    assert_eq!(after_first, traces, "Trace sort must be idempotent");
}

#[test]
fn test_sort_segments_deterministic_input_independence() {
    let mut segs1 = make_determinism_test_segments();
    let mut segs2 = make_determinism_test_segments();
    segs2.reverse(); // Reverse input order

    sort_segments_deterministic(&mut segs1);
    sort_segments_deterministic(&mut segs2);
    assert_eq!(
        segs1, segs2,
        "Deterministic sort must produce same output regardless of input order"
    );
}

// ---------------------------------------------------------------------------
// Tests: Overlap Detection Determinism
// ---------------------------------------------------------------------------

#[test]
fn test_overlap_detection_deterministic() {
    let segs = make_determinism_test_segments();

    let overlaps1 = find_overlaps(&segs);
    let overlaps2 = find_overlaps(&segs);
    assert_eq!(
        overlaps1, overlaps2,
        "Overlap detection must be deterministic"
    );
}

#[test]
fn test_overlap_detection_input_independence() {
    let segs1 = make_determinism_test_segments();
    let mut segs2 = make_determinism_test_segments();
    segs2.reverse();
    segs2.iter_mut().for_each(|s| {
        s.segment_id += 100; // Different IDs
    });

    // Overlaps should be the same pairs regardless of segment ID numbering
    let overlaps1 = find_overlaps(&segs1);
    let overlaps2 = find_overlaps(&segs2);

    // The actual pairs may differ in segment_id values, but count should match
    assert_eq!(
        overlaps1.len(),
        overlaps2.len(),
        "Overlap count must be independent of segment ID ordering"
    );
}

// ---------------------------------------------------------------------------
// Tests: Geometry Refinement Determinism
// ---------------------------------------------------------------------------

#[test]
fn test_refine_layer_deterministic() {
    let shapes = vec![
        vec![(0, 0), (2000, 0), (2000, 2000), (0, 2000)],
        vec![(1000, 1000), (3000, 1000), (3000, 3000), (1000, 3000)],
    ];

    let result1 = refine_layer(shapes.clone(), 100);
    let result2 = refine_layer(shapes, 100);
    assert_eq!(result1.len(), result2.len());

    for (i, (a, b)) in result1.iter().zip(result2.iter()).enumerate() {
        assert_eq!(a.outer, b.outer, "Contour {i} outer ring differs");
        assert_eq!(a.holes, b.holes, "Contour {i} holes differ");
    }
}

// ---------------------------------------------------------------------------
// Tests: Empty and Edge Cases Determinism
// ---------------------------------------------------------------------------

#[test]
fn test_empty_input_deterministic() {
    let empty_contours: Vec<RefinedContour> = vec![];
    let layer_names = std::collections::HashMap::new();

    let dxf1 = export_dxf_deterministic(&empty_contours, &layer_names);
    let dxf2 = export_dxf_deterministic(&empty_contours, &layer_names);
    assert_eq!(dxf1, dxf2);

    let empty_traces: Vec<(u32, Vec<(i64, i64)>)> = vec![];
    let params = SpiceParams {
        substrate_er: 4.5,
        trace_thickness_m: 35e-6,
    };
    let spice1 = export_spice_deterministic(&empty_traces, &params);
    let spice2 = export_spice_deterministic(&empty_traces, &params);
    assert_eq!(spice1, spice2);

    let mut empty_entries: Vec<BomEntry> = vec![];
    let csv1 = export_csv_bom_deterministic(&mut empty_entries);
    let csv2 = export_csv_bom_deterministic(&mut empty_entries);
    assert_eq!(csv1, csv2);
}

#[test]
fn test_single_element_deterministic() {
    let contours = vec![RefinedContour {
        outer: vec![(0, 0), (1000, 0), (1000, 1000), (0, 1000)],
        holes: Vec::new(),
        area: 1_000_000,
    }];
    let layer_names = std::collections::HashMap::new();

    let dxf1 = export_dxf_deterministic(&contours, &layer_names);
    let dxf2 = export_dxf_deterministic(&contours, &layer_names);
    assert_eq!(dxf1, dxf2);
    assert!(dxf1.contains("POLYLINE"));
}

#[test]
fn test_large_coordinate_deterministic() {
    let contours = vec![RefinedContour {
        outer: vec![
            (1_000_000_000, 2_000_000_000),
            (3_000_000_000, 2_000_000_000),
            (3_000_000_000, 4_000_000_000),
            (1_000_000_000, 4_000_000_000),
        ],
        holes: Vec::new(),
        area: 4_000_000_000_000_000_000,
    }];
    let layer_names = std::collections::HashMap::new();

    let dxf1 = export_dxf_deterministic(&contours, &layer_names);
    let dxf2 = export_dxf_deterministic(&contours, &layer_names);
    assert_eq!(dxf1, dxf2);

    // Verify large coordinates are preserved in output
    assert!(dxf1.contains("1000.000000"));
    assert!(dxf1.contains("2000.000000"));
    assert!(dxf1.contains("3000.000000"));
    assert!(dxf1.contains("4000.000000"));
}

#[test]
fn test_negative_coordinates_deterministic() {
    let contours = vec![RefinedContour {
        outer: vec![
            (-1_000_000, -2_000_000),
            (1_000_000, -2_000_000),
            (1_000_000, 2_000_000),
            (-1_000_000, 2_000_000),
        ],
        holes: Vec::new(),
        area: 4_000_000_000,
    }];
    let layer_names = std::collections::HashMap::new();

    let dxf1 = export_dxf_deterministic(&contours, &layer_names);
    let dxf2 = export_dxf_deterministic(&contours, &layer_names);
    assert_eq!(dxf1, dxf2);

    // Verify negative coordinates are preserved
    assert!(dxf1.contains("-1.000000"));
    assert!(dxf1.contains("-2.000000"));
}

// ---------------------------------------------------------------------------
// Tests: Cross-Format Determinism
// ---------------------------------------------------------------------------

#[test]
fn test_all_exports_simultaneously_deterministic() {
    let contours = make_test_contours();
    let layer_names = std::collections::HashMap::new();
    let params = SpiceParams {
        substrate_er: 4.5,
        trace_thickness_m: 35e-6,
    };
    let traces = vec![
        (1, vec![(0, 0), (2_000_000, 0)]),
        (2, vec![(5_000_000, 5_000_000), (7_000_000, 5_000_000)]),
    ];
    let mut bom_entries = vec![BomEntry {
        ref_des: "R1".to_string(),
        value: "10k".to_string(),
        footprint: "0402".to_string(),
        quantity: 1,
    }];

    // Run all exports 5 times
    for run in 0..5 {
        let dxf = export_dxf_deterministic(&contours, &layer_names);
        let spice = export_spice_deterministic(&traces, &params);
        let csv = export_csv_bom_deterministic(&mut bom_entries);

        // Compare with first run
        if run > 0 {
            let dxf_ref = export_dxf_deterministic(&contours, &layer_names);
            let spice_ref = export_spice_deterministic(&traces, &params);
            let csv_ref = export_csv_bom_deterministic(&mut bom_entries);
            assert_eq!(dxf, dxf_ref, "DXF differs on run {run}");
            assert_eq!(spice, spice_ref, "SPICE differs on run {run}");
            assert_eq!(csv, csv_ref, "CSV differs on run {run}");
        }
    }
}
