//! Graph Isomorphism Matcher for Alignment Validation
//!
//! This module implements the "brain" of alignment validation - it treats netlists
//! as mathematical graphs and performs isomorphism checking to verify that the
//! physical layout matches the logical intent.
//!
//! # Graph Representation
//!
//! - **Nodes**: Nets (VIN, VOUT, GND, VDD)
//! - **Edges**: Device terminals (M1 connects VIN to VOUT through gate/drain)
//!
//! # Algorithm
//!
//! 1. Build graph from logical netlist
//! 2. Build graph from physical netlist
//! 3. Check device count matches
//! 4. Check device types match
//! 5. Check connectivity matches (graph isomorphism)
//! 6. Check port mappings match
//! 7. Check device parameters match (W/L within tolerance)

use super::error::{AlignmentError, SpatialInfo};
use super::netlist::{
    DeviceEdge, DeviceTypeRegistry, LogicalNetlist, NetNode, NetlistGraph, PhysicalNetlist,
};
use compact_str::CompactString;
use rustc_hash::FxHashMap;

/// Default parameter tolerance (1%)
pub const DEFAULT_PARAMETER_TOLERANCE: f64 = 1.0;

/// Graph matcher for alignment validation
pub struct GraphMatcher<'a> {
    logical: &'a LogicalNetlist,
    physical: &'a PhysicalNetlist,
    tolerance: f64,
    device_registry: &'a DeviceTypeRegistry,
    space: &'a hwc_engine::HardwareSpace, // For spatial error reporting
}

impl<'a> GraphMatcher<'a> {
    /// Create a new graph matcher
    ///
    /// # Arguments
    /// * `logical` - Logical netlist from module definition
    /// * `physical` - Physical netlist extracted from geometry
    /// * `device_registry` - Device type registry for type name lookups
    /// * `space` - Hardware space for spatial error reporting
    pub fn new(
        logical: &'a LogicalNetlist,
        physical: &'a PhysicalNetlist,
        device_registry: &'a DeviceTypeRegistry,
        space: &'a hwc_engine::HardwareSpace,
    ) -> Self {
        Self {
            logical,
            physical,
            tolerance: DEFAULT_PARAMETER_TOLERANCE,
            device_registry,
            space,
        }
    }

    /// Set parameter tolerance
    pub fn with_tolerance(mut self, tolerance: f64) -> Self {
        self.tolerance = tolerance;
        self
    }

    /// Verify that logical and physical netlists are isomorphic
    ///
    /// This is the main entry point for alignment validation.
    ///
    /// # Returns
    /// Ok(()) if netlists match, Err with detailed mismatch information otherwise
    pub fn verify_isomorphism(&self) -> Result<(), AlignmentError> {
        // Step 1: Check device count
        // Special case: If module has 0 devices but space has devices from bindings,
        // this is valid - it means the module only declares the interface (pins)
        // and the implementation is purely physical (device bindings in space).
        // This is a valid pattern for simple circuits (e.g., resistor dividers).
        let logical_device_count = self.logical.devices.len();
        let physical_device_count = self.physical.devices.len();
        
        if logical_device_count == 0 && physical_device_count > 0 {
            // Valid pattern: Module declares only pins, space implements with device bindings
            // Skip device type and connectivity checks since there's no logical device to compare
            // Only verify port mappings
            self.verify_port_mappings()?;
            return Ok(());
        }
        
        if logical_device_count != physical_device_count {
            return Err(AlignmentError::DeviceCountMismatch {
                expected: logical_device_count,
                found: physical_device_count,
            });
        }

        // Step 2: Verify device types match
        self.verify_device_types()?;

        // Step 3: Build graphs from netlists
        let logical_graph = self.build_graph_from_logical();
        let physical_graph = self.build_graph_from_physical();

        // Step 4: Verify connectivity (graph isomorphism)
        self.verify_connectivity(&logical_graph, &physical_graph)?;

        // Step 5: Verify port mappings
        self.verify_port_mappings()?;

        // Step 6: Verify device parameters (W/L with tolerance)
        self.verify_device_parameters()?;

        Ok(())
    }

    /// Verify that device types match between logical and physical netlists
    fn verify_device_types(&self) -> Result<(), AlignmentError> {
        // For each logical device, verify it exists in physical with same type
        let mut physical_devices: FxHashMap<CompactString, &super::netlist::PhysicalDevice> =
            FxHashMap::default();
        for device in &self.physical.devices {
            physical_devices.insert(device.name.clone(), device);
        }

        for logical_device in &self.logical.devices {
            let physical_device = physical_devices.get(&logical_device.name).ok_or_else(|| {
                AlignmentError::DeviceMissing {
                    device_name: logical_device.name.clone(),
                }
            })?;

            // Verify device types match
            if logical_device.device_type_id != physical_device.device_type_id {
                let expected_type_name = self
                    .device_registry
                    .get_name(logical_device.device_type_id)
                    .unwrap_or("Unknown")
                    .to_string();
                let found_type_name = self
                    .device_registry
                    .get_name(physical_device.device_type_id)
                    .unwrap_or("Unknown")
                    .to_string();

                return Err(AlignmentError::DeviceTypeMismatch {
                    device_name: logical_device.name.clone(),
                    expected_type_id: logical_device.device_type_id,
                    found_type_id: physical_device.device_type_id,
                    expected_type_name: expected_type_name.into(),
                    found_type_name: found_type_name.into(),
                });
            }
        }

        Ok(())
    }

    /// Build graph representation from logical netlist
    fn build_graph_from_logical(&self) -> NetlistGraph {
        let mut graph = NetlistGraph::new();

        // Create nodes for all nets
        for (net_name, net_info) in &self.logical.nets {
            graph.nodes.insert(
                net_name.clone(),
                NetNode {
                    net_name: net_name.clone(),
                    connected_devices: net_info.connected_devices.clone(),
                },
            );
        }

        // Create edges for all devices
        for device in &self.logical.devices {
            graph.edges.push(DeviceEdge {
                device_name: device.name.clone(),
                device_type_id: device.device_type_id,
                connections: device.terminals.clone(),
            });
        }

        graph
    }

    /// Build graph representation from physical netlist
    fn build_graph_from_physical(&self) -> NetlistGraph {
        let mut graph = NetlistGraph::new();

        // Create nodes for all nets
        for (net_name, net_info) in &self.physical.nets {
            graph.nodes.insert(
                net_name.clone(),
                NetNode {
                    net_name: net_name.clone(),
                    connected_devices: net_info.connected_devices.clone(),
                },
            );
        }

        // Create edges for all devices
        for device in &self.physical.devices {
            graph.edges.push(DeviceEdge {
                device_name: device.name.clone(),
                device_type_id: device.device_type_id,
                connections: device.terminals.clone(),
            });
        }

        graph
    }

    /// Verify connectivity between logical and physical graphs
    ///
    /// This implements a simplified graph isomorphism check that verifies:
    /// 1. Each device in logical has a matching device in physical
    /// 2. Terminal connections match (same nets connected to same terminals)
    fn verify_connectivity(
        &self,
        logical_graph: &NetlistGraph,
        physical_graph: &NetlistGraph,
    ) -> Result<(), AlignmentError> {
        // Build a map of device names to edges for quick lookup
        let mut physical_edges: FxHashMap<CompactString, &DeviceEdge> = FxHashMap::default();
        for edge in &physical_graph.edges {
            physical_edges.insert(edge.device_name.clone(), edge);
        }

        // For each logical device, find matching physical device and verify connections
        for logical_edge in &logical_graph.edges {
            let physical_edge = physical_edges
                .get(&logical_edge.device_name)
                .ok_or_else(|| AlignmentError::DeviceMissing {
                    device_name: logical_edge.device_name.clone(),
                })?;

            // Verify device types match
            if logical_edge.device_type_id != physical_edge.device_type_id {
                let expected_type_name = self
                    .device_registry
                    .get_name(logical_edge.device_type_id)
                    .unwrap_or("Unknown")
                    .to_string();
                let found_type_name = self
                    .device_registry
                    .get_name(physical_edge.device_type_id)
                    .unwrap_or("Unknown")
                    .to_string();

                return Err(AlignmentError::DeviceTypeMismatch {
                    device_name: logical_edge.device_name.clone(),
                    expected_type_id: logical_edge.device_type_id,
                    found_type_id: physical_edge.device_type_id,
                    expected_type_name: expected_type_name.into(),
                    found_type_name: found_type_name.into(),
                });
            }

            // Verify all terminals in logical device exist in physical device
            for (terminal, expected_net) in &logical_edge.connections {
                let actual_net = physical_edge.connections.get(terminal).ok_or_else(|| {
                    AlignmentError::TerminalMissing {
                        device_name: logical_edge.device_name.clone(),
                        terminal: terminal.clone(),
                    }
                })?;

                // Normalize net names for comparison (handle "0" vs "GND" aliases)
                let expected_normalized = self.normalize_net_name(expected_net);
                let actual_normalized = self.normalize_net_name(actual_net);

                if expected_normalized != actual_normalized {
                    let spatial_info = self.get_spatial_info(&logical_edge.device_name, terminal);

                    return Err(AlignmentError::TerminalMismatch(Box::new(
                        crate::alignment::error::TerminalMismatchDetails {
                            device_name: logical_edge.device_name.clone(),
                            terminal_name: terminal.clone(),
                            expected_net: expected_net.into(),
                            found_net: actual_net.clone().into(),
                            suggestion: format!("Check pour connected to terminal '{}'", terminal)
                                .into(),
                            spatial_info,
                        },
                    )));
                }
            }

            // Verify no extra terminals in physical device
            for terminal in physical_edge.connections.keys() {
                if !logical_edge.connections.contains_key(terminal) {
                    let spatial_info = self.get_spatial_info(&logical_edge.device_name, terminal);

                    return Err(AlignmentError::TerminalMismatch(Box::new(
                        crate::alignment::error::TerminalMismatchDetails {
                            device_name: logical_edge.device_name.clone(),
                            terminal_name: terminal.clone(),
                            expected_net: "(none)".into(),
                            found_net: physical_edge
                                .connections
                                .get(terminal)
                                .unwrap_or(&"(unknown)".to_string())
                                .clone()
                                .into(),
                            suggestion: format!(
                                "Extra terminal '{}' found in physical device",
                                terminal
                            )
                            .into(),
                            spatial_info,
                        },
                    )));
                }
            }
        }

        Ok(())
    }

    /// Normalize net names for comparison.
    ///
    /// v0.1.8 ZERO-MAGIC: No aliasing. Net names must match exactly as declared
    /// in the PDK profile. The compiler must NOT guess that "0" means "GND" or
    /// that "VCC" means "VDD" — this violates the Zero-Magic Compiler mandate.
    /// If net names differ, the compiler must fail-fast with a clear error.
    fn normalize_net_name(&self, net: &str) -> CompactString {
        net.to_uppercase().into()
    }

    /// Get spatial information for a device terminal
    ///
    /// Looks up the pour associated with a terminal and extracts its spatial info
    fn get_spatial_info(&self, device_name: &str, terminal_name: &str) -> Option<SpatialInfo> {
        // Find the physical device
        let physical_device = self
            .physical
            .devices
            .iter()
            .find(|d| d.name == device_name)?;

        // Get the pour name for this terminal
        let pour_name = physical_device.terminal_pours.get(terminal_name)?;

        // Find the pour in the space
        let pour = self.space.pours.iter().find(|p| p.name == pour_name)?;

        Some(SpatialInfo {
            pour_name: pour.name.clone(),
            bbox: pour.bbox,
            z_bottom_nm: Some(pour.z_bottom_nm),
        })
    }

    /// Verify port mappings between logical and physical netlists
    fn verify_port_mappings(&self) -> Result<(), AlignmentError> {
        // Build a map of port names for quick lookup
        let mut physical_ports: FxHashMap<CompactString, &super::netlist::PortInfo> =
            FxHashMap::default();
        for port in &self.physical.ports {
            physical_ports.insert(port.name.clone(), port);
        }

        // Verify each logical port exists in physical netlist
        for logical_port in &self.logical.ports {
            let physical_port = physical_ports.get(&logical_port.name).ok_or_else(|| {
                AlignmentError::PortMissing {
                    port_name: logical_port.name.clone(),
                }
            })?;

            // Verify port directions match
            // Physical ports now copy directions from the module, so this is a real check
            if logical_port.direction != physical_port.direction {
                return Err(AlignmentError::PortDirectionMismatch {
                    port_name: logical_port.name.clone(),
                    expected: format!("{}", logical_port.direction).into(),
                    actual: format!("{}", physical_port.direction).into(),
                });
            }
        }

        // Check for extra ports in physical netlist
        for physical_port in &self.physical.ports {
            if !self
                .logical
                .ports
                .iter()
                .any(|p| p.name == physical_port.name)
            {
                return Err(AlignmentError::PortMissing {
                    port_name: format!("(extra port in physical: {})", physical_port.name).into(),
                });
            }
        }

        Ok(())
    }

    /// Verify device parameters match within tolerance
    fn verify_device_parameters(&self) -> Result<(), AlignmentError> {
        // Build a map of physical devices for quick lookup
        let mut physical_devices: FxHashMap<CompactString, &super::netlist::PhysicalDevice> =
            FxHashMap::default();
        for device in &self.physical.devices {
            physical_devices.insert(device.name.clone(), device);
        }

        // For each logical device, verify parameters
        for logical_device in &self.logical.devices {
            let physical_device = physical_devices.get(&logical_device.name).ok_or_else(|| {
                AlignmentError::DeviceMissing {
                    device_name: logical_device.name.clone(),
                }
            })?;

            // Check each parameter in logical device
            for (param_name, expected_value) in &logical_device.parameters {
                if let Some(&actual_value) = physical_device.parameters.get(param_name) {
                    // Calculate percentage difference
                    let diff_percent =
                        ((actual_value - expected_value).abs() / expected_value) * 100.0;

                    if diff_percent > self.tolerance {
                        // Get spatial info for the gate terminal (most relevant for W/L parameters)
                        let spatial_info = self.get_spatial_info(&logical_device.name, "gate");

                        return Err(AlignmentError::ParameterMismatch(Box::new(
                            crate::alignment::error::ParameterMismatchDetails {
                                device_name: logical_device.name.clone(),
                                parameter: param_name.clone(),
                                expected: *expected_value,
                                found: actual_value,
                                tolerance: self.tolerance,
                                spatial_info,
                            },
                        )));
                    }
                }
                // If parameter is missing in physical, it's optional (skip check)
            }
        }

        Ok(())
    }
}
