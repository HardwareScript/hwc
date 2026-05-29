//! Electrical Symbol Table for Logic Synthesis
//!
//! Tracks wires, pins, and their properties during logic synthesis.
//! This is separate from the main SymbolTable and is used specifically
//! for validating logic blocks.

use crate::span_utils::{default_span, span_to_source_span};
use compact_str::CompactString;
use hwc_parser::Span;
use miette::{Diagnostic, SourceSpan};
use rustc_hash::FxHashMap;
use thiserror::Error;

/// Errors that can occur during electrical symbol resolution
#[derive(Error, Debug, Clone, Diagnostic)]
pub enum ElectricalSymbolError {
    #[error("Unbound wire '{name}' in logic block")]
    #[diagnostic(
        code(L01),
        url("https://docs.hw-script.org/errors/L01"),
        help("Check your pin declarations and let statements.\nAvailable wires: {available}")
    )]
    UnboundWire {
        #[label("undefined wire used here")]
        span: SourceSpan,
        name: CompactString,
        available: CompactString,
    },

    #[error("Wire '{name}' already declared")]
    #[diagnostic(
        code(L05),
        url("https://docs.hw-script.org/errors/L05"),
        help("Each wire name must be unique within a logic block")
    )]
    DuplicateWire {
        #[label("duplicate wire declared here")]
        span: SourceSpan,
        #[label("first declared here")]
        first_span: Option<SourceSpan>,
        name: CompactString,
    },

    #[error("Cannot assign to input pin '{name}'")]
    #[diagnostic(
        code(L05),
        url("https://docs.hw-script.org/errors/L05"),
        help("Input pins are read-only. Use an internal wire or output pin for assignments")
    )]
    AssignToInput {
        #[label("cannot assign to input pin")]
        span: SourceSpan,
        name: CompactString,
    },

    #[error("Register '{name}' not found")]
    #[diagnostic(
        code(L06),
        url("https://docs.hw-script.org/errors/L06"),
        help("Declare the register with: let {name} = reg(clock: Clk, reset: Rst, init: 0)")
    )]
    RegisterNotFound {
        #[label("register not found")]
        span: SourceSpan,
        name: CompactString,
    },

    #[error("Clock domain crossing detected")]
    #[diagnostic(
        code(L04),
        url("https://docs.hw-script.org/errors/L04"),
        help("Use a synchronizer to safely cross clock domains")
    )]
    ClockDomainCrossing(Box<ClockDomainCrossingDetails>),

    #[error("Multiple clock domains in module")]
    #[diagnostic(
        code(L04),
        url("https://docs.hw-script.org/errors/L04"),
        help(
            "All registers in a module should use the same clock signal.\nFound domains: {domains}"
        )
    )]
    MultipleClockDomains {
        #[label("conflicting clock domain used here")]
        span: SourceSpan,
        domains: CompactString,
    },
}

/// Details for clock domain crossing errors (boxed to reduce enum size)
#[derive(Error, Debug, Clone, Diagnostic)]
#[error("Clock domain crossing detected")]
#[diagnostic(code(L04), url("https://docs.hw-script.org/errors/L04"))]
pub struct ClockDomainCrossingDetails {
    #[label("source wire (domain: {source_domain})")]
    pub source_span: SourceSpan,
    #[label("target wire (domain: {target_domain})")]
    pub target_span: SourceSpan,
    pub source_wire: CompactString,
    pub source_domain: CompactString,
    pub target_wire: CompactString,
    pub target_domain: CompactString,
}

/// Type of wire in the electrical symbol table
#[derive(Debug, Clone, PartialEq)]
pub enum WireType {
    /// Input pin from module declaration
    InputPin,
    /// Output pin from module declaration
    OutputPin,
    /// Internal wire from let statement
    InternalWire,
    /// Register (flip-flop) - has both current state and .next
    Register,
}

/// Wire entry in the electrical symbol table
#[derive(Debug, Clone)]
pub struct WireEntry {
    pub name: CompactString,
    pub wire_type: WireType,
    pub bit_width: Option<usize>,
    pub mutable: bool,
    pub clock_domain: Option<CompactString>,
    pub span: Option<SourceSpan>,
}

/// Electrical Symbol Table for tracking wires and pins in logic blocks
#[derive(Debug, Clone)]
pub struct ElectricalSymbolTable {
    /// Map of wire name to wire entry
    wires: FxHashMap<CompactString, WireEntry>,
    /// Map of clock domain name to list of registers using it
    clock_domains: FxHashMap<CompactString, Vec<CompactString>>,
}

impl ElectricalSymbolTable {
    /// Create a new electrical symbol table
    pub fn new() -> Self {
        Self {
            wires: FxHashMap::default(),
            clock_domains: FxHashMap::default(),
        }
    }

    /// Add an input pin from module declaration
    pub fn add_input_pin(
        &mut self,
        name: CompactString,
        bit_width: Option<usize>,
        span: Option<Span>,
    ) -> Result<(), ElectricalSymbolError> {
        let source_span = span.as_ref().map(span_to_source_span);

        if let Some(existing) = self.wires.get(&name) {
            return Err(ElectricalSymbolError::DuplicateWire {
                span: source_span.unwrap_or_else(default_span),
                first_span: existing.span,
                name,
            });
        }

        self.wires.insert(
            name.clone(),
            WireEntry {
                name,
                wire_type: WireType::InputPin,
                bit_width,
                mutable: false,
                clock_domain: None,
                span: source_span,
            },
        );

        Ok(())
    }

    /// Add an output pin from module declaration
    pub fn add_output_pin(
        &mut self,
        name: CompactString,
        bit_width: Option<usize>,
        span: Option<Span>,
    ) -> Result<(), ElectricalSymbolError> {
        let source_span = span.as_ref().map(span_to_source_span);

        if let Some(existing) = self.wires.get(&name) {
            return Err(ElectricalSymbolError::DuplicateWire {
                span: source_span.unwrap_or_else(default_span),
                first_span: existing.span,
                name,
            });
        }

        self.wires.insert(
            name.clone(),
            WireEntry {
                name,
                wire_type: WireType::OutputPin,
                bit_width,
                mutable: true, // Output pins can be assigned
                clock_domain: None,
                span: source_span,
            },
        );

        Ok(())
    }

    /// Add an internal wire from let statement
    pub fn add_internal_wire(
        &mut self,
        name: CompactString,
        bit_width: Option<usize>,
        mutable: bool,
        span: Option<Span>,
    ) -> Result<(), ElectricalSymbolError> {
        let source_span = span.as_ref().map(span_to_source_span);

        if let Some(existing) = self.wires.get(&name) {
            return Err(ElectricalSymbolError::DuplicateWire {
                span: source_span.unwrap_or_else(default_span),
                first_span: existing.span,
                name,
            });
        }

        self.wires.insert(
            name.clone(),
            WireEntry {
                name,
                wire_type: WireType::InternalWire,
                bit_width,
                mutable,
                clock_domain: None,
                span: source_span,
            },
        );

        Ok(())
    }

    /// Add a register (flip-flop)
    pub fn add_register(
        &mut self,
        name: CompactString,
        bit_width: Option<usize>,
        clock_domain: CompactString,
        span: Option<Span>,
    ) -> Result<(), ElectricalSymbolError> {
        let source_span = span.as_ref().map(span_to_source_span);

        if let Some(existing) = self.wires.get(&name) {
            return Err(ElectricalSymbolError::DuplicateWire {
                span: source_span.unwrap_or_else(default_span),
                first_span: existing.span,
                name,
            });
        }

        // Track this register in the clock domain
        self.clock_domains
            .entry(clock_domain.clone())
            .or_default()
            .push(name.clone());

        self.wires.insert(
            name.clone(),
            WireEntry {
                name,
                wire_type: WireType::Register,
                bit_width,
                mutable: true, // Registers can be assigned via .next
                clock_domain: Some(clock_domain),
                span: source_span,
            },
        );

        Ok(())
    }

    /// Check if a wire exists
    ///
    /// Compiler-generated synthetic wires (prefixed with '_') are automatically considered
    /// to exist, allowing the parser to create internal wires without explicit registration.
    pub fn contains(&self, name: &str) -> bool {
        // Synthetic wires (prefixed with '_') are implicitly trusted
        if name.starts_with('_') {
            return true;
        }
        self.wires.contains_key(name)
    }

    /// Get a wire entry
    pub fn get(&self, name: &str) -> Option<&WireEntry> {
        self.wires.get(name)
    }

    /// Validate that a wire exists, return error with available wires if not
    pub fn validate_wire(
        &self,
        name: &str,
        span: Span,
    ) -> Result<&WireEntry, ElectricalSymbolError> {
        self.wires.get(name).ok_or_else(|| {
            let mut available: Vec<CompactString> = self.wires.keys().cloned().collect();
            available.sort();
            let available_str = if available.is_empty() {
                "none".into()
            } else {
                available.join(", ")
            };
            ElectricalSymbolError::UnboundWire {
                span: span_to_source_span(&span),
                name: name.to_string().into(),
                available: available_str.into(),
            }
        })
    }

    /// Validate that a wire can be assigned to
    pub fn validate_assignment(&self, name: &str, span: Span) -> Result<(), ElectricalSymbolError> {
        let entry = self.validate_wire(name, span)?;
        let source_span = span_to_source_span(&span);

        if entry.wire_type == WireType::InputPin {
            return Err(ElectricalSymbolError::AssignToInput {
                span: source_span,
                name: name.into(),
            });
        }

        if !entry.mutable {
            return Err(ElectricalSymbolError::UnboundWire {
                span: source_span,
                name: format!("{} (immutable)", name).into(),
                available: "Use 'let mut' for mutable wires".into(),
            });
        }

        Ok(())
    }

    /// Get all wire names (for error messages)
    pub fn get_all_wires(&self) -> Vec<CompactString> {
        self.wires.keys().cloned().collect()
    }

    /// Get bit width of a wire
    pub fn get_bit_width(&self, name: &str) -> Option<usize> {
        self.wires.get(name).and_then(|entry| entry.bit_width)
    }

    /// Get clock domain of a wire (if it's a register)
    pub fn get_clock_domain(&self, name: &str) -> Option<&str> {
        self.wires
            .get(name)
            .and_then(|entry| entry.clock_domain.as_deref())
    }

    /// Get all clock domains used in this module
    pub fn get_clock_domains(&self) -> Vec<CompactString> {
        self.clock_domains.keys().cloned().collect()
    }

    /// Get all registers in a specific clock domain
    pub fn get_registers_in_domain(&self, domain: &str) -> Vec<CompactString> {
        self.clock_domains.get(domain).cloned().unwrap_or_default()
    }

    /// Detect clock domain crossing: check if expression uses wires from different domains
    pub fn detect_clock_crossing(
        &self,
        source_wire: &str,
        target_wire: &str,
    ) -> Option<(String, String)> {
        let source_domain = self.get_clock_domain(source_wire)?;
        let target_domain = self.get_clock_domain(target_wire)?;

        if source_domain != target_domain {
            Some((source_domain.into(), target_domain.into()))
        } else {
            None
        }
    }
}

impl Default for ElectricalSymbolTable {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_input_pin() {
        let mut table = ElectricalSymbolTable::new();
        assert!(table.add_input_pin("A".into(), Some(8), None).is_ok());
        assert!(table.contains("A"));

        let entry = table.get("A").unwrap();
        assert_eq!(entry.wire_type, WireType::InputPin);
        assert_eq!(entry.bit_width, Some(8));
    }

    #[test]
    fn test_duplicate_wire() {
        let mut table = ElectricalSymbolTable::new();
        assert!(table.add_input_pin("A".into(), Some(8), None).is_ok());
        assert!(table.add_input_pin("A".into(), Some(8), None).is_err());
    }

    #[test]
    fn test_validate_wire() {
        let mut table = ElectricalSymbolTable::new();
        table.add_input_pin("A".into(), Some(8), None).unwrap();

        let span = Span::new(0, 1);
        assert!(table.validate_wire("A", span).is_ok());
        assert!(table.validate_wire("B", span).is_err());
    }

    #[test]
    fn test_assign_to_input() {
        let mut table = ElectricalSymbolTable::new();
        table.add_input_pin("A".into(), Some(8), None).unwrap();

        let span = Span::new(0, 1);
        assert!(table.validate_assignment("A", span).is_err());
    }

    #[test]
    fn test_assign_to_output() {
        let mut table = ElectricalSymbolTable::new();
        table.add_output_pin("Out".into(), Some(8), None).unwrap();

        let span = Span::new(0, 1);
        assert!(table.validate_assignment("Out", span).is_ok());
    }
}
