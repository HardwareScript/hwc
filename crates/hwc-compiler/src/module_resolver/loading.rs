//! File parsing for import resolution (no caching - always fresh).

use crate::embedded_stdlib;
use crate::module_resolver::ResolverError;
use hwc_parser::{Definition, Lexer, Parser, Program};
use std::path::Path;

impl super::ModuleResolver {
    /// Parse a program from a path (always fresh - no caching)
    pub(super) fn parse_program<'ast>(
        &mut self,
        path: &Path,
        arena: &'ast bumpalo::Bump,
    ) -> Result<Program<'ast>, ResolverError> {
        if path.starts_with("@std/") {
            // Embedded stdlib
            let module_name = path.strip_prefix("@std/").unwrap().to_str().unwrap();
            let defs = self.load_stdlib_embedded(module_name)?;

            // Create a stub Program (stdlib has no imports/re-exports)
            let program = Program {
                imports: vec![],
                re_exports: vec![],
                definitions: defs,
                span: hwc_parser::Span::new(0, 0),
            };

            Ok(program)
        } else {
            // File system - parse fresh each time
            self.parse_file(path, arena)
        }
    }

    /// Parse a file from disk
    pub(super) fn parse_file<'ast>(
        &self,
        path: &Path,
        arena: &'ast bumpalo::Bump,
    ) -> Result<Program<'ast>, ResolverError> {
        let source = std::fs::read_to_string(path).map_err(|e| ResolverError::FileReadError {
            path: path.display().to_string().into(),
            error: e.to_string(),
            span: None,
        })?;

        let lexer = Lexer::new(&source);
        let tokens = lexer.tokenize().map_err(|e| ResolverError::ParseError {
            path: path.display().to_string().into(),
            error: format!("{:?}", e),
            span: None,
        })?;

        let collector =
            crate::DiagnosticCollector::new_with_file(&source, &path.to_string_lossy(), 20);
        let mut parser = Parser::new(tokens, arena);
        let program = parser.parse(&collector);

        if collector.has_errors() {
            return Err(ResolverError::ParseError {
                path: path.display().to_string().into(),
                error: collector.format_errors(),
                span: None,
            });
        }

        Ok(program)
    }

    /// Load stdlib module from embedded source (uses its own internal cache)
    pub(super) fn load_stdlib_embedded<'ast>(
        &mut self,
        name: &str,
    ) -> Result<Vec<Definition<'ast>>, ResolverError> {
        embedded_stdlib::get_stdlib_definitions(name).ok_or_else(|| ResolverError::StdlibNotFound {
            path: name.into(),
            span: None,
        })
    }
}
