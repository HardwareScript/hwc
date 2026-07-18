//! Task B2: Signal Integrity (Crosstalk) Sweep Tests
//!
//! Tests for segment-based crosstalk detection.

use hwc_physics::EMAnalyzer;

#[test]
fn test_detect_parallel_overlap_no_overlap() {
    let analyzer = EMAnalyzer::new();

    let net_a = vec![(0_i64, 0, 0), (5, 0, 0), (10, 0, 0)];
    let net_b = vec![(0_i64, 5, 1), (5, 5, 1), (10, 5, 1)];

    let overlap = analyzer.detect_parallel_overlap(&net_a, &net_b);

    assert_eq!(overlap, 0, "No overlap expected");
}

#[test]
fn test_detect_parallel_overlap_full_overlap() {
    let analyzer = EMAnalyzer::new();

    let net_a = vec![(0_i64, 0, 0), (4, 0, 0)];
    let net_b = vec![(0_i64, 0, 1), (4, 0, 1)];

    let overlap = analyzer.detect_parallel_overlap(&net_a, &net_b);

    assert_eq!(overlap, 4, "Full overlap expected");
}

#[test]
fn test_detect_parallel_overlap_partial() {
    let analyzer = EMAnalyzer::new();

    let net_a = vec![(0_i64, 0, 0), (4, 0, 0)];
    let net_b = vec![(2_i64, 0, 1), (6, 0, 1)];

    let overlap = analyzer.detect_parallel_overlap(&net_a, &net_b);

    assert_eq!(overlap, 2, "Partial overlap expected");
}

#[test]
fn test_detect_parallel_overlap_long_trace() {
    let analyzer = EMAnalyzer::new();

    let net_a = vec![(0_i64, 0, 0), (100, 0, 0)];
    let net_b = vec![(0_i64, 0, 1), (100, 0, 1)];

    let overlap = analyzer.detect_parallel_overlap(&net_a, &net_b);

    assert_eq!(overlap, 100, "Long trace overlap expected");
}

#[test]
fn test_validate_crosstalk_overlap_pass() {
    let analyzer = EMAnalyzer::new();

    let net_a = vec![(0_i64, 0, 0), (1, 0, 0)];
    let net_b = vec![(0_i64, 0, 1), (1, 0, 1)];

    let result = analyzer.validate_crosstalk_overlap("NetA", "NetB", &net_a, &net_b, 10_000_000);

    assert!(result.is_ok(), "Short overlap should pass");
}

#[test]
fn test_validate_crosstalk_overlap_fail() {
    let analyzer = EMAnalyzer::new();

    let net_a = vec![(0_i64, 0, 0), (200, 0, 0)];
    let net_b = vec![(0_i64, 0, 1), (200, 0, 1)];

    let result = analyzer.validate_crosstalk_overlap(
        "HighSpeedClock",
        "DataBus",
        &net_a,
        &net_b,
        100, // 100nm max overlap (will be exceeded by 200nm overlap)
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
fn test_crosstalk_detection_different_segment_lengths() {
    let analyzer = EMAnalyzer::new();

    let net_a = vec![(0_i64, 0, 0), (4, 0, 0)];
    let net_b = vec![(0_i64, 0, 1), (4, 0, 1)];

    let overlap = analyzer.detect_parallel_overlap(&net_a, &net_b);

    assert_eq!(overlap, 4, "Overlap should be segment length");
}

#[test]
fn test_crosstalk_detection_2d_area() {
    let analyzer = EMAnalyzer::new();

    let mut net_a = Vec::new();
    let mut net_b = Vec::new();

    for x in 0_i64..10 {
        net_a.push((x, 0, 0));
        net_b.push((x, 0, 1));
    }

    let overlap = analyzer.detect_parallel_overlap(&net_a, &net_b);

    assert_eq!(overlap, 9, "2D area overlap expected");
}

#[test]
fn test_crosstalk_detection_empty_nets() {
    let analyzer = EMAnalyzer::new();

    let net_a: Vec<(i64, i64, i64)> = vec![];
    let net_b = vec![(0_i64, 0, 0), (1, 0, 0)];

    let overlap = analyzer.detect_parallel_overlap(&net_a, &net_b);

    assert_eq!(overlap, 0, "Empty net should have no overlap");
}

#[test]
fn test_crosstalk_detection_single_point() {
    let analyzer = EMAnalyzer::new();

    let net_a = vec![(5_i64, 5, 0)];
    let net_b = vec![(5_i64, 5, 1)];

    let overlap = analyzer.detect_parallel_overlap(&net_a, &net_b);

    assert_eq!(overlap, 0, "Single point has no segments");
}

#[test]
fn test_crosstalk_detection_performance_large_nets() {
    let analyzer = EMAnalyzer::new();

    let net_a = vec![(0_i64, 0, 0), (1000, 0, 0)];
    let net_b = vec![(0_i64, 1, 1), (1000, 1, 1)];

    let start = std::time::Instant::now();
    let overlap = analyzer.detect_parallel_overlap(&net_a, &net_b);
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

    let net_a = vec![(0_i64, 0, 0), (4, 0, 0)];
    let net_b = vec![(0_i64, 0, 1), (4, 0, 1)];

    let result_strict = analyzer.validate_crosstalk_overlap("NetA", "NetB", &net_a, &net_b, 1);

    assert!(result_strict.is_err(), "Strict threshold should fail");

    let result_relaxed =
        analyzer.validate_crosstalk_overlap("NetA", "NetB", &net_a, &net_b, 10_000_000);

    assert!(result_relaxed.is_ok(), "Relaxed threshold should pass");
}
