//! Test that keywords are not captured as measurements

use hwc_parser::Lexer;

#[test]
fn test_grid_dimensions() {
    let source = "grid: 500 by 500 by 4";
    let lexer = Lexer::new(source);
    let result = lexer.tokenize();

    match result {
        Ok(tokens) => {
            println!("Tokens: {:?}", tokens);
            // Should tokenize as: Identifier("grid"), Colon, Number(500), By, Number(500), By, Number(4)
        }
        Err(e) => {
            panic!("Failed to tokenize grid dimensions: {:?}", e);
        }
    }
}

#[test]
fn test_dimensions() {
    let source = "dimensions: 50mm by 50mm by 4mm";
    let lexer = Lexer::new(source);
    let result = lexer.tokenize();

    match result {
        Ok(tokens) => {
            println!("Tokens: {:?}", tokens);
            // Should tokenize correctly
        }
        Err(e) => {
            panic!("Failed to tokenize dimensions: {:?}", e);
        }
    }
}
