//! Tests for Task C3: Import Path Parsing (Bare Identifiers)
//!
//! v0.1.6 import path syntax:
//! - Bare identifiers: `import Adders from logic/adders`
//! - Package paths: `import Parts from @std/components`
//! - Quoted paths: `import Board from "Custom Path/Board.hw"`
//! - Legacy dot syntax removed: `import FR4 from standard.materials` (no longer supported)

use hwc_diagnostics::DiagnosticCollector;
use hwc_parser::{Lexer, ModulePath, Parser};

#[test]
fn test_bare_identifier_path_single() {
    let source = r#"
import Adders from logic
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Tokenization failed");
    let mut parser = Parser::new(tokens);
    let program = parser.parse(&DiagnosticCollector::new("", 100));

    assert_eq!(program.imports.len(), 1);
    let import = &program.imports[0];

    // Check targets
    match &import.targets {
        hwc_parser::ImportTargets::List(names) => {
            assert_eq!(names.len(), 1);
            assert_eq!(names[0].name.as_str(), "Adders");
        }
        _ => panic!("Expected List targets"),
    }

    match &import.path {
        ModulePath::Relative(path) => assert_eq!(path, "logic"),
        _ => panic!("Expected Relative path, got {:?}", import.path),
    }
}

#[test]
fn test_bare_identifier_path_with_slashes() {
    let source = r#"
import Adders from logic/adders
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Tokenization failed");
    let mut parser = Parser::new(tokens);
    let program = parser.parse(&DiagnosticCollector::new("", 100));

    assert_eq!(program.imports.len(), 1);
    let import = &program.imports[0];

    // Check targets
    match &import.targets {
        hwc_parser::ImportTargets::List(names) => {
            assert_eq!(names.len(), 1);
            assert_eq!(names[0].name.as_str(), "Adders");
        }
        _ => panic!("Expected List targets"),
    }

    match &import.path {
        ModulePath::Relative(path) => assert_eq!(path, "logic/adders"),
        _ => panic!("Expected Relative path, got {:?}", import.path),
    }
}

#[test]
fn test_bare_identifier_path_nested() {
    let source = r#"
import Gates from logic/gates/basic
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Tokenization failed");
    let mut parser = Parser::new(tokens);
    let program = parser.parse(&DiagnosticCollector::new("", 100));

    assert_eq!(program.imports.len(), 1);
    let import = &program.imports[0];

    // Check targets
    match &import.targets {
        hwc_parser::ImportTargets::List(names) => {
            assert_eq!(names.len(), 1);
            assert_eq!(names[0].name.as_str(), "Gates");
        }
        _ => panic!("Expected List targets"),
    }

    match &import.path {
        ModulePath::Relative(path) => assert_eq!(path, "logic/gates/basic"),
        _ => panic!("Expected Relative path, got {:?}", import.path),
    }
}

#[test]
fn test_package_path_std() {
    let source = r#"
import Parts from @std/components
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Tokenization failed");
    let mut parser = Parser::new(tokens);
    let program = parser.parse(&DiagnosticCollector::new("", 100));

    assert_eq!(program.imports.len(), 1);
    let import = &program.imports[0];

    // Check targets
    match &import.targets {
        hwc_parser::ImportTargets::List(names) => {
            assert_eq!(names.len(), 1);
            assert_eq!(names[0].name.as_str(), "Parts");
        }
        _ => panic!("Expected List targets"),
    }

    match &import.path {
        ModulePath::Package { org, name } => {
            assert_eq!(org, "std");
            assert_eq!(name, "components");
        }
        _ => panic!("Expected Package path, got {:?}", import.path),
    }
}

#[test]
fn test_package_path_external() {
    let source = r#"
import Motor from @robotics/motor
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Tokenization failed");
    let mut parser = Parser::new(tokens);
    let program = parser.parse(&DiagnosticCollector::new("", 100));

    assert_eq!(program.imports.len(), 1);
    let import = &program.imports[0];

    // Check targets
    match &import.targets {
        hwc_parser::ImportTargets::List(names) => {
            assert_eq!(names.len(), 1);
            assert_eq!(names[0].name.as_str(), "Motor");
        }
        _ => panic!("Expected List targets"),
    }

    match &import.path {
        ModulePath::Package { org, name } => {
            assert_eq!(org, "robotics");
            assert_eq!(name, "motor");
        }
        _ => panic!("Expected Package path, got {:?}", import.path),
    }
}

#[test]
fn test_quoted_path_with_spaces() {
    let source = r#"
import CustomBoard from "Custom Path/Board.hw"
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Tokenization failed");
    let mut parser = Parser::new(tokens);
    let program = parser.parse(&DiagnosticCollector::new("", 100));

    assert_eq!(program.imports.len(), 1);
    let import = &program.imports[0];

    // Check targets
    match &import.targets {
        hwc_parser::ImportTargets::List(names) => {
            assert_eq!(names.len(), 1);
            assert_eq!(names[0].name.as_str(), "CustomBoard");
        }
        _ => panic!("Expected List targets"),
    }

    match &import.path {
        ModulePath::Quoted(path) => assert_eq!(path, "Custom Path/Board.hw"),
        _ => panic!("Expected Quoted path, got {:?}", import.path),
    }
}

#[test]
fn test_quoted_path_without_spaces() {
    // This should work but is unnecessary (quotes not needed)
    let source = r#"
import Adders from "logic/adders"
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Tokenization failed");
    let mut parser = Parser::new(tokens);
    let program = parser.parse(&DiagnosticCollector::new("", 100));

    assert_eq!(program.imports.len(), 1);
    let import = &program.imports[0];

    // Check targets
    match &import.targets {
        hwc_parser::ImportTargets::List(names) => {
            assert_eq!(names.len(), 1);
            assert_eq!(names[0].name.as_str(), "Adders");
        }
        _ => panic!("Expected List targets"),
    }

    match &import.path {
        ModulePath::Quoted(path) => assert_eq!(path, "logic/adders"),
        _ => panic!("Expected Quoted path, got {:?}", import.path),
    }
}

#[test]
fn test_legacy_dot_syntax_rejected() {
    // Legacy dot syntax `standard.materials` fully removed pre-release (see ast/import.rs and parser/definitions/mod.rs).
    // Parser now only accepts / relative, @package, quoted.
    let source = r#"
import FR4 from standard.materials
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Tokenization failed");
    let mut parser = Parser::new(tokens);
    let program = parser.parse(&DiagnosticCollector::new("", 100));

    // Should not parse any imports due to unrecognized path syntax (dot handling deleted).
    assert!(program.imports.is_empty(), "Legacy dot syntax for imports must be rejected");
}

#[test]
fn test_multiple_imports_mixed_syntax() {
    let source = r#"
import Adders from logic/adders
import Parts from @std/components
import CustomBoard from "Custom Path/Board.hw"
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Tokenization failed");
    let mut parser = Parser::new(tokens);
    let program = parser.parse(&DiagnosticCollector::new("", 100));

    // Legacy 4th import removed from test; dot syntax no longer parses.
    assert_eq!(program.imports.len(), 3);

    // Check first import (bare identifier)
    match &program.imports[0].path {
        ModulePath::Relative(path) => assert_eq!(path, "logic/adders"),
        _ => panic!("Expected Relative path"),
    }

    // Check second import (package)
    match &program.imports[1].path {
        ModulePath::Package { org, name } => {
            assert_eq!(org, "std");
            assert_eq!(name, "components");
        }
        _ => panic!("Expected Package path"),
    }

    // Check third import (quoted)
    match &program.imports[2].path {
        ModulePath::Quoted(path) => assert_eq!(path, "Custom Path/Board.hw"),
        _ => panic!("Expected Quoted path"),
    }
}

#[test]
fn test_import_with_definition() {
    let source = r#"
import Adders from logic/adders

component Resistor:
    pins: [A, B]
"#;

    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Tokenization failed");
    let mut parser = Parser::new(tokens);
    let program = parser.parse(&DiagnosticCollector::new("", 100));

    assert_eq!(program.imports.len(), 1);
    assert_eq!(program.definitions.len(), 1);

    let import = &program.imports[0];

    // Check targets
    match &import.targets {
        hwc_parser::ImportTargets::List(names) => {
            assert_eq!(names.len(), 1);
            assert_eq!(names[0].name.as_str(), "Adders");
        }
        _ => panic!("Expected List targets"),
    }

    match &import.path {
        ModulePath::Relative(path) => assert_eq!(path, "logic/adders"),
        _ => panic!("Expected Relative path"),
    }
}
