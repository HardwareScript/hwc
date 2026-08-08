//! Bridge definition parser (v0.2.0 - First-Class Bridge Elevation)

use crate::ast::*;
use crate::lexer::{Span, Token};
use compact_str::CompactString;

impl<'ast> super::super::Parser<'ast> {
    /// Parse a top-level bridge definition (v0.2.0)
    ///
    /// Syntax:
    /// ```hw
    /// bridge Silicon_N to Aluminum:
    ///     interface: Titanium_Silicide
    ///     thickness: 50nm
    ///     fill: Tungsten
    /// ```
    ///
    /// or shorthand (backward compatibility):
    /// ```hw
    /// bridge Silicon_N to Aluminum: Titanium_Silicide
    /// ```
    pub(crate) fn parse_bridge(
        &mut self,
        collector: &crate::DiagnosticCollector,
        is_exported: bool,
    ) -> Option<BridgeDefinition> {
        let start_pos = self.current_span().start;

        // Consume 'bridge' token
        if !self.check(&Token::Bridge) {
            collector.report(self.error("Expected 'bridge' keyword"));
            return None;
        }
        self.advance();

        // Parse 'from' material name
        let from = match self.expect_identifier() {
            Ok(ident) => CompactString::from(ident.name.as_str()),
            Err(e) => {
                collector.report(e);
                return None;
            }
        };

        // Expect 'to' keyword
        if let Err(e) = self.expect(&Token::To) {
            collector.report(e);
            return None;
        }

        // Parse 'to' material name
        let to = match self.expect_identifier() {
            Ok(ident) => CompactString::from(ident.name.as_str()),
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

        // Check for shorthand (material on same line) vs full syntax (indented block)
        let (interface_material, interface_thickness, fill_material) =
            if self.check(&Token::Newline) {
                // Full syntax with indented properties
                self.advance(); // consume newline

                // Skip any blank lines
                while self.check(&Token::Newline) {
                    self.advance();
                }

                // Expect indent
                if let Err(e) = self.expect(&Token::Indent) {
                    collector.report(e);
                    return None;
                }

                self.parse_bridge_properties(collector)?
            } else {
                // Shorthand: material name directly after colon
                match self.expect_identifier() {
                    Ok(ident) => (CompactString::from(ident.name.as_str()), None, None),
                    Err(e) => {
                        collector.report(e);
                        return None;
                    }
                }
            };

        let end_pos = self.previous_span().end;

        Some(BridgeDefinition {
            name: Identifier {
                name: CompactString::from(format!("{}_{}", from, to)),
                span: Span::new(start_pos, end_pos),
            },
            is_exported,
            from,
            to,
            interface_material,
            interface_thickness,
            fill_material,
            span: Span::new(start_pos, end_pos),
        })
    }

    /// Parse bridge properties block (interface, thickness, fill)
    fn parse_bridge_properties(
        &mut self,
        collector: &crate::DiagnosticCollector,
    ) -> Option<(CompactString, Option<Measurement>, Option<CompactString>)> {
        let mut interface_material = None;
        let mut interface_thickness = None;
        let mut fill_material = None;

        // Parse properties until dedent
        while !self.is_at_end() {
            // Consume any blank lines between properties before checking for dedent
            while self.check(&Token::Newline) {
                self.advance();
            }

            // Stop at block end
            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            // Extract property key from either a plain identifier OR a reserved keyword
            // that happens to be used as a property name (e.g. `interface` → Token::Interface).
            let key: &str = match self.current() {
                Some(t) => match &t.token {
                    Token::Identifier(s) => s.as_str(),
                    // `interface` is a reserved keyword in the lexer, so handle it explicitly.
                    Token::Interface => "interface",
                    _ => break,
                },
                None => break,
            };

            match key {
                "interface" => {
                    self.advance(); // consume 'interface'
                    if let Err(e) = self.expect(&Token::Colon) {
                        collector.report(e);
                        continue;
                    }
                    self.skip_whitespace();

                    match self.expect_identifier() {
                        Ok(ident) => {
                            interface_material = Some(CompactString::from(ident.name.as_str()));
                        }
                        Err(e) => {
                            collector.report(e);
                        }
                    }
                }
                "thickness" => {
                    self.advance(); // consume 'thickness'
                    if let Err(e) = self.expect(&Token::Colon) {
                        collector.report(e);
                        continue;
                    }
                    self.skip_whitespace();

                    match self.parse_measurement() {
                        Ok(measurement) => {
                            interface_thickness = Some(measurement);
                        }
                        Err(e) => {
                            collector.report(e);
                        }
                    }
                }
                "fill" => {
                    self.advance(); // consume 'fill'
                    if let Err(e) = self.expect(&Token::Colon) {
                        collector.report(e);
                        continue;
                    }
                    self.skip_whitespace();

                    match self.expect_identifier() {
                        Ok(ident) => {
                            fill_material = Some(CompactString::from(ident.name.as_str()));
                        }
                        Err(e) => {
                            collector.report(e);
                        }
                    }
                }
                _ => {
                    collector.report(self.error(&format!("Unknown bridge property: '{}'", key)));
                    // Skip to next line
                    while !self.is_at_end()
                        && !self.check(&Token::Newline)
                        && !self.check(&Token::Dedent)
                    {
                        self.advance();
                    }
                }
            }

            self.skip_whitespace();
        }

        if self.check(&Token::Dedent) {
            self.advance();
        }

        // Validate required field
        if interface_material.is_none() {
            collector.report(self.error("Missing required 'interface' property in bridge"));
            return None;
        }

        Some((
            interface_material.unwrap(),
            interface_thickness,
            fill_material,
        ))
    }
}
