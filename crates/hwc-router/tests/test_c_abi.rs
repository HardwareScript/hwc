use hwc_router::ffi::{HwcRoutingOutput64, HwcViaInstance64, HwcWireSegment64};

#[test]
fn test_c_abi_struct_layout_and_helpers() {
    let wire = HwcWireSegment64 {
        net_id: 1,
        layer_idx: 0,
        start_x_pm: 0,
        start_y_pm: 0,
        start_z_pm: 0,
        end_x_pm: 1_000_000,
        end_y_pm: 0,
        end_z_pm: 0,
        width_pm: 140_000,
    };

    let via = HwcViaInstance64 {
        net_id: 1,
        x_pm: 1_000_000,
        y_pm: 0,
        z_bottom_pm: 0,
        z_top_pm: 360_000,
        from_layer_idx: 0,
        to_layer_idx: 1,
        diameter_pm: 150_000,
    };

    let wires = vec![wire];
    let vias = vec![via];

    let output = HwcRoutingOutput64::success(&wires, &vias);
    assert_eq!(output.wire_count, 1);
    assert_eq!(output.via_count, 1);
    assert_eq!(output.status_code, 0);
    assert!(output.error_msg.is_null());
}
