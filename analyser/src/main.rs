use std::collections::HashMap;

/// Result of the vertex marking algorithm
#[derive(Debug, Clone)]
pub enum MarkingResult<V: Clone> {
    /// Successfully found vertex labels
    Labels(HashMap<V, u64>),
    /// Found a positive cycle (contains at least one non-zero edge)
    PositiveCycle(Vec<V>),
}

/// Directed edge with weight
#[derive(Debug, Clone)]
pub struct Edge<V> {
    pub source: V,
    pub target: V,
    pub weight: u64,
}

impl<V> Edge<V> {
    pub fn new(source: V, target: V, weight: u64) -> Self {
        Edge { source, target, weight }
    }
}

/// Solves the vertex marking problem for a directed graph.
///
/// Given a digraph with natural number edge weights, finds either:
/// - Natural number labels for vertices such that for every edge (u, v):
///   label(u) - label(v) >= weight(u, v)
/// - A cycle with at least one non-zero weighted edge (proving impossibility)
///
/// # Arguments
/// * `vertices` - Slice of all vertex identifiers
/// * `edges` - Slice of directed edges with weights
///
/// # Returns
/// * `MarkingResult::Labels` - HashMap of vertex labels if solution exists
/// * `MarkingResult::PositiveCycle` - Vector of vertices forming the cycle if impossible
pub fn solve_vertex_marking<V>(vertices: &[V], edges: &[Edge<V>]) -> MarkingResult<V>
where
    V: Clone + Eq + std::hash::Hash,
{
    let n = vertices.len();
    
    // Create index mapping for efficient lookup
    let vertex_index: HashMap<&V, usize> = vertices
        .iter()
        .enumerate()
        .map(|(i, v)| (v, i))
        .collect();
    
    // Distance represents the longest path (maximum label decrease) from virtual source
    // Using i64 to handle potential large sums, though we check for cycles
    let mut dist: Vec<i64> = vec![0; n];
    let mut parent: Vec<Option<usize>> = vec![None; n];
    
    // Bellman-Ford: relax edges n times
    // n-1 iterations for shortest paths, 1 more to detect positive cycles
    for iteration in 0..n {
        let mut updated = false;
        
        for edge in edges {
            let u_idx = vertex_index[&edge.source];
            let v_idx = vertex_index[&edge.target];
            let w = edge.weight as i64;
            
            // Constraint: label[u] - label[v] >= w
            // Rewritten for longest path: dist[v] >= dist[u] + w
            if dist[u_idx].saturating_add(w) > dist[v_idx] {
                dist[v_idx] = dist[u_idx].saturating_add(w);
                parent[v_idx] = Some(u_idx);
                updated = true;
                
                // Update on n-th iteration means positive cycle exists
                if iteration == n - 1 {
                    let cycle = extract_positive_cycle(v_idx, &parent, vertices);
                    return MarkingResult::PositiveCycle(cycle);
                }
            }
        }
        
        // Early termination if no updates
        if !updated {
            break;
        }
    }
    
    // Compute labels: label[v] = max_dist - dist[v] + 1
    // This ensures all labels are natural numbers (>= 1)
    let max_dist = dist.iter().copied().max().unwrap_or(0);
    
    let labels: HashMap<V, u64> = vertices
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let label = (max_dist - dist[i] + 1) as u64;
            (v.clone(), label)
        })
        .collect();
    
    MarkingResult::Labels(labels)
}

/// Extracts a positive cycle from the parent pointers
fn extract_positive_cycle<V: Clone>(
    start_idx: usize,
    parent: &[Option<usize>],
    vertices: &[V],
) -> Vec<V> {
    let n = parent.len();
    
    // Walk back n steps to ensure we're inside the cycle
    let mut v = start_idx;
    for _ in 0..n {
        if let Some(p) = parent[v] {
            v = p;
        }
    }
    
    // Extract the cycle starting from v
    let cycle_start = v;
    let mut cycle = vec![vertices[v].clone()];
    
    if let Some(mut current) = parent[v] {
        while current != cycle_start {
            cycle.push(vertices[current].clone());
            current = parent[current].unwrap();
        }
    }
    
    // Add the starting vertex to close the cycle
    cycle.push(vertices[cycle_start].clone());
    cycle.reverse();
    
    cycle
}

/// Verifies that a labeling satisfies all edge constraints
pub fn verify_solution<V>(labels: &HashMap<V, u64>, edges: &[Edge<V>]) -> bool
where
    V: Eq + std::hash::Hash,
{
    edges.iter().all(|edge| {
        let u_label = labels.get(&edge.source).copied().unwrap_or(0);
        let v_label = labels.get(&edge.target).copied().unwrap_or(0);
        u_label >= v_label + edge.weight
    })
}

// ============================================================================
// Tests and Examples
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_chain() {
        // A -> B -> C with weights 2 and 1
        // Should find labels like A=4, B=2, C=1
        let vertices = vec!["A", "B", "C"];
        let edges = vec![
            Edge::new("A", "B", 2),
            Edge::new("B", "C", 1),
        ];

        match solve_vertex_marking(&vertices, &edges) {
            MarkingResult::Labels(labels) => {
                assert!(verify_solution(&labels, &edges));
                println!("Chain solution: {:?}", labels);
            }
            MarkingResult::PositiveCycle(_) => {
                panic!("Should have found a solution");
            }
        }
    }

    #[test]
    fn test_positive_cycle() {
        // A -> B -> C -> A with weights 2, 1, 0
        // Total cycle weight = 3 > 0, so impossible
        let vertices = vec!["A", "B", "C"];
        let edges = vec![
            Edge::new("A", "B", 2),
            Edge::new("B", "C", 1),
            Edge::new("C", "A", 0),
        ];

        match solve_vertex_marking(&vertices, &edges) {
            MarkingResult::Labels(_) => {
                panic!("Should have detected positive cycle");
            }
            MarkingResult::PositiveCycle(cycle) => {
                println!("Detected positive cycle: {:?}", cycle);
                assert!(cycle.len() >= 2);
            }
        }
    }

    #[test]
    fn test_zero_weight_cycle() {
        // A -> B -> C -> A with all zero weights
        // Total cycle weight = 0, so solution exists (all same label)
        let vertices = vec!["A", "B", "C"];
        let edges = vec![
            Edge::new("A", "B", 0),
            Edge::new("B", "C", 0),
            Edge::new("C", "A", 0),
        ];

        match solve_vertex_marking(&vertices, &edges) {
            MarkingResult::Labels(labels) => {
                assert!(verify_solution(&labels, &edges));
                println!("Zero cycle solution: {:?}", labels);
            }
            MarkingResult::PositiveCycle(_) => {
                panic!("Should have found a solution for zero-weight cycle");
            }
        }
    }

    #[test]
    fn test_complex_graph() {
        // More complex graph with multiple paths
        let vertices = vec![1, 2, 3, 4, 5];
        let edges = vec![
            Edge::new(1, 2, 3),
            Edge::new(1, 3, 1),
            Edge::new(2, 4, 2),
            Edge::new(3, 4, 4),
            Edge::new(4, 5, 1),
        ];

        match solve_vertex_marking(&vertices, &edges) {
            MarkingResult::Labels(labels) => {
                assert!(verify_solution(&labels, &edges));
                println!("Complex graph solution: {:?}", labels);
            }
            MarkingResult::PositiveCycle(cycle) => {
                panic!("Should have found a solution, got cycle: {:?}", cycle);
            }
        }
    }

    #[test]
    fn test_disconnected_components() {
        // Two disconnected components
        let vertices = vec!["A", "B", "C", "D"];
        let edges = vec![
            Edge::new("A", "B", 2),
            Edge::new("C", "D", 3),
        ];

        match solve_vertex_marking(&vertices, &edges) {
            MarkingResult::Labels(labels) => {
                assert!(verify_solution(&labels, &edges));
                println!("Disconnected solution: {:?}", labels);
            }
            MarkingResult::PositiveCycle(_) => {
                panic!("Should have found a solution");
            }
        }
    }
}

// ============================================================================
// Example main function
// ============================================================================

fn main() {
    println!("=== Vertex Marking Algorithm Demo ===\n");

    // Example 1: Solvable graph
    println!("Example 1: Simple DAG");
    let vertices = vec!["Start", "Middle", "End"];
    let edges = vec![
        Edge::new("Start", "Middle", 5),
        Edge::new("Middle", "End", 3),
    ];

    match solve_vertex_marking(&vertices, &edges) {
        MarkingResult::Labels(labels) => {
            println!("  Solution found:");
            for (v, label) in &labels {
                println!("    {} -> {}", v, label);
            }
            println!("  Verified: {}", verify_solution(&labels, &edges));
        }
        MarkingResult::PositiveCycle(cycle) => {
            println!("  Impossible! Positive cycle: {:?}", cycle);
        }
    }

    // Example 2: Impossible (positive cycle)
    println!("\nExample 2: Graph with positive cycle");
    let vertices = vec!["A", "B", "C"];
    let edges = vec![
        Edge::new("A", "B", 2),
        Edge::new("B", "C", 2),
        Edge::new("C", "A", 1),
    ];

    match solve_vertex_marking(&vertices, &edges) {
        MarkingResult::Labels(labels) => {
            println!("  Solution found: {:?}", labels);
        }
        MarkingResult::PositiveCycle(cycle) => {
            println!("  Impossible! Positive cycle detected:");
            println!("    Cycle: {:?}", cycle);
            
            // Calculate total cycle weight
            let total_weight: u64 = cycle.windows(2)
                .filter_map(|w| {
                    edges.iter()
                        .find(|e| e.source == w[0] && e.target == w[1])
                        .map(|e| e.weight)
                })
                .sum();
            println!("    Total cycle weight: {} > 0", total_weight);
        }
    }

    // Example 3: Zero-weight cycle (solvable)
    println!("\nExample 3: Graph with zero-weight cycle");
    let vertices = vec![1, 2, 3];
    let edges = vec![
        Edge::new(1, 2, 0),
        Edge::new(2, 3, 0),
        Edge::new(3, 1, 0),
    ];

    match solve_vertex_marking(&vertices, &edges) {
        MarkingResult::Labels(labels) => {
            println!("  Solution found:");
            for (v, label) in &labels {
                println!("    {} -> {}", v, label);
            }
            println!("  Verified: {}", verify_solution(&labels, &edges));
        }
        MarkingResult::PositiveCycle(cycle) => {
            println!("  Impossible! Positive cycle: {:?}", cycle);
        }
    }
}