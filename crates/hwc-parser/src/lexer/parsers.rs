//! Parsing callbacks for native SI unit measurements
//!
//! # PHYSICS-FIRST ARCHITECTURE: Why Some Units Are Hardcoded
//!
//! Hardware Script hardcodes a minimal set of "physics units" at the lexer level,
//! while all other units are resolved through the standard library. This is a
//! deliberate architectural decision, not a limitation.
//!
//! ## Hardcoded "Physics Units" (Lexer Level)
//!
//! These units represent fundamental physical quantities that the compiler MUST
//! understand to perform its core functions:
//!
//! - **Distance** (mm, cm, µm/um, nm): Grid calculations, routing, collision detection
//! - **Voltage** (V, mV, kV): Safety clearance validation (IPC-2221)
//! - **Current** (A, mA, µA/uA): Trace width calculations (IPC-2221)
//! - **Temperature** (C, °C): Thermal limit validation
//!
//! These are NOT "convenience features" - they are NECESSARY for the compiler to
//! understand basic hardware geometry and safety constraints. A hardware file
//! without distance units is physically meaningless.
//!
//! ## Why This Differs from Rust/C
//!
//! Unlike general-purpose languages (Rust, C, Python), Hardware Script describes
//! **physical reality**. You cannot design hardware without fundamental physical
//! units, just as you cannot write physics equations without meters and seconds.
//!
//! Comparison:
//! - Rust: `HashMap` is a data structure (optional, library-defined)
//! - Hardware Script: `mm` is a physical unit (mandatory, reality-defined)
//!
//! ## All Other Units via Standard Library
//!
//! Everything beyond core physics units is defined in `stdlib/primitives/units.hw`:
//! - Resistance: Ω, kΩ, MΩ, GΩ
//! - Capacitance: F, µF, nF, pF
//! - Inductance: H, µH, mH, nH
//! - Frequency: Hz, kHz, MHz, GHz
//! - Custom units: User-defined
//!
//! This keeps the compiler lean while supporting unlimited extensibility.
//!
//! ## ASCII Aliases for Unicode Symbols
//!
//! For ergonomics, we provide ASCII alternatives for Unicode symbols:
//! - `µm` or `um` → Micrometers
//! - `µA` or `uA` → Microamperes
//! - `°C` or `C` → Celsius
//!
//! This ensures Hardware Script works in any text editor, even those with poor
//! Unicode support.

use super::token::Token;
use super::units::*;

/// Parse a generic measurement: NUMBER immediately followed by UNIT_STRING (no space)
/// This is the universal parser that handles all measurements, including unknown units.
/// Known physics units are mapped to their specific enums, unknown units become Custom(String).
pub fn parse_generic_measurement(lex: &mut logos::Lexer<Token>) -> Option<Measurement> {
    let text = lex.slice();

    // Find where the unit starts (first non-digit, non-dot, non-sign, non-e/E character)
    let mut split_idx = 0;
    let chars: Vec<char> = text.chars().collect();

    for (i, &ch) in chars.iter().enumerate() {
        if ch.is_ascii_digit() || ch == '.' || ch == '-' || ch == '+' {
            split_idx = i + 1;
        } else if ch == 'e' || ch == 'E' {
            // Scientific notation: check if next char is +/- or digit
            if i + 1 < chars.len() {
                let next = chars[i + 1];
                if next == '+' || next == '-' || next.is_ascii_digit() {
                    split_idx = i + 1;
                    continue;
                }
            }
            break;
        } else {
            // Found start of unit
            break;
        }
    }

    if split_idx == 0 || split_idx >= text.len() {
        return None;
    }

    let num_str = &text[..split_idx];
    let unit_str = &text[split_idx..];

    if num_str.is_empty() || unit_str.is_empty() {
        return None;
    }

    let value = num_str.parse::<f64>().ok()?;

    // Match ONLY the 4 core units the compiler needs for geometry/safety
    // Everything else (including resistance, capacitance, inductance, frequency, etc.)
    // becomes Custom(String) and is defined in stdlib/units.hw
    //
    // IMPORTANT: Check for compound units first (e.g., cm2/Vs should not match "cm")
    // If the unit string contains digits after the base unit, it's a compound unit
    let has_trailing_chars = unit_str.len() > 2
        && unit_str
            .chars()
            .nth(2)
            .is_some_and(|c| c.is_ascii_digit() || c == '/' || c == '²' || c == '³');

    let unit = if has_trailing_chars {
        // Compound unit - treat as Custom
        Unit::Custom(unit_str.to_string())
    } else {
        match unit_str {
            // === CORE COMPILER UNITS ===

            // Distance (needed for placement and routing)
            "mm" => Unit::Distance(DistanceUnit::Millimeters),
            "cm" => Unit::Distance(DistanceUnit::Centimeters),
            "µm" | "um" => Unit::Distance(DistanceUnit::Micrometers),
            "nm" => Unit::Distance(DistanceUnit::Nanometers),
            "pm" => Unit::Distance(DistanceUnit::Picometers),

            // Voltage (needed for safety clearances)
            "kV" => Unit::Voltage(VoltageUnit::Kilovolts),
            "mV" => Unit::Voltage(VoltageUnit::Millivolts),
            "V" => Unit::Voltage(VoltageUnit::Volts),

            // Current (needed for trace width calculations)
            "mA" => Unit::Current(CurrentUnit::Milliamperes),
            "µA" | "uA" => Unit::Current(CurrentUnit::Microamperes),
            "A" => Unit::Current(CurrentUnit::Amperes),

            // Temperature (needed for thermal calculations)
            "C" | "°C" => Unit::Temperature(TemperatureUnit::Celsius),

            // === EVERYTHING ELSE → CUSTOM (defined in stdlib/units.hw) ===
            // This includes:
            // - Resistance: Ω, kΩ, MΩ, GΩ
            // - Capacitance: F, µF, nF, pF
            // - Inductance: H, µH, mH
            // - Frequency: Hz, kHz, MHz, GHz
            // - Tolerance: %, ppm, ppb
            // - Battery: mAh, Ah
            // - Power: W, mW, kW
            // - Signal: dBm, dBµV
            // - Material: kg/m³, W/mK, Ω·m, A/mm², V/m, cm2/Vs, eV
            // - Angle: °, rad, mrad
            // - Wire: AWG, SWG
            // - And any future user-defined units
            _ => Unit::Custom(unit_str.to_string()),
        }
    };

    Some(Measurement { value, unit })
}


/// Parse multi-line documentation block: ##[ ... ]##
pub fn parse_doc_block(lex: &mut logos::Lexer<Token>) -> Option<String> {
    let remainder = lex.remainder();

    // Find the closing ]## (whitespace before it is optional)
    if let Some(end_pos) = remainder.find("]##") {
        let content = &remainder[..end_pos];
        // Bump the lexer past the content and closing delimiter
        lex.bump(end_pos + 3); // content + "]##"
        Some(content.trim().to_string())
    } else {
        // Unclosed doc block - return None to signal error
        None
    }
}
