//! Task B2: Crosstalk Detection Integration Test
//!
//! This test demonstrates how crosstalk detection integrates with the physics engine
//! and would be used in a real PCB design validation workflow.

use hwc_physics::EMAnalyzer;

/// Simulates a real PCB design with multiple signal layers
#[test]
fn test_crosstalk_detection_multilayer_pcb() {
    let analyzer = EMAnalyzer::new();

    let clock_trace: Vec<(i64, i64, i64)> = (0..500).map(|x| (x, 10, 0)).collect();

    let mut data_bus_traces: Vec<Vec<(i64, i64, i64)>> = Vec::new();
    for bus_line in 0..8 {
        let trace: Vec<(i64, i64, i64)> =
            (0..500).map(|x| (x, 12 + bus_line * 2, 0)).collect();
        data_bus_traces.push(trace);
    }

    let mut violations = Vec::new();

    for (i, data_trace) in data_bus_traces.iter().enumerate() {
        let result = analyzer.validate_crosstalk_overlap(
            "CLK",
            &format!("DATA{}", i),
            &clock_trace,
            data_trace,
            5_000_000,
        );

        if let Err(violation) = result {
            violations.push((i, violation));
        }
    }

    println!("Checked clock vs 8 data lines");
    println!("Found {} crosstalk violations", violations.len());

    assert_eq!(
        violations.len(),
        0,
        "No crosstalk violations expected with proper spacing"
    );
}

#[test]
fn test_crosstalk_detection_adjacent_layers() {
    let analyzer = EMAnalyzer::new();

    let layer0_trace: Vec<(i64, i64, i64)> = (0..1000).map(|x| (x, 50, 0)).collect();
    let layer1_trace: Vec<(i64, i64, i64)> = (0..1000).map(|x| (x, 50, 1)).collect();

    let result = analyzer.validate_crosstalk_overlap(
        "TopTrace",
        "BottomTrace",
        &layer0_trace,
        &layer1_trace,
        10_000_000,
    );

    assert!(result.is_ok(), "Should pass with 10mm threshold");

    let result_strict = analyzer.validate_crosstalk_overlap(
        "TopTrace",
        "BottomTrace",
        &layer0_trace,
        &layer1_trace,
        500, // 500nm threshold (overlap is 999nm, should fail)
    );

    assert!(result_strict.is_err(), "Should fail with strict threshold");
}

#[test]
fn test_crosstalk_detection_differential_pairs() {
    let analyzer = EMAnalyzer::new();

    let diff_p: Vec<(i64, i64, i64)> = (0..500).map(|x| (x, 10, 0)).collect();
    let diff_n: Vec<(i64, i64, i64)> = (0..500).map(|x| (x, 11, 0)).collect();
    let other_signal: Vec<(i64, i64, i64)> = (0..500).map(|x| (x, 15, 0)).collect();

    let result_pair = analyzer.validate_crosstalk_overlap(
        "USB_DP", "USB_DN", &diff_p, &diff_n,
        50_000_000,
    );

    assert!(
        result_pair.is_ok(),
        "Differential pair coupling is intentional"
    );

    let result_other = analyzer.validate_crosstalk_overlap(
        "USB_DP",
        "OTHER_SIGNAL",
        &diff_p,
        &other_signal,
        5_000_000,
    );

    assert!(
        result_other.is_ok(),
        "No overlap with properly spaced signal"
    );
}

#[test]
fn test_crosstalk_detection_bus_routing() {
    let analyzer = EMAnalyzer::new();

    let mut bus_traces: Vec<Vec<(i64, i64, i64)>> = Vec::new();

    for bit in 0..32 {
        let trace: Vec<(i64, i64, i64)> =
            (0..1000).map(|x| (x, bit * 2, 0)).collect();
        bus_traces.push(trace);
    }

    let mut violations = 0;

    for i in 0..31 {
        let result = analyzer.validate_crosstalk_overlap(
            &format!("DATA{}", i),
            &format!("DATA{}", i + 1),
            &bus_traces[i],
            &bus_traces[i + 1],
            5_000_000,
        );

        if result.is_err() {
            violations += 1;
        }
    }

    println!("Checked 31 adjacent bus line pairs");
    println!("Found {} crosstalk violations", violations);

    assert_eq!(violations, 0, "Bus lines properly spaced");
}

#[test]
fn test_crosstalk_detection_via_transition() {
    let analyzer = EMAnalyzer::new();

    let trace_layer1: Vec<(i64, i64, i64)> = (500..1000).map(|x| (x, 10, 1)).collect();
    let parallel_trace: Vec<(i64, i64, i64)> = (0..1000).map(|x| (x, 10, 1)).collect();

    let result = analyzer.validate_crosstalk_overlap(
        "Signal1_L1",
        "Signal2_L1",
        &trace_layer1,
        &parallel_trace,
        10_000_000,
    );

    assert!(result.is_ok(), "Should pass with 10mm threshold");
}

#[test]
fn test_crosstalk_detection_priority_based_thresholds() {
    let analyzer = EMAnalyzer::new();

    let clock: Vec<(i64, i64, i64)> = (0..1000).map(|x| (x, 10, 0)).collect();
    let led_control: Vec<(i64, i64, i64)> = (0..1000).map(|x| (x, 10, 1)).collect();

    let result_strict = analyzer.validate_crosstalk_overlap(
        "CLK_100MHz",
        "LED_CTRL",
        &clock,
        &led_control,
        500, // 500nm threshold (overlap is 999nm, should fail)
    );

    assert!(
        result_strict.is_err(),
        "High-speed signal should have strict threshold"
    );

    let result_relaxed = analyzer.validate_crosstalk_overlap(
        "LED_CTRL_A",
        "LED_CTRL_B",
        &clock,
        &led_control,
        50_000_000,
    );

    assert!(
        result_relaxed.is_ok(),
        "Low-speed signals can have relaxed threshold"
    );
}

#[test]
fn test_crosstalk_detection_ground_plane_shielding() {
    let analyzer = EMAnalyzer::new();

    let layer0_trace: Vec<(i64, i64, i64)> = (0..1000).map(|x| (x, 50, 0)).collect();
    let layer3_trace: Vec<(i64, i64, i64)> = (0..1000).map(|x| (x, 50, 3)).collect();

    let result = analyzer.validate_crosstalk_overlap(
        "TopSignal",
        "BottomSignal",
        &layer0_trace,
        &layer3_trace,
        20_000_000,
    );

    assert!(
        result.is_ok(),
        "Ground plane shielding allows relaxed threshold"
    );
}
