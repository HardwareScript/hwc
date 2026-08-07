//! Subcircuit Definition Parser (v0.3.0+)
//!
//! Parses native typed subcircuit definitions with validated AST elements.
//!
//! Syntax:
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

use crate::ast::{Identifier, Node, SubcircuitDefinition, SubcircuitElement, SubcircuitParameter};
use crate::lexer::{Span, Token};
use crate::parser::error::{span_to_source_span, ParseError};
use compact_str::CompactString;

impl crate::parser::Parser {
    /// Parse a subcircuit definition
    ///
    /// Expected format:
    /// ```
    /// subcircuit <name>:
    ///     terminals: [<name>, ...]
    ///     parameters: [<name> = <default>, ...]  # optional
    ///     elements:
    ///         <name>: <element_type>(<args>)
    ///         ...
    /// ```
    pub(in super::super) fn parse_subcircuit(
        &mut self,
        collector: &crate::DiagnosticCollector,
        is_exported: bool,
    ) -> Option<SubcircuitDefinition> {
        let start_pos = self.current_span().start;

        // Consume 'subcircuit' keyword
        self.advance();

        // Parse name
        let name = match self.expect_identifier() {
            Ok(id) => id,
            Err(e) => {
                collector.report(e);
                return None;
            }
        };

        // Expect colon
        if let Err(e) = self.expect(&Token::Colon) {
            collector.report(e);
            return None;
        }

        // Expect newline and indentation
        if let Err(e) = self.expect(&Token::Newline) {
            collector.report(e);
            return None;
        }

        if let Err(_e) = self.expect(&Token::Indent) {
            collector.report(ParseError::ExpectedIndent {
                span: span_to_source_span(&self.current_span()),
                message: format!("subcircuit '{}' body", name.name).into(),
            });
            return None;
        }

        let mut terminals = Vec::new();
        let mut parameters = Vec::new();
        let mut elements = Vec::new();

        // Parse properties
        while !self.check(&Token::Dedent) && !self.is_at_end() {
            // Skip blank lines
            if self.check(&Token::Newline) {
                self.advance();
                continue;
            }

            if let Some(current) = self.current() {
                match &current.token {
                    Token::Identifier(_) => {
                        let prop_name = match self.expect_identifier_string() {
                            Ok(name) => name,
                            Err(_) => {
                                self.sync_to_next_definition();
                                return None;
                            }
                        };

                        match prop_name.as_str() {
                            "terminals" => {
                                terminals = self.parse_subcircuit_terminals(collector, &name)?;
                            }
                            "parameters" => {
                                parameters = self.parse_subcircuit_parameters(collector, &name)?;
                            }
                            "elements" => {
                                elements = self.parse_subcircuit_elements(collector, &name)?;
                            }
                            _ => {
                                collector.report(ParseError::UnexpectedToken {
                                    span: span_to_source_span(&self.current_span()),
                                    expected: "terminals, parameters, or elements".into(),
                                    found: format!("field '{}'", prop_name).into(),
                                });
                                self.sync_to_next_definition();
                            }
                        }
                    }
                    Token::Dedent => break,
                    _ => {
                        collector.report(ParseError::UnexpectedToken {
                            span: span_to_source_span(&self.current_span()),
                            expected: "terminals, parameters, or elements".into(),
                            found: format!("{:?}", current.token).into(),
                        });
                        self.sync_to_next_definition();
                    }
                }
            } else {
                break;
            }
        }

        // Consume dedent
        if self.check(&Token::Dedent) {
            self.advance();
        }

        let end_pos = self.previous_span().end;
        let span = Span::new(start_pos, end_pos);

        // Validate required fields
        if terminals.is_empty() {
            collector.report(ParseError::UnexpectedToken {
                span: span_to_source_span(&span),
                expected: format!(
                    "subcircuit '{}' requires 'terminals' field. Add 'terminals: [PLUS, MINUS, ...]'",
                    name.name
                ).into(),
                found: "missing terminals".into(),
            });
            return None;
        }

        if elements.is_empty() {
            collector.report(ParseError::UnexpectedToken {
                span: span_to_source_span(&span),
                expected: format!(
                    "subcircuit '{}' requires 'elements' field. Add 'elements:' block",
                    name.name
                ).into(),
                found: "missing elements".into(),
            });
            return None;
        }

        Some(SubcircuitDefinition {
            name,
            terminals,
            parameters,
            elements,
            is_exported,
            span,
        })
    }

    /// Parse terminals list: terminals: [A, B, C]
    fn parse_subcircuit_terminals(
        &mut self,
        collector: &crate::DiagnosticCollector,
        _subcircuit_name: &Identifier,
    ) -> Option<Vec<CompactString>> {
        // Expect colon
        if let Err(e) = self.expect(&Token::Colon) {
            collector.report(e);
            return None;
        }

        // Parse list: [A, B, C]
        if let Err(e) = self.expect(&Token::OpenBracket) {
            collector.report(e);
            return None;
        }

        let mut terminals = Vec::new();
        while !self.check(&Token::CloseBracket) && !self.is_at_end() {
            if let Ok(terminal) = self.expect_identifier_string() {
                terminals.push(terminal.into());

                if self.check(&Token::Comma) {
                    self.advance();
                }
            } else {
                break;
            }
        }

        if let Err(e) = self.expect(&Token::CloseBracket) {
            collector.report(e);
            return None;
        }

        // Consume newline
        if self.check(&Token::Newline) {
            self.advance();
        }

        Some(terminals)
    }

    /// Parse parameters list: parameters: [W = 1.0um, L = 1.0um]
    fn parse_subcircuit_parameters(
        &mut self,
        collector: &crate::DiagnosticCollector,
        _subcircuit_name: &Identifier,
    ) -> Option<Vec<SubcircuitParameter>> {
        // Expect colon
        if let Err(e) = self.expect(&Token::Colon) {
            collector.report(e);
            return None;
        }

        // Parse list: [W = 1.0um, L = 1.0um]
        if let Err(e) = self.expect(&Token::OpenBracket) {
            collector.report(e);
            return None;
        }

        let mut parameters = Vec::new();
        while !self.check(&Token::CloseBracket) && !self.is_at_end() {
            let param_start = self.current_span().start;
            
            if let Ok(param_name) = self.expect_identifier_string() {
                let default_value = if self.check(&Token::Equals) {
                    self.advance();
                    Some(self.parse_expression().ok()?)
                } else {
                    None
                };

                let param_end = self.previous_span().end;
                parameters.push(SubcircuitParameter {
                    name: param_name.into(),
                    default_value,
                    span: Span::new(param_start, param_end),
                });

                if self.check(&Token::Comma) {
                    self.advance();
                }
            } else {
                break;
            }
        }

        if let Err(e) = self.expect(&Token::CloseBracket) {
            collector.report(e);
            return None;
        }

        // Consume newline
        if self.check(&Token::Newline) {
            self.advance();
        }

        Some(parameters)
    }

    /// Parse elements block
    fn parse_subcircuit_elements(
        &mut self,
        collector: &crate::DiagnosticCollector,
        _subcircuit_name: &Identifier,
    ) -> Option<Vec<SubcircuitElement>> {
        // Expect colon
        if let Err(e) = self.expect(&Token::Colon) {
            collector.report(e);
            return None;
        }

        // Expect newline and indent
        if let Err(e) = self.expect(&Token::Newline) {
            collector.report(e);
            return None;
        }

        if let Err(e) = self.expect(&Token::Indent) {
            collector.report(e);
            return None;
        }

        let mut elements = Vec::new();

        while !self.check(&Token::Dedent) && !self.is_at_end() {
            if self.check(&Token::Newline) {
                self.advance();
                continue;
            }

            // Parse element: name: Type(args)
            if let Some(element) = self.parse_subcircuit_element(collector) {
                elements.push(element);
            } else {
                self.sync_to_next_definition();
            }
        }

        // Consume dedent
        if self.check(&Token::Dedent) {
            self.advance();
        }

        Some(elements)
    }

    /// Parse a single element: R1: Resistor(nodes: [PLUS, MINUS], value: 100ohm)
    ///
    /// Generic syntax: name: Type(nodes: [node1, node2, ...], param: value, ...)
    fn parse_subcircuit_element(
        &mut self,
        collector: &crate::DiagnosticCollector,
    ) -> Option<SubcircuitElement> {
        let start_pos = self.current_span().start;

        // Parse element name
        let element_name: CompactString = self.expect_identifier_string().ok()?.into();

        // Expect colon
        if let Err(e) = self.expect(&Token::Colon) {
            collector.report(e);
            return None;
        }

        // Parse element type (Resistor, Capacitor, Mosfet, SubcircuitInstance, etc.)
        let element_type: CompactString = self.expect_identifier_string().ok()?.into();

        // Expect left paren
        if let Err(e) = self.expect(&Token::OpenParen) {
            collector.report(e);
            return None;
        }

        let mut nodes = Vec::new();
        let mut parameters = Vec::new();

        // Parse named parameters: nodes: [...], value: ..., W: ..., etc.
        while !self.check(&Token::CloseParen) && !self.is_at_end() {
            // Parse parameter name
            let param_name = self.expect_identifier_string().ok()?;

            // Expect colon
            self.expect(&Token::Colon).ok()?;

            // Check if it's "nodes" (special case: list of nodes)
            if param_name == "nodes" {
                // Parse node list: [node1, node2, ...]
                self.expect(&Token::OpenBracket).ok()?;
                
                while !self.check(&Token::CloseBracket) && !self.is_at_end() {
                    if let Some(node) = self.parse_node(collector) {
                        nodes.push(node);
                    }
                    
                    if self.check(&Token::Comma) {
                        self.advance();
                    }
                }
                
                self.expect(&Token::CloseBracket).ok()?;
            } else {
                // Regular parameter: parse as expression
                let value = self.parse_expression().ok()?;
                parameters.push((param_name.into(), value));
            }

            // Optional comma
            if self.check(&Token::Comma) {
                self.advance();
            }
        }

        // Expect right paren
        if let Err(e) = self.expect(&Token::CloseParen) {
            collector.report(e);
            return None;
        }

        // Consume newline
        if self.check(&Token::Newline) {
            self.advance();
        }

        let end_pos = self.previous_span().end;
        let span = Span::new(start_pos, end_pos);

        Some(SubcircuitElement {
            name: element_name,
            element_type,
            nodes,
            parameters,
            span,
        })
    }

    /// Parse a node reference: Terminal, internal_node, or 0 (ground)
    fn parse_node(&mut self, collector: &crate::DiagnosticCollector) -> Option<Node> {
        if let Some(current) = self.current() {
            match &current.token {
                Token::Identifier(_) => {
                    let node_name = self.expect_identifier_string().ok()?;
                    // Check if it's uppercase (terminal) or lowercase (internal node)
                    if node_name.chars().next().unwrap().is_uppercase() {
                        Some(Node::Terminal(node_name.into()))
                    } else {
                        Some(Node::Internal(node_name.into()))
                    }
                }
                Token::Integer(num) => {
                    let num_val = *num;
                    self.advance();
                    if num_val == 0 {
                        Some(Node::Ground)
                    } else {
                        collector.report(ParseError::UnexpectedToken {
                            span: span_to_source_span(&self.previous_span()),
                            expected: "node reference (identifier or 0 for ground)".into(),
                            found: num_val.to_string().into(),
                        });
                        None
                    }
                }
                _ => {
                    collector.report(ParseError::UnexpectedToken {
                        span: span_to_source_span(&self.current_span()),
                        expected: "node reference (identifier or 0)".into(),
                        found: format!("{}", current.token).into(),
                    });
                    None
                }
            }
        } else {
            None
        }
    }
}
