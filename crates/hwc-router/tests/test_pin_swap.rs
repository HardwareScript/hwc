use compact_str::CompactString;
use hwc_router::track_assign::{try_swap_symmetric_pins, InputSymmetryGroup};

#[test]
fn test_pin_swap_detection() {
    let symmetry = InputSymmetryGroup::Pair(CompactString::new("A"), CompactString::new("B"));

    // Scenario: Net A comes from top (y = 100), Net B from bottom (y = 50)
    // But Pin A is at bottom (y = 10) and Pin B is at top (y = 20)
    // The nets cross physically! Swapping A <-> B untangles them.
    let should_swap = try_swap_symmetric_pins(&symmetry, 10, 20, 100, 50);
    assert!(should_swap);

    // Scenario: Net A comes from top (y = 100), Net B from bottom (y = 50)
    // Pin A is at top (y = 20), Pin B is at bottom (y = 10)
    // Nets are parallel without crossing.
    let should_not_swap = try_swap_symmetric_pins(&symmetry, 20, 10, 100, 50);
    assert!(!should_not_swap);
}
