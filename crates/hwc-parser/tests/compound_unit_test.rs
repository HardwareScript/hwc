//! Test compound unit handling

use hwc_parser::Lexer;

#[test]
fn test_compound_unit_cm2_per_vs() {
    let source = "electron_mobility: 1400cm2/Vs";
    let lexer = Lexer::new(source);
    let result = lexer.tokenize();

    match result {
        Ok(tokens) => {
            println!("Tokens: {:?}", tokens);
            // Should successfully tokenize as single measurement
        }
        Err(e) => {
            panic!("Failed to tokenize compound unit: {:?}", e);
        }
    }
}
