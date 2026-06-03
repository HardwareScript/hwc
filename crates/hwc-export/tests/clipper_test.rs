#[cfg(test)]
mod tests {
    use clipper2_rust::core::FillRule;
    use clipper2_rust::{Path64, Point64};

    #[test]
    fn test_coreldraw_boolean_handshake() {
        // 1. Define Shape A: A 10mm x 10mm square (coordinates in nanometers)
        // Millimeters to nanometers: 10mm = 10,000,000 nm
        let mut square = Path64::new();
        square.push(Point64::new(-5_000_000, -5_000_000));
        square.push(Point64::new(5_000_000, -5_000_000));
        square.push(Point64::new(5_000_000, 5_000_000));
        square.push(Point64::new(-5_000_000, 5_000_000));

        // 2. Define Shape B: A 5mm radius circle, offset to the right at X = 5mm
        let cx = 5_000_000;
        let cy = 0;
        let radius = 5_000_000;
        let segments = 32;

        let mut circle = Path64::new();
        for i in 0..segments {
            let angle = (i as f64 / segments as f64) * 2.0 * std::f64::consts::PI;
            let x = cx + (radius as f64 * angle.cos()) as i64;
            let y = cy + (radius as f64 * angle.sin()) as i64;
            circle.push(Point64::new(x, y));
        }

        let subjects = vec![square];
        let clips = vec![circle];

        // --- OPERATION 1: UNION (Weld) ---
        // v0.1.8: Use NonZero to ensure overlapping shapes merge into a solid mass
        let weld_result = clipper2_rust::union_64(&subjects, &clips, FillRule::NonZero);
        assert!(!weld_result.is_empty());
        assert_eq!(weld_result.len(), 1);
        println!("Weld Success: Unified shape has {} vertices", weld_result[0].len());

        // --- OPERATION 2: DIFFERENCE (Trim) ---
        // Difference still works correctly with NonZero subjects/clips
        let trim_result = clipper2_rust::difference_64(&subjects, &clips, FillRule::NonZero);
        assert!(!trim_result.is_empty());

        // The result should be a single chopped polygon
        assert_eq!(trim_result.len(), 1);
        println!("Trim Success: Chopped shape has {} vertices", trim_result[0].len());

        // Verify the bite mark: The point (5mm, 0) is inside the circle,
        // so it must have been removed from the square.
        for point in &trim_result[0] {
            // Ensure no vertex is sitting in the center of the deleted circle
            assert!(!(point.x == 5_000_000 && point.y == 0));
        }
    }
}
