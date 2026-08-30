use compact_str::CompactString;
use hwc_engine::geometry::Point3D;
use hwc_engine::netlist::NetId;
use hwc_router::eco::{EcoPatchManager, GaFillerCell, MetalEcoRouter};

#[test]
fn test_freeze_silicon_immutability_check() {
    let patch_mgr = EcoPatchManager::default();

    // Mutating metal2 is allowed in freeze-silicon mode
    let metal_edit = vec![CompactString::new("metal2"), CompactString::new("metal3")];
    assert!(patch_mgr.verify_base_silicon_immutability(&metal_edit).is_ok());

    // Mutating diff or poly triggers freeze-silicon violation
    let illegal_edit = vec![CompactString::new("poly")];
    assert!(patch_mgr.verify_base_silicon_immutability(&illegal_edit).is_err());
}

#[test]
fn test_spare_filler_allocation_and_jumper() {
    let patch_mgr = EcoPatchManager::default();
    let mut spares = vec![
        GaFillerCell {
            id: 1,
            name: CompactString::new("ga_0"),
            location: Point3D::new(100_000, 100_000, 0),
            is_committed: false,
        },
        GaFillerCell {
            id: 2,
            name: CompactString::new("ga_1"),
            location: Point3D::new(500_000, 500_000, 0),
            is_committed: false,
        },
    ];

    let target = Point3D::new(110_000, 105_000, 0);
    let nearest = patch_mgr.find_nearest_spare_filler(target, &mut spares);
    assert!(nearest.is_some());
    let spare = nearest.unwrap();
    assert_eq!(spare.id, 1);
    spare.is_committed = true;

    // Route metal jumper
    let eco_router = MetalEcoRouter::default();
    let res = eco_router.route_jumper(NetId::new(0), spare.location, target);
    assert!(res.is_ok());
    let output = res.unwrap();
    assert_eq!(output.traces.len(), 2);
    assert_eq!(output.vias.len(), 1);
}
