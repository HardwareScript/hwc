//! Connectivity errors and their user-facing diagnostics.

use crate::netlist::NetId;
use compact_str::CompactString;
use std::fmt;

/// Connectivity errors detected during hierarchical validation
#[derive(Debug, Clone)]
pub enum ConnectivityError {
    /// A net appears in multiple child instances but has no parent-level routing
    IsolatedChildInstances {
        /// Network ID
        net_id: NetId,

        /// List of (instance_name, original_net_name) pairs
        instances: Vec<(CompactString, Option<CompactString>)>,
    },
}

impl fmt::Display for ConnectivityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConnectivityError::IsolatedChildInstances { net_id, instances } => {
                writeln!(f, "❌ HIERARCHICAL CONNECTIVITY ERROR")?;
                writeln!(f)?;
                writeln!(
                    f,
                    "Net {:?} exists in {} child instances but has NO parent-level routing:",
                    net_id,
                    instances.len()
                )?;
                writeln!(f)?;

                for (instance, original_net) in instances {
                    if let Some(orig) = original_net {
                        writeln!(f, "  • Instance '{}' (original net: '{}')", instance, orig)?;
                    } else {
                        writeln!(f, "  • Instance '{}'", instance)?;
                    }
                }

                writeln!(f)?;
                writeln!(
                    f,
                    "These instances have internal routing but are NOT connected to each other."
                )?;
                writeln!(f)?;
                writeln!(f, "Suggested fix:")?;
                writeln!(
                    f,
                    "  Add a parent-level route statement in your space to connect these instances."
                )?;
                writeln!(f, "  Example:")?;
                writeln!(
                    f,
                    "    route {}.PowerRail to {}.PowerRail:",
                    instances[0].0,
                    instances
                        .get(1)
                        .map(|(i, _)| i.as_str())
                        .unwrap_or("OtherInst")
                )?;
                writeln!(f, "        net: YourNetName")?;
                writeln!(f, "        width: 200nm")?;
                writeln!(f, "        layer: metal1")?;

                Ok(())
            }
        }
    }
}
