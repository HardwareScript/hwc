use hwc_parser::Unit;

pub(super) fn measurement_to_nm(measurement: &hwc_parser::Measurement) -> i64 {
    let value_nm = match measurement.unit {
        Unit::Millimeter => measurement.value * 1_000_000.0,
        Unit::Centimeter => measurement.value * 10_000_000.0,
        Unit::Micrometer => measurement.value * 1_000.0,
        Unit::Nanometer => measurement.value,
        _ => panic!(
            "measurement_to_nm: cannot convert {:?} to nanometers (not a length unit)",
            measurement.unit
        ),
    };
    value_nm as i64
}

pub(super) fn measurement_to_volts(measurement: &hwc_parser::Measurement) -> f64 {
    match measurement.unit {
        Unit::Volt => measurement.value,
        Unit::Millivolt => measurement.value / 1_000.0,
        Unit::Kilovolt => measurement.value * 1_000.0,
        _ => panic!(
            "measurement_to_volts: cannot convert {:?} to volts (not a voltage unit)",
            measurement.unit
        ),
    }
}

pub(super) fn measurement_to_celsius(measurement: &hwc_parser::Measurement) -> f64 {
    match measurement.unit {
        Unit::Celsius => measurement.value,
        _ => measurement.value,
    }
}

pub(super) fn convert_to_base_unit(value: f64, _unit: &Unit) -> f64 {
    value
}
