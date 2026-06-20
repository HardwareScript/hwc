//! Export Isolation Layer (Roadmap 5.5)
//!
//! Separates pristine vector data from rendering by providing format-specific
//! serializers. Each export function generates its output on-the-fly and discards
//! intermediate mesh data from memory after serialization.
//!
//! Supported formats: DXF (2D), GLB (3D mesh), SPICE netlist, CSV BOM.
//!
//! All coordinates use i64 nanometers in the core path. f64 is only used
//! in format-specific output (SPICE parameters, GLB vertex conversion).

use std::collections::HashMap;

use crate::geometry_router::geometry_refinement::{
    self, RefinedContour,
};

// ---------------------------------------------------------------------------
// Format enum
// ---------------------------------------------------------------------------

/// Supported export formats.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExportFormat {
    /// GL Transmission Format (binary mesh).
    Glb,
    /// AutoCAD DXF (2D polyline).
    Dxf,
    /// SPICE netlist.
    Spice,
    /// CSV bill of materials.
    CsvBom,
}

// ---------------------------------------------------------------------------
// DXF export
// ---------------------------------------------------------------------------

/// Maps layer IDs (u8) to human-readable layer names.
#[derive(Clone, Debug, Default)]
pub struct LayerMap {
    pub layer_names: HashMap<u8, String>,
}

/// Export contours as 2D DXF with POLYLINE entities.
///
/// Each contour becomes a closed POLYLINE with vertex records.
/// Holes are emitted as separate closed polylines on the same layer.
pub fn export_dxf(contours: &[RefinedContour], layers: &LayerMap) -> String {
    let mut out = String::with_capacity(4096);

    // DXF header
    out.push_str("0\nSECTION\n2\nHEADER\n");
    out.push_str("0\nENDSEC\n");

    // Tables section (minimal)
    out.push_str("0\nSECTION\n2\nTABLES\n");
    out.push_str("0\nTABLE\n2\nLAYER\n70\n1\n");
    out.push_str("0\nLAYER\n2\nCOPPER\n70\n0\n62\n1\n6\nCONTINUOUS\n");
    out.push_str("0\nENDTAB\n");
    out.push_str("0\nENDSEC\n");

    // Entities section
    out.push_str("0\nSECTION\n2\nENTITIES\n");

    for (i, contour) in contours.iter().enumerate() {
        let layer_name = layers
            .layer_names
            .get(&(i as u8))
            .map(String::as_str)
            .unwrap_or("COPPER");

        // Outer contour as POLYLINE
        write_polyline(&mut out, &contour.outer, layer_name);

        // Holes as separate closed polylines
        for hole in &contour.holes {
            write_polyline(&mut out, hole, layer_name);
        }
    }

    out.push_str("0\nENDSEC\n");
    out.push_str("0\nEOF\n");
    out
}

/// Write a single closed POLYLINE entity to the DXF string.
fn write_polyline(out: &mut String, ring: &[(i64, i64)], layer: &str) {
    if ring.len() < 2 {
        return;
    }

    out.push_str("0\nPOLYLINE\n");
    out.push_str("8\n");
    out.push_str(layer);
    out.push('\n');
    out.push_str("66\n1\n"); // Vertices follow
    out.push_str("70\n1\n"); // Closed polyline

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

/// Convert nanometers to millimeters as a string (fixed-point safe).
#[inline]
fn nm_to_mm_string(nm: i64) -> String {
    let whole = nm / 1_000_000;
    let rem = (nm % 1_000_000).abs();
    format!("{whole}.{rem:06}")
}

// ---------------------------------------------------------------------------
// GLB export
// ---------------------------------------------------------------------------

/// Export contours as a GLB binary mesh.
///
/// Triangulates contours, extrudes to 3D by duplicating top/bottom faces,
/// and generates the GLB binary format (magic 0x46546C67, version 2).
/// Mesh data is discarded from memory after export.
pub fn export_glb(contours: &[RefinedContour], extrude_height_nm: i64) -> Vec<u8> {
    // Triangulate all contours on-the-fly
    let triangles_2d = geometry_refinement::triangulate_all(contours);

    // Generate 3D mesh by extruding: duplicate bottom face (z=0) and top face (z=height)
    let h = extrude_height_nm as f32;
    let mut vertices: Vec<f32> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    // Bottom face vertices (z=0)
    for tri in &triangles_2d {
        let base = (vertices.len() / 3) as u32;
        for &(x, y) in &tri.vertices {
            vertices.push(x as f32);
            vertices.push(y as f32);
            vertices.push(0.0);
        }
        indices.push(base);
        indices.push(base + 1);
        indices.push(base + 2);
    }

    // Top face vertices (z=h) — offset index
    let top_offset = (vertices.len() / 3) as u32;
    for tri in &triangles_2d {
        for &(x, y) in &tri.vertices {
            vertices.push(x as f32);
            vertices.push(y as f32);
            vertices.push(h);
        }
    }
    // Top face indices (reversed winding for outward normals)
    for i in 0..triangles_2d.len() {
        let base = top_offset + (i as u32) * 3;
        indices.push(base);
        indices.push(base + 2);
        indices.push(base + 1);
    }

    // Side faces: connect corresponding edges of bottom and top
    // For simplicity, add degenerate side triangles from the triangle edges
    for tri in &triangles_2d {
        let tri_idx = triangles_2d.iter().position(|t| std::ptr::eq(t, tri)).unwrap_or(0);
        let bottom_base = tri_idx as u32 * 3;
        let top_base = top_offset + tri_idx as u32 * 3;
        // Emit 6 side triangles (2 per edge of the triangle)
        for j in 0..3u32 {
            let b0 = bottom_base + j;
            let b1 = bottom_base + (j + 1) % 3;
            let t0 = top_base + j;
            let t1 = top_base + (j + 1) % 3;
            indices.push(b0);
            indices.push(b1);
            indices.push(t0);
            indices.push(t1);
            indices.push(t0);
            indices.push(b1);
        }
    }

    // Build GLB binary
    build_glb(&vertices, &indices)
}

/// Build a minimal GLB v2 binary file.
///
/// Structure: 12-byte header + JSON chunk + BIN chunk.
fn build_glb(vertices: &[f32], indices: &[u32]) -> Vec<u8> {
    let vertex_data = f32_slice_to_le_bytes(vertices);
    let index_data = u32_slice_to_le_bytes(indices);

    // Build accessor/buffer views for the JSON chunk
    let vertex_count = vertices.len() / 3;
    let index_count = indices.len();

    let json_chunk_length = compute_json_chunk_size(vertex_count, index_count);
    let bin_chunk_length = vertex_data.len() + index_data.len();

    // Pad bin chunk to 4-byte boundary
    let bin_chunk_length_padded = (bin_chunk_length + 3) & !3;
    let total_length = 12 + 8 + json_chunk_length + 8 + bin_chunk_length_padded;

    let mut out = Vec::with_capacity(total_length);

    // Header: magic (0x46546C67 = "glTF"), version 2, total length
    out.extend_from_slice(&0x46546C67u32.to_le_bytes());
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total_length as u32).to_le_bytes());

    // JSON chunk
    let json = build_glb_json(vertex_count, index_count, vertex_data.len(), index_data.len());
    out.extend_from_slice(&(json.len() as u32).to_le_bytes());
    out.extend_from_slice(&0x4E4F534Au32.to_le_bytes()); // "JSON"
    out.extend_from_slice(json.as_bytes());

    // BIN chunk
    out.extend_from_slice(&(bin_chunk_length_padded as u32).to_le_bytes());
    out.extend_from_slice(&0x004E4942u32.to_le_bytes()); // "BIN\0"
    out.extend_from_slice(&vertex_data);
    out.extend_from_slice(&index_data);
    // Pad to 4-byte boundary
    let padding = bin_chunk_length_padded - bin_chunk_length;
    for _ in 0..padding {
        out.push(0);
    }

    out
}

/// Compute the approximate JSON chunk size for GLB.
fn compute_json_chunk_size(vertex_count: usize, index_count: usize) -> usize {
    // Rough upper bound for the JSON template
    let base = 800;
    let per_vertex = 20;
    let per_index = 10;
    base + vertex_count * per_vertex + index_count * per_index
}

/// Build the GLB JSON chunk (minimal glTF 2.0).
fn build_glb_json(
    vertex_count: usize,
    index_count: usize,
    vertex_byte_len: usize,
    index_byte_len: usize,
) -> String {
    let vertex_offset = 0;
    let index_offset = vertex_byte_len;

    format!(
        r#"{{
  "asset": {{"version": "2.0", "generator": "hwc-engine"}},
  "scene": 0,
  "scenes": [{{"nodes": [0]}}],
  "nodes": [{{"mesh": 0}}],
  "meshes": [{{
    "primitives": [{{
      "attributes": {{"POSITION": 0}},
      "indices": 1,
      "mode": 4
    }}]
  }}]],
  "accessors": [
    {{
      "bufferView": 0,
      "componentType": 5126,
      "count": {vertex_count},
      "type": "VEC3",
      "max": [1e10, 1e10, 1e10],
      "min": [-1e10, -1e10, -1e10]
    }},
    {{
      "bufferView": 1,
      "componentType": 5125,
      "count": {index_count},
      "type": "SCALAR"
    }}
  ],
  "bufferViews": [
    {{
      "buffer": 0,
      "byteOffset": {vertex_offset},
      "byteLength": {vertex_byte_len},
      "target": 34962
    }},
    {{
      "buffer": 0,
      "byteOffset": {index_offset},
      "byteLength": {index_byte_len},
      "target": 34963
    }}
  ],
  "buffers": [{{
    "byteLength": {total_bytes}
  }}]
}}"#,
        vertex_count = vertex_count,
        index_count = index_count,
        vertex_offset = vertex_offset,
        vertex_byte_len = vertex_byte_len,
        index_offset = index_offset,
        index_byte_len = index_byte_len,
        total_bytes = vertex_byte_len + index_byte_len,
    )
}

#[inline]
fn f32_slice_to_le_bytes(slice: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(slice.len() * 4);
    for &v in slice {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    bytes
}

#[inline]
fn u32_slice_to_le_bytes(slice: &[u32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(slice.len() * 4);
    for &v in slice {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    bytes
}

// ---------------------------------------------------------------------------
// SPICE netlist export
// ---------------------------------------------------------------------------

/// Parameters for SPICE netlist generation.
#[derive(Clone, Debug)]
pub struct SpiceParams {
    /// Substrate relative permittivity (εr).
    pub substrate_er: f64,
    /// Trace thickness in meters.
    pub trace_thickness_m: f64,
}

/// Export traces as a SPICE subcircuit netlist.
///
/// Each trace segment is modeled as an R/C element with geometry-based values.
/// Trace resistance: R = ρ * L / (W * T)
/// Trace capacitance: C = ε0 * εr * L * W / d
pub fn export_spice_netlist(
    traces: &[(u32, Vec<(i64, i64)>)],
    params: &SpiceParams,
) -> String {
    let mut out = String::with_capacity(2048);
    let e0 = 8.854_187_812_8e-12; // vacuum permittivity

    out.push_str("* HWC Auto-Generated SPICE Netlist\n");
    out.push_str(".SUBCKT PCB_BOARD\n");

    for (net_id, points) in traces {
        if points.len() < 2 {
            continue;
        }

        // Calculate total trace length in meters
        let mut total_length_nm: i64 = 0;
        for w in points.windows(2) {
            let dx = w[1].0 - w[0].0;
            let dy = w[1].1 - w[0].1;
            total_length_nm += (dx * dx + dy * dy as i64).isqrt();
            // Approximate Manhattan distance for non-diagonal segments
            total_length_nm += dx.abs() + dy.abs();
        }

        let length_m = total_length_nm as f64 * 1e-9;
        let width_m = 200e-6; // Default trace width 200μm
        let thickness_m = params.trace_thickness_m;

        // Copper resistivity: ~1.68e-8 Ω·m
        let rho = 1.68e-8_f64;
        let resistance = rho * length_m / (width_m * thickness_m);

        // Capacitance to ground plane
        let substrate_distance_m = 50e-6; // Default 50μm dielectric
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

    out.push_str(".ENDS PCB_BOARD\n");
    out
}

// ---------------------------------------------------------------------------
// CSV BOM export
// ---------------------------------------------------------------------------

/// A bill-of-materials entry.
#[derive(Clone, Debug)]
pub struct BomEntry {
    pub ref_des: String,
    pub value: String,
    pub footprint: String,
    pub quantity: u32,
}

/// Export a bill of materials as CSV with header row.
pub fn export_csv_bom(components: &[BomEntry]) -> String {
    let mut out = String::with_capacity(1024);
    out.push_str("Reference,Value,Footprint,Quantity\n");

    for entry in components {
        out.push_str(&format!(
            "{},{},{},{}\n",
            escape_csv(&entry.ref_des),
            escape_csv(&entry.value),
            escape_csv(&entry.footprint),
            entry.quantity,
        ));
    }

    out
}

/// Escape a CSV field (wrap in quotes if it contains commas or quotes).
#[inline]
fn escape_csv(field: &str) -> String {
    if field.contains(',') || field.contains('"') || field.contains('\n') {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

// ---------------------------------------------------------------------------
// Export orchestrator
// ---------------------------------------------------------------------------

/// Options for the export orchestrator.
#[derive(Clone, Debug, Default)]
pub struct ExportOptions {
    pub output_path: String,
    pub extrude_height_nm: Option<i64>,
    pub layer_map: Option<LayerMap>,
}

/// Result of an export operation.
#[derive(Clone, Debug)]
pub struct ExportResult {
    pub format: ExportFormat,
    pub data: Vec<u8>,
    pub file_size: usize,
}

/// Dispatch to the appropriate format-specific export function.
///
/// Returns the serialized data as bytes along with metadata.
pub fn export(
    contours: &[RefinedContour],
    format: ExportFormat,
    options: &ExportOptions,
) -> ExportResult {
    let data = match &format {
        ExportFormat::Dxf => {
            let layers = options.layer_map.clone().unwrap_or_default();
            let dxf_str = export_dxf(contours, &layers);
            dxf_str.into_bytes()
        }
        ExportFormat::Glb => {
            let height = options.extrude_height_nm.unwrap_or(1_400_000); // 1.4mm default
            export_glb(contours, height)
        }
        ExportFormat::Spice => {
            // SPICE needs trace data, not contours — return empty with header
            let header = "* HWC SPICE Export (no trace data provided)\n";
            header.as_bytes().to_vec()
        }
        ExportFormat::CsvBom => {
            // BOM needs component data — return empty with header
            b"Reference,Value,Footprint,Quantity\n".to_vec()
        }
    };

    let file_size = data.len();
    ExportResult {
        format,
        data,
        file_size,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_rect_contour(x0: i64, y0: i64, x1: i64, y1: i64) -> RefinedContour {
        RefinedContour {
            outer: vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1)],
            holes: Vec::new(),
            area: ((x1 - x0) * (y1 - y0)) as i128,
        }
    }

    #[test]
    fn test_export_dxf_contains_polyline() {
        let contours = vec![make_rect_contour(0, 0, 1_000_000, 1_000_000)];
        let dxf = export_dxf(&contours, &LayerMap::default());
        assert!(dxf.contains("POLYLINE"));
        assert!(dxf.contains("VERTEX"));
        assert!(dxf.contains("SEQEND"));
        assert!(dxf.contains("0\nEOF\n"));
    }

    #[test]
    fn test_export_dxf_with_layer_names() {
        let contours = vec![make_rect_contour(0, 0, 1_000_000, 1_000_000)];
        let mut layers = LayerMap::default();
        layers.layer_names.insert(0, "F_COPPER".to_string());
        let dxf = export_dxf(&contours, &layers);
        assert!(dxf.contains("F_COPPER"));
    }

    #[test]
    fn test_export_glb_magic_bytes() {
        let contours = vec![make_rect_contour(0, 0, 1_000_000, 1_000_000)];
        let glb = export_glb(&contours, 1_400_000);
        assert!(glb.len() >= 12);
        // Magic: 0x46546C67 = "glTF"
        assert_eq!(glb[0], 0x67); // 'g'
        assert_eq!(glb[1], 0x6C); // 'l'
        assert_eq!(glb[2], 0x54); // 'T'
        assert_eq!(glb[3], 0x46); // 'F'
        // Version: 2
        assert_eq!(glb[4], 2);
        assert_eq!(glb[5], 0);
        assert_eq!(glb[6], 0);
        assert_eq!(glb[7], 0);
    }

    #[test]
    fn test_export_glb_has_json_chunk() {
        let contours = vec![make_rect_contour(0, 0, 1_000_000, 1_000_000)];
        let glb = export_glb(&contours, 1_400_000);
        // JSON chunk starts at offset 12, magic "JSON" at offset 16
        assert_eq!(glb[16], 0x4A); // 'J'
        assert_eq!(glb[17], 0x53); // 'S'
        assert_eq!(glb[18], 0x4F); // 'O'
        assert_eq!(glb[19], 0x4E); // 'N'
    }

    #[test]
    fn test_export_spice_netlist_valid() {
        let traces = vec![
            (1, vec![(0, 0), (1_000_000, 0), (1_000_000, 1_000_000)]),
            (2, vec![(500_000, 0), (500_000, 2_000_000)]),
        ];
        let params = SpiceParams {
            substrate_er: 4.5,
            trace_thickness_m: 35e-6,
        };
        let spice = export_spice_netlist(&traces, &params);
        assert!(spice.contains(".SUBCKT PCB_BOARD"));
        assert!(spice.contains(".ENDS PCB_BOARD"));
        assert!(spice.contains("R1"));
        assert!(spice.contains("C1"));
        assert!(spice.contains("R2"));
        assert!(spice.contains("C2"));
    }

    #[test]
    fn test_export_spice_empty_traces() {
        let spice = export_spice_netlist(&[], &SpiceParams { substrate_er: 4.5, trace_thickness_m: 35e-6 });
        assert!(spice.contains(".SUBCKT PCB_BOARD"));
        assert!(spice.contains(".ENDS PCB_BOARD"));
    }

    #[test]
    fn test_export_csv_bom() {
        let components = vec![
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
        let csv = export_csv_bom(&components);
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines[0], "Reference,Value,Footprint,Quantity");
        assert_eq!(lines[1], "R1,10k,0402,1");
        assert_eq!(lines[2], "C1,100nF,0603,5");
    }

    #[test]
    fn test_export_csv_bom_escapes_commas() {
        let components = vec![BomEntry {
            ref_des: "U1".to_string(),
            value: "10k,1%".to_string(),
            footprint: "0402".to_string(),
            quantity: 1,
        }];
        let csv = export_csv_bom(&components);
        assert!(csv.contains("\"10k,1%\""));
    }

    #[test]
    fn test_export_csv_bom_empty() {
        let csv = export_csv_bom(&[]);
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines[0], "Reference,Value,Footprint,Quantity");
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn test_export_dispatcher_dxf() {
        let contours = vec![make_rect_contour(0, 0, 1_000_000, 1_000_000)];
        let options = ExportOptions::default();
        let result = export(&contours, ExportFormat::Dxf, &options);
        assert_eq!(result.format, ExportFormat::Dxf);
        assert!(result.file_size > 0);
        let s = String::from_utf8(result.data).unwrap();
        assert!(s.contains("POLYLINE"));
    }

    #[test]
    fn test_export_dispatcher_glb() {
        let contours = vec![make_rect_contour(0, 0, 1_000_000, 1_000_000)];
        let options = ExportOptions {
            extrude_height_nm: Some(2_000_000),
            ..ExportOptions::default()
        };
        let result = export(&contours, ExportFormat::Glb, &options);
        assert_eq!(result.format, ExportFormat::Glb);
        assert!(result.file_size >= 12);
        // Check magic
        assert_eq!(result.data[0], 0x67);
    }

    #[test]
    fn test_export_dispatcher_spice() {
        let contours = vec![make_rect_contour(0, 0, 1_000_000, 1_000_000)];
        let options = ExportOptions::default();
        let result = export(&contours, ExportFormat::Spice, &options);
        assert_eq!(result.format, ExportFormat::Spice);
        let s = String::from_utf8(result.data).unwrap();
        assert!(s.contains("SPICE"));
    }

    #[test]
    fn test_export_dispatcher_csv_bom() {
        let contours = vec![];
        let options = ExportOptions::default();
        let result = export(&contours, ExportFormat::CsvBom, &options);
        assert_eq!(result.format, ExportFormat::CsvBom);
        let s = String::from_utf8(result.data).unwrap();
        assert!(s.contains("Reference,Value,Footprint,Quantity"));
    }

    #[test]
    fn test_nm_to_mm_string() {
        assert_eq!(nm_to_mm_string(0), "0.000000");
        assert_eq!(nm_to_mm_string(1_000_000), "1.000000");
        assert_eq!(nm_to_mm_string(2_500_000), "2.500000");
        assert_eq!(nm_to_mm_string(-1_000_000), "-1.000000");
    }

    #[test]
    fn test_escape_csv() {
        assert_eq!(escape_csv("simple"), "simple");
        assert_eq!(escape_csv("has,comma"), "\"has,comma\"");
        assert_eq!(escape_csv("has\"quote"), "\"has\"\"quote\"");
    }
}
