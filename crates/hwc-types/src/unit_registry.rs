//! Unit registry for fast lookup and validation.
//!
//! This is the single source of truth for unit conversions across the compiler.
//! The registry is built once from stdlib unit definitions and shared via reference.

use compact_str::CompactString;
use rustc_hash::FxHashMap;

use super::SiDimension;

/// Minimal unit information needed for registry operations.
///
/// This is decoupled from the parser's `UnitDefinition` to avoid circular
/// crate dependencies. The compiler converts `UnitDefinition` → `UnitInfo`
/// when building the registry.
#[derive(Debug, Clone)]
pub struct UnitInfo {
    pub symbol: CompactString,
    pub aliases: Vec<CompactString>,
    pub multiplier: Option<f64>,
    pub dimension: CompactString,
    pub si_dimension: Option<SiDimension>,
}

impl UnitInfo {
    /// Convert a value in this unit to its base SI value.
    pub fn to_base_si(&self, value: f64) -> Option<f64> {
        self.multiplier.map(|m| value * m)
    }
}

/// Fast lookup registry for unit definitions loaded from stdlib/primitives/units.hw.
///
/// Built once during compilation and immutable after construction.
/// Thread-safe via `Arc` in compiler context.
#[derive(Debug, Clone)]
pub struct UnitRegistry {
    units: FxHashMap<CompactString, UnitInfo>,
}

impl UnitRegistry {
    /// Create a new registry from unit definitions.
    pub fn new(definitions: Vec<UnitInfo>) -> Self {
        let mut units = FxHashMap::default();
        for def in definitions {
            units.insert(def.symbol.clone(), def.clone());
            for alias in &def.aliases {
                units.insert(alias.clone(), def.clone());
            }
        }
        Self { units }
    }

    /// Build standard SI unit registry containing all base and compound units
    pub fn standard() -> Self {
        let mut defs = Vec::new();
        let mut add = |sym: &str, aliases: &[&str], mul: f64, dim_name: &str, si_dim: SiDimension| {
            defs.push(UnitInfo {
                symbol: sym.into(),
                aliases: aliases.iter().map(|&s| s.into()).collect(),
                multiplier: Some(mul),
                dimension: dim_name.into(),
                si_dimension: Some(si_dim),
            });
        };

        // Length
        add("pm", &[], 1e-12, "length", SiDimension::LENGTH);
        add("nm", &[], 1e-9, "length", SiDimension::LENGTH);
        add("um", &["µm", "u"], 1e-6, "length", SiDimension::LENGTH);
        add("mm", &[], 1e-3, "length", SiDimension::LENGTH);
        add("cm", &[], 1e-2, "length", SiDimension::LENGTH);
        add("m", &[], 1.0, "length", SiDimension::LENGTH);
        add("mil", &[], 0.0000254, "length", SiDimension::LENGTH);
        add("in", &[], 0.0254, "length", SiDimension::LENGTH);

        // Area (L^2)
        add("pm2", &["pm²"], 1e-24, "area", SiDimension::AREA);
        add("nm2", &["nm²"], 1e-18, "area", SiDimension::AREA);
        add("um2", &["um²", "µm2", "µm²"], 1e-12, "area", SiDimension::AREA);
        add("mm2", &["mm²"], 1e-6, "area", SiDimension::AREA);
        add("cm2", &["cm²"], 1e-4, "area", SiDimension::AREA);
        add("m2", &["m²"], 1.0, "area", SiDimension::AREA);

        // Volume (L^3)
        add("um3", &["um³", "µm3", "µm³"], 1e-18, "volume", SiDimension::VOLUME);
        add("mm3", &["mm³"], 1e-9, "volume", SiDimension::VOLUME);
        add("m3", &["m³"], 1.0, "volume", SiDimension::VOLUME);

        // Time
        add("fs", &[], 1e-15, "time", SiDimension::TIME);
        add("ps", &[], 1e-12, "time", SiDimension::TIME);
        add("ns", &[], 1e-9, "time", SiDimension::TIME);
        add("us", &["µs"], 1e-6, "time", SiDimension::TIME);
        add("ms", &[], 1e-3, "time", SiDimension::TIME);
        add("s", &[], 1.0, "time", SiDimension::TIME);

        // Current
        add("pA", &[], 1e-12, "current", SiDimension::CURRENT);
        add("nA", &[], 1e-9, "current", SiDimension::CURRENT);
        add("uA", &["µA"], 1e-6, "current", SiDimension::CURRENT);
        add("mA", &[], 1e-3, "current", SiDimension::CURRENT);
        add("A", &[], 1.0, "current", SiDimension::CURRENT);

        // Voltage
        add("nV", &[], 1e-9, "voltage", SiDimension::VOLTAGE);
        add("uV", &["µV"], 1e-6, "voltage", SiDimension::VOLTAGE);
        add("mV", &[], 1e-3, "voltage", SiDimension::VOLTAGE);
        add("V", &[], 1.0, "voltage", SiDimension::VOLTAGE);
        add("kV", &[], 1e3, "voltage", SiDimension::VOLTAGE);

        // Resistance
        add("uOhm", &["uohm", "µΩ", "uΩ"], 1e-6, "resistance", SiDimension::RESISTANCE);
        add("mOhm", &["mohm", "mΩ"], 1e-3, "resistance", SiDimension::RESISTANCE);
        add("Ohm", &["ohm", "Ω"], 1.0, "resistance", SiDimension::RESISTANCE);
        add("kOhm", &["kohm", "kΩ"], 1e3, "resistance", SiDimension::RESISTANCE);
        add("MOhm", &["megohm", "MΩ"], 1e6, "resistance", SiDimension::RESISTANCE);

        // Sheet Resistance (Ohm/sq)
        add("Ohm_sq", &["ohm_sq", "Ω/sq", "Ohm/sq"], 1.0, "sheet_resistance", SiDimension::SHEET_RES);
        add("mOhm_sq", &["mohm_sq", "mΩ/sq"], 1e-3, "sheet_resistance", SiDimension::SHEET_RES);
        add("kOhm_sq", &["kohm_sq", "kΩ/sq"], 1e3, "sheet_resistance", SiDimension::SHEET_RES);

        // Capacitance
        add("aF", &[], 1e-18, "capacitance", SiDimension::CAPACITANCE);
        add("fF", &[], 1e-15, "capacitance", SiDimension::CAPACITANCE);
        add("pF", &[], 1e-12, "capacitance", SiDimension::CAPACITANCE);
        add("nF", &[], 1e-9, "capacitance", SiDimension::CAPACITANCE);
        add("uF", &["µF"], 1e-6, "capacitance", SiDimension::CAPACITANCE);
        add("mF", &[], 1e-3, "capacitance", SiDimension::CAPACITANCE);
        add("F", &[], 1.0, "capacitance", SiDimension::CAPACITANCE);

        // Capacitance Density (Capacitance / Area)
        add("fF_um2", &["fF/um2", "fF/µm²", "fF_um²"], 1e-3, "capacitance_density", SiDimension::CAPACITANCE_DENSITY);
        add("aF_um2", &["aF/um2", "aF/µm²", "aF_um²"], 1e-6, "capacitance_density", SiDimension::CAPACITANCE_DENSITY);
        add("pC_um2", &["pC/um2"], 1e-6, "capacitance_density", SiDimension::CAPACITANCE_DENSITY);

        // Inductance
        add("pH", &[], 1e-12, "inductance", SiDimension::INDUCTANCE);
        add("nH", &[], 1e-9, "inductance", SiDimension::INDUCTANCE);
        add("uH", &["µH"], 1e-6, "inductance", SiDimension::INDUCTANCE);
        add("mH", &[], 1e-3, "inductance", SiDimension::INDUCTANCE);
        add("H", &[], 1.0, "inductance", SiDimension::INDUCTANCE);

        // Power
        add("pW", &[], 1e-12, "power", SiDimension::POWER);
        add("nW", &[], 1e-9, "power", SiDimension::POWER);
        add("uW", &["µW"], 1e-6, "power", SiDimension::POWER);
        add("mW", &[], 1e-3, "power", SiDimension::POWER);
        add("W", &[], 1.0, "power", SiDimension::POWER);
        add("kW", &[], 1e3, "power", SiDimension::POWER);

        // Frequency
        add("Hz", &[], 1.0, "frequency", SiDimension::FREQUENCY);
        add("kHz", &[], 1e3, "frequency", SiDimension::FREQUENCY);
        add("MHz", &[], 1e6, "frequency", SiDimension::FREQUENCY);
        add("GHz", &[], 1e9, "frequency", SiDimension::FREQUENCY);

        // Angle
        add("deg", &["°"], 1.0, "angle", SiDimension::ANGLE);
        add("rad", &[], 180.0 / std::f64::consts::PI, "angle", SiDimension::ANGLE);

        // Temperature
        add("K", &[], 1.0, "temperature", SiDimension::TEMPERATURE);
        add("mK", &[], 1e-3, "temperature", SiDimension::TEMPERATURE);
        add("C", &["°C", "degC"], 1.0, "temperature", SiDimension::TEMPERATURE);

        Self::new(defs)
    }

    /// Check if a unit string is defined.
    pub fn is_defined(&self, unit_str: &str) -> bool {
        self.units.contains_key(unit_str)
    }

    /// Get the unit info for a unit string.
    pub fn get(&self, unit_str: &str) -> Option<&UnitInfo> {
        self.units.get(unit_str)
    }

    /// Convert a value to its base SI unit.
    pub fn to_base_si(&self, value: f64, unit_str: &str) -> Option<f64> {
        self.get(unit_str).and_then(|def| def.to_base_si(value))
    }

    /// Get the dimension of a unit (e.g., "current", "voltage").
    pub fn get_dimension(&self, unit_str: &str) -> Option<&str> {
        self.get(unit_str).map(|def| def.dimension.as_str())
    }

    /// Validate that a unit matches the expected dimension.
    pub fn validate_dimension(
        &self,
        unit_str: &str,
        expected_dimension: &str,
    ) -> Result<(), String> {
        match self.get_dimension(unit_str) {
            Some(dim) if dim == expected_dimension => Ok(()),
            Some(dim) => Err(format!(
                "Unit '{}' has dimension '{}', expected '{}'",
                unit_str, dim, expected_dimension
            )),
            None => Err(format!("Unit '{}' is not defined", unit_str)),
        }
    }

    /// Convert with dimension validation.
    pub fn convert_with_validation(
        &self,
        value: f64,
        unit_str: &str,
        expected_dimension: &str,
    ) -> Result<f64, String> {
        self.validate_dimension(unit_str, expected_dimension)?;
        self.to_base_si(value, unit_str)
            .ok_or_else(|| format!("Cannot convert unit '{}' (no multiplier defined)", unit_str))
    }

    /// Picometers per SI base meter (1 m = 10^12 pm)
    pub const PICOMETERS_PER_METER: f64 = 1_000_000_000_000.0;

    /// Nanometers per SI base meter (1 m = 10^9 nm)
    pub const NANOMETERS_PER_METER: f64 = 1_000_000_000.0;

    /// Convert a distance measurement value to picometers using base SI meters.
    pub fn to_picometers(&self, value: f64, unit_str: &str) -> Result<i64, String> {
        let meters = self.convert_with_validation(value, unit_str, "length")?;
        Ok((meters * Self::PICOMETERS_PER_METER).round() as i64)
    }

    /// Convert a distance measurement value to nanometers using base SI meters.
    pub fn to_nanometers(&self, value: f64, unit_str: &str) -> Result<i64, String> {
        let meters = self.convert_with_validation(value, unit_str, "length")?;
        Ok((meters * Self::NANOMETERS_PER_METER).round() as i64)
    }

    /// Get all registered unit symbols.
    pub fn all_symbols(&self) -> Vec<&str> {
        self.units.keys().map(|s| s.as_str()).collect()
    }

    /// Get the number of registered units.
    pub fn len(&self) -> usize {
        self.units.len()
    }

    /// Check if the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.units.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_unit(
        symbol: &str,
        aliases: Vec<&str>,
        multiplier: Option<f64>,
        dimension: &str,
    ) -> UnitInfo {
        UnitInfo {
            symbol: symbol.into(),
            aliases: aliases.into_iter().map(|s| s.into()).collect(),
            multiplier,
            dimension: dimension.into(),
        }
    }

    #[test]
    fn test_registry_lookup() {
        let units = vec![
            test_unit("mA", vec!["milliamp"], Some(1e-3), "current"),
            test_unit("µA", vec!["uA"], Some(1e-6), "current"),
        ];
        let registry = UnitRegistry::new(units);

        assert!(registry.is_defined("mA"));
        assert!(registry.is_defined("milliamp"));
        assert!(registry.is_defined("µA"));
        assert!(!registry.is_defined("kA"));
    }

    #[test]
    fn test_registry_conversion() {
        let units = vec![test_unit("mA", vec![], Some(1e-3), "current")];
        let registry = UnitRegistry::new(units);

        let result = registry.to_base_si(100.0, "mA").unwrap();
        assert!((result - 0.1).abs() < 1e-10);
    }

    #[test]
    fn test_dimension_validation() {
        let units = vec![
            test_unit("mA", vec![], Some(1e-3), "current"),
            test_unit("mV", vec![], Some(1e-3), "voltage"),
        ];
        let registry = UnitRegistry::new(units);

        assert!(registry.validate_dimension("mA", "current").is_ok());
        assert!(registry.validate_dimension("mA", "voltage").is_err());
        assert!(registry.validate_dimension("mV", "voltage").is_ok());
    }

    #[test]
    fn test_convert_with_validation() {
        let units = vec![test_unit("mA", vec![], Some(1e-3), "current")];
        let registry = UnitRegistry::new(units);

        let result = registry
            .convert_with_validation(50.0, "mA", "current")
            .unwrap();
        assert!((result - 0.05).abs() < 1e-10);

        assert!(registry
            .convert_with_validation(50.0, "mA", "voltage")
            .is_err());
    }
}
