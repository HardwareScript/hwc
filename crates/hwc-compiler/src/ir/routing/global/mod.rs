mod builder;
mod config;
mod engine;
mod memoization;
mod post_process;
mod registry;

use crate::ir::errors::IrError;
pub use config::{AutoRouter, RouterConfig};

impl<'a> AutoRouter<'a> {
    /// Create a new global automatic router.
    pub fn new(
        space: &'a mut hwc_engine::HardwareSpace,
        _symbol_table: &'a crate::SymbolTable,
        stackup_manager: &'a crate::ir::stackup_manager::StackupManager,
        profile: Option<&'a hwc_parser::ProfileDefinition>,
        net_frequencies: rustc_hash::FxHashMap<hwc_engine::netlist::NetId, f64>,
        auto_routes: Vec<hwc_parser::Route>,
        route_net_policies: rustc_hash::FxHashMap<
            hwc_engine::netlist::NetId,
            hwc_engine::RoutingPattern,
        >,
    ) -> Self {
        let config = RouterConfig::new(
            net_frequencies,
            auto_routes,
            route_net_policies,
            rustc_hash::FxHashMap::default(),
        );
        Self {
            space,
            stackup_manager,
            profile,
            config,
            query_store: None,
        }
    }

    /// Route all nets in the design using the GeometryRouter adaptive pipeline.
    pub fn route_all_nets(&mut self) -> Result<(), IrError> {
        // Phase 1-3: Build routing data and obstacles
        let data = self.build_routing_data()?;

        if data.resolved_routes.is_empty() {
            return Err(IrError::RoutingError(
                "No explicit route statements found. All nets must have explicit 'route X to Y' statements.".into(),
            ));
        }

        // Phase 4-7: Setup and run GeometryRouter
        let result = self.setup_and_run_engine(&data)?;

        // Phase 8-11: Post-process routes (Meanders, Miter, Z-res, Legalization)
        self.post_process_routes(result, &data)?;

        Ok(())
    }
}
