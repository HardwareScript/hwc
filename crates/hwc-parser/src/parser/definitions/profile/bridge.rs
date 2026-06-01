use crate::ast::*;
use crate::lexer::{Span, Token};
use super::super::super::error::{span_to_source_span, ParseError};

impl super::super::super::Parser {
    /// Parse a bridge rule: `bridge Silicon to Copper: Cobalt_Silicide`
    /// or compound: `bridge Silicon to Copper: \n interface: ...`
    pub(super) fn parse_bridge_rule(&mut self) -> Result<BridgeRule, ParseError> {
        let start_pos = self.current_span().start;
        
        self.expect(&Token::Bridge)?;
        
        let from_mat = self.expect_identifier()?;
        self.expect(&Token::To)?;
        let to_mat = self.expect_identifier()?;
        self.expect(&Token::Colon)?;
        
        // Two forms: 
        // 1. Single line: `bridge A to B: Material`
        // 2. Multi-line compound stack: `bridge A to B:\n  interface: ...`
        
        let mut interface_material = None;
        let mut interface_thickness = None;
        let mut fill_material = None;
        
        if self.check(&Token::Newline) {
            self.advance();
            self.expect(&Token::Indent)?;
            
            while !self.check(&Token::Dedent) && !self.is_at_end() {
                self.skip_whitespace();
                if self.check(&Token::Dedent) || self.is_at_end() {
                    break;
                }
                
                let field_name = self.expect_identifier_or_keyword()?;
                self.expect(&Token::Colon)?;
                
                match field_name.as_str() {
                    "interface" => {
                        interface_material = Some(self.expect_identifier()?);
                        self.skip_whitespace();
                    }
                    "thickness" => {
                        interface_thickness = Some(self.parse_measurement()?);
                        self.skip_whitespace();
                    }
                    "fill" => {
                        fill_material = Some(self.expect_identifier()?);
                        self.skip_whitespace();
                    }
                    _ => {
                        return Err(self.error(&format!("Unknown bridge constraint: '{}'", field_name)));
                    }
                }
            }
            self.expect(&Token::Dedent)?;
        } else {
            // Single line fallback
            interface_material = Some(self.expect_identifier()?);
            self.skip_whitespace();
        }
        
        let end_pos = self.previous_span().end;
        
        let interface_material = interface_material.ok_or_else(|| ParseError::General {
            span: span_to_source_span(&Span::new(start_pos, end_pos)),
            message: "Bridge rule must specify at least an 'interface' material".into(),
        })?;
        
        Ok(BridgeRule {
            from: from_mat.name,
            to: to_mat.name,
            interface_material: interface_material.name,
            interface_thickness,
            fill_material: fill_material.map(|id| id.name),
            span: Span::new(start_pos, end_pos),
        })
    }
}
