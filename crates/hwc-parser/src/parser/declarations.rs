//! HardwareScript v0.3.0 Top-Level Declarations Parser

use crate::ast::*;
use crate::lexer::Token;
use crate::parser::{ParseError, Parser};
use compact_str::CompactString;

impl Parser {
    /// Parse an import declaration:
    /// `import * from @std/primitives/units`
    /// `import { sky130_nmos, pad } from @std/layout/sky130`
    /// `import "path/to/file"`
    pub fn parse_import(&mut self) -> Result<ImportDecl, ParseError> {
        let start_pos = self.current_span().start;
        self.expect_token(&Token::Import, "Expected 'import'")?;

        let symbols = if self.check(&Token::Asterisk) {
            self.advance(); // consume `*`
            ImportSymbols::All
        } else if self.check(&Token::OpenBrace) {
            self.advance(); // consume `{`
            let mut list = Vec::new();
            while !self.check(&Token::CloseBrace) && !self.is_at_end() {
                let ident = self.expect_identifier()?;
                list.push(CompactString::from(ident.name.as_str()));
                if self.check(&Token::Comma) {
                    self.advance();
                } else {
                    break;
                }
            }
            self.expect_token(&Token::CloseBrace, "Expected '}' after import symbol list")?;
            ImportSymbols::Named(list)
        } else if let Some(Token::ImportPath(path)) = self.current().map(|t| &t.token) {
            let path_str = path.clone();
            let end_pos = self.current_span().end;
            self.advance();
            return Ok(ImportDecl {
                symbols: ImportSymbols::All,
                from: path_str,
                span: Span::new(start_pos, end_pos),
            });
        } else if let Some(Token::String(s)) = self.current().map(|t| &t.token) {
            let path_str = s.clone();
            let end_pos = self.current_span().end;
            self.advance();
            return Ok(ImportDecl {
                symbols: ImportSymbols::All,
                from: path_str,
                span: Span::new(start_pos, end_pos),
            });
        } else {
            let ident = self.expect_identifier()?;
            ImportSymbols::Single(ident.name.as_str().into())
        };

        self.expect_token(&Token::From, "Expected 'from' after import symbols")?;

        let (from_path, end_pos) = if let Some(Token::ImportPath(path)) = self.current().map(|t| &t.token) {
            let p = path.clone();
            let end = self.current_span().end;
            self.advance();
            (p, end)
        } else if let Some(Token::String(s)) = self.current().map(|t| &t.token) {
            let p = s.clone();
            let end = self.current_span().end;
            self.advance();
            (p, end)
        } else {
            let ident = self.expect_identifier()?;
            let end = self.previous_span().end;
            (ident.name.to_string(), end)
        };

        // Optional trailing semicolon
        if self.check(&Token::Semicolon) {
            self.advance();
        }

        Ok(ImportDecl {
            symbols,
            from: from_path,
            span: Span::new(start_pos, end_pos),
        })
    }

    /// Parse an export list: `export { Item1, Item2, Item3 }`
    pub fn parse_export_list(&mut self, start_pos: usize) -> Result<ExportDecl, ParseError> {
        self.expect_token(&Token::OpenBrace, "Expected '{' for export list")?;
        
        let mut symbols = Vec::new();
        while !self.check(&Token::CloseBrace) && !self.is_at_end() {
            let ident = self.expect_identifier()?;
            symbols.push(CompactString::from(ident.name.as_str()));
            if self.check(&Token::Comma) {
                self.advance();
            } else {
                break;
            }
        }
        
        let close_span = self.expect_token(&Token::CloseBrace, "Expected '}' after export symbol list")?;
        
        // Optional trailing semicolon
        if self.check(&Token::Semicolon) {
            self.advance();
        }
        
        Ok(ExportDecl {
            symbols,
            span: Span::new(start_pos, close_span.end),
        })
    }

    /// Parse a top-level function declaration
    pub fn parse_function_decl(&mut self, is_exported: bool, start_pos: usize) -> Result<FunctionDecl, ParseError> {
        self.expect_token(&Token::Fn, "Expected 'fn'")?;
        let name = self.expect_identifier()?;

        self.expect_token(&Token::OpenParen, "Expected '(' for function parameter list")?;
        let mut parameters = Vec::new();

        while !self.check(&Token::CloseParen) && !self.is_at_end() {
            let param_start = self.current_span().start;
            let param_ident = self.expect_identifier()?;
            let param_name: CompactString = param_ident.name.as_str().into();

            self.expect_token(&Token::Colon, "Expected ':' after parameter name")?;
            let type_annotation = self.parse_type_expr()?;

            let default_value = if self.check(&Token::Equals) {
                self.advance();
                Some(self.parse_expression()?)
            } else {
                None
            };

            let param_end = if let Some(def) = &default_value {
                def.span().end
            } else {
                type_annotation.span().end
            };

            parameters.push(Parameter {
                name: param_name,
                type_annotation,
                default_value,
                span: Span::new(param_start, param_end),
            });

            if self.check(&Token::Comma) {
                self.advance();
            } else {
                break;
            }
        }

        self.expect_token(&Token::CloseParen, "Expected ')' to close parameter list")?;

        let return_type = if self.check(&Token::Arrow) {
            self.advance();
            Some(self.parse_type_expr()?)
        } else {
            None
        };

        let body = self.parse_block()?;
        let end_pos = body.span.end;

        Ok(FunctionDecl {
            is_exported,
            name,
            parameters,
            return_type,
            body,
            span: Span::new(start_pos, end_pos),
        })
    }

    /// Parse a top-level struct declaration: `struct Name { field: Type, ... }`
    pub fn parse_struct_decl(&mut self, is_exported: bool, start_pos: usize) -> Result<StructDecl, ParseError> {
        self.expect_token(&Token::Struct, "Expected 'struct'")?;
        let name = self.expect_identifier()?;

        self.expect_token(&Token::OpenBrace, "Expected '{' for struct body")?;
        let mut fields = Vec::new();

        while !self.check(&Token::CloseBrace) && !self.is_at_end() {
            let field_start = self.current_span().start;
            let field_ident = self.expect_identifier()?;
            let field_name: CompactString = field_ident.name.as_str().into();

            self.expect_token(&Token::Colon, "Expected ':' after field name")?;
            let type_annotation = self.parse_type_expr()?;
            let field_end = type_annotation.span().end;

            fields.push(StructFieldDecl {
                name: field_name,
                type_annotation,
                span: Span::new(field_start, field_end),
            });

            if self.check(&Token::Comma) {
                self.advance();
            } else {
                break;
            }
        }

        let close_span = self.expect_token(&Token::CloseBrace, "Expected '}' to close struct body")?;

        Ok(StructDecl {
            is_exported,
            name,
            fields,
            span: Span::new(start_pos, close_span.end),
        })
    }

    /// Parse a top-level enum declaration: `enum Name { Variant1, Variant2(Type), ... }`
    pub fn parse_enum_decl(&mut self, is_exported: bool, start_pos: usize) -> Result<EnumDecl, ParseError> {
        self.expect_token(&Token::Enum, "Expected 'enum'")?;
        let name = self.expect_identifier()?;

        self.expect_token(&Token::OpenBrace, "Expected '{' for enum body")?;
        let mut variants = Vec::new();

        while !self.check(&Token::CloseBrace) && !self.is_at_end() {
            let var_start = self.current_span().start;
            let var_ident = self.expect_identifier()?;
            let var_name: CompactString = var_ident.name.as_str().into();

            let (payload, var_end) = if self.check(&Token::OpenParen) {
                self.advance();
                let mut tuple_types = Vec::new();
                while !self.check(&Token::CloseParen) && !self.is_at_end() {
                    tuple_types.push(self.parse_type_expr()?);
                    if self.check(&Token::Comma) {
                        self.advance();
                    } else {
                        break;
                    }
                }
                let cp = self.expect_token(&Token::CloseParen, "Expected ')'")?;
                (Some(EnumVariantPayload::Tuple(tuple_types)), cp.end)
            } else {
                (None, self.previous_span().end)
            };

            variants.push(EnumVariantDecl {
                name: var_name,
                payload,
                span: Span::new(var_start, var_end),
            });

            if self.check(&Token::Comma) {
                self.advance();
            } else {
                break;
            }
        }

        let close_span = self.expect_token(&Token::CloseBrace, "Expected '}' to close enum body")?;

        Ok(EnumDecl {
            is_exported,
            name,
            variants,
            span: Span::new(start_pos, close_span.end),
        })
    }

    /// Parse a constant declaration: `(export)? const NAME: Type = value`
    pub fn parse_const_decl(&mut self, is_exported: bool, start_pos: usize) -> Result<ConstDecl, ParseError> {
        self.expect_token(&Token::Const, "Expected 'const'")?;
        let name = self.expect_identifier()?;

        // Optional type annotation
        let type_annotation = if self.check(&Token::Colon) {
            self.advance();
            Some(self.parse_type_expr()?)
        } else {
            None
        };

        self.expect_token(&Token::Equals, "Expected '=' after const name")?;
        let value = self.parse_expression()?;

        // Optional trailing semicolon
        if self.check(&Token::Semicolon) {
            self.advance();
        }

        let end_pos = self.previous_span().end;

        Ok(ConstDecl {
            is_exported,
            name,
            type_annotation,
            value,
            span: Span::new(start_pos, end_pos),
        })
    }

    /// Parse an attribute: `#[name(arg1, arg2, ...)]` or `#[name]`
    pub fn parse_attribute(&mut self) -> Result<Attribute, ParseError> {
        let start_pos = self.current_span().start;
        self.expect_token(&Token::HashBracket, "Expected '#[' to begin attribute")?;
        let name = self.expect_identifier()?;

        let mut arguments = Vec::new();
        if self.check(&Token::OpenParen) {
            self.advance();
            while !self.check(&Token::CloseParen) && !self.is_at_end() {
                arguments.push(self.parse_expression()?);
                if self.check(&Token::Comma) {
                    self.advance();
                } else {
                    break;
                }
            }
            self.expect_token(&Token::CloseParen, "Expected ')' after attribute arguments")?;
        }

        let close_span = self.expect_token(&Token::CloseBracket, "Expected ']' to close attribute")?;
        Ok(Attribute {
            name,
            arguments,
            span: Span::new(start_pos, close_span.end),
        })
    }

    /// Parse consecutive attributes: `(#[attr])*`
    pub fn parse_attributes(&mut self) -> Result<Vec<Attribute>, ParseError> {
        let mut attributes = Vec::new();
        while self.check(&Token::HashBracket) {
            attributes.push(self.parse_attribute()?);
        }
        Ok(attributes)
    }

    /// Parse a space declaration: `(#[attr])* space Name implements Interface { ... }`
    pub fn parse_space_decl(&mut self, mut attributes: Vec<Attribute>, start_pos: usize) -> Result<SpaceDecl, ParseError> {
        while self.check(&Token::HashBracket) {
            attributes.push(self.parse_attribute()?);
        }
        self.expect_token(&Token::Space, "Expected 'space'")?;
        let name = self.expect_identifier()?;

        let implements = if self.check(&Token::Implements) {
            self.advance();
            Some(self.expect_identifier()?)
        } else {
            None
        };

        self.expect_token(&Token::OpenBrace, "Expected '{' for space body")?;

        let mut dimensions = None;
        let mut profile = None;
        let mut nets = Vec::new();
        let mut statements = Vec::new();

        while !self.check(&Token::CloseBrace) && !self.is_at_end() {
            while self.check(&Token::Semicolon) {
                self.advance();
            }
            if self.check(&Token::CloseBrace) || self.is_at_end() {
                break;
            }

            if self.check_identifier("nets") {
                // `nets { VDD: { ... }, VSS: { ... } }`
                self.advance(); // consume `nets`
                self.expect_token(&Token::OpenBrace, "Expected '{' after nets")?;

                while !self.check(&Token::CloseBrace) && !self.is_at_end() {
                    let net_start = self.current_span().start;
                    let net_ident = self.expect_identifier()?;
                    let net_name: CompactString = net_ident.name.as_str().into();

                    self.expect_token(&Token::Colon, "Expected ':' after net name")?;
                    self.expect_token(&Token::OpenBrace, "Expected '{' for net properties")?;

                    let mut properties = Vec::new();
                    while !self.check(&Token::CloseBrace) && !self.is_at_end() {
                        let prop_ident = self.expect_identifier()?;
                        let prop_name: CompactString = prop_ident.name.as_str().into();
                        self.expect_token(&Token::Colon, "Expected ':' after property name")?;
                        let prop_val = self.parse_expression()?;
                        properties.push((prop_name, prop_val));

                        if self.check(&Token::Comma) {
                            self.advance();
                        } else {
                            break;
                        }
                    }

                    let close_net = self.expect_token(&Token::CloseBrace, "Expected '}' to close net properties")?;
                    if self.check(&Token::Semicolon) {
                        self.advance();
                    }

                    nets.push(NetDecl {
                        name: net_name,
                        properties,
                        span: Span::new(net_start, close_net.end),
                    });
                }

                self.expect_token(&Token::CloseBrace, "Expected '}' to close nets block")?;
                continue;
            }

            let is_dimensions = match self.current().map(|t| &t.token) {
                Some(Token::Identifier(s)) => s == "dimensions",
                _ => false,
            };

            if is_dimensions {
                self.advance();
                self.expect_token(&Token::Colon, "Expected ':' after dimensions")?;
                self.expect_token(&Token::OpenBracket, "Expected '[' for dimensions [width, height]")?;
                let width = self.parse_expression()?;
                self.expect_token(&Token::Comma, "Expected ',' between dimensions")?;
                let height = self.parse_expression()?;
                self.expect_token(&Token::CloseBracket, "Expected ']' to close dimensions")?;
                if self.check(&Token::Semicolon) {
                    self.advance();
                }
                dimensions = Some((width, height));
                continue;
            }

            let is_profile = match self.current().map(|t| &t.token) {
                Some(Token::Profile) => true,
                Some(Token::Identifier(s)) => s == "profile",
                _ => false,
            };

            if is_profile {
                self.advance();
                self.expect_token(&Token::Colon, "Expected ':' after profile")?;
                let prof_ident = self.expect_identifier()?;
                if self.check(&Token::Semicolon) {
                    self.advance();
                }
                profile = Some(prof_ident);
                continue;
            }

            // Otherwise, it is a statement (e.g. let, region, function call, route, etc.)
            let stmt = self.parse_statement()?;
            statements.push(stmt);
            if self.check(&Token::Semicolon) {
                self.advance();
            }
        }

        let close_span = self.expect_token(&Token::CloseBrace, "Expected '}' to close space body")?;

        Ok(SpaceDecl {
            attributes,
            name,
            implements,
            dimensions,
            profile,
            nets,
            statements,
            span: Span::new(start_pos, close_span.end),
        })
    }

    /// Parse a module declaration: `(#[attr])* module Name { pins: [input In, output Out], logic { ... }, ... }`
    pub fn parse_module_decl(&mut self, mut attributes: Vec<Attribute>, start_pos: usize) -> Result<ModuleDecl, ParseError> {
        while self.check(&Token::HashBracket) {
            attributes.push(self.parse_attribute()?);
        }
        self.expect_token(&Token::Module, "Expected 'module'")?;
        let name = self.expect_identifier()?;

        self.expect_token(&Token::OpenBrace, "Expected '{' for module body")?;
        let mut pins = Vec::new();
        let mut logic_blocks = Vec::new();
        let mut routes = Vec::new();

        while !self.check(&Token::CloseBrace) && !self.is_at_end() {
            while self.check(&Token::Semicolon) {
                self.advance();
            }
            if self.check(&Token::CloseBrace) || self.is_at_end() {
                break;
            }

            if self.check_identifier("pins") {
                self.advance(); // consume `pins`
                self.expect_token(&Token::Colon, "Expected ':' after pins")?;
                self.expect_token(&Token::OpenBracket, "Expected '[' for pin list")?;

                while !self.check(&Token::CloseBracket) && !self.is_at_end() {
                    let pin_start = self.current_span().start;
                    let first_ident = self.expect_identifier()?;

                    // Check if first ident was direction (input, output, inout, power, ground)
                    let (dir, pin_name) = match first_ident.name.as_str() {
                        "input" | "output" | "inout" | "power" | "ground" => {
                            let second_ident = self.expect_identifier()?;
                            (Some(first_ident.name.as_str().into()), second_ident.name.as_str().into())
                        }
                        other => (None, other.into()),
                    };

                    let pin_end = self.previous_span().end;
                    pins.push(PinDecl {
                        direction: dir,
                        name: pin_name,
                        span: Span::new(pin_start, pin_end),
                    });

                    if self.check(&Token::Comma) {
                        self.advance();
                    } else {
                        break;
                    }
                }

                self.expect_token(&Token::CloseBracket, "Expected ']' to close pin list")?;
                if self.check(&Token::Semicolon) {
                    self.advance();
                }
                continue;
            }

            if self.check(&Token::Logic) {
                let logic_start = self.current_span().start;
                let logic_blk = self.parse_logic_block(logic_start)?;
                logic_blocks.push(logic_blk);
                if self.check(&Token::Semicolon) {
                    self.advance();
                }
                continue;
            }

            // Route statement or other statement inside module
            let stmt = self.parse_statement()?;
            routes.push(stmt);
            if self.check(&Token::Semicolon) {
                self.advance();
            }
        }

        let close_span = self.expect_token(&Token::CloseBrace, "Expected '}' to close module body")?;

        Ok(ModuleDecl {
            attributes,
            name,
            pins,
            logic_blocks,
            routes,
            span: Span::new(start_pos, close_span.end),
        })
    }

    /// Parse a material declaration: `material Name { prop: val, ... }`
    pub fn parse_material_decl(&mut self, is_exported: bool, start_pos: usize) -> Result<MaterialDecl, ParseError> {
        self.expect_token(&Token::Material, "Expected 'material'")?;
        let name = self.expect_identifier()?;

        self.expect_token(&Token::OpenBrace, "Expected '{' for material body")?;
        let mut properties = Vec::new();

        while !self.check(&Token::CloseBrace) && !self.is_at_end() {
            let prop_ident = self.expect_identifier()?;
            let prop_name: CompactString = prop_ident.name.as_str().into();
            self.expect_token(&Token::Colon, "Expected ':' after material property name")?;
            let prop_val = self.parse_expression()?;
            properties.push((prop_name, prop_val));

            if self.check(&Token::Semicolon) {
                self.advance();
            }
        }

        let close_span = self.expect_token(&Token::CloseBrace, "Expected '}' to close material body")?;

        Ok(MaterialDecl {
            is_exported,
            name,
            properties,
            span: Span::new(start_pos, close_span.end),
        })
    }

    /// Parse a profile declaration: `profile Name { section Name { prop: val } }`
    pub fn parse_profile_decl(&mut self, is_exported: bool, start_pos: usize) -> Result<ProfileDecl, ParseError> {
        self.expect_token(&Token::Profile, "Expected 'profile'")?;
        let name = self.expect_identifier()?;

        self.expect_token(&Token::OpenBrace, "Expected '{' for profile body")?;
        let mut sections = Vec::new();

        while !self.check(&Token::CloseBrace) && !self.is_at_end() {
            let sec_start = self.current_span().start;
            let sec_type_ident = self.expect_identifier()?;
            let sec_type: CompactString = sec_type_ident.name.as_str().into();

            // Section name can be an identifier or string literal
            let sec_name = if !self.check(&Token::OpenBrace) {
                if let Some(Token::String(s)) = self.current().map(|t| &t.token) {
                    let name = s.clone();
                    self.advance();
                    Some(CompactString::from(name))
                } else {
                    let id = self.expect_identifier()?;
                    Some(CompactString::from(id.name.as_str()))
                }
            } else {
                None
            };

            self.expect_token(&Token::OpenBrace, "Expected '{' for profile section body")?;
            let mut fields = Vec::new();

            while !self.check(&Token::CloseBrace) && !self.is_at_end() {
                // Check if this is a nested subsection (identifier followed by {)
                // or a field (identifier followed by :)
                let is_subsection = if let Some(Token::Identifier(_)) = self.current().map(|t| &t.token) {
                    // Look ahead to see if next token after identifier is '{' or ':'
                    if self.current + 1 < self.tokens.len() {
                        matches!(self.tokens[self.current + 1].token, Token::OpenBrace)
                    } else {
                        false
                    }
                } else {
                    false
                };

                if is_subsection {
                    // Parse nested subsection: layer_name { field: value, ... }
                    let subsec_start = self.current_span().start;
                    let subsec_ident = self.expect_identifier()?;
                    let subsec_name: CompactString = subsec_ident.name.as_str().into();
                    
                    self.expect_token(&Token::OpenBrace, "Expected '{' for nested subsection")?;
                    let mut subsec_fields = Vec::new();
                    
                    while !self.check(&Token::CloseBrace) && !self.is_at_end() {
                        let fld_ident = self.expect_identifier()?;
                        let fld_name: CompactString = fld_ident.name.as_str().into();
                        self.expect_token(&Token::Colon, "Expected ':' after field name")?;
                        let fld_val = self.parse_expression()?;
                        subsec_fields.push((fld_name, fld_val));

                        if self.check(&Token::Semicolon) {
                            self.advance();
                        }
                        if self.check(&Token::Comma) {
                            self.advance();
                        }
                    }
                    
                    let subsec_close = self.expect_token(&Token::CloseBrace, "Expected '}' to close nested subsection")?;
                    
                    // Add the nested subsection as a special field entry
                    // Store it as a struct-like expression
                    let subsec_expr = Expression::StructInstance {
                        name: subsec_name.clone(),
                        fields: subsec_fields.into_iter().map(|(k, v)| {
                            let span = v.span();
                            FieldInit {
                                name: k,
                                value: Some(v),
                                span,
                            }
                        }).collect(),
                        span: Span::new(subsec_start, subsec_close.end),
                    };
                    fields.push((subsec_name, subsec_expr));
                } else {
                    // Parse regular field: field_name: value
                    let fld_ident = self.expect_identifier()?;
                    let fld_name: CompactString = fld_ident.name.as_str().into();
                    self.expect_token(&Token::Colon, "Expected ':' after profile field name")?;
                    let fld_val = self.parse_expression()?;
                    fields.push((fld_name, fld_val));

                    if self.check(&Token::Semicolon) {
                        self.advance();
                    }
                }
            }

            let sec_close = self.expect_token(&Token::CloseBrace, "Expected '}' to close profile section")?;
            sections.push(ProfileSection {
                section_type: sec_type,
                name: sec_name,
                fields,
                span: Span::new(sec_start, sec_close.end),
            });
        }

        let close_span = self.expect_token(&Token::CloseBrace, "Expected '}' to close profile body")?;

        Ok(ProfileDecl {
            is_exported,
            name,
            sections,
            span: Span::new(start_pos, close_span.end),
        })
    }

    /// Parse a device declaration: `device Name { type: DeviceType, ... }`
    pub fn parse_device_decl(&mut self, is_exported: bool, start_pos: usize) -> Result<DeviceDecl, ParseError> {
        self.expect_token(&Token::Device, "Expected 'device'")?;
        let name = self.expect_identifier()?;

        self.expect_token(&Token::OpenBrace, "Expected '{' for device body")?;
        let mut sections = Vec::new();

        while !self.check(&Token::CloseBrace) && !self.is_at_end() {
            let sec_start = self.current_span().start;
            let sec_ident = self.expect_identifier()?;
            let sec_name: CompactString = sec_ident.name.as_str().into();

            if self.check(&Token::Colon) {
                self.advance();
                let expr = self.parse_expression()?;
                let end = expr.span().end;
                if self.check(&Token::Semicolon) {
                    self.advance();
                }
                sections.push(DeviceSection {
                    name: sec_name,
                    fields: vec![("value".into(), expr)],
                    span: Span::new(sec_start, end),
                });
            } else if self.check(&Token::OpenBrace) {
                self.advance();
                let mut fields = Vec::new();
                while !self.check(&Token::CloseBrace) && !self.is_at_end() {
                    let fld_ident = self.expect_identifier()?;
                    let fld_name: CompactString = fld_ident.name.as_str().into();
                    self.expect_token(&Token::Colon, "Expected ':' after device field")?;
                    let fld_val = self.parse_expression()?;
                    fields.push((fld_name, fld_val));
                    if self.check(&Token::Semicolon) || self.check(&Token::Comma) {
                        self.advance();
                    }
                }
                let close_sec = self.expect_token(&Token::CloseBrace, "Expected '}' to close device section")?;
                sections.push(DeviceSection {
                    name: sec_name,
                    fields,
                    span: Span::new(sec_start, close_sec.end),
                });
            }
        }

        let close_span = self.expect_token(&Token::CloseBrace, "Expected '}' to close device body")?;

        Ok(DeviceDecl {
            is_exported,
            name,
            sections,
            span: Span::new(start_pos, close_span.end),
        })
    }

    /// Parse a test declaration: `test Name for TargetSpace { dc: { ... }, tran: { ... } }`
    pub fn parse_test_decl(&mut self, start_pos: usize) -> Result<TestDecl, ParseError> {
        self.expect_token(&Token::Test, "Expected 'test'")?;
        let name = self.expect_identifier()?;
        self.expect_token(&Token::For, "Expected 'for' after test name")?;
        let target = self.expect_identifier()?;

        self.expect_token(&Token::OpenBrace, "Expected '{' for test body")?;
        let mut configs = Vec::new();

        while !self.check(&Token::CloseBrace) && !self.is_at_end() {
            let cfg_start = self.current_span().start;
            let cfg_ident = self.expect_identifier()?;
            let cfg_name: CompactString = cfg_ident.name.as_str().into();

            self.expect_token(&Token::Colon, "Expected ':' after test config name")?;
            self.expect_token(&Token::OpenBrace, "Expected '{' for test config parameters")?;

            let mut params = Vec::new();
            while !self.check(&Token::CloseBrace) && !self.is_at_end() {
                let p_ident = self.expect_identifier()?;
                let p_name: CompactString = p_ident.name.as_str().into();
                self.expect_token(&Token::Colon, "Expected ':' after parameter name")?;
                let p_val = self.parse_expression()?;
                params.push((p_name, p_val));

                if self.check(&Token::Comma) {
                    self.advance();
                } else {
                    break;
                }
            }

            let close_cfg = self.expect_token(&Token::CloseBrace, "Expected '}' to close test config")?;
            if self.check(&Token::Semicolon) {
                self.advance();
            }

            configs.push(TestConfig {
                name: cfg_name,
                params,
                span: Span::new(cfg_start, close_cfg.end),
            });
        }

        let close_span = self.expect_token(&Token::CloseBrace, "Expected '}' to close test body")?;

        Ok(TestDecl {
            name,
            target,
            configs,
            span: Span::new(start_pos, close_span.end),
        })
    }
}
