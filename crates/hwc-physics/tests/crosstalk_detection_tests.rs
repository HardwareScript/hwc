//! Task B2: Signal Integrity (Crosstalk) Sweep Tests
//!
//! Tests for dilation-based crosstalk detection using bit-counting.

use hwc_physics::EMAnalyzer;

#[test]
fn test_detect_parallel_overlap_no_overlap() {
    let analyzer = EMAnalyzer::new();

    // Two nets that don't overlap in X-Y plane
    let net_a = vec![(0, 0, 0), (1, 0, 0), (2, 0, 0)];
    let net_b = vec![(0, 5, 1), (1, 5, 1), (2, 5, 1)];

    let overlap = analyzer.detect_parallel_overlap(&net_a, &net_b, 100_000); // 100μm voxels

    assert_eq!(overlap, 0, "No overlap expected");
}

#[test]
fn test_detect_parallel_overlap_full_overlap() {
    let analyzer = EMAnalyzer::new();

    // Two nets on different layers but same X-Y coordinates
    let net_a = vec![(0, 0, 0), (1, 0, 0), (2, 0, 0), (3, 0, 0)];
    let net_b = vec![(0, 0, 1), (1, 0, 1), (2, 0, 1), (3, 0, 1)];

    let overlap = analyzer.detect_parallel_overlap(&net_a, &net_b, 100_000); // 100μm voxels

    // 4 overlapping voxels, sqrt(4) = 2 voxels length
    // 2 * 100_000nm = 200_000nm = 0.2mm
    assert_eq!(overlap, 200_000, "Full overlap expected");
}

#[test]
fn test_detect_parallel_overlap_partial() {
    let analyzer = EMAnalyzer::new();

    // Partial overlap: 2 out of 4 voxels overlap
    let net_a = vec![(0, 0, 0), (1, 0, 0), (2, 0, 0), (3, 0, 0)];
    let net_b = vec![(2, 0, 1), (3, 0, 1), (4, 0, 1), (5, 0, 1)];

    let overlap = analyzer.detect_parallel_overlap(&net_a, &net_b, 100_000); // 100μm voxels

    // 2 overlapping voxels, sqrt(2) ≈ 1.414 voxels length
    // 1.414 * 100_000nm ≈ 141_421nm
    assert!(
        (141_000..=142_000).contains(&overlap),
        "Partial overlap expected, got {}",
        overlap
    );
}

#[test]
fn test_detect_parallel_overlap_long_trace() {
    let analyzer = EMAnalyzer::new();

    // Long parallel traces (100 voxels each)
    let mut net_a = Vec::new();
    let mut net_b = Vec::new();

    for x in 0..100 {
        net_a.push((x, 0, 0));
        net_b.push((x, 0, 1)); // Same X-Y, different Z
    }

    let overlap = analyzer.detect_parallel_overlap(&net_a, &net_b, 100_000); // 100μm voxels

    // 100 overlapping voxels, sqrt(100) = 10 voxels length
    // 10 * 100_000nm = 1_000_000nm = 1mm
    assert_eq!(overlap, 1_000_000, "Long trace overlap expected");
}

#[test]
fn test_validate_crosstalk_overlap_pass() {
    let analyzer = EMAnalyzer::new();

    // Short overlap (safe)
    let net_a = vec![(0, 0, 0), (1, 0, 0)];
    let net_b = vec![(0, 0, 1), (1, 0, 1)];

    let result = analyzer.validate_crosstalk_overlap(
        "NetA", "NetB", &net_a, &net_b, 100_000,    // 100μm voxels
        10_000_000, // 10mm max overlap
    );

    assert!(result.is_ok(), "Short overlap should pass");
}

#[test]
fn test_validate_crosstalk_overlap_fail() {
    let analyzer = EMAnalyzer::new();

    // Long parallel overlap (dangerous)
    let mut net_a = Vec::new();
    let mut net_b = Vec::new();

    // Create 200 voxel overlap (sqrt(200) ≈ 14.14 voxels = 1.414mm)
    for x in 0..200 {
        net_a.push((x, 0, 0));
        net_b.push((x, 0, 1));
    }

    let result = analyzer.validate_crosstalk_overlap(
        "HighSpeedClock",
        "DataBus",
        &net_a,
        &net_b,
        100_000,   // 100μm voxels
        1_000_000, // 1mm max overlap (will be exceeded)
    );

    assert!(result.is_err(), "Long overlap should fail");

    if let Err(violation) = result {
        match violation {
            hwc_physics::electromagnetic::EMViolation::Crosstalk {
                net_a,
                net_b,
                crosstalk_coefficient,
                ..
            } => {
                assert_eq!(net_a, "HighSpeedClock");
                assert_eq!(net_b, "DataBus");
                assert!(
                    crosstalk_coefficient > 1.0,
                    "Crosstalk coefficient should exceed threshold"
                );
            }
            _ => panic!("Expected Crosstalk violation"),
        }
    }
}

#[test]
fn test_crosstalk_detection_different_voxel_sizes() {
    let analyzer = EMAnalyzer::new();

    // Same physical overlap, different voxel sizes
    let net_a = vec![(0, 0, 0), (1, 0, 0), (2, 0, 0), (3, 0, 0)];
    let net_b = vec![(0, 0, 1), (1, 0, 1), (2, 0, 1), (3, 0, 1)];

    // Test with 100μm voxels
    let overlap_100um = analyzer.detect_parallel_overlap(&net_a, &net_b, 100_000);

    // Test with 50μm voxels (should be half the overlap length)
    let overlap_50um = analyzer.detect_parallel_overlap(&net_a, &net_b, 50_000);

    assert_eq!(
        overlap_100um,
        overlap_50um * 2,
        "Overlap should scale with voxel size"
    );
}

#[test]
fn test_crosstalk_detection_2d_area() {
    let analyzer = EMAnalyzer::new();

    // Create a 10x10 area overlap
    let mut net_a = Vec::new();
    let mut net_b = Vec::new();

    for x in 0..10 {
        for y in 0..10 {
            net_a.push((x, y, 0));
            net_b.push((x, y, 1));
        }
    }

    let overlap = analyzer.detect_parallel_overlap(&net_a, &net_b, 100_000); // 100μm voxels

    // 100 overlapping voxels, sqrt(100) = 10 voxels length
    // 10 * 100_000nm = 1_000_000nm = 1mm
    assert_eq!(overlap, 1_000_000, "2D area overlap expected");
}

#[test]
fn test_crosstalk_detection_empty_nets() {
    let analyzer = EMAnalyzer::new();

    let net_a: Vec<(usize, usize, usize)> = vec![];
    let net_b = vec![(0, 0, 0), (1, 0, 0)];

    let overlap = analyzer.detect_parallel_overlap(&net_a, &net_b, 100_000);

    assert_eq!(overlap, 0, "Empty net should have no overlap");
}

#[test]
fn test_crosstalk_detection_single_voxel() {
    let analyzer = EMAnalyzer::new();

    let net_a = vec![(5, 5, 0)];
    let net_b = vec![(5, 5, 1)];

    let overlap = analyzer.detect_parallel_overlap(&net_a, &net_b, 100_000);

    // 1 overlapping voxel, sqrt(1) = 1 voxel length
    // 1 * 100_000nm = 100_000nm = 0.1mm
    assert_eq!(overlap, 100_000, "Single voxel overlap expected");
}

#[test]
fn test_crosstalk_detection_performance_large_nets() {
    let analyzer = EMAnalyzer::new();

    // Create large nets (1000 voxels each)
    let mut net_a = Vec::new();
    let mut net_b = Vec::new();

    for i in 0..1000 {
        net_a.push((i, 0, 0));
        net_b.push((i, 1, 1)); // Different Y, so no overlap
    }

    let start = std::time::Instant::now();
    let overlap = analyzer.detect_parallel_overlap(&net_a, &net_b, 100_000);
    let duration = start.elapsed();

    assert_eq!(overlap, 0, "No overlap expected");
    assert!(
        duration.as_millis() < 10,
        "Detection should be fast, took {:?}",
        duration
    );
}

#[test]
fn test_crosstalk_configurable_thresholds() {
    let analyzer = EMAnalyzer::new();

    let net_a = vec![(0, 0, 0), (1, 0, 0), (2, 0, 0), (3, 0, 0)];
    let net_b = vec![(0, 0, 1), (1, 0, 1), (2, 0, 1), (3, 0, 1)];

    // Test with strict threshold (should fail)
    let result_strict = analyzer.validate_crosstalk_overlap(
        "NetA", "NetB", &net_a, &net_b, 100_000, // 100μm voxels
        100_000, // 0.1mm max overlap (very strict)
    );

    assert!(result_strict.is_err(), "Strict threshold should fail");

    // Test with relaxed threshold (should pass)
    let result_relaxed = analyzer.validate_crosstalk_overlap(
        "NetA", "NetB", &net_a, &net_b, 100_000,    // 100μm voxels
        10_000_000, // 10mm max overlap (relaxed)
    );

    assert!(result_relaxed.is_ok(), "Relaxed threshold should pass");
}
