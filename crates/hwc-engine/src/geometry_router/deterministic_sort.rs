use std::collections::{BTreeSet, HashMap};

/// Error returned when a cycle is detected in the dependency graph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CycleError {
    pub cycle_path: Vec<u64>,
}

/// Deterministic topological sort using Kahn's algorithm.
///
/// Accepts a list of `(node_id, dependency_ids)` pairs where `dependency_ids`
/// are the IDs that `node_id` depends on (prerequisites). Returns nodes in
/// topological order so every node appears after all its dependencies.
///
/// When multiple nodes have in-degree 0, the one with the smallest `u64` ID
/// is always chosen via `BTreeSet`, guaranteeing identical output across runs.
pub fn deterministic_toposort(nodes: &[(u64, Vec<u64>)]) -> Result<Vec<u64>, CycleError> {
    let known: std::collections::HashSet<u64> = nodes.iter().map(|&(id, _)| id).collect();

    let mut in_degree: HashMap<u64, usize> = HashMap::new();
    let mut dependents: HashMap<u64, Vec<u64>> = HashMap::new();

    for &(id, _) in nodes {
        in_degree.entry(id).or_insert(0);
    }

    for &(id, ref deps) in nodes {
        for &dep in deps {
            if known.contains(&dep) {
                *in_degree.entry(id).or_insert(0) += 1;
                dependents.entry(dep).or_default().push(id);
            }
        }
    }

    let mut ready: BTreeSet<u64> = BTreeSet::new();
    for (&id, &deg) in &in_degree {
        if deg == 0 {
            ready.insert(id);
        }
    }

    let mut sorted: Vec<u64> = Vec::with_capacity(in_degree.len());

    while let Some(&node) = ready.iter().next() {
        ready.remove(&node);
        sorted.push(node);

        if let Some(deps) = dependents.get(&node) {
            for &dep in deps {
                if let Some(deg) = in_degree.get_mut(&dep) {
                    *deg -= 1;
                    if *deg == 0 {
                        ready.insert(dep);
                    }
                }
            }
        }
    }

    if sorted.len() != in_degree.len() {
        let mut cycle: Vec<u64> = in_degree
            .keys()
            .filter(|k| !sorted.contains(k))
            .copied()
            .collect();
        cycle.sort_unstable();
        return Err(CycleError { cycle_path: cycle });
    }

    Ok(sorted)
}

/// High-level API for sorting compilation definitions.
///
/// Each definition has an ID and a list of IDs it depends on.
pub fn sort_definitions(definitions: &[(u64, Vec<u64>)]) -> Result<Vec<u64>, CycleError> {
    deterministic_toposort(definitions)
}

/// Verify that two topological orders are identical (determinism check).
#[inline]
pub fn verify_deterministic_order(order1: &[u64], order2: &[u64]) -> bool {
    order1 == order2
}

/// Verify that every node appears after all its dependencies in the order.
pub fn verify_all_dependencies_satisfied(order: &[u64], deps: &[(u64, Vec<u64>)]) -> bool {
    let positions: HashMap<u64, usize> = order.iter().enumerate().map(|(i, &id)| (id, i)).collect();

    for &(id, ref dep_ids) in deps {
        if let Some(&pos) = positions.get(&id) {
            for &dep in dep_ids {
                if let Some(&dep_pos) = positions.get(&dep) {
                    if dep_pos >= pos {
                        return false;
                    }
                }
            }
        }
    }

    true
}
