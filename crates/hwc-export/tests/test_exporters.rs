use hwc_engine::geometry::{BoundingBox, Point3D};
use hwc_export::gdsii::{GdsBoundary, GdsCutMask, GdsiiWriter};
use hwx_export_test::*;
use std::io::Cursor;

mod hwx_export_test {
    pub use hwc_export::hwx::{HwxContainer, HWX_MAGIC};
    pub use hwc_export::oasis::OasisWriter;
    pub use hwc_export::substrate::triangulate_and_extrude;
    pub use hwc_export::welder::{
        circle_to_path, rect_to_path, stroke_polyline, trace_segment_to_path, weld_copper_geometry,
    };
}

#[test]
fn test_clipper2_copper_welding_and_strokes() {
    let bbox1 = BoundingBox::new(Point3D::new(0, 0, 0), Point3D::new(1000, 1000, 0));
    let bbox2 = BoundingBox::new(Point3D::new(500, 500, 0), Point3D::new(1500, 1500, 0));

    let path1 = rect_to_path(&bbox1);
    let path2 = rect_to_path(&bbox2);

    let welded = weld_copper_geometry(&[path1, path2]);
    assert!(!welded.is_empty());

    // Test stroke polyline
    let pts = vec![
        Point3D::new(0, 0, 0),
        Point3D::new(1000, 0, 0),
        Point3D::new(1000, 1000, 0),
    ];
    let stroked = stroke_polyline(&pts, 140);
    assert!(!stroked.is_empty());

    // Test circle pad
    let circle = circle_to_path(500, 500, 200, 16);
    assert_eq!(circle.len(), 16);

    // Test trace segment
    let seg = trace_segment_to_path(Point3D::new(0, 0, 0), Point3D::new(1000, 0, 0), 140);
    assert_eq!(seg.len(), 4);
}

#[test]
fn test_earcut_substrate_triangulation() {
    let outer = vec![(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)];
    let holes = vec![vec![(20.0, 20.0), (40.0, 20.0), (40.0, 40.0), (20.0, 40.0)]];

    let mesh = triangulate_and_extrude("substrate_core", &outer, &holes, 0.0, 1.6);
    assert_eq!(mesh.name, "substrate_core");
    assert!(!mesh.vertices.is_empty());
    assert!(!mesh.triangles.is_empty());
}

#[test]
fn test_gdsii_binary_stream_writer() {
    let mut buffer = Vec::new();
    {
        let mut writer = GdsiiWriter::new(&mut buffer);
        writer.write_header(600).unwrap();
        writer.write_bgnlib().unwrap();
        writer.write_libname("HARDWARESCRIPT_LIB").unwrap();
        writer.write_units().unwrap();
        writer.write_bgnstr().unwrap();
        writer.write_strname("TOP_CELL").unwrap();

        let boundary = GdsBoundary {
            layer: 1,
            datatype: 0,
            points: vec![(0, 0), (1000, 0), (1000, 1000), (0, 1000), (0, 0)],
        };
        writer.write_boundary(&boundary).unwrap();

        // Sub-2nm cut mask polygon
        let cut_mask = GdsCutMask {
            layer: 42,
            datatype: 0,
            min_x: 100,
            min_y: 200,
            max_x: 150,
            max_y: 250,
        };
        writer.write_cut_mask(&cut_mask).unwrap();

        writer.write_endstr().unwrap();
        writer.write_endlib().unwrap();
    }

    assert!(!buffer.is_empty());
    // GDSII header first record starts with 0x0006 0x0002
    assert_eq!(buffer[0], 0x00);
    assert_eq!(buffer[1], 0x06);
    assert_eq!(buffer[2], 0x00);
    assert_eq!(buffer[3], 0x02);
}

#[test]
fn test_oasis_binary_stream_writer() {
    let mut buffer = Vec::new();
    {
        let mut writer = OasisWriter::new(&mut buffer);
        writer.write_magic_header().unwrap();
        writer.write_start("1.0").unwrap();
        writer.write_cell_start("TOP_CELL").unwrap();
        writer.write_rectangle(1, 0, 1000, 500, 0, 0).unwrap();
        writer.write_end().unwrap();
    }

    assert!(!buffer.is_empty());
    assert!(buffer.starts_with(b"%SEMI-OASIS\r\n"));
}

#[test]
fn test_hwx_round_trip_serialization() {
    let metadata = r#"{"design_name":"top_soc","version":"0.3.1"}"#.to_string();
    let payload = vec![0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE, 0xBA, 0xBE];

    let container = HwxContainer::new(metadata.clone(), payload.clone());

    let mut buf = Vec::new();
    container.write_to(&mut buf).unwrap();

    let mut cursor = Cursor::new(buf);
    let loaded = HwxContainer::read_from(&mut cursor).unwrap();

    assert_eq!(loaded.header.magic, *HWX_MAGIC);
    assert_eq!(loaded.metadata_json, metadata);
    assert_eq!(loaded.payload_bytes, payload);
}
