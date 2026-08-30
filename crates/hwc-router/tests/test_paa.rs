use hwc_engine::geometry::Point3D;
use hwc_router::paa::{score_access_point, PaaScoringConfig};

#[test]
fn test_paa_scoring_within_pin_bounds() {
    let config = PaaScoringConfig::default();
    let pt = Point3D::new(500_000, 500_000, 0);

    // Pin bounding box from 0 to 1_000_000 pm (1 um x 1 um)
    let ap = score_access_point(
        pt,
        0,
        true,
        0,
        1_000_000,
        0,
        1_000_000,
        &config,
    );

    assert!(ap.is_some());
    let ap = ap.unwrap();
    assert_eq!(ap.point, pt);
    assert_eq!(ap.layer_idx, 0);
    assert!(ap.score >= config.preferred_direction_bonus);
}

#[test]
fn test_paa_scoring_outside_bounds_rejected() {
    let config = PaaScoringConfig::default();
    // Point too close to edge for via landing
    let pt = Point3D::new(10_000, 10_000, 0);

    let ap = score_access_point(
        pt,
        0,
        true,
        0,
        1_000_000,
        0,
        1_000_000,
        &config,
    );

    assert!(ap.is_none());
}
