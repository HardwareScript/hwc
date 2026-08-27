//! Stage 1: Spatial via clustering and parallel contact resistance calculation.

use rustc_hash::FxHashMap;

use super::geometry::{distance_2d, get_bbox_centroid};
use super::types::{ExtractedClusterNode, VIA_CLUSTER_RADIUS_NM};
use crate::netlist::types::{ParasiticElement, PhysicalNetlist, PhysicalNetlistGraph};
use hwc_engine::space::ContactMetadata;
use hwc_engine::HardwareSpace;

/// Spatially cluster via arrays and compute parallel via stack resistance.
pub fn extract_via_stacks(
    space: &HardwareSpace,
    _physical_netlist: Option<&PhysicalNetlist>,
    graph: &mut PhysicalNetlistGraph,
    extracted_layer_nodes: &mut FxHashMap<(String, String), Vec<ExtractedClusterNode>>,
) {
    eprintln!("[VIA STACKS] Starting via stack extraction...");
    eprintln!("[VIA STACKS] Total contacts in space: {}", space.contacts.len());
    
    // Group contacts by net
    let mut contacts_by_net: FxHashMap<String, Vec<&ContactMetadata>> = FxHashMap::default();
    for contact in &space.contacts {
        if let Some(net_name) = &contact.net {
            if contact.from_layer.is_some() && contact.to_layer.is_some() {
                contacts_by_net
                    .entry(net_name.to_string())
                    .or_default()
                    .push(contact);
            }
        }
    }
    
    eprintln!("[VIA STACKS] Contacts grouped by net: {} nets", contacts_by_net.len());
    for (net_name, contacts) in &contacts_by_net {
        eprintln!("[VIA STACKS]   Net '{}': {} contacts", net_name, contacts.len());
    }

    // Process spatial clusters per net
    for (net_name, net_contacts) in contacts_by_net {
        eprintln!("[VIA STACKS] Processing net '{}' with {} contacts", net_name, net_contacts.len());
        let cluster_groups = cluster_contacts_spatially(&net_contacts, VIA_CLUSTER_RADIUS_NM);
        let total_clusters = cluster_groups.len();
        eprintln!("[VIA STACKS]   Formed {} spatial clusters", total_clusters);

        for (cluster_idx, cluster_contacts) in cluster_groups.into_iter().enumerate() {
            eprintln!("[VIA STACKS]   Cluster {}: {} contacts", cluster_idx, cluster_contacts.len());
            let mut sum_x = 0.0;
            let mut sum_y = 0.0;
            for c in &cluster_contacts {
                let p = get_bbox_centroid(c.bbox.as_ref());
                sum_x += p.0;
                sum_y += p.1;
            }
            let cluster_centroid = (
                sum_x / cluster_contacts.len() as f64,
                sum_y / cluster_contacts.len() as f64,
            );

            let mut transitions: FxHashMap<(String, String), Vec<&ContactMetadata>> = FxHashMap::default();
            for contact in cluster_contacts {
                if let (Some(from_l), Some(to_l)) = (&contact.from_layer, &contact.to_layer) {
                    transitions
                        .entry((from_l.to_string(), to_l.to_string()))
                        .or_default()
                        .push(contact);
                }
            }
            eprintln!("[VIA STACKS]     Transitions: {}", transitions.len());

            for ((from_layer, to_layer), contacts_in_stack) in transitions {
                let num_vias = contacts_in_stack.len();
                eprintln!("[VIA STACKS]     Transition {}→{}: {} vias", from_layer, to_layer, num_vias);
                if let Some(first_contact) = contacts_in_stack.first() {
                    eprintln!("[VIA STACKS]       Material: {}", first_contact.material_name);
                    eprintln!("[VIA STACKS]       Drill diameter: {:?}", first_contact.drill_diameter_nm);
                    
                    if let Some(mat_id) = space.material_registry.get_id(&first_contact.material_name) {
                        eprintln!("[VIA STACKS]       ✓ Material ID found");
                        if let Some(mat_props) = space.material_registry.get_physical_props(mat_id) {
                            eprintln!("[VIA STACKS]       ✓ Material props found");
                            if let Some(contact_resistance) = mat_props.get("contact_resistance") {
                                eprintln!("[VIA STACKS]       ✓ Contact resistance: {}", contact_resistance);
                                if let Some(drill_diameter_nm) = first_contact.drill_diameter_nm {
                                    eprintln!("[VIA STACKS]       ✓ Processing via resistance calculation...");
                                    
                                    let drill_radius_cm = (drill_diameter_nm as f64 * 1e-9 * 100.0) / 2.0;
                                    let via_area_cm2 = std::f64::consts::PI * drill_radius_cm * drill_radius_cm;

                                    // Strongly-typed material interface resolution:
                                    // Query the materials of the connecting layers from space pour metadata
                                    let from_mat_name = space.pours.iter()
                                        .find(|p| p.layer_name == from_layer)
                                        .map(|p| p.material_name.as_str());

                                    let to_mat_name = space.pours.iter()
                                        .find(|p| p.layer_name == to_layer)
                                        .map(|p| p.material_name.as_str());

                                    let from_mat_id = from_mat_name.and_then(|name| space.material_registry.get_id(name));
                                    let to_mat_id = to_mat_name.and_then(|name| space.material_registry.get_id(name));

                                    // Check if either connecting material defines an interfacial contact_resistance
                                    let effective_contact_res = from_mat_id
                                        .and_then(|id| space.material_registry.get_physical_props(id))
                                        .and_then(|props| props.get("contact_resistance"))
                                        .or_else(|| {
                                            to_mat_id
                                                .and_then(|id| space.material_registry.get_physical_props(id))
                                                .and_then(|props| props.get("contact_resistance"))
                                        })
                                        .unwrap_or(contact_resistance);

                                    let single_via_resistance = effective_contact_res / via_area_cm2;
                                    let total_via_resistance = single_via_resistance / (num_vias as f64);

                                    if total_via_resistance > 0.001 {
                                        let via_name = if total_clusters == 1 {
                                            format!("via_{}_{}_{}", net_name, from_layer, to_layer)
                                        } else {
                                            format!("via_{}_{}_{}_{}", net_name, from_layer, to_layer, cluster_idx)
                                        };

                                        let node_a = if total_clusters == 1 {
                                            format!("n{}_{}", net_name, to_layer)
                                        } else {
                                            format!("n{}_{}_{}", net_name, to_layer, cluster_idx)
                                        };

                                        let node_b = if total_clusters == 1 {
                                            format!("n{}_{}", net_name, from_layer)
                                        } else {
                                            format!("n{}_{}_{}", net_name, from_layer, cluster_idx)
                                        };

                                        graph.parasitics.push(ParasiticElement::TraceResistor {
                                            name: via_name,
                                            node_a: node_a.clone(),
                                            node_b: node_b.clone(),
                                            value_ohms: total_via_resistance,
                                        });

                                        let list_a = extracted_layer_nodes
                                            .entry((net_name.clone(), to_layer.clone()))
                                            .or_default();
                                        if !list_a.iter().any(|item| item.node == node_a) {
                                            list_a.push(ExtractedClusterNode {
                                                node: node_a,
                                                centroid: cluster_centroid,
                                            });
                                        }

                                        let list_b = extracted_layer_nodes
                                            .entry((net_name.clone(), from_layer.clone()))
                                            .or_default();
                                        if !list_b.iter().any(|item| item.node == node_b) {
                                            list_b.push(ExtractedClusterNode {
                                                node: node_b,
                                                centroid: cluster_centroid,
                                            });
                                        }
                                    }
                                } else {
                                    eprintln!("[VIA STACKS]       ✗ drill_diameter_nm is None");
                                }
                            } else {
                                eprintln!("[VIA STACKS]       ✗ No contact_resistance property");
                            }
                        } else {
                            eprintln!("[VIA STACKS]       ✗ Material props not found");
                        }
                    } else {
                        eprintln!("[VIA STACKS]       ✗ Material ID not found");
                    }
                }
            }
        }
    }
}

/// Disjoint-set spatial clustering of contacts within a proximity radius.
fn cluster_contacts_spatially<'c>(
    contacts: &[&'c ContactMetadata],
    radius_nm: f64,
) -> Vec<Vec<&'c ContactMetadata>> {
    let n = contacts.len();
    if n == 0 {
        return Vec::new();
    }

    let mut parent: Vec<usize> = (0..n).collect();
    fn find(parent: &mut [usize], i: usize) -> usize {
        if parent[i] == i {
            i
        } else {
            let root = find(parent, parent[i]);
            parent[i] = root;
            root
        }
    }
    fn union(parent: &mut [usize], i: usize, j: usize) {
        let root_i = find(parent, i);
        let root_j = find(parent, j);
        if root_i != root_j {
            parent[root_i] = root_j;
        }
    }

    let centroids: Vec<(f64, f64)> = contacts
        .iter()
        .map(|c| get_bbox_centroid(c.bbox.as_ref()))
        .collect();

    for i in 0..n {
        for j in (i + 1)..n {
            if distance_2d(centroids[i], centroids[j]) <= radius_nm {
                union(&mut parent, i, j);
            }
        }
    }

    let mut cluster_map: FxHashMap<usize, Vec<&'c ContactMetadata>> = FxHashMap::default();
    for i in 0..n {
        let root = find(&mut parent, i);
        cluster_map.entry(root).or_default().push(contacts[i]);
    }

    let mut result: Vec<Vec<&'c ContactMetadata>> = cluster_map.into_values().collect();
    result.sort_by(|a, b| {
        let c_a = {
            let mut sx = 0.0;
            let mut sy = 0.0;
            for c in a {
                let p = get_bbox_centroid(c.bbox.as_ref());
                sx += p.0;
                sy += p.1;
            }
            (sx / a.len() as f64, sy / a.len() as f64)
        };
        let c_b = {
            let mut sx = 0.0;
            let mut sy = 0.0;
            for c in b {
                let p = get_bbox_centroid(c.bbox.as_ref());
                sx += p.0;
                sy += p.1;
            }
            (sx / b.len() as f64, sy / b.len() as f64)
        };
        c_a.0
            .partial_cmp(&c_b.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| c_a.1.partial_cmp(&c_b.1).unwrap_or(std::cmp::Ordering::Equal))
    });
    result
}
