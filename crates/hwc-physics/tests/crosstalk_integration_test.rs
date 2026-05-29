//! Task B2: Crosstalk Detection Integration Test
//!
//! This test demonstrates how crosstalk detection integrates with the physics engine
//! and would be used in a real PCB design validation workflow.

use hwc_physics::EMAnalyzer;

/// Simulates a real PCB design with multiple signal layers
#[test]
fn test_crosstalk_detection_multilayer_pcb() {
    let analyzer = EMAnalyzer::new();

    // Simulate a 4-layer PCB design:
    // Layer 0: Top signal layer
    // Layer 1: Ground plane
    // Layer 2: Power plane
    // Layer 3: Bottom signal layer

    // High-speed clock trace on Layer 0
    let mut clock_trace = Vec::new();
    for x in 0..500 {
        clock_trace.push((x, 10, 0)); // Horizontal trace
    }

    // Data bus traces on Layer 0 (parallel to clock)
    let mut data_bus_traces: Vec<Vec<(usize, usize, usize)>> = Vec::new();
    for bus_line in 0..8 {
        let mut trace = Vec::new();
        for x in 0..500 {
            trace.push((x, 12 + bus_line * 2, 0)); // Parallel horizontal traces
        }
        data_bus_traces.push(trace);
    }

    // Check crosstalk between clock and each data line
    let mut violations = Vec::new();

    for (i, data_trace) in data_bus_traces.iter().enumerate() {
        let result = analyzer.validate_crosstalk_overlap(
            "CLK",
            &format!("DATA{}", i),
            &clock_trace,
            data_trace,
            100_000,   // 100μm voxels
            5_000_000, // 5mm max parallel overlap (strict for high-speed signals)
        );

        if let Err(violation) = result {
            violations.push((i, violation));
        }
    }

    println!("Checked clock vs 8 data lines");
    println!("Found {} crosstalk violations", violations.len());

    // In this design, traces are separated by 2 voxels (200μm), so no overlap expected
    assert_eq!(
        violations.len(),
        0,
        "No crosstalk violations expected with proper spacing"
    );
}

#[test]
fn test_crosstalk_detection_adjacent_layers() {
    let analyzer = EMAnalyzer::new();

    // Simulate traces on adjacent layers (Layer 0 and Layer 1)
    // This is the most critical case for crosstalk

    // Trace on Layer 0
    let mut layer0_trace = Vec::new();
    for x in 0..1000 {
        layer0_trace.push((x, 50, 0));
    }

    // Trace on Layer 1 (directly below, same X-Y coordinates)
    let mut layer1_trace = Vec::new();
    for x in 0..1000 {
        layer1_trace.push((x, 50, 1));
    }

    let result = analyzer.validate_crosstalk_overlap(
        "TopTrace",
        "BottomTrace",
        &layer0_trace,
        &layer1_trace,
        100_000,    // 100μm voxels
        10_000_000, // 10mm max overlap
    );

    // This should fail - 1000 voxels overlap = sqrt(1000) ≈ 31.6 voxels = 3.16mm
    // But wait, that's under 10mm, so it should pass
    assert!(result.is_ok(), "Should pass with 10mm threshold");

    // Now test with stricter threshold
    let result_strict = analyzer.validate_crosstalk_overlap(
        "TopTrace",
        "BottomTrace",
        &layer0_trace,
        &layer1_trace,
        100_000,   // 100μm voxels
        1_000_000, // 1mm max overlap (strict)
    );

    assert!(result_strict.is_err(), "Should fail with 1mm threshold");
}

#[test]
fn test_crosstalk_detection_differential_pairs() {
    let analyzer = EMAnalyzer::new();

    // Differential pairs should be close together (intentional coupling)
    // But should not couple with other signals

    // Differential pair (P and N)
    let mut diff_p = Vec::new();
    let mut diff_n = Vec::new();

    for x in 0..500 {
        diff_p.push((x, 10, 0));
        diff_n.push((x, 11, 0)); // 1 voxel spacing (100μm)
    }

    // Another signal nearby
    let mut other_signal = Vec::new();
    for x in 0..500 {
        other_signal.push((x, 15, 0)); // 4 voxels away
    }

    // Check coupling between differential pair (should be acceptable)
    let result_pair = analyzer.validate_crosstalk_overlap(
        "USB_DP", "USB_DN", &diff_p, &diff_n, 100_000,    // 100μm voxels
        50_000_000, // 50mm max overlap (very relaxed for differential pairs)
    );

    assert!(
        result_pair.is_ok(),
        "Differential pair coupling is intentional"
    );

    // Check coupling with other signal (should have no overlap due to spacing)
    let result_other = analyzer.validate_crosstalk_overlap(
        "USB_DP",
        "OTHER_SIGNAL",
        &diff_p,
        &other_signal,
        100_000,   // 100μm voxels
        5_000_000, // 5mm max overlap
    );

    assert!(
        result_other.is_ok(),
        "No overlap with properly spaced signal"
    );
}

#[test]
fn test_crosstalk_detection_bus_routing() {
    let analyzer = EMAnalyzer::new();

    // Simulate a 32-bit data bus with parallel routing
    let mut bus_traces: Vec<Vec<(usize, usize, usize)>> = Vec::new();

    for bit in 0..32 {
        let mut trace = Vec::new();
        for x in 0..1000 {
            trace.push((x, bit * 2, 0)); // 2 voxel spacing between traces
        }
        bus_traces.push(trace);
    }

    // Check crosstalk between adjacent bus lines
    let mut violations = 0;

    for i in 0..31 {
        let result = analyzer.validate_crosstalk_overlap(
            &format!("DATA{}", i),
            &format!("DATA{}", i + 1),
            &bus_traces[i],
            &bus_traces[i + 1],
            100_000,   // 100μm voxels
            5_000_000, // 5mm max overlap
        );

        if result.is_err() {
            violations += 1;
        }
    }

    println!("Checked 31 adjacent bus line pairs");
    println!("Found {} crosstalk violations", violations);

    // With 2 voxel spacing, no overlap expected
    assert_eq!(violations, 0, "Bus lines properly spaced");
}

#[test]
fn test_crosstalk_detection_via_transition() {
    let analyzer = EMAnalyzer::new();

    // Simulate a trace that transitions between layers via a via
    // Layer 0: Horizontal trace
    // Via at x=500
    // Layer 1: Continuation

    let mut trace_layer0 = Vec::new();
    for x in 0..500 {
        trace_layer0.push((x, 10, 0));
    }

    let mut trace_layer1 = Vec::new();
    for x in 500..1000 {
        trace_layer1.push((x, 10, 1));
    }

    // Another trace on Layer 1 that runs parallel
    let mut parallel_trace = Vec::new();
    for x in 0..1000 {
        parallel_trace.push((x, 10, 1));
    }

    // Check crosstalk between the layer 1 segments
    let result = analyzer.validate_crosstalk_overlap(
        "Signal1_L1",
        "Signal2_L1",
        &trace_layer1,
        &parallel_trace,
        100_000,    // 100μm voxels
        10_000_000, // 10mm max overlap
    );

    // 500 voxels overlap, sqrt(500) ≈ 22.36 voxels = 2.236mm
    assert!(result.is_ok(), "Should pass with 10mm threshold");
}

#[test]
fn test_crosstalk_detection_priority_based_thresholds() {
    let analyzer = EMAnalyzer::new();

    // High-priority signal (clock) - strict threshold
    let mut clock = Vec::new();
    for x in 0..1000 {
        clock.push((x, 10, 0));
    }

    // Low-priority signal (LED control) - relaxed threshold
    let mut led_control = Vec::new();
    for x in 0..1000 {
        led_control.push((x, 10, 1)); // Same X-Y, different layer
    }

    // Check with strict threshold (for high-speed signals)
    let result_strict = analyzer.validate_crosstalk_overlap(
        "CLK_100MHz",
        "LED_CTRL",
        &clock,
        &led_control,
        100_000,   // 100μm voxels
        1_000_000, // 1mm max overlap (strict for high-speed)
    );

    // Should fail with strict threshold
    assert!(
        result_strict.is_err(),
        "High-speed signal should have strict threshold"
    );

    // Check with relaxed threshold (for low-speed signals)
    let result_relaxed = analyzer.validate_crosstalk_overlap(
        "LED_CTRL_A",
        "LED_CTRL_B",
        &clock,
        &led_control,
        100_000,    // 100μm voxels
        50_000_000, // 50mm max overlap (relaxed for low-speed)
    );

    // Should pass with relaxed threshold
    assert!(
        result_relaxed.is_ok(),
        "Low-speed signals can have relaxed threshold"
    );
}

#[test]
fn test_crosstalk_detection_ground_plane_shielding() {
    let analyzer = EMAnalyzer::new();

    // Simulate traces on non-adjacent layers (Layer 0 and Layer 3)
    // with ground planes in between (Layer 1 and Layer 2)

    let mut layer0_trace = Vec::new();
    for x in 0..1000 {
        layer0_trace.push((x, 50, 0));
    }

    let mut layer3_trace = Vec::new();
    for x in 0..1000 {
        layer3_trace.push((x, 50, 3));
    }

    // Even though they have same X-Y coordinates, ground planes provide shielding
    // So we can use a more relaxed threshold
    let result = analyzer.validate_crosstalk_overlap(
        "TopSignal",
        "BottomSignal",
        &layer0_trace,
        &layer3_trace,
        100_000,    // 100μm voxels
        20_000_000, // 20mm max overlap (relaxed due to ground plane shielding)
    );

    assert!(
        result.is_ok(),
        "Ground plane shielding allows relaxed threshold"
    );
}
