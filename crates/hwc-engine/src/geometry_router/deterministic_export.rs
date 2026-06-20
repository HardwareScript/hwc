//! Bit-Identical Serialization (Roadmap 8.5)
//!
//! Provides deterministic export functions that produce byte-identical output
//! across multiple runs for the same input. All sorting is lexicographic with
//! well-defined tie-breakers. No floating-point comparison in sort keys.
//!
//! All coordinates use i64 nanometers. No f64 in core path.

use std::collections::HashMap;

use sha2::{Digest, Sha256};

use crate::geometry_router::export_isolation::{
    BomEntry, SpiceParams,
};
use crate::geometry_router::geometry_refinement::RefinedContour;
use crate::geometry_router::spatial_index::IndexedSegment;

// ---------------------------------------------------------------------------
// Sorting utilities
// ---------------------------------------------------------------------------

/// Sort indexed segments deterministically by (net_id, layer, start.x, start.y, end.x, end.y).
#[inline]
pub fn sort_segments_deterministic(segments: &mut [IndexedSegment]) {
    segments.sort_by(|a, b| {
        a.net_id
            .cmp(&b.net_id)
            .then_with(|| a.layer.cmp(&b.layer))
            .then_with(|| a.start.x.cmp(&b.start.x))
            .then_with(|| a.start.y.cmp(&b.start.y))
            .then_with(|| a.end.x.cmp(&b.end.x))
            .then_with(|| a.end.y.cmp(&b.end.y))
            .then_with(|| a.segment_id.cmp(&b.segment_id))
    });
}

/// Signed area of a 2D ring via the shoelace formula (i128).
#[inline]
fn signed_area(ring: &[(i64, i64)]) -> i128 {
    let mut area: i128 = 0;
    let len = ring.len();
    for i in 0..len {
        let (x0, y0) = ring[i];
        let (x1, y1) = ring[(i + 1) % len];
        area += i128::from(x0) * i128::from(y1);
        area -= i128::from(x1) * i128::from(y0);
    }
    area / 2
}

/// Sort contours deterministically by first point (x, then y),
/// breaking ties by signed area (CCW before CW).
#[inline]
pub fn sort_contours_deterministic(contours: &mut [Vec<(i64, i64)>]) {
    contours.sort_by(|a, b| {
        let first_a = a.first().copied().unwrap_or((i64::MAX, i64::MAX));
        let first_b = b.first().copied().unwrap_or((i64::MAX, i64::MAX));
        first_a
            .0
            .cmp(&first_b.0)
            .then_with(|| first_a.1.cmp(&first_b.1))
            .then_with(|| signed_area(a).cmp(&signed_area(b)))
    });
}

/// Sort traces deterministically by net_id.
#[inline]
pub fn sort_traces_deterministic(traces: &mut [(u32, Vec<(i64, i64)>)]) {
    traces.sort_by_key(|(net_id, _)| *net_id);
}

/// Sort mesh vertices deterministically by (x, y, z) as f32.
#[inline]
pub fn sort_mesh_vertices_deterministic(vertices: &mut [(f32, f32, f32)]) {
    vertices.sort_by(|a, b| {
        a.0.to_bits()
            .cmp(&b.0.to_bits())
            .then_with(|| a.1.to_bits().cmp(&b.1.to_bits()))
            .then_with(|| a.2.to_bits().cmp(&b.2.to_bits()))
    });
}

/// Sort triangles deterministically by (v0, v1, v2).
#[inline]
pub fn sort_triangles_deterministic(triangles: &mut [(u32, u32, u32)]) {
    triangles.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)).then_with(|| a.2.cmp(&b.2)));
}

// ---------------------------------------------------------------------------
// Deterministic DXF export
// ---------------------------------------------------------------------------

/// Convert nanometers to millimeters as a fixed-point string (deterministic).
#[inline]
fn nm_to_mm_string(nm: i64) -> String {
    let whole = nm / 1_000_000;
    let rem = (nm % 1_000_000).abs();
    format!("{whole}.{rem:06}")
}

/// Write a single closed POLYLINE entity to the DXF string.
#[inline]
fn write_polyline_deterministic(out: &mut String, ring: &[(i64, i64)], layer: &str) {
    if ring.len() < 2 {
        return;
    }

    out.push_str("0\nPOLYLINE\n");
    out.push_str("8\n");
    out.push_str(layer);
    out.push('\n');
    out.push_str("66\n1\n");
    out.push_str("70\n1\n");

    for &(x, y) in ring {
        let x_mm = nm_to_mm_string(x);
        let y_mm = nm_to_mm_string(y);
        out.push_str("0\nVERTEX\n");
        out.push_str("8\n");
        out.push_str(layer);
        out.push('\n');
        out.push_str("10\n");
        out.push_str(&x_mm);
        out.push('\n');
        out.push_str("20\n");
        out.push_str(&y_mm);
        out.push('\n');
        out.push_str("30\n0.0\n");
    }

    out.push_str("0\nSEQEND\n");
}

/// Export contours as DXF with deterministic ordering.
///
/// Contours are sorted by (layer_id, first point x, first point y) before writing.
/// Within each contour, the outer ring is written first (CCW), then holes (CW).
/// Output is byte-identical across runs for the same input.
pub fn export_dxf_deterministic(contours: &[RefinedContour], layer_names: &HashMap<u8, String>) -> String {
    let mut out = String::with_capacity(4096);

    // DXF header
    out.push_str("0\nSECTION\n2\nHEADER\n");
    out.push_str("0\nENDSEC\n");

    // Tables section
    out.push_str("0\nSECTION\n2\nTABLES\n");
    out.push_str("0\nTABLE\n2\nLAYER\n70\n1\n");
    out.push_str("0\nLAYER\n2\nCOPPER\n70\n0\n62\n1\n6\nCONTINUOUS\n");
    out.push_str("0\nENDTAB\n");
    out.push_str("0\nENDSEC\n");

    // Entities section
    out.push_str("0\nSECTION\n2\nENTITIES\n");

    // Build a tagged list: (layer_id, first_x, first_y, contour)
    let mut tagged: Vec<(u8, i64, i64, &RefinedContour)> = Vec::with_capacity(contours.len());
    for (i, contour) in contours.iter().enumerate() {
        let layer_id = (i % 256) as u8;
        let first = contour.outer.first().copied().unwrap_or((i64::MAX, i64::MAX));
        tagged.push((layer_id, first.0, first.1, contour));
    }

    // Sort deterministically
    tagged.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1.cmp(&b.1))
            .then_with(|| a.2.cmp(&b.2))
    });

    for (layer_id, _, _, contour) in &tagged {
        let layer_name = layer_names
            .get(layer_id)
            .map(String::as_str)
            .unwrap_or("COPPER");

        // Outer contour first
        write_polyline_deterministic(&mut out, &contour.outer, layer_name);

        // Holes sorted by first point
        let mut sorted_holes = contour.holes.clone();
        sort_contours_deterministic(&mut sorted_holes);
        for hole in &sorted_holes {
            write_polyline_deterministic(&mut out, hole, layer_name);
        }
    }

    out.push_str("0\nENDSEC\n");
    out.push_str("0\nEOF\n");
    out
}

// ---------------------------------------------------------------------------
// Deterministic SPICE export
// ---------------------------------------------------------------------------

/// Export traces as a SPICE subcircuit netlist with deterministic ordering.
///
/// Traces are sorted by net_id before writing. R/C values are computed from
/// geometry and are deterministic for the same input. Coupling cards are sorted
/// by the coupled net pair.
pub fn export_spice_deterministic(
    traces: &[(u32, Vec<(i64, i64)>)],
    params: &SpiceParams,
) -> String {
    let mut out = String::with_capacity(2048);
    let e0 = 8.854_187_812_8e-12;

    out.push_str("* HWC Auto-Generated SPICE Netlist\n");
    out.push_str(".SUBCKT PCB_BOARD\n");

    // Sort traces by net_id for deterministic output
    let mut sorted = traces.to_vec();
    sort_traces_deterministic(&mut sorted);

    for (net_id, points) in &sorted {
        if points.len() < 2 {
            continue;
        }

        // Calculate total trace length deterministically
        let mut total_length_nm: i64 = 0;
        for w in points.windows(2) {
            let dx = w[1].0 - w[0].0;
            let dy = w[1].1 - w[0].1;
            total_length_nm += dx.abs() + dy.abs();
        }

        let length_m = total_length_nm as f64 * 1e-9;
        let width_m = 200e-6;
        let thickness_m = params.trace_thickness_m;

        let rho = 1.68e-8_f64;
        let resistance = rho * length_m / (width_m * thickness_m);

        let substrate_distance_m = 50e-6;
        let capacitance = e0 * params.substrate_er * length_m * width_m / substrate_distance_m;

        out.push_str(&format!(
            "R{net}  n{net}  0  {res:.6e}\n",
            net = net_id,
            res = resistance,
        ));
        out.push_str(&format!(
            "C{net}  n{net}  0  {cap:.6e}\n",
            net = net_id,
            cap = capacitance,
        ));
    }

    // Coupling cards (K_coupling) sorted by coupled net pair
    let mut coupling_pairs: Vec<(u32, u32)> = Vec::new();
    for i in 0..sorted.len() {
        for j in (i + 1)..sorted.len() {
            let net_a = sorted[i].0;
            let net_b = sorted[j].0;
            if net_a != net_b {
                coupling_pairs.push((net_a.min(net_b), net_a.max(net_b)));
            }
        }
    }
    coupling_pairs.sort();
    coupling_pairs.dedup();

    for (net_a, net_b) in &coupling_pairs {
        out.push_str(&format!(
            "K_coupling_{a}_{b}  n{a}  n{b}  0.3\n",
            a = net_a,
            b = net_b,
        ));
    }

    out.push_str(".ENDS PCB_BOARD\n");
    out
}

// ---------------------------------------------------------------------------
// Deterministic CSV BOM export
// ---------------------------------------------------------------------------

/// Escape a CSV field deterministically.
#[inline]
fn escape_csv_deterministic(field: &str) -> String {
    if field.contains(',') || field.contains('"') || field.contains('\n') {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

/// Export a bill of materials as CSV with deterministic ordering.
///
/// Entries are sorted by (ref_des, value, footprint). Output is byte-identical
/// across runs for the same input.
pub fn export_csv_bom_deterministic(entries: &mut [BomEntry]) -> String {
    let mut out = String::with_capacity(1024);
    out.push_str("Reference,Value,Footprint,Quantity\n");

    entries.sort_by(|a, b| {
        a.ref_des
            .cmp(&b.ref_des)
            .then_with(|| a.value.cmp(&b.value))
            .then_with(|| a.footprint.cmp(&b.footprint))
    });

    for entry in entries {
        out.push_str(&format!(
            "{},{},{},{}\n",
            escape_csv_deterministic(&entry.ref_des),
            escape_csv_deterministic(&entry.value),
            escape_csv_deterministic(&entry.footprint),
            entry.quantity,
        ));
    }

    out
}

// ---------------------------------------------------------------------------
// Export verification
// ---------------------------------------------------------------------------

/// Compare two export outputs for byte-identical content.
#[inline]
pub fn verify_bit_identical(output1: &[u8], output2: &[u8]) -> bool {
    output1 == output2
}

/// Run an export function twice and verify the outputs are byte-identical.
pub fn verify_export_deterministic<F>(export_fn: F) -> bool
where
    F: Fn() -> Vec<u8>,
{
    let out1 = export_fn();
    let out2 = export_fn();
    verify_bit_identical(&out1, &out2)
}

// ---------------------------------------------------------------------------
// Deterministic hash for verification
// ---------------------------------------------------------------------------

/// Compute a SHA-256 content hash of the given data.
#[inline]
pub fn content_hash(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&result);
    hash
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_segment(
        segment_id: usize,
        net_id: usize,
        layer: i64,
        sx: i64,
        sy: i64,
        ex: i64,
        ey: i64,
    ) -> IndexedSegment {
        use crate::geometry::Point3D;
        IndexedSegment {
            segment_id,
            net_id,
            width_nm: 200_000,
            start: Point3D::new(sx, sy, 0),
            end: Point3D::new(ex, ey, 0),
            layer,
        }
    }

    fn make_rect_contour(x0: i64, y0: i64, x1: i64, y1: i64) -> RefinedContour {
        RefinedContour {
            outer: vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1)],
            holes: Vec::new(),
            area: ((x1 - x0) * (y1 - y0)) as i128,
        }
    }

    #[test]
    fn test_sort_segments_deterministic_consistent_order() {
        let mut segs = vec![
            make_segment(3, 2, 0, 500, 0, 600, 0),
            make_segment(0, 1, 0, 100, 0, 200, 0),
            make_segment(1, 2, 0, 300, 0, 400, 0),
            make_segment(2, 1, 0, 100, 0, 200, 0),
        ];

        let mut expected = segs.clone();
        sort_segments_deterministic(&mut expected);

        sort_segments_deterministic(&mut segs);
        assert_eq!(segs, expected);

        // Running sort again should produce identical order
        let mut segs2 = vec![
            make_segment(2, 1, 0, 100, 0, 200, 0),
            make_segment(0, 1, 0, 100, 0, 200, 0),
            make_segment(3, 2, 0, 500, 0, 600, 0),
            make_segment(1, 2, 0, 300, 0, 400, 0),
        ];
        sort_segments_deterministic(&mut segs2);
        assert_eq!(segs, segs2);
    }

    #[test]
    fn test_sort_segments_deterministic_tie_breaks() {
        // Same net_id, different layers
        let mut segs = vec![
            make_segment(0, 1, 1, 0, 0, 1, 0),
            make_segment(1, 1, 0, 0, 0, 1, 0),
        ];
        sort_segments_deterministic(&mut segs);
        assert_eq!(segs[0].layer, 0);
        assert_eq!(segs[1].layer, 1);

        // Same net_id, same layer, different start
        let mut segs = vec![
            make_segment(0, 1, 0, 200, 0, 1, 0),
            make_segment(1, 1, 0, 100, 0, 1, 0),
        ];
        sort_segments_deterministic(&mut segs);
        assert_eq!(segs[0].start.x, 100);
        assert_eq!(segs[1].start.x, 200);
    }

    #[test]
    fn test_sort_contours_deterministic_consistent_order() {
        let mut contours = vec![
            vec![(500, 500), (600, 500), (600, 600), (500, 600)],
            vec![(100, 100), (200, 100), (200, 200), (100, 200)],
            vec![(300, 300), (400, 300), (400, 400), (300, 400)],
        ];

        let mut expected = contours.clone();
        sort_contours_deterministic(&mut expected);

        sort_contours_deterministic(&mut contours);
        assert_eq!(contours, expected);

        // Reverse input order should produce same result
        let mut contours2 = vec![
            vec![(300, 300), (400, 300), (400, 400), (300, 400)],
            vec![(500, 500), (600, 500), (600, 600), (500, 600)],
            vec![(100, 100), (200, 100), (200, 200), (100, 200)],
        ];
        sort_contours_deterministic(&mut contours2);
        assert_eq!(contours, contours2);
    }

    #[test]
    fn test_export_dxf_deterministic_is_byte_identical() {
        let contours = vec![
            make_rect_contour(1000, 2000, 3000, 4000),
            make_rect_contour(0, 0, 1_000_000, 1_000_000),
        ];
        let layer_names = HashMap::new();

        let dxf1 = export_dxf_deterministic(&contours, &layer_names);
        let dxf2 = export_dxf_deterministic(&contours, &layer_names);
        let dxf3 = export_dxf_deterministic(&contours, &layer_names);

        assert_eq!(dxf1, dxf2);
        assert_eq!(dxf2, dxf3);
        assert!(dxf1.contains("POLYLINE"));
        assert!(dxf1.contains("0\nEOF\n"));
    }

    #[test]
    fn test_export_dxf_deterministic_with_layer_names() {
        let contours = vec![make_rect_contour(0, 0, 1_000_000, 1_000_000)];
        let mut layer_names = HashMap::new();
        layer_names.insert(0, "F_COPPER".to_string());
        let dxf = export_dxf_deterministic(&contours, &layer_names);
        assert!(dxf.contains("F_COPPER"));
    }

    #[test]
    fn test_export_spice_deterministic_is_byte_identical() {
        let traces = vec![
            (2, vec![(500_000, 0), (500_000, 2_000_000)]),
            (1, vec![(0, 0), (1_000_000, 0), (1_000_000, 1_000_000)]),
        ];
        let params = SpiceParams {
            substrate_er: 4.5,
            trace_thickness_m: 35e-6,
        };

        let spice1 = export_spice_deterministic(&traces, &params);
        let spice2 = export_spice_deterministic(&traces, &params);
        let spice3 = export_spice_deterministic(&traces, &params);

        assert_eq!(spice1, spice2);
        assert_eq!(spice2, spice3);
        assert!(spice1.contains(".SUBCKT PCB_BOARD"));
        assert!(spice1.contains(".ENDS PCB_BOARD"));
        assert!(spice1.contains("R1"));
        assert!(spice1.contains("R2"));
    }

    #[test]
    fn test_export_spice_deterministic_sorted_by_net_id() {
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
        let r1_pos = spice.find("R1  ").unwrap_or(0);
        let r2_pos = spice.find("R2  ").unwrap_or(0);
        let r3_pos = spice.find("R3  ").unwrap_or(0);
        assert!(r1_pos < r2_pos);
        assert!(r2_pos < r3_pos);
    }

    #[test]
    fn test_export_spice_deterministic_coupling_sorted() {
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
        let k1 = spice.find("K_coupling_1_2").unwrap_or(0);
        let k2 = spice.find("K_coupling_1_3").unwrap_or(0);
        let k3 = spice.find("K_coupling_2_3").unwrap_or(0);
        assert!(k1 < k2);
        assert!(k2 < k3);
    }

    #[test]
    fn test_export_csv_bom_deterministic_is_byte_identical() {
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
        ];

        let csv1 = export_csv_bom_deterministic(&mut entries.clone());
        let csv2 = export_csv_bom_deterministic(&mut entries.clone());
        let csv3 = export_csv_bom_deterministic(&mut entries.clone());

        assert_eq!(csv1, csv2);
        assert_eq!(csv2, csv3);
    }

    #[test]
    fn test_export_csv_bom_deterministic_sorted() {
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
    fn test_sort_mesh_vertices_deterministic() {
        let mut verts = vec![(1.0, 0.0, 0.0), (0.0, 1.0, 0.0), (0.0, 0.0, 1.0)];
        sort_mesh_vertices_deterministic(&mut verts);
        // (0,0,1) < (0,1,0) < (1,0,0) in f32 bit ordering
        assert_eq!(verts[0], (0.0, 0.0, 1.0));
        assert_eq!(verts[1], (0.0, 1.0, 0.0));
        assert_eq!(verts[2], (1.0, 0.0, 0.0));
    }

    #[test]
    fn test_sort_triangles_deterministic() {
        let mut tris = vec![(2, 1, 0), (0, 1, 2), (1, 0, 2)];
        sort_triangles_deterministic(&mut tris);
        assert_eq!(tris, vec![(0, 1, 2), (1, 0, 2), (2, 1, 0)]);
    }

    #[test]
    fn test_verify_bit_identical_passes() {
        let data = b"identical bytes";
        assert!(verify_bit_identical(data, data));

        let a = vec![1u8, 2, 3];
        let b = vec![1u8, 2, 3];
        assert!(verify_bit_identical(&a, &b));
    }

    #[test]
    fn test_verify_bit_identical_fails_for_different_data() {
        let a = vec![1u8, 2, 3];
        let b = vec![1u8, 2, 4];
        assert!(!verify_bit_identical(&a, &b));
    }

    #[test]
    fn test_verify_export_deterministic_dxf() {
        let contours = vec![make_rect_contour(0, 0, 1_000_000, 1_000_000)];
        let layer_names = HashMap::new();
        let result = verify_export_deterministic(|| {
            export_dxf_deterministic(&contours, &layer_names).into_bytes()
        });
        assert!(result);
    }

    #[test]
    fn test_verify_export_deterministic_spice() {
        let traces = vec![
            (1, vec![(0, 0), (1_000_000, 0)]),
            (2, vec![(0, 0), (0, 1_000_000)]),
        ];
        let params = SpiceParams {
            substrate_er: 4.5,
            trace_thickness_m: 35e-6,
        };
        let result = verify_export_deterministic(|| {
            export_spice_deterministic(&traces, &params).into_bytes()
        });
        assert!(result);
    }

    #[test]
    fn test_verify_export_deterministic_csv() {
        let entries = vec![
            BomEntry {
                ref_des: "R1".to_string(),
                value: "10k".to_string(),
                footprint: "0402".to_string(),
                quantity: 1,
            },
        ];
        let result = verify_export_deterministic(|| {
            export_csv_bom_deterministic(&mut entries.clone()).into_bytes()
        });
        assert!(result);
    }

    #[test]
    fn test_content_hash_deterministic() {
        let data = b"test data for hashing";
        let h1 = content_hash(data);
        let h2 = content_hash(data);
        assert_eq!(h1, h2);

        // Different data should produce different hash
        let data2 = b"different test data";
        let h3 = content_hash(data2);
        assert_ne!(h1, h3);
    }

    #[test]
    fn test_content_hash_is_32_bytes() {
        let hash = content_hash(b"anything");
        assert_eq!(hash.len(), 32);
    }

    #[test]
    fn test_nm_to_mm_string_deterministic() {
        // Ensure the helper produces consistent output
        assert_eq!(nm_to_mm_string(0), "0.000000");
        assert_eq!(nm_to_mm_string(1_000_000), "1.000000");
        assert_eq!(nm_to_mm_string(2_500_000), "2.500000");
        assert_eq!(nm_to_mm_string(-1_000_000), "-1.000000");
    }

    #[test]
    fn test_export_dxf_deterministic_empty_contours() {
        let dxf1 = export_dxf_deterministic(&[], &HashMap::new());
        let dxf2 = export_dxf_deterministic(&[], &HashMap::new());
        assert_eq!(dxf1, dxf2);
        assert!(dxf1.contains("0\nEOF\n"));
    }

    #[test]
    fn test_export_spice_deterministic_empty_traces() {
        let params = SpiceParams {
            substrate_er: 4.5,
            trace_thickness_m: 35e-6,
        };
        let spice1 = export_spice_deterministic(&[], &params);
        let spice2 = export_spice_deterministic(&[], &params);
        assert_eq!(spice1, spice2);
        assert!(spice1.contains(".SUBCKT PCB_BOARD"));
        assert!(spice1.contains(".ENDS PCB_BOARD"));
    }

    #[test]
    fn test_export_csv_bom_deterministic_empty() {
        let csv1 = export_csv_bom_deterministic(&mut []);
        let csv2 = export_csv_bom_deterministic(&mut []);
        assert_eq!(csv1, csv2);
        assert_eq!(csv1, "Reference,Value,Footprint,Quantity\n");
    }
}
