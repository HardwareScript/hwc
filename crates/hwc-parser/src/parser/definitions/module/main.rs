use crate::ast::{ModuleDefinition, ModuleStatement, Span};
use crate::lexer::Token;
use crate::parser::Parser;

impl Parser {
    /// Parse a module definition: `module Name:`
    ///
    /// Syntax:
    /// ```hw
    /// module 64Bit_ALU:
    ///     pins:
    ///         Bus_A[64]
    ///         Bus_B[64]
    ///         CarryIn
    ///     
    ///     for i in 0..63:
    ///         add SingleBit_ALU named Bit[i]
    ///         route Bus_A[i] to Bit[i].In_A
    /// ```
    pub fn parse_module(
        &mut self,
        collector: &crate::DiagnosticCollector,
    ) -> Option<ModuleDefinition> {
        let start = self.current_span();

        // Expect 'module' keyword
        if let Err(e) = self.expect(&Token::Module) {
            collector.report(e);
            self.sync_to_next_definition();
            return None;
        }

        // Parse module name (identifier)
        let name = match self.expect_identifier() {
            Ok(id) => id,
            Err(e) => {
                collector.report(e);
                self.sync_to_next_definition();
                return None;
            }
        };

        // Expect colon
        if let Err(e) = self.expect(&Token::Colon) {
            collector.report(e);
            self.sync_to_next_definition();
            return None;
        }
        if let Err(e) = self.expect(&Token::Newline) {
            collector.report(e);
            self.sync_to_next_definition();
            return None;
        }
        if let Err(e) = self.expect(&Token::Indent) {
            collector.report(e);
            self.sync_to_next_definition();
            return None;
        }

        // Skip any comments/whitespace immediately after indent
        self.skip_whitespace();

        let mut pins = Vec::new();
        let mut statements = Vec::new();
        let mut logic = None;
        let mut intrinsic_layout = None;

        // Parse module body
        let mut loop_iterations = 0;
        while !self.check(&Token::Dedent) && !self.is_at_end() {
            loop_iterations += 1;

            if loop_iterations > 1000 {
                collector.report(
                    self.error("Module parser stuck in infinite loop - this is a compiler bug"),
                );
                break;
            }

            // Skip comments and blank lines
            self.skip_whitespace();

            // Check if we've reached the end after skipping whitespace
            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            // v0.1.6: Check for 'pins' or 'logic' identifiers
            if let Some(current) = self.current() {
                if let Token::Identifier(name) = &current.token {
                    match name.as_str() {
                        "pins" => {
                            pins = self.parse_module_pins().unwrap_or_default();
                            continue;
                        }
                        // Context-aware pin role declarations (property-style)
                        "input" | "output" | "power" | "ground" | "inout" => {
                            if let Ok(pin_decls) = self.parse_pin_role_property() {
                                pins.extend(pin_decls);
                            }
                            continue;
                        }
                        _ => {}
                    }
                }
            }

            if self.check(&Token::Logic) {
                logic = self.parse_logic_block(collector);
            } else if self.check(&Token::Add) {
                if let Ok(add) = self.parse_module_add() {
                    statements.push(ModuleStatement::AddComponent(add));
                }
            } else if self.check(&Token::Route) {
                if let Ok(route) = self.parse_module_route() {
                    statements.push(ModuleStatement::Route(route));
                }
            } else if self.check(&Token::For) {
                if let Ok(for_loop) = self.parse_for_loop() {
                    statements.push(ModuleStatement::For(for_loop));
                }
            } else if self.check(&Token::If) {
                if let Ok(if_stmt) = self.parse_if_conditional() {
                    statements.push(ModuleStatement::If(if_stmt));
                }
            } else if let Some(current) = self.current() {
                if let Token::Identifier(name) = &current.token {
                    if name.as_str() == "layout" {
                        // v0.1.7: Parse intrinsic module layout (Physical Pile Paradox / Physical Macros)
                        self.advance(); // 'layout'
                        if let Err(e) = self.expect(&Token::Colon) {
                            collector.report(e);
                            self.sync_to_next_definition();
                            break;
                        }
                        if let Err(e) = self.expect(&Token::Newline) {
                            collector.report(e);
                            self.sync_to_next_definition();
                            break;
                        }
                        if let Err(e) = self.expect(&Token::Indent) {
                            collector.report(e);
                            self.sync_to_next_definition();
                            break;
                        }

                        let mut layout_stmts: Vec<crate::LayoutStatement> = Vec::new();
                        while !self.check(&Token::Dedent) && !self.is_at_end() {
                            self.skip_whitespace();
                            if self.check(&Token::For) {
                                if let Ok(s) = self.parse_layout_for_loop() {
                                    layout_stmts.push(s);
                                } else {
                                    break;
                                }
                            } else if self.check(&Token::If) {
                                if let Ok(s) = self.parse_layout_if_conditional() {
                                    layout_stmts.push(s);
                                } else {
                                    break;
                                }
                            } else if !self.check(&Token::Dedent) {
                                if let Ok(p) = self.parse_layout_placement() {
                                    layout_stmts.push(crate::LayoutStatement::Placement(p));
                                } else {
                                    break;
                                }
                            }
                        }
                        if let Err(e) = self.expect(&Token::Dedent) {
                            collector.report(e);
                        }
                        intrinsic_layout = Some(layout_stmts);
                    } else {
                        // Unknown identifier in module body
                        let current_token = format!("{}", current.token);
                        let err = self.error(&format!(
                            "Expected 'pins:', 'logic:', 'add', 'route', 'for', or 'if' in module body, found {}",
                            current_token
                        ));
                        collector.report(err);
                        self.sync_to_next_definition();
                        break;
                    }
                } else {
                    // Not an identifier and not a recognized keyword
                    let current_token = format!("{}", current.token);
                    let err = self.error(&format!(
                        "Expected 'pins:', 'logic:', 'add', 'route', 'for', or 'if' in module body, found {}",
                        current_token
                    ));
                    collector.report(err);
                    self.sync_to_next_definition();
                    break;
                }
            } else {
                break;
            }
        }

        if let Err(e) = self.expect(&Token::Dedent) {
            collector.report(e);
        }

        let span = Span::new(start.start, self.previous_span().end);

        Some(ModuleDefinition {
            name,
            pins,
            statements,
            logic,
            intrinsic_layout,
            span,
        })
    }
}
