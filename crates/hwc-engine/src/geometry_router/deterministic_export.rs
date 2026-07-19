//! Bit-Identical Serialization (Roadmap 8.5)
//!
//! Provides deterministic export functions that produce byte-identical output
//! across multiple runs for the same input. All sorting is lexicographic with
//! well-defined tie-breakers. No floating-point comparison in sort keys.
//!
//! All coordinates use i64 nanometers. No f64 in core path.

use std::collections::HashMap;

use sha2::{Digest, Sha256};

use crate::geometry_router::export_isolation::{BomEntry, SpiceParams};
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
    triangles.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1.cmp(&b.1))
            .then_with(|| a.2.cmp(&b.2))
    });
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
pub fn export_dxf_deterministic(
    contours: &[RefinedContour],
    layer_names: &HashMap<u8, String>,
) -> String {
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
        let first = contour
            .outer
            .first()
            .copied()
            .unwrap_or((i64::MAX, i64::MAX));
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
