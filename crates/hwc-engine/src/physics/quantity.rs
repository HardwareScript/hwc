// crates/hwc-engine/src/physics/quantity.rs
use serde::{Deserialize, Serialize};

/// Strongly-typed physical quantity with self-formatting SPICE semantics.
/// Every value carries its exact physical dimension and SI base magnitude.
/// ZERO string inspection. ZERO name guessing.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum PhysicalQuantity {
    /// Physical distance / spatial extent (stored in base meters: m)
    Length(f64),
    
    /// Surface area (stored in base square meters: m²)
    Area(f64),
    
    /// Electrical resistance (stored in base Ohms: Ω)
    Resistance(f64),
    
    /// Electrostatic capacitance (stored in base Farads: F)
    Capacitance(f64),
    
    /// Magnetic inductance (stored in base Henries: H)
    Inductance(f64),
    
    /// Pure scalar / ratio (dimensionless: number of squares, multipliers, etc.)
    Dimensionless(f64),
}

impl PhysicalQuantity {
    /// Formats the quantity into standard SPICE engineering representation.
    /// ZERO knowledge of parameter names. Formatting is driven purely by physical dimensions.
    pub fn to_spice_repr(&self) -> String {
        match self {
            Self::Length(meters) => {
                // SPICE standard for semiconductor layout dimensions: micrometers (u)
                let um = meters * 1e6;
                format!("{:.2}u", um)
            }
            
            Self::Area(m2) => {
                // SPICE standard for semiconductor diffusion area: picometers² (p) -> 10^-12 m²
                let p_units = m2 * 1e12;
                format!("{:.2}p", p_units)
            }
            
            Self::Resistance(ohms) => {
                if *ohms < 1e3 {
                    format!("{:.2}", ohms)
                } else if *ohms < 1e6 {
                    format!("{:.2}k", ohms / 1e3)
                } else {
                    format!("{:.2}meg", ohms / 1e6)
                }
            }
            
            Self::Capacitance(farads) => {
                if *farads < 1e-12 {
                    format!("{:.2}f", farads * 1e15)
                } else if *farads < 1e-9 {
                    format!("{:.2}p", farads * 1e12)
                } else {
                    format!("{:.2}e", farads)
                }
            }
            
            Self::Inductance(henries) => {
                if *henries < 1e-9 {
                    format!("{:.2}p", henries * 1e12)
                } else if *henries < 1e-6 {
                    format!("{:.2}n", henries * 1e9)
                } else {
                    format!("{:.2}u", henries * 1e6)
                }
            }
            
            Self::Dimensionless(val) => {
                format!("{:.2}", val)
            }
        }
    }
    
    /// Access the raw SI base magnitude (m, m², Ω, F, H, 1)
    pub fn as_raw_si(&self) -> f64 {
        match self {
            Self::Length(v)
            | Self::Area(v)
            | Self::Resistance(v)
            | Self::Capacitance(v)
            | Self::Inductance(v)
            | Self::Dimensionless(v) => *v,
        }
    }
}

// Algebraic dimensional division: Length / Length = Dimensionless, etc.
impl std::ops::Div for PhysicalQuantity {
    type Output = Result<Self, String>;

    fn div(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (Self::Length(a), Self::Length(b)) => {
                if b.abs() <= 1e-15 {
                    return Err("Division by zero length in metric algebra".into());
                }
                Ok(Self::Dimensionless(a / b))
            }
            (Self::Area(a), Self::Area(b)) => {
                if b.abs() <= 1e-24 {
                    return Err("Division by zero area in metric algebra".into());
                }
                Ok(Self::Dimensionless(a / b))
            }
            (Self::Resistance(a), Self::Resistance(b)) => {
                if b.abs() <= 1e-12 {
                    return Err("Division by zero resistance in metric algebra".into());
                }
                Ok(Self::Dimensionless(a / b))
            }
            (Self::Length(a), Self::Dimensionless(b)) => {
                if b.abs() <= 1e-15 {
                    return Err("Division by zero scalar in metric algebra".into());
                }
                Ok(Self::Length(a / b))
            }
            (Self::Area(a), Self::Dimensionless(b)) => {
                if b.abs() <= 1e-15 {
                    return Err("Division by zero scalar in metric algebra".into());
                }
                Ok(Self::Area(a / b))
            }
            (Self::Resistance(a), Self::Dimensionless(b)) => {
                if b.abs() <= 1e-15 {
                    return Err("Division by zero scalar in metric algebra".into());
                }
                Ok(Self::Resistance(a / b))
            }
            (Self::Dimensionless(a), Self::Dimensionless(b)) => {
                if b.abs() <= 1e-15 {
                    return Err("Division by zero scalar in metric algebra".into());
                }
                Ok(Self::Dimensionless(a / b))
            }
            (lhs, rhs) => Err(format!(
                "Invalid dimensional division: cannot divide {:?} by {:?}",
                lhs, rhs
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_length_formatting() {
        let length = PhysicalQuantity::Length(1e-6); // 1 micrometer
        assert_eq!(length.to_spice_repr(), "1.00u");
    }

    #[test]
    fn test_area_formatting() {
        let area = PhysicalQuantity::Area(1e-12); // 1 pm²
        assert_eq!(area.to_spice_repr(), "1.00p");
    }

    #[test]
    fn test_resistance_formatting() {
        assert_eq!(PhysicalQuantity::Resistance(100.0).to_spice_repr(), "100.00");
        assert_eq!(PhysicalQuantity::Resistance(10_000.0).to_spice_repr(), "10.00k");
        assert_eq!(PhysicalQuantity::Resistance(2_000_000.0).to_spice_repr(), "2.00meg");
    }

    #[test]
    fn test_capacitance_formatting() {
        assert_eq!(PhysicalQuantity::Capacitance(1e-15).to_spice_repr(), "1.00f");
        assert_eq!(PhysicalQuantity::Capacitance(1e-12).to_spice_repr(), "1.00p");
    }

    #[test]
    fn test_dimensionless_formatting() {
        let scalar = PhysicalQuantity::Dimensionless(2.5);
        assert_eq!(scalar.to_spice_repr(), "2.50");
    }

    #[test]
    fn test_dimensional_division() {
        let length_a = PhysicalQuantity::Length(650e-9);
        let length_b = PhysicalQuantity::Length(1000e-9);
        let ratio = (length_a / length_b).unwrap();
        assert_eq!(ratio, PhysicalQuantity::Dimensionless(0.65));
        assert_eq!(ratio.to_spice_repr(), "0.65");

        let area_a = PhysicalQuantity::Area(2e-12);
        let area_b = PhysicalQuantity::Area(1e-12);
        assert_eq!((area_a / area_b).unwrap(), PhysicalQuantity::Dimensionless(2.0));

        let res_a = PhysicalQuantity::Resistance(1000.0);
        let scalar = PhysicalQuantity::Dimensionless(2.0);
        assert_eq!((res_a / scalar).unwrap(), PhysicalQuantity::Resistance(500.0));
    }
}
