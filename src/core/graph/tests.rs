use super::*;

fn test_graph() -> TaskGraph {
    TaskGraph::new(Uuid::new_v4(), "test-graph")
}

fn test_task(title: &str) -> GraphTask {
    GraphTask::new(title, SuccessCriteria::new("Task completed"))
}

#[test]
fn create_graph() {
    let graph = test_graph();
    assert_eq!(graph.state, GraphState::Draft);
    assert!(graph.tasks.is_empty());
}

#[test]
fn add_tasks() {
    let mut graph = test_graph();

    let t1 = graph.add_task(test_task("Task 1")).unwrap();
    let t2 = graph.add_task(test_task("Task 2")).unwrap();

    assert_eq!(graph.tasks.len(), 2);
    assert!(graph.tasks.contains_key(&t1));
    assert!(graph.tasks.contains_key(&t2));
}

#[test]
fn add_dependencies() {
    let mut graph = test_graph();

    let t1 = graph.add_task(test_task("Task 1")).unwrap();
    let t2 = graph.add_task(test_task("Task 2")).unwrap();

    graph.add_dependency(t2, t1).unwrap();

    assert!(graph.dependencies[&t2].contains(&t1));
}

#[test]
fn prevent_cycles() {
    let mut graph = test_graph();

    let t1 = graph.add_task(test_task("Task 1")).unwrap();
    let t2 = graph.add_task(test_task("Task 2")).unwrap();
    let t3 = graph.add_task(test_task("Task 3")).unwrap();

    graph.add_dependency(t2, t1).unwrap();
    graph.add_dependency(t3, t2).unwrap();

    let result = graph.add_dependency(t1, t3);
    assert_eq!(result, Err(GraphError::CycleDetected));
}

#[test]
fn prevent_self_dependency() {
    let mut graph = test_graph();
    let t1 = graph.add_task(test_task("Task 1")).unwrap();

    let result = graph.add_dependency(t1, t1);
    assert_eq!(result, Err(GraphError::CycleDetected));
}

#[test]
fn topological_order() {
    let mut graph = test_graph();

    let t1 = graph.add_task(test_task("Task 1")).unwrap();
    let t2 = graph.add_task(test_task("Task 2")).unwrap();
    let t3 = graph.add_task(test_task("Task 3")).unwrap();

    graph.add_dependency(t2, t1).unwrap();
    graph.add_dependency(t3, t2).unwrap();

    let order = graph.topological_order();
    let pos1 = order.iter().position(|&x| x == t1).unwrap();
    let pos2 = order.iter().position(|&x| x == t2).unwrap();
    let pos3 = order.iter().position(|&x| x == t3).unwrap();

    assert!(pos1 < pos2);
    assert!(pos2 < pos3);
}

#[test]
fn root_tasks() {
    let mut graph = test_graph();

    let t1 = graph.add_task(test_task("Task 1")).unwrap();
    let t2 = graph.add_task(test_task("Task 2")).unwrap();
    let t3 = graph.add_task(test_task("Task 3")).unwrap();

    graph.add_dependency(t2, t1).unwrap();
    graph.add_dependency(t3, t1).unwrap();

    let roots = graph.root_tasks();
    assert_eq!(roots.len(), 1);
    assert!(roots.contains(&t1));
}

#[test]
fn validate_and_lock() {
    let mut graph = test_graph();
    graph.add_task(test_task("Task 1")).unwrap();

    assert!(graph.validate().is_ok());
    assert_eq!(graph.state, GraphState::Validated);

    assert!(graph.lock().is_ok());
    assert_eq!(graph.state, GraphState::Locked);
}

#[test]
fn cannot_modify_locked_graph() {
    let mut graph = test_graph();
    graph.add_task(test_task("Task 1")).unwrap();
    graph.validate().unwrap();
    graph.lock().unwrap();

    let result = graph.add_task(test_task("Task 2"));
    assert_eq!(result, Err(GraphError::GraphLocked));
}

#[test]
fn cannot_validate_empty_graph() {
    let mut graph = test_graph();
    let result = graph.validate();
    assert_eq!(result, Err(GraphError::EmptyGraph));
}

#[test]
fn graph_serialization() {
    let mut graph = test_graph();
    let t1 = graph.add_task(test_task("Task 1")).unwrap();
    let t2 = graph.add_task(test_task("Task 2")).unwrap();
    graph.add_dependency(t2, t1).unwrap();

    let json = serde_json::to_string(&graph).unwrap();
    let restored: TaskGraph = serde_json::from_str(&json).unwrap();

    assert_eq!(graph.id, restored.id);
    assert_eq!(graph.tasks.len(), restored.tasks.len());
}

#[test]
fn dependents() {
    let mut graph = test_graph();

    let t1 = graph.add_task(test_task("Task 1")).unwrap();
    let t2 = graph.add_task(test_task("Task 2")).unwrap();
    let t3 = graph.add_task(test_task("Task 3")).unwrap();

    graph.add_dependency(t2, t1).unwrap();
    graph.add_dependency(t3, t1).unwrap();

    let dependents = graph.dependents(t1);
    assert_eq!(dependents.len(), 2);
    assert!(dependents.contains(&t2));
    assert!(dependents.contains(&t3));
}
