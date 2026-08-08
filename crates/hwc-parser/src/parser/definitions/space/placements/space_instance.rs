//! Hierarchical space instantiation parser (v0.2.1)
//!
//! Syntax:
//! ```text
//! add space PMOS_Cell named PMOS_Inst at [x: 0nm, y: 0nm] rotated 0deg:
//!     net_map: [VDD_Rail: VDD, Out_Pad: Out, Gate_Strip: In]
//! ```

use crate::ast::*;
use crate::lexer::{Span, Token};
use crate::ParseError;

impl<'ast> crate::parser::Parser<'ast> {
    /// Parse space instance placement: `add space SpaceName named InstName at [...] rotated ...deg:`
    pub(in crate::parser) fn parse_space_instance(
        &mut self,
    ) -> Result<SpaceInstancePlacement, ParseError> {
        let start_pos = self.current_span().start;

        // Consume 'add'
        self.expect(&Token::Add)?;

        // Consume 'space'
        self.expect(&Token::Space)?;

        // Parse the space name (the definition being instantiated)
        let space_name = self.expect_identifier()?;

        // Parse 'named InstanceName'
        self.expect(&Token::Named)?;
        let instance_name = ComponentName {
            base: self.expect_identifier_string()?.into(),
            index: None,          // Space instances don't use array indexing
            template_parts: None, // Space instances don't use template interpolation
            span: self.previous_span(),
        };

        // Parse position: 'at [x: 0nm, y: 0nm, z: 0nm]'
        // All coordinates have x, y, z - enforced by parse_coordinate()
        self.expect(&Token::At)?;
        let position = self.parse_coordinate()?;

        // Parse REQUIRED rotation: 'rotated 0deg' - no defaults allowed
        if !self.check(&Token::Rotated) {
            return Err(self.error(
                "Space instantiation requires explicit rotation. Add 'rotated 0deg', 'rotated 90deg', 'rotated 180deg', or 'rotated 270deg'"
            ));
        }
        let rotation = self.parse_rotation()?;

        // Expect colon and indented block for net_map
        self.expect(&Token::Colon)?;

        // Expect newline after colon (no inline net_map allowed)
        if !self.check(&Token::Newline) && !self.is_at_end() {
            return Err(self.error("Expected newline after space instance header"));
        }
        if self.check(&Token::Newline) {
            self.advance();
        }

        if !self.check(&Token::Indent) {
            return Err(self.error("Expected indented block with 'net_map' for space instance"));
        }
        self.advance(); // consume indent

        // Parse net_map block - REQUIRED and MUST NOT be empty
        let mut net_map = rustc_hash::FxHashMap::default();

        // Expect 'net_map: [...] ' syntax - this is REQUIRED
        if let Some(current) = self.current() {
            if let Token::Identifier(name) = &current.token {
                if name == "net_map" {
                    self.advance(); // consume 'net_map'
                    self.expect(&Token::Colon)?;
                    self.skip_whitespace();

                    // Parse the map: [ChildNet: ParentNet, ...]
                    self.expect(&Token::OpenBracket)?;
                    self.skip_whitespace();

                    // Parse at least one mapping - empty net_map is not allowed
                    if self.check(&Token::CloseBracket) {
                        return Err(self.error(
                            "Space instantiation requires at least one net mapping in net_map. \
                            Example: net_map: [VDD_Rail: VDD, GND_Rail: GND]",
                        ));
                    }

                    while !self.check(&Token::CloseBracket) && !self.is_at_end() {
                        // Parse child net name
                        let child_net = self.expect_identifier_string()?;
                        self.expect(&Token::Colon)?;
                        self.skip_whitespace();

                        // Parse parent net name
                        let parent_net = self.expect_identifier_string()?;

                        // Check for duplicate child net names
                        if net_map.contains_key(child_net.as_str()) {
                            return Err(self.error(&format!(
                                "Duplicate mapping for child net '{}'. Each child net can only be mapped once.",
                                child_net
                            )));
                        }

                        net_map.insert(child_net.into(), parent_net.into());

                        self.skip_whitespace();

                        // Check for comma
                        if self.check(&Token::Comma) {
                            self.advance();
                            self.skip_whitespace();
                        } else {
                            break;
                        }
                    }

                    self.expect(&Token::CloseBracket)?;
                    self.skip_whitespace();
                } else {
                    return Err(self.error(&format!(
                        "Expected 'net_map' in space instance block, found '{}'. \
                        Space instantiation requires explicit net mapping.",
                        name
                    )));
                }
            } else {
                return Err(self.error(
                    "Expected 'net_map' in space instance block. \
                    Space instantiation requires explicit net mapping.",
                ));
            }
        } else {
            return Err(self.error(
                "Expected 'net_map' in space instance block. \
                Space instantiation requires explicit net mapping.",
            ));
        }

        // Consume dedent
        if self.check(&Token::Dedent) {
            self.advance();
        }

        let end_pos = self.previous_span().end;

        Ok(SpaceInstancePlacement {
            space_name,
            instance_name,
            position,
            rotation: Some(rotation), // Always Some() since we validate it's required above
            net_map,
            span: Span::new(start_pos, end_pos),
        })
    }
}
