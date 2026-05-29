//! Module definition parser for v0.1.6
//!
//! Parses `module` blocks with comptime evaluation support.

use crate::ast::{
    ArithmeticOp, ArrayIndex, Condition, ForLoop, IfConditional, ModuleComponentPlacement,
    ModuleDefinition, ModulePinReference, ModuleRoute, ModuleStatement, PinDeclaration,
};
use crate::lexer::{Span, Token};
use crate::parser::{ParseError, Parser};
use smallvec::SmallVec;

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
            // eprintln!("[DEBUG parse_module] Failed to expect Module keyword");
            collector.report(e);
            self.sync_to_next_definition();
            return None;
        }

        // Parse module name (identifier)
        let name = match self.expect_identifier() {
            Ok(id) => id,
            Err(e) => {
                // eprintln!("[DEBUG parse_module] Failed to parse module name");
                collector.report(e);
                self.sync_to_next_definition();
                return None;
            }
        };

        // Expect colon
        if let Err(e) = self.expect(&Token::Colon) {
            // eprintln!("[DEBUG parse_module] Failed to expect colon after module name");
            collector.report(e);
            self.sync_to_next_definition();
            return None;
        }
        if let Err(e) = self.expect(&Token::Newline) {
            // eprintln!("[DEBUG parse_module] Failed to expect newline after colon");
            collector.report(e);
            self.sync_to_next_definition();
            return None;
        }
        if let Err(e) = self.expect(&Token::Indent) {
            // eprintln!("[DEBUG parse_module] Failed to expect indent, current token: {:?}", self.current().map(|t| &t.token));
            collector.report(e);
            self.sync_to_next_definition();
            return None;
        }

        // eprintln!("[DEBUG parse_module] Successfully parsed module header, about to skip whitespace");
        // eprintln!("[DEBUG parse_module] Current token before skip_whitespace: {:?}", self.current().map(|t| &t.token));

        // Skip any comments/whitespace immediately after indent
        self.skip_whitespace();

        // eprintln!("[DEBUG parse_module] After skip_whitespace, current token: {:?}", self.current().map(|t| &t.token));

        let mut pins = Vec::new();
        let mut statements = Vec::new();
        let mut logic = None;
        let mut intrinsic_layout = None; // v0.1.7 Physical Macro support (intrinsic relative layout)

        // Parse module body
        let mut loop_iterations = 0;
        while !self.check(&Token::Dedent) && !self.is_at_end() {
            loop_iterations += 1;
            let _position_before = self.current;

            if loop_iterations > 1000 {
                // eprintln!("[DEBUG] CRITICAL: Module parser infinite loop detected! Breaking.");
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
                        // Syntax: layout: \n    Comp at last.right + 1mm \n    ...
                        self.advance(); // 'layout'
                        if let Err(e) = self.expect(&Token::Colon) { collector.report(e); self.sync_to_next_definition(); break; }
                        if let Err(e) = self.expect(&Token::Newline) { collector.report(e); self.sync_to_next_definition(); break; }
                        if let Err(e) = self.expect(&Token::Indent) { collector.report(e); self.sync_to_next_definition(); break; }

                        let mut layout_stmts: Vec<crate::LayoutStatement> = Vec::new();
                        while !self.check(&Token::Dedent) && !self.is_at_end() {
                            self.skip_whitespace();
                            if self.check(&Token::For) {
                                if let Ok(s) = self.parse_layout_for_loop() { layout_stmts.push(s); } else { break; }
                            } else if self.check(&Token::If) {
                                if let Ok(s) = self.parse_layout_if_conditional() { layout_stmts.push(s); } else { break; }
                            } else if !self.check(&Token::Dedent) {
                                if let Ok(p) = self.parse_layout_placement() {
                                    layout_stmts.push(crate::LayoutStatement::Placement(p));
                                } else { break; }
                            }
                        }
                        if let Err(e) = self.expect(&Token::Dedent) { collector.report(e); }
                        intrinsic_layout = Some(layout_stmts);
                    } else {
                        // Unknown identifier in module body - this is where we report errors
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
                // Unexpected state
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

    /// Parse module pins block
    ///
    /// Supports both inline and block syntax:
    /// ```hw
    /// # Inline:
    /// pins: VCC, GND, Bus_A[64]
    ///
    /// # Block:
    /// pins:
    ///     VCC
    ///     GND
    ///     Bus_A[64]
    /// ```
    fn parse_module_pins(&mut self) -> Result<Vec<PinDeclaration>, ParseError> {
        // v0.1.6: 'pins' is now an identifier
        self.expect_identifier()?; // consume 'pins'
        self.expect(&Token::Colon)?;

        // Use universal list parser (v0.1.6)
        // Supports: [A, B, C] (bracket), A, B, C (inline), or block format
        self.parse_list(|parser| parser.parse_module_pin_declaration())
    }

    /// Parse property-style pin role declaration (v0.1.6 Context-Aware Parsing)
    ///
    /// Syntax:
    /// ```hw
    /// input: VIN
    /// output: VOUT, VOUT2
    /// power: VDD
    /// ground: GND
    /// inout: DATA[8]
    /// ```
    fn parse_pin_role_property(&mut self) -> Result<Vec<PinDeclaration>, ParseError> {
        use crate::PinDirection;

        // Parse the direction identifier (input, output, power, ground, inout)
        let direction_name = self.expect_identifier_string()?;
        let direction = match direction_name.as_str() {
            "input" => PinDirection::Input,
            "output" => PinDirection::Output,
            "power" => PinDirection::Power,
            "ground" => PinDirection::Ground,
            "inout" => PinDirection::Inout,
            _ => {
                return Err(self.error(&format!(
                    "Expected pin direction (input, output, power, ground, inout), found '{}'",
                    direction_name
                )))
            }
        };

        self.expect(&Token::Colon)?;

        // Parse pin list (supports inline comma-separated or block format)
        let pin_names = self.parse_list(|parser| {
            let start = parser.current_span();
            let name = parser.expect_identifier_string()?;

            // Check for array syntax: Bus[64]
            let array_size = if parser.check(&Token::OpenBracket) {
                parser.advance(); // consume '['
                let size = parser.expect_integer()?;
                parser.expect(&Token::CloseBracket)?;
                Some(size)
            } else {
                None
            };

            let span = Span::new(start.start, parser.previous_span().end);

            Ok(PinDeclaration {
                name: name.into(),
                direction,
                array_size,
                span,
            })
        })?;

        Ok(pin_names)
    }

    /// Parse a single module pin declaration: Name or Name[size]
    ///
    /// Supports optional direction keywords (context-aware):
    /// - `input VIN` - input pin (legacy bracket style)
    /// - `output VOUT` - output pin (legacy bracket style)
    /// - `power VDD` - power pin (legacy bracket style)
    /// - `ground GND` - ground pin (legacy bracket style)
    /// - `inout DATA` - bidirectional pin (legacy bracket style)
    /// - `VCC` - directionless pin (defaults to Passive)
    fn parse_module_pin_declaration(&mut self) -> Result<PinDeclaration, ParseError> {
        let start = self.current_span();

        // Step 1: Check for optional direction keyword (context-aware soft keywords)
        // These are parsed as identifiers, not lexer tokens
        use crate::PinDirection;
        let direction = if let Some(current) = self.current() {
            if let Token::Identifier(name) = &current.token {
                match name.as_str() {
                    "input" => {
                        self.advance();
                        PinDirection::Input
                    }
                    "output" => {
                        self.advance();
                        PinDirection::Output
                    }
                    "power" => {
                        self.advance();
                        PinDirection::Power
                    }
                    "ground" => {
                        self.advance();
                        PinDirection::Ground
                    }
                    "inout" => {
                        self.advance();
                        PinDirection::Inout
                    }
                    _ => PinDirection::Passive, // Default - no direction specified
                }
            } else {
                PinDirection::Passive
            }
        } else {
            PinDirection::Passive
        };

        // Step 2: Parse the pin name (bare identifier)
        let name = self.expect_identifier_string()?;

        // Step 3: Check for array syntax: Bus[64]
        let array_size = if self.check(&Token::OpenBracket) {
            self.advance(); // consume '['
            let size = self.expect_integer()?;
            self.expect(&Token::CloseBracket)?;
            Some(size)
        } else {
            None
        };

        let span = Span::new(start.start, self.previous_span().end);

        Ok(PinDeclaration {
            name: name.into(),
            direction,
            array_size,
            span,
        })
    }

    /// Parse component addition in module: `add ComponentType (params) named Instance`
    ///
    /// Note: NO `at [x,y,z]` allowed in modules (that's in space layout blocks)
    fn parse_module_add(&mut self) -> Result<ModuleComponentPlacement, ParseError> {
        let start = self.current_span();

        // // eprintln!("[DEBUG parse_module_add] Starting, current token: {:?}", self.current().map(|t| &t.token));
        self.expect(&Token::Add)?;

        // Parse component type (supports namespaced identifiers like Parts.MCU)
        let component_type = self.expect_namespaced_identifier_string()?;
        // // eprintln!("[DEBUG parse_module_add] Component type: {}, current token: {:?}", component_type, self.current().map(|t| &t.token));

        // Parse optional parameters
        let parameters = if self.check(&Token::OpenParen) {
            // // eprintln!("[DEBUG parse_module_add] Parsed {} parameters, current token: {:?}", params.len(), self.current().map(|t| &t.token));
            self.parse_parameters()?
        } else {
            SmallVec::new()
        };

        // Parse optional name
        let (name, array_index) = if self.check(&Token::Named) {
            // // eprintln!("[DEBUG parse_module_add] Found 'named' keyword");
            self.advance(); // consume 'named'
            let instance_name = self.expect_identifier_string()?;
            // // eprintln!("[DEBUG parse_module_add] Instance name: {}, current token: {:?}", instance_name, self.current().map(|t| &t.token));

            // Check for array index: named Bit[i]
            let index = if self.check(&Token::OpenBracket) {
                self.advance(); // consume '['
                let idx = self.parse_array_index()?;
                self.expect(&Token::CloseBracket)?;
                Some(idx)
            } else {
                None
            };

            (Some(instance_name), index)
        } else {
            // // eprintln!("[DEBUG parse_module_add] No 'named' keyword found, current token: {:?}", self.current().map(|t| &t.token));
            (None, None)
        };

        // Ensure NO 'at' keyword (modules are logical only)
        if self.check(&Token::At) {
            return Err(self.error(
                "Cannot use 'at [x,y,z]' in module definitions. Modules are purely logical. \
                 Use 'layout' blocks in space definitions to map components to physical coordinates."
            ));
        }

        self.consume_statement_end()?;

        let span = Span::new(start.start, self.previous_span().end);

        Ok(ModuleComponentPlacement {
            component_type: component_type.into(),
            parameters,
            name: name.map(|s: String| s.into()),
            array_index,
            span,
        })
    }

    /// Parse route in module: `route From.Pin to To.Pin`
    ///
    /// Note: NO waypoints allowed in modules (pure logical connection)
    fn parse_module_route(&mut self) -> Result<ModuleRoute, ParseError> {
        let start = self.current_span();

        self.expect(&Token::Route)?;

        // Parse from pin
        let from = self.parse_module_pin_reference()?;

        self.expect(&Token::To)?;

        // Parse to pin
        let to = self.parse_module_pin_reference()?;

        // Ensure NO 'path:' block (modules are logical only)
        if self.check(&Token::Colon) {
            return Err(self.error(
                "Cannot use 'path:' waypoints in module routes. Modules define logical connections only. \
                 Physical routing happens in space definitions."
            ));
        }

        self.consume_statement_end()?;

        let span = Span::new(start.start, self.previous_span().end);

        Ok(ModuleRoute { from, to, span })
    }

    /// Parse module pin reference with array indexing support
    ///
    /// Examples:
    /// - `Component.Pin` - simple reference
    /// - `Component[i].Pin` - array component
    /// - `Bus[i]` - array pin (module pin, no component.into())
    /// - `Bit[i-1].CarryOut` - arithmetic in index
    fn parse_module_pin_reference(&mut self) -> Result<ModulePinReference, ParseError> {
        let start = self.current_span();

        // Parse first identifier (could be component or pin.into())
        let first_name = self.expect_identifier_string()?;

        // Check for array index: Name[i]
        let first_index = if self.check(&Token::OpenBracket) {
            self.advance(); // consume '['
            let idx = self.parse_array_index()?;
            self.expect(&Token::CloseBracket)?;
            Some(idx)
        } else {
            None
        };

        // Check if there's a dot (component.pin) or not (just pin.into())
        if self.check(&Token::Dot) {
            self.advance(); // consume '.'

            // Parse pin name
            let pin = self.expect_identifier_string()?;

            // Check for pin array index: Pin[i]
            let pin_index = if self.check(&Token::OpenBracket) {
                self.advance(); // consume '['
                let idx = self.parse_array_index()?;
                self.expect(&Token::CloseBracket)?;
                Some(idx)
            } else {
                None
            };

            let span = Span::new(start.start, self.previous_span().end);

            Ok(ModulePinReference {
                component: first_name.into(),
                component_index: first_index,
                pin: pin.into(),
                pin_index,
                span,
            })
        } else {
            // No dot - this is a module pin reference (e.g., Bus[i])
            // Use empty string for component to indicate module pin
            let span = Span::new(start.start, self.previous_span().end);

            Ok(ModulePinReference {
                component: String::new().into(), // Empty component means module pin
                component_index: None,
                pin: first_name.into(),
                pin_index: first_index,
                span,
            })
        }
    }

    /// Parse array index expression
    ///
    /// Examples:
    /// - `i` - variable
    /// - `0` - literal
    /// - `i-1` - arithmetic (i minus 1)
    /// - `i+1` - arithmetic (i plus 1)
    /// - `i-1` - arithmetic (i minus 1)
    /// - `i*2` - arithmetic (i times 2)
    /// - `i/2` - arithmetic (i divided by 2)
    ///
    /// Note: Negative literals like `-1` are not supported for hardware array indexing.
    /// Hardware buses and bit indices are always non-negative.
    fn parse_array_index(&mut self) -> Result<ArrayIndex, ParseError> {
        // Parse the left operand
        let left = if let Some(current) = self.current() {
            if let Token::Identifier(var) = &current.token {
                let var_name = var.clone();
                self.advance();
                ArrayIndex::Variable(var_name)
            } else if let Token::Integer(n) = &current.token {
                // Reject standalone negative integers as array indices
                if *n < 0 {
                    return Err(self.error(
                        "Negative array indices are not supported. Hardware bus and bit indices must be non-negative.",
                    ));
                }
                let value = *n as usize;
                self.advance();
                ArrayIndex::Literal(value)
            } else {
                return Err(self.error(&format!(
                    "Expected variable or number in array index, found {}",
                    current.token
                )));
            }
        } else {
            return Err(self.error("Unexpected end of input in array index"));
        };

        // Check for arithmetic operator OR integer (lexer collision case)
        if self.check(&Token::Plus)
            || self.check(&Token::Hyphen)
            || self.check(&Token::Asterisk)
            || self.check(&Token::Slash)
        {
            let op = if self.check(&Token::Plus) {
                ArithmeticOp::Add
            } else if self.check(&Token::Hyphen) {
                ArithmeticOp::Subtract
            } else if self.check(&Token::Asterisk) {
                ArithmeticOp::Multiply
            } else {
                ArithmeticOp::Divide
            };
            self.advance(); // consume operator

            let right = if let Some(current) = self.current() {
                if let Token::Identifier(var) = &current.token {
                    let var_name = var.clone();
                    self.advance();
                    ArrayIndex::Variable(var_name)
                } else if let Token::Integer(n) = &current.token {
                    let value = *n as usize;
                    self.advance();
                    ArrayIndex::Literal(value)
                } else {
                    return Err(self.error(&format!(
                        "Expected variable or number after arithmetic operator, found {}",
                        current.token
                    )));
                }
            } else {
                return Err(self.error("Unexpected end of input after arithmetic operator"));
            };

            Ok(ArrayIndex::Arithmetic {
                left: Box::new(left),
                op,
                right: Box::new(right),
            })
        } else if let Some(current) = self.current() {
            // NATIVE FIX: Handle lexer collision where operators get consumed by integer literals
            // Case 1: "i-1" becomes Identifier("i"), Integer(-1)
            // Case 2: "i+1" becomes Identifier("i"), Integer(1) - the + is consumed!
            // We detect this ONLY when left is a Variable (not a Literal)
            if let Token::Integer(n) = &current.token {
                // Only apply this fix if the left side is a variable
                // This prevents breaking literal indices like Data[1]
                if matches!(left, ArrayIndex::Variable(_)) {
                    if *n < 0 {
                        // Convert Integer(-1) into subtraction: i - 1
                        let absolute_value = n.unsigned_abs() as usize;
                        self.advance();
                        return Ok(ArrayIndex::Arithmetic {
                            left: Box::new(left),
                            op: ArithmeticOp::Subtract,
                            right: Box::new(ArrayIndex::Literal(absolute_value)),
                        });
                    } else if *n > 0 {
                        // Convert Integer(1) after variable into addition: i + 1
                        // This handles the lexer consuming the + sign
                        let value = *n as usize;
                        self.advance();
                        return Ok(ArrayIndex::Arithmetic {
                            left: Box::new(left),
                            op: ArithmeticOp::Add,
                            right: Box::new(ArrayIndex::Literal(value)),
                        });
                    }
                }
            }
            Ok(left)
        } else {
            Ok(left)
        }
    }

    /// Parse for loop: `for i in 0..63:`
    ///
    /// Range is inclusive (Ruby-style): 0..63 means 0 to 63 (64 iterations)
    fn parse_for_loop(&mut self) -> Result<ForLoop, ParseError> {
        let start = self.current_span();

        self.expect(&Token::For)?;

        // Parse loop variable
        let variable = self.expect_identifier_string()?;

        self.expect(&Token::In)?;

        // Parse range start
        let range_start = self.expect_integer()?;

        self.expect(&Token::Range)?;

        // Parse range end (inclusive)
        let range_end = self.expect_integer()?;

        self.expect(&Token::Colon)?;
        self.expect(&Token::Newline)?;
        self.skip_whitespace(); // Skip comments between newline and indent
        self.expect(&Token::Indent)?;

        // Parse loop body
        let mut body = Vec::new();
        while !self.check(&Token::Dedent) && !self.is_at_end() {
            // Skip comments and blank lines
            self.skip_whitespace();

            // Check if we've reached the end after skipping whitespace
            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            if self.check(&Token::Add) {
                body.push(ModuleStatement::AddComponent(self.parse_module_add()?));
            } else if self.check(&Token::Route) {
                body.push(ModuleStatement::Route(self.parse_module_route()?));
            } else if self.check(&Token::For) {
                body.push(ModuleStatement::For(self.parse_for_loop()?));
            } else if self.check(&Token::If) {
                body.push(ModuleStatement::If(self.parse_if_conditional()?));
            } else {
                let current_token = self
                    .current()
                    .map(|t| format!("{}", t.token))
                    .unwrap_or_else(|| "end of input".into());
                return Err(self.error(&format!(
                    "Expected 'add', 'route', 'for', or 'if' in for loop body, found {}",
                    current_token
                )));
            }
        }

        self.expect(&Token::Dedent)?;

        let span = Span::new(start.start, self.previous_span().end);

        Ok(ForLoop {
            variable: variable.into(),
            start: range_start,
            end: range_end,
            body,
            span,
        })
    }

    /// Parse if conditional: `if condition:`
    fn parse_if_conditional(&mut self) -> Result<IfConditional, ParseError> {
        let start = self.current_span();

        self.expect(&Token::If)?;

        // Parse condition
        let condition = self.parse_condition()?;

        self.expect(&Token::Colon)?;
        self.expect(&Token::Newline)?;
        self.skip_whitespace(); // Skip comments between newline and indent
        self.expect(&Token::Indent)?;

        // Parse then body
        let mut then_body = Vec::new();
        while !self.check(&Token::Dedent) && !self.is_at_end() {
            // Skip comments and blank lines
            self.skip_whitespace();

            // Check if we've reached the end after skipping whitespace
            if self.check(&Token::Dedent) || self.is_at_end() {
                break;
            }

            if self.check(&Token::Add) {
                then_body.push(ModuleStatement::AddComponent(self.parse_module_add()?));
            } else if self.check(&Token::Route) {
                then_body.push(ModuleStatement::Route(self.parse_module_route()?));
            } else if self.check(&Token::For) {
                then_body.push(ModuleStatement::For(self.parse_for_loop()?));
            } else if self.check(&Token::If) {
                then_body.push(ModuleStatement::If(self.parse_if_conditional()?));
            } else {
                let current_token = self
                    .current()
                    .map(|t| format!("{}", t.token))
                    .unwrap_or_else(|| "end of input".into());
                return Err(self.error(&format!(
                    "Expected 'add', 'route', 'for', or 'if' in if body, found {}",
                    current_token
                )));
            }
        }

        self.expect(&Token::Dedent)?;

        // Parse optional else block
        let else_body = if self.check(&Token::Else) {
            self.advance(); // consume 'else'
            self.expect(&Token::Colon)?;
            self.expect(&Token::Newline)?;
            self.skip_whitespace(); // Skip comments between newline and indent
            self.expect(&Token::Indent)?;

            let mut else_stmts = Vec::new();
            while !self.check(&Token::Dedent) && !self.is_at_end() {
                // Skip comments and blank lines
                self.skip_whitespace();

                // Check if we've reached the end after skipping whitespace
                if self.check(&Token::Dedent) || self.is_at_end() {
                    break;
                }

                if self.check(&Token::Add) {
                    else_stmts.push(ModuleStatement::AddComponent(self.parse_module_add()?));
                } else if self.check(&Token::Route) {
                    else_stmts.push(ModuleStatement::Route(self.parse_module_route()?));
                } else if self.check(&Token::For) {
                    else_stmts.push(ModuleStatement::For(self.parse_for_loop()?));
                } else if self.check(&Token::If) {
                    else_stmts.push(ModuleStatement::If(self.parse_if_conditional()?));
                } else {
                    let current_token = self
                        .current()
                        .map(|t| format!("{}", t.token))
                        .unwrap_or_else(|| "end of input".into());
                    return Err(self.error(&format!(
                        "Expected 'add', 'route', 'for', or 'if' in else body, found {}",
                        current_token
                    )));
                }
            }

            self.expect(&Token::Dedent)?;
            Some(else_stmts)
        } else {
            None
        };

        let span = Span::new(start.start, self.previous_span().end);

        Ok(IfConditional {
            condition,
            then_body,
            else_body,
            span,
        })
    }

    /// Parse condition for if statement
    ///
    /// Examples:
    /// - `i == 0` - equality
    /// - `i < 63` - less than
    /// - `i > 0` - greater than
    /// - `i != 0` - not equals
    fn parse_condition(&mut self) -> Result<Condition, ParseError> {
        // Parse left side
        let left = self.parse_array_index()?;

        // Parse operator
        let condition = if self.check(&Token::Equals) {
            self.advance();
            let right = self.parse_array_index()?;
            Condition::Equals { left, right }
        } else if self.check(&Token::LessThan) {
            self.advance();
            let right = self.parse_array_index()?;
            Condition::LessThan { left, right }
        } else if self.check(&Token::GreaterThan) {
            self.advance();
            let right = self.parse_array_index()?;
            Condition::GreaterThan { left, right }
        } else if self.check(&Token::NotEquals) {
            self.advance();
            let right = self.parse_array_index()?;
            Condition::NotEquals { left, right }
        } else {
            let current_token = self
                .current()
                .map(|t| format!("{}", t.token))
                .unwrap_or_else(|| "end of input".into());
            return Err(self.error(&format!(
                "Expected comparison operator (==, <, >, !=), found {}",
                current_token
            )));
        };

        Ok(condition)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::DiagnosticCollector;

    fn parse_module(source: &str) -> Result<ModuleDefinition, String> {
        let lexer = Lexer::new(source);
        let tokens = lexer.tokenize().expect("Lexer should succeed");
        let mut parser = Parser::new(tokens);
        let collector = DiagnosticCollector::new(source, 100);
        let program = parser.parse(&collector);

        if collector.has_errors() {
            // Print actual errors for debugging
            eprintln!("=== PARSE ERRORS ===");
            collector.print_all();
            eprintln!("====================");
            return Err(collector.summary().to_string());
        }

        assert_eq!(
            program.definitions.len(),
            1,
            "Expected exactly one definition"
        );

        if let crate::ast::Definition::Module(module) =
            program.definitions.into_iter().next().unwrap()
        {
            Ok(module)
        } else {
            panic!("Expected module definition");
        }
    }

    #[test]
    fn test_parse_simple_module() {
        // First, let's test the lexer to see what tokens are generated
        let source = r#"add Resistor_0805 (val: 100Ω, tol: 5%) named R1"#;
        let lexer = Lexer::new(source);
        let tokens = lexer.tokenize().expect("Lexer should succeed");
        eprintln!("=== TOKENS ===");
        for (i, token) in tokens.iter().enumerate() {
            eprintln!("{}: {:?}", i, token);
        }
        eprintln!("==============");

        let source = r#"module LED_Driver:
    pins:
        VCC
        GND
    add Resistor_0805 (val: 100Ω, tol: 5%) named R1
    route VCC to R1.In
"#;

        let module = parse_module(source).unwrap();
        assert_eq!(module.name.as_str(), "LED_Driver");
        assert_eq!(module.pins.len(), 2);
        assert_eq!(module.statements.len(), 2);
    }

    #[test]
    fn test_parse_module_with_array_pins() {
        let source = r#"module ALU_64Bit:
    pins:
        Bus_A[64]
        Bus_B[64]
        CarryIn
"#;

        let module = parse_module(source).unwrap();
        assert_eq!(module.pins.len(), 3);
        assert_eq!(module.pins[0].array_size, Some(64));
        assert_eq!(module.pins[1].array_size, Some(64));
        assert_eq!(module.pins[2].array_size, None);
    }

    #[test]
    fn test_parse_module_with_for_loop() {
        let source = r#"module ALU:
    pins:
        Bus[8]
    for i in 0..7:
        add Bit named B[i]
        route Bus[i] to B[i].In
"#;

        let module = parse_module(source).unwrap();
        assert_eq!(module.statements.len(), 1);

        if let ModuleStatement::For(for_loop) = &module.statements[0] {
            assert_eq!(for_loop.variable, "i");
            assert_eq!(for_loop.start, 0);
            assert_eq!(for_loop.end, 7);
            assert_eq!(for_loop.body.len(), 2);
        } else {
            panic!("Expected for loop");
        }
    }

    #[test]
    fn test_parse_module_with_if_conditional() {
        let source = r#"module ALU:
    pins:
        CarryIn
    for i in 0..7:
        if i = 0:
            route CarryIn to Bit[i].CarryIn
        else:
            route Bit[i - 1].CarryOut to Bit[i].CarryIn
"#;

        let module = parse_module(source).unwrap();

        if let ModuleStatement::For(for_loop) = &module.statements[0] {
            if let ModuleStatement::If(if_stmt) = &for_loop.body[0] {
                assert!(matches!(if_stmt.condition, Condition::Equals { .. }));
                assert_eq!(if_stmt.then_body.len(), 1);
                assert!(if_stmt.else_body.is_some());
            } else {
                panic!("Expected if statement");
            }
        } else {
            panic!("Expected for loop");
        }
    }

    #[test]
    fn test_parse_module_rejects_negative_array_index() {
        let source = r#"module Test:
    pins:
        Bus[64]
    route Bus[-1] to GND
"#;

        // Negative array indices like Bus[-1] are not supported
        // Hardware array indices must be non-negative
        // Use subtraction syntax like Bus[i-1] inside loops instead
        let lexer = Lexer::new(source);
        let tokens = lexer.tokenize().expect("Lexer should succeed");
        let mut parser = Parser::new(tokens);
        let collector = DiagnosticCollector::new(source, 100);
        let _result = parser.parse(&collector);

        // The parser should reject standalone negative indices
        assert!(
            collector.has_errors(),
            "Parser should reject negative array index Bus[-1]"
        );
    }

    #[test]
    fn test_parse_module_rejects_negative_component_index() {
        let source = r#"module Test:
    pins:
        Out
    route Comp[-5].Pin to Out
"#;

        let lexer = Lexer::new(source);
        let tokens = lexer.tokenize().expect("Lexer should succeed");
        let mut parser = Parser::new(tokens);
        let collector = DiagnosticCollector::new(source, 100);
        let _result = parser.parse(&collector);

        // Should reject negative component array index
        assert!(
            collector.has_errors(),
            "Parser should reject negative component index Comp[-5]"
        );
    }

    #[test]
    fn test_parse_module_accepts_arithmetic_subtraction() {
        // This should work: i-1 in a loop context is arithmetic, not a negative literal
        let source = r#"module Test:
    pins:
        Bus[64]
    for i in 0..64:
        if i > 0:
            route Bus[i-1] to Bus[i]
"#;

        let lexer = Lexer::new(source);
        let tokens = lexer.tokenize().expect("Lexer should succeed");
        let mut parser = Parser::new(tokens);
        let collector = DiagnosticCollector::new(source, 100);
        let _result = parser.parse(&collector);

        // Should accept i-1 as arithmetic expression
        assert!(
            !collector.has_errors(),
            "Parser should accept arithmetic expression i-1: {}",
            collector.summary()
        );
    }
}
