use super::super::super::error::ParseError;
use crate::ast::*;
use crate::lexer::Token;

impl<'ast> super::super::super::Parser<'ast> {
    /// Parse physical layer stackup block (v0.1.7 Z-Axis Abstraction)
    ///
    /// Syntax:
    ///     stackup:
    ///         substrate: [material: Silicon_P, thickness: 300um, routable: false]
    ///         active:    [material: Silicon_N, thickness: 200nm, routable: false]
    ///         poly:      [material: Polysilicon, thickness: 150nm, routable: local_only]
    ///         metal1:    [material: Aluminum, thickness: 400nm, routable: true]
    pub(super) fn parse_stackup_constraints(&mut self) -> Result<LayerStackup, ParseError> {
        let _start_pos = self.current_span().start;
        let mut layers = Vec::new();

        while !self.check(&Token::Dedent) && !self.is_at_end() {
            self.skip_whitespace();

            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            // Layer name: l1, d1, inner1, etc.
            let name = self.expect_identifier()?;

            self.expect(&Token::Colon)?;

            // Expect opening bracket for the layer properties
            self.expect(&Token::OpenBracket)?;

            let mut material = None;
            let mut thickness = None;
            let mut routable = None;

            // Parse key: value pairs inside the brackets (comma separated)
            while !self.check(&Token::CloseBracket) && !self.is_at_end() {
                self.skip_whitespace();

                if self.check(&Token::CloseBracket) {
                    break;
                }

                // "material" is a soft keyword elsewhere; allow it as a stackup property key
                let key = self.expect_identifier_or_keyword_string()?;
                self.expect(&Token::Colon)?;

                match key.as_str() {
                    "material" => {
                        let mat = self.expect_namespaced_identifier_string()?;
                        material = Some(mat);
                    }
                    "thickness" => {
                        // Use parse_expression so we can support variables/expressions later
                        thickness = Some(self.parse_expression()?);
                    }
                    "routable" => {
                        // v0.1.8: Parse routable mode for Physical Synthesis Guardrails.
                        // Table-driven: each layer declares its routability.
                        let mode_str = self.expect_identifier()?;
                        routable = Some(match mode_str.as_str() {
                            "true" => RoutableMode::True,
                            "false" => RoutableMode::False,
                            "local_only" => RoutableMode::LocalOnly,
                            _ => {
                                return Err(self.error(&format!(
                                    "Unknown routable mode: '{}' (expected 'true', 'false', or 'local_only')",
                                    mode_str
                                )));
                            }
                        });
                    }
                    _ => {
                        return Err(
                            self.error(&format!("Unknown stackup layer property: '{}'", key))
                        );
                    }
                }

                // Optional comma between properties
                if self.check(&Token::Comma) {
                    self.advance();
                }

                self.skip_whitespace();
            }

            self.expect(&Token::CloseBracket)?;
            self.skip_whitespace();

            let material = material
                .ok_or_else(|| self.error("Stackup layer definition must include 'material'"))?;
            let thickness = thickness
                .ok_or_else(|| self.error("Stackup layer definition must include 'thickness'"))?;

            layers.push(StackupLayer {
                name,
                material: material.into(),
                thickness,
                routable,
            });
        }

        let _end_pos = self.previous_span().end;

        Ok(LayerStackup { layers })
    }
}
