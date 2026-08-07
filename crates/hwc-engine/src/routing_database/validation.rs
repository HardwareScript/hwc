//! Hierarchical connectivity validation.
//!
//! A net that appears in more than one child instance must be joined by a
//! parent-level interconnect; otherwise the instances are electrically
//! isolated despite sharing a net ID.

use super::database::HierarchicalRoutingDatabase;
use super::errors::ConnectivityError;
use super::provenance::RouteSource;
use compact_str::CompactString;

impl HierarchicalRoutingDatabase {
    /// Validate hierarchical connectivity
    ///
    /// Checks that nets appearing in multiple child instances have parent-level
    /// routing to connect them. Returns detailed error information if not.
    pub fn validate_hierarchical_connectivity(&self) -> Result<(), Vec<ConnectivityError>> {
        let mut errors = Vec::new();

        // Group child routes by net_id to find nets in multiple instances
        let net_to_instances = self.nets_to_instances();

        // Check each net that appears in multiple child instances
        for (net_id, instances) in &net_to_instances {
            if instances.len() <= 1 {
                continue;
            }

            // Net exists in multiple child instances - check for parent routing
            if self.has_parent_route_for_net(*net_id) {
                continue;
            }

            // Get original net names from each instance
            let instance_details: Vec<_> = instances
                .iter()
                .map(|inst| (inst.clone(), self.original_net_name_for(inst)))
                .collect();

            errors.push(ConnectivityError::IsolatedChildInstances {
                net_id: *net_id,
                instances: instance_details,
            });
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Validate routing database consistency.
    ///
    /// Returns errors if child routes exist without parent interconnects for nets
    /// that appear in multiple instances.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        // Group child routes by net_id
        let nets_with_multiple_instances = self.nets_to_instances();

        // Check that nets appearing in multiple instances have parent routes
        for (net_id, instances) in nets_with_multiple_instances {
            if instances.len() <= 1 {
                continue;
            }

            if !self.has_parent_route_for_net(net_id) {
                errors.push(format!(
                    "Net {:?} appears in {} instances ({}) but has no parent-level interconnect",
                    net_id,
                    instances.len(),
                    instances.join(", ")
                ));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Best-effort lookup of the original (child-space) net name for an instance.
    fn original_net_name_for(&self, instance: &CompactString) -> Option<CompactString> {
        self.route_provenance.values().find_map(|src| match src {
            RouteSource::ChildInstance {
                instance: i,
                original_net,
            } if i == instance => Some(original_net.clone()),
            _ => None,
        })
    }
}
