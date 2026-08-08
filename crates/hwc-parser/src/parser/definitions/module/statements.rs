use crate::ast::arena::ModuleComponentId;
use crate::ast::{ModuleComponentPlacement, ModulePinReference, ModuleRoute, Span};
use crate::lexer::Token;
use crate::parser::{ParseError, Parser};
use smallvec::SmallVec;

impl Parser {
    /// Parse component addition in module: `add ComponentType (params) named Instance`
    /// Returns arena-allocated reference for zero-copy AST
    pub(super) fn parse_module_add(
        &mut self,
    ) -> Result<ModuleComponentId, ParseError> {
        let start = self.current_span();

        self.expect(&Token::Add)?;

        // Parse component type (supports namespaced identifiers like Parts.MCU)
        let component_type = self.expect_namespaced_identifier_string()?;

        // Parse optional parameters
        let parameters = if self.check(&Token::OpenParen) {
            self.parse_parameters()?
        } else {
            SmallVec::new()
        };

        // Parse optional name
        let (name, array_index) = if self.check(&Token::Named) {
            self.advance(); // consume 'named'
            let instance_name = self.expect_identifier_string()?;

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

        // Return by value
        let placement = ModuleComponentPlacement {
            component_type: component_type.into(),
            parameters,
            name: name.map(|s: String| s.into()),
            array_index,
            span,
        };

        // Arena-allocate and return ID
        Ok(self.arena.alloc_module_component(placement))
    }

    /// Parse route in module: `route From.Pin to To.Pin`
    pub(super) fn parse_module_route(&mut self) -> Result<ModuleRoute, ParseError> {
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
    pub(super) fn parse_module_pin_reference(&mut self) -> Result<ModulePinReference, ParseError> {
        let start = self.current_span();

        // Parse first identifier (could be component or pin name)
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

        // Check if there's a dot (component.pin) or not (just pin name)
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
}
