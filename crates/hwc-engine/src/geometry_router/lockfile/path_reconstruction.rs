use rustc_hash::FxHashMap;

pub(super) fn reconstruct_path_topology(
    mut segments: Vec<crate::space::LineSegment>,
) -> Vec<crate::space::LineSegment> {
    if segments.len() <= 1 {
        return segments;
    }

    let mut connections: FxHashMap<usize, Vec<usize>> = FxHashMap::default();

    for (i, seg_i) in segments.iter().enumerate() {
        for (j, seg_j) in segments.iter().enumerate() {
            if i == j {
                continue;
            }
            if seg_i.end == seg_j.start
                || seg_i.end == seg_j.end
                || seg_i.start == seg_j.start
                || seg_i.start == seg_j.end
            {
                connections.entry(i).or_default().push(j);
            }
        }
    }

    let start_idx = connections
        .iter()
        .find(|(_, neighbors)| neighbors.len() == 1)
        .map(|(idx, _)| *idx)
        .unwrap_or(0);

    let mut ordered = Vec::new();
    let mut visited = vec![false; segments.len()];
    let mut current = start_idx;

    while !visited[current] {
        visited[current] = true;
        ordered.push(current);

        if let Some(neighbors) = connections.get(&current) {
            if let Some(&next) = neighbors.iter().find(|&&n| !visited[n]) {
                current = next;
            } else {
                break;
            }
        } else {
            break;
        }
    }

    for (i, &vis) in visited.iter().enumerate() {
        if !vis {
            ordered.push(i);
        }
    }

    let original_segments = segments.clone();
    segments.clear();
    for &idx in &ordered {
        segments.push(original_segments[idx].clone());
    }

    segments
}
