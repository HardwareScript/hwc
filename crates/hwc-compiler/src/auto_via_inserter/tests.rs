use hwc_engine::geometry::{BoundingBox, Point3D};

use super::{AutoViaInserter, OverlapRegion, ViaLibrary, ViaType};

#[test]
fn finds_exact_via_match() {
    let library = ViaLibrary {
        vias: vec![ViaType::new(
            "Via1_2".into(),
            "Copper".into(),
            1,
            2,
            0.3,
            0.15,
            0,
            0,
            crate::shape_generators::circle_contour(300_000, 16),
        )],
    };

    let via = library.find_via_for_layers(1, 2, false).unwrap();
    assert_eq!(via.name, "Via1_2");
}

#[test]
fn prefers_larger_spanning_via_for_power_nets() {
    let library = ViaLibrary {
        vias: vec![
            ViaType::new(
                "ThroughHoleSmall".into(),
                "Copper".into(),
                0,
                6,
                0.2,
                0.1,
                0,
                0,
                crate::shape_generators::circle_contour(200_000, 16),
            ),
            ViaType::new(
                "ThroughHoleLarge".into(),
                "Copper".into(),
                0,
                6,
                0.4,
                0.2,
                0,
                0,
                crate::shape_generators::circle_contour(400_000, 16),
            ),
        ],
    };

    let via = library.find_via_for_layers(2, 4, true).unwrap();
    assert_eq!(via.name, "ThroughHoleLarge");
}

#[test]
fn finds_overlap_region() {
    let inserter = AutoViaInserter::new();
    let bbox1 = BoundingBox::new(
        Point3D::new(0, 0, 0),
        Point3D::new(5_000_000, 5_000_000, 1_000_000),
    );
    let bbox2 = BoundingBox::new(
        Point3D::new(3_000_000, 3_000_000, 1_000_000),
        Point3D::new(8_000_000, 8_000_000, 2_000_000),
    );

    let overlap = inserter.find_overlap(&bbox1, &bbox2).unwrap();

    assert_eq!(overlap.bbox.min.x, 3_000_000);
    assert_eq!(overlap.bbox.min.y, 3_000_000);
    assert_eq!(overlap.bbox.max.x, 5_000_000);
    assert_eq!(overlap.bbox.max.y, 5_000_000);
    assert_eq!(overlap.center_x_nm, 4_000_000);
    assert_eq!(overlap.center_y_nm, 4_000_000);
}

#[test]
fn rejects_insufficient_enclosure() {
    let inserter = AutoViaInserter::new();
    let overlap = OverlapRegion {
        bbox: BoundingBox::new(Point3D::new(0, 0, 0), Point3D::new(400_000, 400_000, 0)),
        center_x_nm: 200_000,
        center_y_nm: 200_000,
    };
    let via_type = ViaType::new(
        "TestVia".into(),
        "Copper".into(),
        1,
        2,
        0.3,
        0.15,
        0,
        0,
        crate::shape_generators::circle_contour(300_000, 16),
    );

    assert!(inserter.verify_enclosure(&overlap, &via_type).is_err());
}
