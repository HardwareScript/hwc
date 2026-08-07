//! Reconstruction of the unified `analytic_routes` vector.
//!
//! This is the ONLY way to populate `space.analytic_routes`. Parent
//! interconnects are already `AnalyticTrace`s; child routes are converted from
//! `TraceSegment` form here, including resolution of their stackup layer.

mod layer_resolution;

use super::database::HierarchicalRoutingDatabase;
use crate::netlist::NetlistArena;
use crate::space::{AnalyticTrace, CrossSection, CurrentRating, LineSegment, StackupLayer};

use layer_resolution::resolve_child_route_layer;

impl HierarchicalRoutingDatabase {
    /// Build the unified analytic_routes vector from the routing database.
    ///
    /// This is the ONLY way to populate `space.analytic_routes`.
    /// Child routes are converted from TraceSegment to AnalyticTrace format.
    ///
    /// # Arguments
    ///
    /// * `netlist` - Reference to the netlist for getting net names
    /// * `stackup_layers` - Reference to the stackup layers for looking up layer bounds
    ///
    /// # Panics
    ///
    /// Panics if a child route cannot be matched to any stackup layer, since
    /// that indicates the route has no valid physical layer lineage.
    pub fn build_analytic_routes(
        &self,
        netlist: &NetlistArena,
        stackup_layers: &[StackupLayer],
    ) -> Vec<AnalyticTrace> {
        let mut routes = Vec::new();

        // Parent interconnects are already AnalyticTrace
        routes.extend(self.parent_interconnects.iter().cloned());

        // Convert child routes to AnalyticTrace format
        for ((instance_name, net_id), segments) in &self.child_instance_routes {
            if segments.is_empty() {
                continue;
            }

            let width_nm = segments[0].width_nm;
            let material = segments[0].material_id;

            let line_segments: Vec<LineSegment> = segments
                .iter()
                .map(|seg| LineSegment::new(seg.start, seg.end))
                .collect();

            let net_name = netlist
                .get_net(*net_id)
                .map(|n| n.name.clone())
                .unwrap_or_else(|| format!("{}_child_route", instance_name).into());

            eprintln!(
                "[ROUTING DB] Processing child route: instance='{}', net={:?}, {} segments",
                instance_name,
                net_id,
                line_segments.len()
            );

            let resolved =
                resolve_child_route_layer(&line_segments, stackup_layers, instance_name, *net_id);

            let (layer_z_range, route_layer_name) = match resolved {
                Some((z_range, name)) => (Some(z_range), name),
                None => {
                    eprintln!(
                        "[ROUTING DB] FATAL: Could not determine layer for child route: instance='{}', net={:?}",
                        instance_name, net_id
                    );
                    panic!(
                        "Child route for instance '{}', net {:?} has no matching stackup layer",
                        instance_name, net_id
                    );
                }
            };

            routes.push(AnalyticTrace::with_layer_z_range(
                *net_id,
                CrossSection::new(width_nm, 400),
                line_segments,
                material,
                net_name,
                CurrentRating::new(0.0, 0.0),
                layer_z_range,
                route_layer_name, // v0.2.2: Explicit layer lineage
            ));
        }

        routes
    }
}
