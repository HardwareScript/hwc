use hwc_parser::Unit;

pub(super) fn measurement_to_nm(measurement: &hwc_parser::Measurement) -> i64 {
    measurement.to_nanometers_i64().unwrap_or_else(|| {
        panic!(
            "measurement_to_nm: cannot convert {:?} to nanometers (not a length unit)",
            measurement.unit
        )
    })
}

pub(super) fn measurement_to_volts(measurement: &hwc_parser::Measurement) -> f64 {
    match measurement.unit {
        Unit::Volt | Unit::Millivolt | Unit::Kilovolt => {
            measurement.value * measurement.unit.base_si_multiplier().unwrap_or(1.0)
        }
        _ => panic!(
            "measurement_to_volts: cannot convert {:?} to volts (not a voltage unit)",
            measurement.unit
        ),
    }
}

pub(super) fn measurement_to_celsius(measurement: &hwc_parser::Measurement) -> f64 {
    measurement.value
}

pub(super) fn convert_to_base_unit(value: f64, unit: &Unit) -> f64 {
    unit.base_si_multiplier().map(|m| value * m).unwrap_or(value)
}
