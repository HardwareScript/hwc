//! Core storage definition for the hierarchical routing database.
//!
//! The behaviour of this struct is split across sibling modules
//! (`registration`, `connectivity`, `export`, `analytic`, `validation`,
//! `statistics`), each of which adds an `impl` block. This module owns the
//! field definitions, construction, and the small accessors/queries.

use super::ids::RouteId;
use super::provenance::RouteSource;
use crate::geometry::TraceSegment;
use crate::netlist::NetId;
use crate::space::AnalyticTrace;
use compact_str::CompactString;
use rustc_hash::FxHashMap;
use std::collections::HashSet;

/// Key identifying a group of child-instance route segments.
pub(super) type ChildRouteKey = (CompactString, NetId);

/// Hierarchical routing database
///
/// This is the single source of truth for all routing data in a space,
/// maintaining clear separation between child and parent routes.
#[derive(Debug, Clone)]
pub struct HierarchicalRoutingDatabase {
    /// Routes from child space instances (immutable after flattening)
    /// Key: (instance_name, net_id)
    /// Value: Route segments already transformed to parent coordinates
    pub(super) child_instance_routes: FxHashMap<ChildRouteKey, Vec<TraceSegment>>,

    /// Parent-level interconnect routes
    /// These connect between instances or to external ports
    pub(super) parent_interconnects: Vec<AnalyticTrace>,

    /// Metadata for debugging and error reporting
    /// Maps route_id to source information
    pub(super) route_provenance: FxHashMap<RouteId, RouteSource>,

    /// Counter for generating unique RouteIds
    pub(super) next_route_id: u64,
}

impl HierarchicalRoutingDatabase {
    /// Create a new empty routing database
    pub fn new() -> Self {
        Self {
            child_instance_routes: FxHashMap::default(),
            parent_interconnects: Vec::new(),
            route_provenance: FxHashMap::default(),
            next_route_id: 0,
        }
    }

    /// Allocate the next unique [`RouteId`].
    pub(super) fn allocate_route_id(&mut self) -> RouteId {
        let route_id = RouteId::new(self.next_route_id);
        self.next_route_id += 1;
        route_id
    }

    /// Clear all routing data (used during re-registration)
    pub fn clear(&mut self) {
        self.child_instance_routes.clear();
        self.parent_interconnects.clear();
        self.route_provenance.clear();
        self.next_route_id = 0;
    }

    /// Get parent interconnects (for analytic_routes compatibility)
    pub fn get_parent_interconnects(&self) -> &[AnalyticTrace] {
        &self.parent_interconnects
    }

    /// Get mutable parent interconnects
    pub fn get_parent_interconnects_mut(&mut self) -> &mut [AnalyticTrace] {
        &mut self.parent_interconnects
    }


    /// Get all child instance names
    pub fn get_child_instances(&self) -> Vec<CompactString> {
        self.child_instance_routes
            .keys()
            .map(|(inst, _)| inst.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect()
    }

    /// Check if a net has any routing data (child or parent)
    pub fn has_routing_for_net(&self, net_id: NetId) -> bool {
        self.has_child_routing_for_net(net_id) || self.has_parent_route_for_net(net_id)
    }

    /// Check whether any child instance contributes routing for `net_id`.
    pub(super) fn has_child_routing_for_net(&self, net_id: NetId) -> bool {
        self.child_instance_routes.keys().any(|(_, n)| *n == net_id)
    }

    /// Check whether a parent-level interconnect exists for `net_id`.
    pub(super) fn has_parent_route_for_net(&self, net_id: NetId) -> bool {
        self.parent_interconnects
            .iter()
            .any(|trace| trace.net_id == net_id)
    }

    /// Group child-instance routes by net, listing the instances involved.
    ///
    /// Shared by the two validation entry points.
    pub(super) fn nets_to_instances(&self) -> FxHashMap<NetId, Vec<CompactString>> {
        let mut net_to_instances: FxHashMap<NetId, Vec<CompactString>> = FxHashMap::default();

        for (instance, net_id) in self.child_instance_routes.keys() {
            net_to_instances
                .entry(*net_id)
                .or_default()
                .push(instance.clone());
        }

        net_to_instances
    }
}

impl Default for HierarchicalRoutingDatabase {
    fn default() -> Self {
        Self::new()
    }
}
