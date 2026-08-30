use hwc_router::types::VolumetricTensor3D;

#[test]
fn test_volumetric_tensor_dimensions_and_indexing() {
    let tensor = VolumetricTensor3D::new(10, 20, 5, 2_720_000, 2_720_000);
    assert_eq!(tensor.dim_x, 10);
    assert_eq!(tensor.dim_y, 20);
    assert_eq!(tensor.dim_z, 5);

    // Total cells = 10 * 20 * 5 = 1000
    assert_eq!(tensor.cap_x.len(), 1000);
    assert_eq!(tensor.cap_y.len(), 1000);
    assert_eq!(tensor.occ_x.len(), 1000);
    assert_eq!(tensor.occ_y.len(), 1000);

    let idx = tensor.index(9, 19, 4);
    assert_eq!(idx, 999);
}

#[test]
fn test_congestion_at_pm() {
    let mut tensor = VolumetricTensor3D::new(4, 4, 1, 1_000_000, 1_000_000);
    // Initially zero occupancy
    let cong_initial = tensor.congestion_at_pm(500_000, 500_000);
    assert_eq!(cong_initial, 0.0);

    // Add occupancy = capacity (10 / 10 = 1.0)
    tensor.add_occ_x(0, 0, 0, 10);
    let cong_full = tensor.congestion_at_pm(500_000, 500_000);
    assert_eq!(cong_full, 1.0);
}
