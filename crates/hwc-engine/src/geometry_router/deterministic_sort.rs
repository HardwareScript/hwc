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

#[cfg(test)]
#[allow(unwrap_used, expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_dag() {
        let nodes = vec![(3, vec![1, 2]), (2, vec![1]), (1, vec![])];
        let order = deterministic_toposort(&nodes).unwrap();
        assert_eq!(order, vec![1, 2, 3]);
    }

    #[test]
    fn test_cycle_detection() {
        let nodes = vec![(1, vec![2]), (2, vec![1])];
        let result = deterministic_toposort(&nodes);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.cycle_path.len() >= 2);
        assert!(err.cycle_path.contains(&1));
        assert!(err.cycle_path.contains(&2));
    }

    #[test]
    fn test_tie_breaking_determinism() {
        let nodes = vec![
            (5, vec![1, 2]),
            (4, vec![1, 2]),
            (3, vec![1, 2]),
            (1, vec![]),
            (2, vec![]),
        ];
        let order1 = deterministic_toposort(&nodes).unwrap();
        let order2 = deterministic_toposort(&nodes).unwrap();
        assert_eq!(order1, order2);
        assert_eq!(order1, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_determinism_verification() {
        let nodes = vec![(10, vec![5, 3]), (5, vec![3]), (3, vec![])];
        let order1 = deterministic_toposort(&nodes).unwrap();
        let order2 = deterministic_toposort(&nodes).unwrap();
        assert!(verify_deterministic_order(&order1, &order2));
    }

    #[test]
    fn test_dependencies_satisfied() {
        let nodes = vec![(4, vec![2, 3]), (3, vec![1]), (2, vec![1]), (1, vec![])];
        let order = deterministic_toposort(&nodes).unwrap();
        assert!(verify_all_dependencies_satisfied(&order, &nodes));
    }

    #[test]
    fn test_empty_graph() {
        let nodes: Vec<(u64, Vec<u64>)> = vec![];
        let order = deterministic_toposort(&nodes).unwrap();
        assert!(order.is_empty());
    }

    #[test]
    fn test_single_node() {
        let nodes = vec![(42, vec![])];
        let order = deterministic_toposort(&nodes).unwrap();
        assert_eq!(order, vec![42]);
    }

    #[test]
    fn test_sort_definitions_api() {
        let defs = vec![(3, vec![1, 2]), (2, vec![1]), (1, vec![])];
        let order = sort_definitions(&defs).unwrap();
        assert_eq!(order, vec![1, 2, 3]);
    }

    #[test]
    fn test_complex_dag() {
        let nodes = vec![
            (6, vec![4, 5]),
            (5, vec![3]),
            (4, vec![2]),
            (3, vec![1]),
            (2, vec![1]),
            (1, vec![]),
        ];
        let order = deterministic_toposort(&nodes).unwrap();
        assert_eq!(order, vec![1, 2, 3, 4, 5, 6]);
        assert!(verify_all_dependencies_satisfied(&order, &nodes));
    }

    #[test]
    fn test_external_dependency_ignored() {
        let nodes = vec![(2, vec![1, 999]), (1, vec![])];
        let order = deterministic_toposort(&nodes).unwrap();
        assert_eq!(order, vec![1, 2]);
    }

    #[test]
    fn test_three_way_cycle() {
        let nodes = vec![(1, vec![3]), (2, vec![1]), (3, vec![2])];
        let result = deterministic_toposort(&nodes);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.cycle_path.len(), 3);
    }

    #[test]
    fn test_linear_chain() {
        let nodes = vec![
            (5, vec![4]),
            (4, vec![3]),
            (3, vec![2]),
            (2, vec![1]),
            (1, vec![]),
        ];
        let order = deterministic_toposort(&nodes).unwrap();
        assert_eq!(order, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_diamond_dag() {
        let nodes = vec![(4, vec![2, 3]), (3, vec![1]), (2, vec![1]), (1, vec![])];
        let order = deterministic_toposort(&nodes).unwrap();
        assert_eq!(order[0], 1);
        assert_eq!(order[3], 4);
        assert!(verify_all_dependencies_satisfied(&order, &nodes));
    }
}
