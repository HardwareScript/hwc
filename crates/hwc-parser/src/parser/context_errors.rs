//! Context-Aware Error System for Hardware Script Parser
//!
//! This module provides intelligent error messages by tracking parsing context
//! and providing actionable suggestions based on what the parser expected vs. what it found.

use crate::lexer::{Span, Token};
use crate::parser::error::ParseError;

/// Parsing context to track what structural element we're inside
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParsingContext {
    TopLevel,
    MaterialDefinition,
    ComponentDefinition,
    ModuleDefinition,
    ProfileDefinition,
    SpaceDefinition,
    ComponentPlacement,
    PourStatement,
    RouteStatement,
    ContactStatement,
    PropertyBlock,
    LogicBlock,
}

/// Component placement sub-context for detailed error messages
#[derive(Debug, Clone, Default)]
pub struct PlacementParseState {
    pub has_position: bool,
    pub has_elevation: bool,
    pub has_rotation: bool,
    pub has_config_block: bool,
    pub has_mount: bool,
    pub has_standoff: bool,
    pub has_net_bindings: bool,
}

/// Route statement sub-context for detailed error messages
#[derive(Debug, Clone, Default)]
pub struct RouteParseState {
    pub has_from: bool,
    pub has_to: bool,
    pub has_width: bool,
    pub has_layer: bool,
    pub has_strategy: bool,
    pub has_current_limit: bool,
}

/// Pour statement sub-context for detailed error messages
#[derive(Debug, Clone, Default)]
pub struct PourParseState {
    pub has_material: bool,
    pub has_name: bool,
    pub has_layer: bool,
    pub has_net: bool,
    pub has_boundary: bool,
    pub has_device: bool,
}

/// Space definition sub-context for detailed error messages
#[derive(Debug, Clone, Default)]
pub struct SpaceParseState {
    pub has_dimensions: bool,
    pub has_resolution: bool,
    pub has_origin: bool,
    pub has_profile: bool,
    pub has_nets: bool,
}

/// Context-aware error generator
pub struct ContextErrorGenerator {
    context_stack: Vec<ParsingContext>,
}

impl ContextErrorGenerator {
    pub fn new() -> Self {
        Self {
            context_stack: vec![ParsingContext::TopLevel],
        }
    }

    pub fn enter_context(&mut self, ctx: ParsingContext) {
        self.context_stack.push(ctx);
    }

    pub fn exit_context(&mut self) {
        if self.context_stack.len() > 1 {
            self.context_stack.pop();
        }
    }

    pub fn current_context(&self) -> ParsingContext {
        *self
            .context_stack
            .last()
            .unwrap_or(&ParsingContext::TopLevel)
    }

    /// Generate a context-aware error for unexpected tokens
    pub fn unexpected_token_error(
        &self,
        found: &Token,
        span: &Span,
        placement_state: Option<&PlacementParseState>,
    ) -> ParseError {
        let message = match self.current_context() {
            ParsingContext::ComponentPlacement => {
                self.component_placement_error(found, placement_state)
            }
            ParsingContext::SpaceDefinition => self.space_definition_error(found),
            ParsingContext::PropertyBlock => self.property_block_error(found),
            _ => format!("Unexpected token: {}", found),
        };

        ParseError::General {
            span: crate::parser::error::span_to_source_span(span),
            message: message.into(),
        }
    }

    /// Generate a context-aware error for route statements
    pub fn route_error(
        &self,
        found: &Token,
        span: &Span,
        state: Option<&RouteParseState>,
    ) -> ParseError {
        let message = self.route_statement_error(found, state);

        ParseError::General {
            span: crate::parser::error::span_to_source_span(span),
            message: message.into(),
        }
    }

    /// Generate a context-aware error for pour statements
    pub fn pour_error(
        &self,
        found: &Token,
        span: &Span,
        state: Option<&PourParseState>,
    ) -> ParseError {
        let message = self.pour_statement_error(found, state);

        ParseError::General {
            span: crate::parser::error::span_to_source_span(span),
            message: message.into(),
        }
    }

    /// Generate specific error for component placement context
    fn component_placement_error(
        &self,
        found: &Token,
        state: Option<&PlacementParseState>,
    ) -> String {
        if let Some(state) = state {
            // Check for common ordering mistakes
            if matches!(found, Token::On) && state.has_rotation && !state.has_elevation {
                return format!(
                    "Placement syntax error: 'on layer:' must come BEFORE 'rotated'.\n\n\
                    Current (incorrect): add Type named N at [...] rotated 0deg on layer: L\n\
                    Correct syntax:      add Type named N at [...] on layer: L rotated 0deg:\n\n\
                    Expected order: at [position] on layer: <L> rotated <A>:"
                );
            }

            if matches!(found, Token::Rotated) && !state.has_elevation {
                return format!(
                    "Placement syntax error: 'rotated' must come AFTER 'on layer:' or 'on z:'.\n\n\
                    Current (incorrect): add Type named N at [...] rotated 0deg on layer: L\n\
                    Correct syntax:      add Type named N at [...] on layer: L rotated 0deg:\n\n\
                    Expected order: at [position] on layer: <L> rotated <A>:"
                );
            }

            if matches!(found, Token::Colon) && !state.has_elevation {
                return format!(
                    "Component placement missing required 'on layer:' or 'on z:' elevation.\n\n\
                    Syntax: add Type named N at [x: 1mm, y: 2mm] on layer: metal1 rotated 0deg:\n\
                    Or:     add Type named N at [x: 1mm, y: 2mm] on z: 1mm rotated 0deg:"
                );
            }

            if !state.has_rotation && (state.has_config_block || matches!(found, Token::Colon)) {
                return format!(
                    "Component placement missing required rotation.\n\n\
                    Syntax: add Type named N at [...] on layer: L rotated 0deg:\n\
                    Use 'rotated 0deg' if no rotation is needed."
                );
            }
        }

        format!(
            "Unexpected {} in component placement. Expected component placement syntax:\n\
                 add Type named N at [x: X, y: Y] on layer: L rotated A:\n\
                     mount: top|bottom|embedded\n\
                     standoff: <value>\n\
                     net: [pin1: Net1, pin2: Net2]",
            found
        )
    }

    /// Generate specific error for space definition context
    fn space_definition_error(&self, found: &Token) -> String {
        // Provide suggestions for common mistakes
        if let Token::Identifier(name) = found {
            match name.as_str() {
                "component" | "module" | "material" | "profile" => {
                    return format!(
                        "Found '{}' keyword inside space definition.\n\
                         Space blocks can only contain: dimensions, resolution, origin, profile, nets, add, route, expose, for.\n\n\
                         Tip: Component and module definitions belong at the top level, not inside a space.",
                        name
                    );
                }
                "dimension" => {
                    return "Found 'dimension' (singular). Did you mean 'dimensions' (plural)?\n\
                            Syntax: dimensions: <width> by <height> by <depth>"
                        .to_string();
                }
                "grid" => {
                    return "Found 'grid'. In v0.1.6+, use 'resolution:' instead.\n\
                            Syntax: resolution: 1nm"
                        .to_string();
                }
                _ => {}
            }
        }

        format!(
            "Unexpected {} in space definition.\n\
             Valid space statements:\n\
             - dimensions: <w> by <h> by <d>\n\
             - resolution: <step>\n\
             - origin: <origin_xy> by <origin_z>\n\
             - profile: <ProfileName>\n\
             - nets: <net_declarations>\n\
             - add <component_placement>\n\
             - route <routing_statement>\n\
             - expose <port_exposure>\n\
             - for <loop_statement>",
            found
        )
    }

    /// Generate specific error for route statement context
    fn route_statement_error(&self, found: &Token, state: Option<&RouteParseState>) -> String {
        if let Some(state) = state {
            if !state.has_from && !state.has_to {
                return format!(
                    "Route statement incomplete. Expected: route <from> to <to>\n\
                     Examples:\n\
                     - route M1.gate to VIN_Pad\n\
                     - route Pad1 to Pad2\n\
                     - route Component1.pin1 to Component2.pin2"
                );
            }

            if state.has_from && !state.has_to && !matches!(found, Token::To) {
                return format!(
                    "Route statement missing 'to' keyword.\n\
                     Syntax: route <from> to <to>:\n\
                         width: <value>\n\
                         layer: <layer>"
                );
            }
        }

        format!(
            "Unexpected {} in route statement.\n\
             Route syntax:\n\
             route <from> to <to>:\n\
                 width: <value>\n\
                 layer: <layer>\n\
                 strategy: <strategy_name>\n\
                 current_limit_ac: {{ rms: <value>, peak: <value> }}",
            found
        )
    }

    /// Generate specific error for pour statement context
    fn pour_statement_error(&self, found: &Token, state: Option<&PourParseState>) -> String {
        if let Some(state) = state {
            if !state.has_material {
                return format!(
                    "Pour statement missing material specification.\n\
                     Syntax: add pour(<Material>) named <Name> on layer: <layer>:\n\
                         boundary: [x: X1, y: Y1] to [x: X2, y: Y2]"
                );
            }

            if state.has_material && !state.has_layer {
                return format!(
                    "Pour statement missing 'on layer:' clause.\n\
                     Syntax: add pour(<Material>) named <Name> on layer: <layer>:\n\
                         boundary: [x: X1, y: Y1] to [x: X2, y: Y2]"
                );
            }
        }

        format!(
            "Unexpected {} in pour statement.\n\
             Pour syntax:\n\
             add pour(<Material>) named <Name> on layer: <layer>:\n\
                 net: <NetName>\n\
                 device: <device_terminal>\n\
                 boundary: [x: X1, y: Y1] to [x: X2, y: Y2]",
            found
        )
    }

    /// Generate specific error for property block context
    fn property_block_error(&self, found: &Token) -> String {
        if matches!(found, Token::Equals) {
            return "Found '=' in property block. Use ':' for properties.\n\
                    The Boundary Law: ':' for declarative properties, '=' for behavioral logic.\n\
                    Example: resistance: 10kΩ"
                .to_string();
        }

        format!(
            "Unexpected {} in property block. Expected 'property: value' format",
            found
        )
    }

    /// Generate error with lookahead analysis
    pub fn unexpected_token_with_lookahead(
        &self,
        found: &Token,
        next: Option<&Token>,
        span: &Span,
    ) -> ParseError {
        let message = if matches!(found, Token::On) && matches!(next, Some(Token::Identifier(_))) {
            "Found 'on layer:' in unexpected position. Check the order of placement clauses.\n\
             Correct order: at [position] on layer: <layer> rotated <angle>:"
                .to_string()
        } else {
            format!("Unexpected token: {}", found)
        };

        ParseError::General {
            span: crate::parser::error::span_to_source_span(span),
            message: message.into(),
        }
    }
}

impl Default for ContextErrorGenerator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_placement_ordering_error() {
        let mut gen = ContextErrorGenerator::new();
        gen.enter_context(ParsingContext::ComponentPlacement);

        let state = PlacementParseState {
            has_position: true,
            has_rotation: true,
            has_elevation: false,
            ..Default::default()
        };

        let span = Span::new(0, 2);
        let error = gen.unexpected_token_error(&Token::On, &span, Some(&state));

        let error_msg = format!("{}", error);
        assert!(error_msg.contains("'on layer:' must come BEFORE 'rotated'"));
    }
}
