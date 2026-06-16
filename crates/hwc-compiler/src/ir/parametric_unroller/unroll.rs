//! Core unrolling logic for for loops
//!
//! Handles the main loop expansion and delegates to specialized unrollers
//! for each statement type (components, pours, contacts, routes).

use super::collision::{
    print_identity_collision_warning, print_same_iteration_collision_warnings, CollisionWarning,
};
use super::substitution::{
    format_net_name, unroll_component, unroll_contact, unroll_pour, unroll_route,
};
use crate::ir::errors::IrError;
use crate::SymbolTable;
use compact_str::CompactString;
use hwc_parser::{
    ComponentPlacement, ContactPlacement, PourPlacement, Route, SpaceForLoop, SpaceStatement,
};
use rustc_hash::{FxHashMap, FxHashSet};

/// Result of unrolling a for loop
pub struct UnrolledStatements {
    pub components: Vec<ComponentPlacement>,
    pub pours: Vec<PourPlacement>,
    pub contacts: Vec<ContactPlacement>,
    pub routes: Vec<Route>,
}

/// Unroll a for loop into individual statements
///
/// **CRITICAL**: Hardware Script uses **inclusive ranges** (Ruby-style)
/// - `0..7` means [0,1,2,3,4,5,6,7] (8 items, both endpoints included)
/// - This matches hardware conventions (e.g., "bus bits 0..7" = 8 bits)
///
/// **v0.1.6 Sprint 3.4**: Supports `last` keyword for relative positioning
/// - `last.right` refers to the most recently placed component in the space
/// - Resolution happens during constraint solving, not during unrolling
/// - This allows `last` to work across loop boundaries (God-Tier feature!)
pub fn unroll_for_loop(
    for_loop: &SpaceForLoop,
    _symbol_table: &SymbolTable,
) -> Result<UnrolledStatements, IrError> {
    let mut components = Vec::new();
    let mut pours = Vec::new();
    let mut contacts = Vec::new();
    let mut routes = Vec::new();

    // Identity collision detection: Track nets used in each iteration
    let mut net_usage_per_iteration: FxHashMap<usize, FxHashSet<CompactString>> =
        FxHashMap::default();
    let mut collision_warnings: Vec<CollisionWarning> = Vec::new();

    // INCLUSIVE range iteration (Hardware Script spec: Ruby-style ranges)
    // 0..7 produces [0,1,2,3,4,5,6,7], NOT [0,1,2,3,4,5,6]
    for i in for_loop.start..=for_loop.end {
        let mut nets_in_this_iteration = FxHashSet::default();

        // Process each statement in the loop body
        for statement in &for_loop.body {
            match statement {
                SpaceStatement::Component(comp) => {
                    // Note: 'last' keyword is NOT resolved here
                    // It will be resolved during constraint solving when the BoundingBoxTracker
                    // knows about all previously placed components
                    let unrolled_comp = unroll_component(comp, &for_loop.variable, i)?;
                    components.push(unrolled_comp);
                }
                SpaceStatement::Pour(pour) => {
                    let unrolled_pour = unroll_pour(pour, &for_loop.variable, i)?;

                    // Track net usage for collision detection
                    if let Some(ref net) = unrolled_pour.net {
                        let net_str = format_net_name(net);
                        if !nets_in_this_iteration.insert(net_str.clone()) {
                            // Same net used twice in this iteration - record collision
                            collision_warnings.push(CollisionWarning {
                                iteration: i,
                                net_name: net_str.clone(),
                                object_type: "pour".into(),
                                object_name: unrolled_pour.name.to_string(),
                            });
                        }
                    }

                    pours.push(unrolled_pour);
                }
                SpaceStatement::Contact(contact) => {
                    let unrolled_contact = unroll_contact(contact, &for_loop.variable, i)?;

                    // Track net usage for collision detection
                    if let Some(ref net) = unrolled_contact.net {
                        let net_str = format_net_name(net);
                        if !nets_in_this_iteration.insert(net_str.clone()) {
                            // Same net used twice in this iteration - record collision
                            collision_warnings.push(CollisionWarning {
                                iteration: i,
                                net_name: net_str.clone(),
                                object_type: "contact".into(),
                                object_name: unrolled_contact
                                    .name
                                    .as_ref()
                                    .map(|n| n.to_string())
                                    .unwrap_or_else(|| "unnamed".into()),
                            });
                        }
                    }

                    contacts.push(unrolled_contact);
                }
                SpaceStatement::Route(route) => {
                    let unrolled_route = unroll_route(route, &for_loop.variable, i)?;
                    routes.push(unrolled_route);
                }
                SpaceStatement::ForLoop(nested_loop) => {
                    // Recursively unroll nested loops
                    let nested_unrolled = unroll_for_loop(nested_loop, _symbol_table)?;

                    // CRITICAL FIX (v0.1.7): After unrolling a nested loop, we MUST substitute
                    // the current loop variable into ALL statements returned from the inner loop.
                    // Otherwise, nested loops like 'for i ... for j ... [x: i + j]' will fail
                    // because 'i' remains unresolved in the final output.
                    for comp in nested_unrolled.components {
                        components.push(unroll_component(&comp, &for_loop.variable, i)?);
                    }
                    for pour in nested_unrolled.pours {
                        pours.push(unroll_pour(&pour, &for_loop.variable, i)?);
                    }
                    for contact in nested_unrolled.contacts {
                        contacts.push(unroll_contact(&contact, &for_loop.variable, i)?);
                    }
                    for route in nested_unrolled.routes {
                        routes.push(unroll_route(&route, &for_loop.variable, i)?);
                    }
                }
            }
        }

        // Store nets used in this iteration
        net_usage_per_iteration.insert(i, nets_in_this_iteration);
    }

    // Check for identity collisions across iterations
    // (same net name generated by different loop values due to truncation/rounding)
    let mut all_nets_to_iterations: FxHashMap<CompactString, Vec<usize>> = FxHashMap::default();
    for (iteration, nets) in &net_usage_per_iteration {
        for net in nets {
            all_nets_to_iterations
                .entry(net.clone())
                .or_default()
                .push(*iteration);
        }
    }

    // Warn about nets that appear in multiple iterations
    for (net_name, iterations) in &all_nets_to_iterations {
        if iterations.len() > 1 {
            print_identity_collision_warning(net_name, iterations, &for_loop.variable);
        }
    }

    // Warn about nets used multiple times within the same iteration
    if !collision_warnings.is_empty() {
        print_same_iteration_collision_warnings(&collision_warnings);
    }

    Ok(UnrolledStatements {
        components,
        pours,
        contacts,
        routes,
    })
}
