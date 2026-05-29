//! Render block parsing

use super::super::super::error::ParseError;
use crate::ast::*;
use crate::lexer::Token;

impl super::super::super::Parser {
    pub(in crate::parser) fn parse_render_block(&mut self) -> Result<RenderBlock, ParseError> {
        let mut render_type = None;
        let mut shape = None;
        let mut body_color = None;
        let mut endcap_color = None;
        let mut label = None;
        let mut asset = None;
        let mut view = None; // NEW v0.1.6: Orientation hint

        while !self.is_at_end() && !self.check(&Token::Dedent) {
            if self.check(&Token::Newline) || self.check(&Token::Indent) {
                self.advance();
                continue;
            }

            if self.check(&Token::Dedent) {
                break;
            }

            // v0.1.6: Check if we've hit the start of the next block (by identifier name.into())
            if let Some(current) = self.current() {
                if let Token::Identifier(name) = &current.token {
                    if matches!(name.as_str(), "electrical" | "layout" | "pins" | "metadata") {
                        break;
                    }
                }
            }

            // v0.1.6: Property keys are just identifiers
            let key_str = if let Some(spanned) = self.current() {
                if let Token::Identifier(id) = &spanned.token {
                    let key = id.clone();
                    self.advance();
                    key
                } else {
                    break;
                }
            } else {
                break;
            };

            self.expect(&Token::Colon)?;

            let value = if let Some(spanned) = self.current() {
                match &spanned.token {
                    Token::String(s) => {
                        let val = s.clone();
                        self.advance();
                        val
                    }
                    Token::Identifier(id) => {
                        let val = id.clone();
                        self.advance();
                        val
                    }
                    _ => String::new(),
                }
            } else {
                String::new()
            };

            match key_str.as_str() {
                "type" => render_type = Some(value),
                "shape" => shape = Some(value),
                "body_color" => body_color = Some(value),
                "endcap_color" => endcap_color = Some(value),
                "label" => label = Some(value),
                "asset" => asset = Some(value),
                "view" => view = Some(value),
                _ => {}
            }
        }

        // Consume the dedent that ends the render block
        if self.check(&Token::Dedent) {
            self.advance();
        }

        Ok(RenderBlock {
            render_type: render_type.map(|s: String| s.into()),
            shape: shape.map(|s: String| s.into()),
            body_color: body_color.map(|s: String| s.into()),
            endcap_color: endcap_color.map(|s: String| s.into()),
            label: label.map(|s: String| s.into()),
            asset: asset.map(|s: String| s.into()),
            view: view.map(|s: String| s.into()),
        })
    }
}
