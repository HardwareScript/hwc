//! Route registration entry points.
//!
//! All routes enter the database through this module: child-instance routes
//! during hierarchical flattening, and parent-level interconnects created by
//! manual routing statements or by the AutoRouter.

use super::database::HierarchicalRoutingDatabase;
use super::provenance::RouteSource;
use crate::geometry::TraceSegment;
use crate::netlist::NetId;
use crate::space::AnalyticTrace;
use compact_str::CompactString;

impl HierarchicalRoutingDatabase {
    /// Register routes from a child instance (called during hierarchical flattening)
    ///
    /// # Parameters
    ///
    /// - `instance_name`: Name of the child instance (e.g., "PMOS_Inst")
    /// - `net_id`: Network ID in the parent space (after remapping)
    /// - `original_net_name`: Original network name in the child space
    /// - `segments`: Route segments already transformed to parent coordinates
    pub fn register_child_routes(
        &mut self,
        instance_name: CompactString,
        net_id: NetId,
        original_net_name: CompactString,
        segments: Vec<TraceSegment>,
    ) {
        let source = RouteSource::ChildInstance {
            instance: instance_name.clone(),
            original_net: original_net_name,
        };

        // Store provenance for each segment
        for _ in &segments {
            let route_id = self.allocate_route_id();
            self.route_provenance.insert(route_id, source.clone());
        }

        // Store the segments
        let key = (instance_name, net_id);
        self.child_instance_routes
            .entry(key.clone())
            .or_default()
            .extend(segments);

        eprintln!(
            "[ROUTING DB] Registered child routes: instance='{}', net_id={:?}, source={}",
            key.0, key.1, source
        );
    }

    /// Register a parent-level interconnect route
    ///
    /// # Parameters
    ///
    /// - `trace`: The analytic trace created by parent-level routing
    /// - `from_entity`: Source entity name
    /// - `to_entity`: Destination entity name
    pub fn register_parent_route(
        &mut self,
        trace: AnalyticTrace,
        from_entity: CompactString,
        to_entity: CompactString,
    ) {
        let source = RouteSource::ParentLevel {
            from_entity: from_entity.clone(),
            to_entity: to_entity.clone(),
        };

        // Store provenance for each segment
        for _ in &trace.segments {
            let route_id = self.allocate_route_id();
            self.route_provenance.insert(route_id, source.clone());
        }

        eprintln!(
            "[ROUTING DB] Registered parent route: net='{}' (id={:?}), from='{}', to='{}', segments={}",
            trace.net_name,
            trace.net_id,
            from_entity,
            to_entity,
            trace.segments.len()
        );

        self.parent_interconnects.push(trace);
    }

    /// Register a parent-level route created by the AutoRouter.
    ///
    /// This is called during AutoRouter's route creation, not post-processing.
    /// Validates that this net doesn't already have a parent route.
    pub fn register_autorouter_route(
        &mut self,
        trace: AnalyticTrace,
        from_entity: CompactString,
        to_entity: CompactString,
    ) -> Result<(), String> {
        if self.has_parent_route_for_net(trace.net_id) {
            return Err(format!(
                "Duplicate parent route for net {:?}. Parent routes must be registered exactly once.",
                trace.net_id
            ));
        }

        let source = RouteSource::ParentLevel {
            from_entity,
            to_entity,
        };

        let route_id = self.allocate_route_id();
        self.route_provenance.insert(route_id, source);

        self.parent_interconnects.push(trace);
        Ok(())
    }
}
