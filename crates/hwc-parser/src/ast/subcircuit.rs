//! Native Typed Subcircuit Definitions (v0.3.0+)
//!
//! Replaces raw SPICE string blobs with typed, validated AST structures.
//! This enables compile-time validation, multi-simulator export, and IDE autocomplete.
//!
//! Philosophy: Zero Compiler Magic + Type Safety
//! - All elements are typed and validated at parse time
//! - Units are explicit and checked (350.0ohm, 2.0fF)
//! - Expressions are evaluated with dimensional analysis
//! - SPICE/Spectre/Verilog-A export is generated from AST
//!
//! Example:
//! ```hw
//! export subcircuit sky130_fd_pr__res_high_po:
//!     terminals: [PLUS, MINUS, BULK]
//!     parameters: [W = 1.0um, L = 1.0um]
//!     elements:
//!         R_head: Resistor(PLUS, node_1, val: 362.0ohm)
//!         R_body: Resistor(node_1, node_2, val: 350.0ohm_sq * (L / W))
//!         R_tail: Resistor(node_2, MINUS, val: 362.0ohm)
//!         C_sub1: Capacitor(PLUS, BULK, val: 2.0fF_um2 * W * L)
//!         C_sub2: Capacitor(MINUS, BULK, val: 2.0fF_um2 * W * L)
//! ```

use compact_str::CompactString;
use serde::{Deserialize, Serialize};

use super::common::Identifier;
use super::expression::Expression;
use crate::lexer::Span;

/// Native typed subcircuit definition
///
/// This replaces SpiceSubcircuitDefinition's raw string body with a typed AST.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubcircuitDefinition {
    /// Subcircuit name (e.g., sky130_fd_pr__res_high_po)
    pub name: Identifier,

    /// Terminal names (e.g., [PLUS, MINUS, BULK])
    pub terminals: Vec<CompactString>,

    /// Parameters with optional default values (e.g., [W = 1.0um, L = 1.0um])
    pub parameters: Vec<SubcircuitParameter>,

    /// Internal circuit elements (resistors, capacitors, etc.)
    pub elements: Vec<SubcircuitElement>,

    /// Whether this subcircuit is exported
    pub is_exported: bool,

    /// Source location for error reporting
    pub span: Span,
}

/// Subcircuit parameter declaration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubcircuitParameter {
    /// Parameter name (e.g., W, L, MULT)
    pub name: CompactString,

    /// Optional default value with units (e.g., 1.0um, 0.18um)
    pub default_value: Option<Expression>,

    /// Source span
    pub span: Span,
}

/// Node reference in a subcircuit element
///
/// Can be a terminal, internal node, or ground.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Node {
    /// Terminal reference (e.g., PLUS, MINUS, BULK)
    Terminal(CompactString),

    /// Internal node (e.g., node_1, node_2, n_drain)
    Internal(CompactString),

    /// Ground reference (0, GND, gnd)
    Ground,
}

/// Typed circuit element in a subcircuit
///
/// This is a generic element structure that doesn't hardcode specific types.
/// The element type is just a string identifier, and the parser captures
/// the structure without validating what types are "allowed".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubcircuitElement {
    /// Element name (e.g., "R_head", "C_sub1")
    pub name: CompactString,

    /// Element type (e.g., "Resistor", "Capacitor", "Mosfet", "Subcircuit")
    pub element_type: CompactString,

    /// Ordered list of node connections
    pub nodes: Vec<Node>,

    /// Named parameters (e.g., "value" -> Expression, "W" -> Expression)
    pub parameters: Vec<(CompactString, Expression)>,

    /// Source span
    pub span: Span,
}

impl SubcircuitDefinition {
    /// Validate the subcircuit definition
    pub fn validate(&self) -> Result<(), String> {
        // Terminals must not be empty
        if self.terminals.is_empty() {
            return Err(format!(
                "Subcircuit '{}' must declare at least one terminal",
                self.name.name
            ));
        }

        // Elements must not be empty
        if self.elements.is_empty() {
            return Err(format!(
                "Subcircuit '{}' must contain at least one element",
                self.name.name
            ));
        }

        // Validate that all node references are either terminals or internal nodes
        let mut internal_nodes = std::collections::HashSet::new();
        for element in &self.elements {
            for node in &element.nodes {
                if let Node::Internal(ref name) = node {
                    internal_nodes.insert(name.clone());
                }
            }
        }

        // Ensure all terminal nodes referenced in elements actually exist
        for element in &self.elements {
            for node in &element.nodes {
                if let Node::Terminal(ref name) = node {
                    if !self.terminals.contains(name) {
                        return Err(format!(
                            "Element '{}' references undefined terminal '{}'. \
                             Declared terminals: {:?}",
                            element.name, name, self.terminals
                        ));
                    }
                }
            }
        }

        Ok(())
    }
}

impl SubcircuitElement {
    /// Get all nodes referenced by this element
    pub fn nodes(&self) -> Vec<&Node> {
        self.nodes.iter().collect()
    }
}

impl Node {
    /// Convert node to SPICE netlist format
    pub fn to_spice(&self) -> String {
        match self {
            Node::Terminal(name) => name.to_string(),
            Node::Internal(name) => name.to_string(),
            Node::Ground => "0".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_to_spice() {
        assert_eq!(Node::Terminal("PLUS".into()).to_spice(), "PLUS");
        assert_eq!(Node::Internal("node_1".into()).to_spice(), "node_1");
        assert_eq!(Node::Ground.to_spice(), "0");
    }

    #[test]
    fn test_subcircuit_validation_passes() {
        let subckt = SubcircuitDefinition {
            name: Identifier::new("test".into(), Span::new(0, 0)),
            terminals: vec!["A".into(), "B".into()],
            parameters: vec![],
            elements: vec![SubcircuitElement {
                name: "R1".into(),
                element_type: "Resistor".into(),
                nodes: vec![Node::Terminal("A".into()), Node::Terminal("B".into())],
                parameters: vec![(
                    "value".into(),
                    Expression::FloatLiteral {
                        value: 100.0,
                        span: Span::new(0, 0),
                    },
                )],
                span: Span::new(0, 0),
            }],
            is_exported: true,
            span: Span::new(0, 0),
        };

        assert!(subckt.validate().is_ok());
    }

    #[test]
    fn test_subcircuit_validation_fails_undefined_terminal() {
        let subckt = SubcircuitDefinition {
            name: Identifier::new("test".into(), Span::new(0, 0)),
            terminals: vec!["A".into(), "B".into()],
            parameters: vec![],
            elements: vec![SubcircuitElement {
                name: "R1".into(),
                element_type: "Resistor".into(),
                nodes: vec![Node::Terminal("A".into()), Node::Terminal("C".into())], // Undefined terminal
                parameters: vec![(
                    "value".into(),
                    Expression::FloatLiteral {
                        value: 100.0,
                        span: Span::new(0, 0),
                    },
                )],
                span: Span::new(0, 0),
            }],
            is_exported: true,
            span: Span::new(0, 0),
        };

        assert!(subckt.validate().is_err());
        assert!(subckt
            .validate()
            .unwrap_err()
            .contains("undefined terminal 'C'"));
    }
}
