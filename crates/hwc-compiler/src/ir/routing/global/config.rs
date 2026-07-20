use hwc_engine::netlist::NetId;
use hwc_engine::HardwareSpace;
use rustc_hash::FxHashMap;

/// v0.1.8: Configuration for the global router.
pub struct RouterConfig {
    /// v0.1.7: Net frequencies in Hz for SI-aware routing.
    pub net_frequencies: FxHashMap<NetId, f64>,
    /// v0.1.7: Individual route requests.
    pub auto_routes: Vec<hwc_parser::Route>,
    /// v0.1.8: Per-net routing pattern policies.
    pub route_net_policies: FxHashMap<NetId, hwc_engine::RoutingPattern>,
}

impl RouterConfig {
    pub fn new(
        net_frequencies: FxHashMap<NetId, f64>,
        auto_routes: Vec<hwc_parser::Route>,
        route_net_policies: FxHashMap<NetId, hwc_engine::RoutingPattern>,
    ) -> Self {
        Self {
            net_frequencies,
            auto_routes,
            route_net_policies,
        }
    }
}

/// Global automatic router for connecting all pins in the netlist.
pub struct AutoRouter<'a> {
    pub space: &'a mut HardwareSpace,
    /// Stackup manager for Z-axis resolution
    pub stackup_manager: &'a crate::ir::stackup_manager::StackupManager,
    /// Active profile definition (for ASIC detection and layer info)
    pub profile: Option<&'a hwc_parser::ProfileDefinition>,
    /// Configuration for the router.
    pub config: RouterConfig,
    /// v0.1.8: Salsa-style memoized query store for per-G-cell routing cache.
    pub query_store: Option<hwc_engine::geometry_router::query_engine::QueryStore>,
}
