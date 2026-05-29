//! Test UTF-8 handling in lexer

use hwc_parser::Lexer;

#[test]
fn test_utf8_omega() {
    let source = "resistivity: 1.68e-8Ω·m";
    let lexer = Lexer::new(source);
    let result = lexer.tokenize();

    match result {
        Ok(tokens) => {
            println!("Tokens: {:?}", tokens);
            // Should successfully tokenize
        }
        Err(e) => {
            panic!("Failed to tokenize UTF-8: {:?}", e);
        }
    }
}

#[test]
fn test_utf8_superscript() {
    let source = "density: 8960kg/m³";
    let lexer = Lexer::new(source);
    let result = lexer.tokenize();

    match result {
        Ok(tokens) => {
            println!("Tokens: {:?}", tokens);
            // Should successfully tokenize
        }
        Err(e) => {
            panic!("Failed to tokenize UTF-8 superscript: {:?}", e);
        }
    }
}

#[test]
fn test_utf8_middle_dot() {
    let source = "thermal: 401W/(m·K)";
    let lexer = Lexer::new(source);
    let result = lexer.tokenize();

    match result {
        Ok(tokens) => {
            println!("Tokens: {:?}", tokens);
            // Should successfully tokenize
        }
        Err(e) => {
            panic!("Failed to tokenize UTF-8 middle dot: {:?}", e);
        }
    }
}
