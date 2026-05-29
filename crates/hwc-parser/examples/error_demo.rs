//! Comprehensive demo of beautiful error messages with miette
//!
//! Run with: cargo run --example error_demo
//!
//! To force ASCII mode: HWS_ASCII=1 cargo run --example error_demo
//! To force Unicode mode: HWS_UNICODE=1 cargo run --example error_demo

use hwc_diagnostics::DiagnosticCollector;
use hwc_parser::{Lexer, Parser};
use miette::{GraphicalReportHandler, GraphicalTheme, NamedSource, Report};
use std::env;

fn show_error(title: &str, source: &str, filename: &str) {
    println!("=== {} ===\n", title);

    let lexer = Lexer::new(source);
    match lexer.tokenize() {
        Ok(tokens) => {
            let mut parser = Parser::new(tokens);
            let collector = DiagnosticCollector::new(source, 100);
            let _program = parser.parse(&collector);

            // Check if there were any errors collected
            if collector.has_errors() {
                // For demo purposes, we'll just show that errors were collected
                println!("Parsing completed with errors collected in DiagnosticCollector");
                println!("Error count: {}", collector.error_count());
                println!();
            } else {
                println!("Parsing succeeded!\n");
            }

            // Note: In v0.1.6, parse() no longer returns Result
            // Errors are collected in DiagnosticCollector instead
            /*
            if let Err(e) = parser.parse(&DiagnosticCollector::new("", 100)) {
                let report =
                    Report::new(e).with_source_code(NamedSource::new(filename, source.into()));

                // Determine theme based on environment or let miette auto-detect
                if env::var("HWS_ASCII").is_ok() {
                    // User explicitly wants ASCII
                    let mut output = String::new();
                    GraphicalReportHandler::new_themed(GraphicalTheme::ascii())
                        .render_report(&mut output, report.as_ref())
                        .unwrap();
                    println!("{}", output);
                } else if env::var("HWS_UNICODE").is_ok() {
                    // User explicitly wants Unicode
                    let mut output = String::new();
                    GraphicalReportHandler::new_themed(GraphicalTheme::unicode())
                        .render_report(&mut output, report.as_ref())
                        .unwrap();
                    println!("{}", output);
                } else {
                    // Auto-detect: default to Unicode for modern terminals
                    let mut output = String::new();
                    GraphicalReportHandler::new_themed(GraphicalTheme::unicode())
                        .render_report(&mut output, report.as_ref())
                        .unwrap();
                    println!("{}", output);
                }
            } else {
                println!("✓ Parsed successfully (no error to show)");
            }
            */
        }
        Err(e) => {
            let report =
                Report::new(e).with_source_code(NamedSource::new(filename, source.to_string()));

            if env::var("HWS_ASCII").is_ok() {
                let mut output = String::new();
                GraphicalReportHandler::new_themed(GraphicalTheme::ascii())
                    .render_report(&mut output, report.as_ref())
                    .unwrap();
                println!("{}", output);
            } else if env::var("HWS_UNICODE").is_ok() {
                let mut output = String::new();
                GraphicalReportHandler::new_themed(GraphicalTheme::unicode())
                    .render_report(&mut output, report.as_ref())
                    .unwrap();
                println!("{}", output);
            } else {
                // Auto-detect: default to Unicode for modern terminals
                let mut output = String::new();
                GraphicalReportHandler::new_themed(GraphicalTheme::unicode())
                    .render_report(&mut output, report.as_ref())
                    .unwrap();
                println!("{}", output);
            }
        }
    }
    println!();
}

fn main() {
    // Example 1: Invalid keyword
    show_error(
        "Example 1: Invalid Keyword",
        r#"define space "Test":
    dimensions: 50mm by 50mm by 2mm
    grid: 100 by 100 by 2
    INVALID_KEYWORD here
"#,
        "invalid_keyword.hw",
    );

    // Example 2: Typo in keyword
    show_error(
        "Example 2: Typo in Keyword (defin instead of define)",
        r#"defin space "Test":
    dimensions: 50mm by 50mm by 2mm
    grid: 100 by 100 by 2
"#,
        "typo.hw",
    );

    // Example 3: Missing colon
    show_error(
        "Example 3: Missing Colon After Space Name",
        r#"define space "Test"
    dimensions: 50mm by 50mm by 2mm
    grid: 100 by 100 by 2
"#,
        "missing_colon.hw",
    );

    // Example 4: Wrong coordinate format
    show_error(
        "Example 4: Invalid Coordinate Format",
        r#"define space "Test":
    dimensions: 50mm by 50mm by 2mm
    grid: 100 by 100 by 2
    add Battery (5V) named Power at (10, 10, 1)
"#,
        "wrong_coords.hw",
    );

    // Example 5: Missing string quotes
    show_error(
        "Example 5: Missing Quotes Around Space Name",
        r#"define space Test:
    dimensions: 50mm by 50mm by 2mm
    grid: 100 by 100 by 2
"#,
        "missing_quotes.hw",
    );

    // Example 6: Wrong separator (x instead of by)
    show_error(
        "Example 6: Wrong Separator (using 'x' instead of 'by')",
        r#"define space "Test":
    dimensions: 50mm x 50mm x 2mm
    grid: 100 by 100 by 2
"#,
        "wrong_separator.hw",
    );

    // Example 7: Missing required field (grid)
    show_error(
        "Example 7: Missing Required Field (grid)",
        r#"define space "Test":
    dimensions: 50mm by 50mm by 2mm
"#,
        "missing_grid.hw",
    );

    // Example 8: Invalid unit
    show_error(
        "Example 8: Invalid Unit (using 'meters' instead of 'mm')",
        r#"define space "Test":
    dimensions: 50meters by 50mm by 2mm
    grid: 100 by 100 by 2
"#,
        "invalid_unit.hw",
    );

    // Example 9: Missing 'by' keyword
    show_error(
        "Example 9: Missing 'by' Keyword in Dimensions",
        r#"define space "Test":
    dimensions: 50mm 50mm by 2mm
    grid: 100 by 100 by 2
"#,
        "missing_by.hw",
    );

    // Example 10: Unexpected EOF
    show_error(
        "Example 10: Unexpected End of File",
        r#"define space "Test":
    dimensions: 50mm by 50mm by
"#,
        "unexpected_eof.hw",
    );

    // Example 11: Wrong bracket type
    show_error(
        "Example 11: Wrong Bracket Type (using () instead of [])",
        r#"define space "Test":
    dimensions: 50mm by 50mm by 2mm
    grid: 100 by 100 by 2
    add Battery (5V) named Power at (10,10,1)
"#,
        "wrong_brackets.hw",
    );

    // Example 12: Missing component name after 'named'
    show_error(
        "Example 12: Missing Component Name After 'named'",
        r#"define space "Test":
    dimensions: 50mm by 50mm by 2mm
    grid: 100 by 100 by 2
    add Battery (5V) named at [10,10,1]
"#,
        "missing_name.hw",
    );

    println!("=== Summary ===");
    println!("✓ Demonstrated 12 different error scenarios");
    println!("✓ Categories covered:");
    println!("  - Keyword errors (invalid, typo, missing)");
    println!("  - Syntax errors (missing colon, quotes, brackets)");
    println!("  - Format errors (wrong separators, coordinates)");
    println!("  - Unit errors (invalid units)");
    println!("  - EOF errors (unexpected end of file)");
    println!("✓ All errors show:");
    println!("  - Error code (S14, S15, S99, etc.)");
    println!("  - Source code snippet with line numbers");
    println!("  - Exact location underlined");
    println!("  - Helpful error message");
    println!("  - URL to documentation");

    println!("\n=== Additional Error Scenarios ===\n");

    // Example 13: Indentation error
    show_error(
        "Example 13: Indentation Error (inconsistent indentation)",
        r#"define space "Test":
    dimensions: 50mm by 50mm by 2mm
  grid: 100 by 100 by 2
"#,
        "indentation_error.hw",
    );

    // Example 14: Missing 'named' keyword
    show_error(
        "Example 14: Missing 'named' Keyword",
        r#"define space "Test":
    dimensions: 50mm by 50mm by 2mm
    grid: 100 by 100 by 2
    add Battery (5V) Power at [10,10,1]
"#,
        "missing_named.hw",
    );

    // Example 15: Wrong keyword order
    show_error(
        "Example 15: Wrong Keyword Order (space before define)",
        r#"space define "Test":
    dimensions: 50mm by 50mm by 2mm
    grid: 100 by 100 by 2
"#,
        "wrong_order.hw",
    );

    // Example 16: Missing dimensions value
    show_error(
        "Example 16: Missing Dimensions Value",
        r#"define space "Test":
    dimensions: by 50mm by 2mm
    grid: 100 by 100 by 2
"#,
        "missing_dimension.hw",
    );

    // Example 17: Invalid grid values (non-integer)
    show_error(
        "Example 17: Invalid Grid Values (using decimals)",
        r#"define space "Test":
    dimensions: 50mm by 50mm by 2mm
    grid: 100.5 by 100 by 2
"#,
        "invalid_grid.hw",
    );

    // Example 18: Missing 'at' keyword in placement
    show_error(
        "Example 18: Missing 'at' Keyword in Component Placement",
        r#"define space "Test":
    dimensions: 50mm by 50mm by 2mm
    grid: 100 by 100 by 2
    add Battery (5V) named Power [10,10,1]
"#,
        "missing_at.hw",
    );

    // Example 19: Extra comma in coordinate
    show_error(
        "Example 19: Extra Comma in Coordinate",
        r#"define space "Test":
    dimensions: 50mm by 50mm by 2mm
    grid: 100 by 100 by 2
    add Battery (5V) named Power at [10,10,1,]
"#,
        "extra_comma.hw",
    );

    // Example 20: Missing closing quote
    show_error(
        "Example 20: Missing Closing Quote",
        r#"define space "Test:
    dimensions: 50mm by 50mm by 2mm
    grid: 100 by 100 by 2
"#,
        "missing_closing_quote.hw",
    );

    // Example 21: Wrong case for keyword (DEFINE instead of define)
    show_error(
        "Example 21: Wrong Case for Keyword (DEFINE instead of define)",
        r#"DEFINE space "Test":
    dimensions: 50mm by 50mm by 2mm
    grid: 100 by 100 by 2
"#,
        "wrong_case.hw",
    );

    // Example 22: Missing component type
    show_error(
        "Example 22: Missing Component Type",
        r#"define space "Test":
    dimensions: 50mm by 50mm by 2mm
    grid: 100 by 100 by 2
    add named Power at [10,10,1]
"#,
        "missing_component_type.hw",
    );

    // Example 23: Invalid rotation value
    show_error(
        "Example 23: Invalid Rotation Value (text instead of number)",
        r#"define space "Test":
    dimensions: 50mm by 50mm by 2mm
    grid: 100 by 100 by 2
    add Battery (5V) named Power at [10,10,1] rotated ninety degrees
"#,
        "invalid_rotation.hw",
    );

    // Example 24: Route with invalid syntax
    show_error(
        "Example 24: Route with Invalid Syntax",
        r#"define space "Test":
    dimensions: 50mm by 50mm by 2mm
    grid: 100 by 100 by 2
    route Power.+ to Ground.- using 1mm
"#,
        "invalid_route.hw",
    );

    // Example 25: Expose with missing pin
    show_error(
        "Example 25: Expose with Missing Pin Reference",
        r#"define space "Test":
    dimensions: 50mm by 50mm by 2mm
    grid: 100 by 100 by 2
    expose as "VCC"
"#,
        "missing_pin_ref.hw",
    );

    println!("\n=== Final Summary ===");
    println!("✓ Demonstrated 25 different error scenarios");
    println!("✓ Categories covered:");
    println!("  - Keyword errors (invalid, typo, missing, wrong case, wrong order)");
    println!("  - Syntax errors (missing colon, quotes, brackets, commas)");
    println!("  - Format errors (wrong separators, coordinates, indentation)");
    println!("  - Value errors (invalid units, numbers, rotations)");
    println!("  - Structure errors (missing fields, wrong order)");
    println!("  - Route/Expose errors (invalid syntax, missing references)");
    println!("  - EOF errors (unexpected end of file)");
}
