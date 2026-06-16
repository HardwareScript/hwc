mod errors;
mod helpers;
mod inference;

pub use errors::{WidthError, WidthValidationResult, WidthWarning};
pub use inference::WidthInference;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbol_table::SymbolTable;

    use hwc_parser::logic::*;
    use hwc_parser::Span;

    #[test]
    fn test_literal_width() {
        let symbol_table = SymbolTable::new();
        let inference = WidthInference::new(&symbol_table);

        let expr = LogicExpression::Literal {
            value: 0xFF,
            span: Span::new(0, 0),
        };

        assert_eq!(inference.infer_expression_width(&expr).unwrap(), 8);
    }

    #[test]
    fn test_variable_width() {
        let symbol_table = SymbolTable::new();
        let mut inference = WidthInference::new(&symbol_table);
        inference.register_width("x".into(), 16);

        let expr = LogicExpression::Variable {
            name: "x".into(),
            span: Span::new(0, 0),
        };

        assert_eq!(inference.infer_expression_width(&expr).unwrap(), 16);
    }

    #[test]
    fn test_add_width() {
        let symbol_table = SymbolTable::new();
        let mut inference = WidthInference::new(&symbol_table);
        inference.register_width("a".into(), 8);
        inference.register_width("b".into(), 8);

        let expr = LogicExpression::Binary {
            left: Box::new(LogicExpression::Variable {
                name: "a".into(),
                span: Span::new(0, 0),
            }),
            operator: LogicOperator::Add,
            right: Box::new(LogicExpression::Variable {
                name: "b".into(),
                span: Span::new(0, 0),
            }),
            span: Span::new(0, 0),
        };

        assert_eq!(inference.infer_expression_width(&expr).unwrap(), 9);
    }

    #[test]
    fn test_slice_width() {
        let symbol_table = SymbolTable::new();
        let inference = WidthInference::new(&symbol_table);

        let expr = LogicExpression::ArrayAccess {
            base: Box::new(LogicExpression::Variable {
                name: "bus".into(),
                span: Span::new(0, 0),
            }),
            range: Range::Slice { high: 7, low: 0 },
            span: Span::new(0, 0),
        };

        assert_eq!(inference.infer_expression_width(&expr).unwrap(), 8);
    }

    #[test]
    fn test_width_mismatch() {
        let symbol_table = SymbolTable::new();
        let mut inference = WidthInference::new(&symbol_table);
        inference.register_width("x".into(), 8);

        let expr = LogicExpression::Variable {
            name: "x".into(),
            span: Span::new(0, 0),
        };

        let result = inference.validate_assignment("out", 4, &expr, false);
        assert!(matches!(result, WidthValidationResult::Error(_)));
    }
}
