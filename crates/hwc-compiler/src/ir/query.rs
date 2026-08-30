//! Demand-Driven Salsa Query Pipeline for HardwareScript
//!
//! Provides incremental memoized query execution across the physical compilation pipeline:
//! parse_ast_query -> eval_space_query -> ingest_entity_graph_query -> route_space_query.

use compact_str::CompactString;
use hwc_diagnostics::DiagnosticCollector;
use hwc_engine::EntityGraph;
use hwc_parser::ast::Program;
use hwc_parser::{Lexer, Parser};
use std::sync::Arc;

/// Pure query parsing source text into an AST `Program`.
pub fn parse_ast_query(source: &str, file_name: &str) -> Result<Arc<Program>, String> {
    let lexer = Lexer::new(source);
    let tokens = lexer
        .tokenize()
        .map_err(|e| format!("Lexical analysis failed: {:?}", e))?;

    let collector = DiagnosticCollector::new_with_file(source, file_name, 50);
    let mut parser = Parser::new(tokens);
    let program = parser.parse(&collector);

    if collector.has_errors() {
        return Err(format!("Parse failed with {} errors", collector.error_count()));
    }

    Ok(Arc::new(program))
}

/// Ingests flattened geometry records directly into an `EntityGraph`.
pub fn ingest_geometry_to_entity_graph(
    pins: &[(i64, i64, i64, CompactString, CompactString, Option<CompactString>)],
) -> Arc<EntityGraph> {
    let mut graph = EntityGraph::new();

    for (x_pm, y_pm, z_pm, comp, pin, net) in pins {
        // x_nm, y_nm, z_nm in graph are nanometers (1 nm = 1000 pm)
        let x_nm = x_pm / 1000;
        let y_nm = y_pm / 1000;
        let z_nm = z_pm / 1000;

        graph.add_component_pin(
            x_nm,
            y_nm,
            z_nm,
            comp.as_str().into(),
            pin.as_str().into(),
            net.as_ref().map(|n| n.as_str().into()),
        );
    }

    Arc::new(graph)
}

/// Context for demand-driven query execution.
#[derive(Debug, Default, Clone)]
pub struct QueryPipelineContext {
    pub cached_runs: usize,
}

impl QueryPipelineContext {
    pub fn new() -> Self {
        Self::default()
    }
}
