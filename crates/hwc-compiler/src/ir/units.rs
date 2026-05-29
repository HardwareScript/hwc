//! Unit conversion utilities for display and error messages.

use compact_str::CompactString;

/// Conversion factors from nanometers to various units.
pub mod conversion {
    /// Nanometers per millimeter (1mm = 1,000,000nm)
    pub const NM_PER_MM: f64 = 1_000_000.0;

    /// Nanometers per micrometer (1µm = 1,000nm)
    pub const NM_PER_UM: f64 = 1_000.0;

    /// Nanometers per centimeter (1cm = 10,000,000nm)
    pub const NM_PER_CM: f64 = 10_000_000.0;
}

/// Convert nanometers to millimeters for display.
#[inline]
pub fn nm_to_mm(nm: i64) -> f64 {
    nm as f64 / conversion::NM_PER_MM
}

/// Convert nanometers to micrometers for display.
#[inline]
pub fn nm_to_um(nm: i64) -> f64 {
    nm as f64 / conversion::NM_PER_UM
}

/// Convert nanometers to centimeters for display.
#[inline]
pub fn nm_to_cm(nm: i64) -> f64 {
    nm as f64 / conversion::NM_PER_CM
}

/// Format a position in nanometers as millimeters with 3 decimal places.
pub fn format_position_mm(x_nm: i64, y_nm: i64, z_nm: i64) -> CompactString {
    format!(
        "[{:.3}, {:.3}, {:.3}]",
        nm_to_mm(x_nm),
        nm_to_mm(y_nm),
        nm_to_mm(z_nm)
    )
    .into()
}

/// Format a distance in nanometers with appropriate unit.
///
/// Automatically selects the most readable unit:
/// - < 1µm: display in nm
/// - < 1mm: display in µm
/// - >= 1mm: display in mm
pub fn format_distance(distance_nm: i64) -> CompactString {
    if distance_nm < 1_000 {
        format!("{}nm", distance_nm).into()
    } else if distance_nm < 1_000_000 {
        format!("{:.1}µm", nm_to_um(distance_nm)).into()
    } else {
        format!("{:.3}mm", nm_to_mm(distance_nm)).into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nm_to_mm() {
        assert_eq!(nm_to_mm(1_000_000), 1.0);
        assert_eq!(nm_to_mm(500_000), 0.5);
        assert_eq!(nm_to_mm(14_900_000), 14.9);
    }

    #[test]
    fn test_nm_to_um() {
        assert_eq!(nm_to_um(1_000), 1.0);
        assert_eq!(nm_to_um(500), 0.5);
        assert_eq!(nm_to_um(254_000), 254.0);
    }

    #[test]
    fn test_format_position_mm() {
        assert_eq!(
            format_position_mm(14_900_000, 18_750_000, 1_000_000),
            "[14.900, 18.750, 1.000]"
        );
    }

    #[test]
    fn test_format_distance() {
        assert_eq!(format_distance(500), "500nm");
        assert_eq!(format_distance(1_500), "1.5µm");
        assert_eq!(format_distance(254_000), "254.0µm");
        assert_eq!(format_distance(1_250_000), "1.250mm");
    }
}
