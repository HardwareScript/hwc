use compact_str::CompactString;
use hwc_compiler::ir::eco_query::{
    base_silicon_snapshot_query, verify_freeze_silicon_immutability_query, BaseSiliconSnapshot,
};
use hwc_compiler::ir::query::{ingest_geometry_to_entity_graph, parse_ast_query};

#[test]
fn test_parse_ast_query_success() {
    let source = r#"
space CoreSoC {
    dimensions: [100.0um, 100.0um]
}
"#;
    let result = parse_ast_query(source, "CoreSoC.hw");
    assert!(result.is_ok());
    let program = result.unwrap();
    assert_eq!(program.items.len(), 1);
}

#[test]
fn test_ingest_geometry_to_entity_graph() {
    let pins = vec![
        (
            1_000_000,
            2_000_000,
            0,
            CompactString::new("inv_0"),
            CompactString::new("A"),
            Some(CompactString::new("net_in")),
        ),
        (
            5_000_000,
            2_000_000,
            0,
            CompactString::new("inv_0"),
            CompactString::new("Y"),
            Some(CompactString::new("net_out")),
        ),
    ];

    let graph = ingest_geometry_to_entity_graph(&pins);
    assert_eq!(graph.get_component_pins().len(), 2);
}

#[test]
fn test_freeze_silicon_eco_queries() {
    let snapshot = BaseSiliconSnapshot::default();

    // Legal metal modifications (M2, M3)
    let metal_layers = vec![
        CompactString::new("metal2"),
        CompactString::new("metal3"),
        CompactString::new("via2"),
    ];
    assert!(verify_freeze_silicon_immutability_query(&snapshot, &metal_layers).is_ok());

    // Illegal base layer mutation (poly, diff)
    let illegal_layers = vec![CompactString::new("poly")];
    assert!(verify_freeze_silicon_immutability_query(&snapshot, &illegal_layers).is_err());

    // Custom snapshot creation
    let custom = base_silicon_snapshot_query(&[CompactString::new("custom_well")]);
    assert!(custom.locked_layers.contains(&CompactString::new("custom_well")));
}
