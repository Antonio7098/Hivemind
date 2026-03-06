use super::*;
use crate::core::graph::{GraphTask, SuccessCriteria, TaskGraph};

fn setup_test() -> (TaskGraph, TaskFlow) {
    let project_id = Uuid::new_v4();
    let mut graph = TaskGraph::new(project_id, "test");

    let t1 = graph
        .add_task(GraphTask::new("Task 1", SuccessCriteria::new("Done")))
        .unwrap();
    let t2 = graph
        .add_task(GraphTask::new("Task 2", SuccessCriteria::new("Done")))
        .unwrap();
    let t3 = graph
        .add_task(GraphTask::new("Task 3", SuccessCriteria::new("Done")))
        .unwrap();

    graph.add_dependency(t2, t1).unwrap();
    graph.add_dependency(t3, t2).unwrap();

    let task_ids: Vec<_> = graph.tasks.keys().copied().collect();
    let flow = TaskFlow::new(graph.id, project_id, &task_ids);

    (graph, flow)
}

#[test]
fn initial_readiness() {
    let (graph, mut flow) = setup_test();
    let mut scheduler = Scheduler::new(&graph, &mut flow);

    let ready = scheduler.update_readiness();
    assert_eq!(ready.len(), 1);

    let root_tasks = graph.root_tasks();
    assert!(ready.iter().all(|id| root_tasks.contains(id)));
}

#[test]
fn dependency_chain_execution() {
    let (graph, mut flow) = setup_test();
    let task_ids: Vec<_> = graph.topological_order();
    let t1 = task_ids[0];
    let t2 = task_ids[1];
    let t3 = task_ids[2];

    {
        let mut scheduler = Scheduler::new(&graph, &mut flow);
        let result = scheduler.schedule();
        assert!(matches!(result, ScheduleResult::Ready(ref tasks) if tasks.contains(&t1)));
        scheduler.start_task(t1).unwrap();
    }

    {
        let mut scheduler = Scheduler::new(&graph, &mut flow);
        let result = scheduler.schedule();
        assert!(matches!(result, ScheduleResult::Blocked));
        scheduler.complete_execution(t1).unwrap();
        scheduler.record_verification(t1, true, false).unwrap();
    }

    {
        let mut scheduler = Scheduler::new(&graph, &mut flow);
        let result = scheduler.schedule();
        assert!(matches!(result, ScheduleResult::Ready(ref tasks) if tasks.contains(&t2)));
        scheduler.start_task(t2).unwrap();
        scheduler.complete_execution(t2).unwrap();
        scheduler.record_verification(t2, true, false).unwrap();
    }

    {
        let mut scheduler = Scheduler::new(&graph, &mut flow);
        let result = scheduler.schedule();
        assert!(matches!(result, ScheduleResult::Ready(ref tasks) if tasks.contains(&t3)));
        scheduler.start_task(t3).unwrap();
        scheduler.complete_execution(t3).unwrap();
        scheduler.record_verification(t3, true, false).unwrap();
    }

    {
        let mut scheduler = Scheduler::new(&graph, &mut flow);
        let result = scheduler.schedule();
        assert!(matches!(result, ScheduleResult::Complete));
    }
}

#[test]
fn retry_cycle() {
    let (graph, mut flow) = setup_test();
    let t1 = graph.root_tasks()[0];

    {
        let mut scheduler = Scheduler::new(&graph, &mut flow);
        scheduler.update_readiness();
        scheduler.start_task(t1).unwrap();
        scheduler.complete_execution(t1).unwrap();
        scheduler.record_verification(t1, false, true).unwrap();
    }

    let exec = flow.get_task_execution(t1).unwrap();
    assert_eq!(exec.state, TaskExecState::Retry);

    {
        let mut scheduler = Scheduler::new(&graph, &mut flow);
        scheduler.start_task(t1).unwrap();
    }

    let exec = flow.get_task_execution(t1).unwrap();
    assert_eq!(exec.attempt_count, 2);
}

#[test]
fn failure_handling() {
    let (graph, mut flow) = setup_test();
    let t1 = graph.root_tasks()[0];

    let mut scheduler = Scheduler::new(&graph, &mut flow);
    scheduler.update_readiness();
    scheduler.start_task(t1).unwrap();
    scheduler.complete_execution(t1).unwrap();
    scheduler.record_verification(t1, false, false).unwrap();

    let result = scheduler.schedule();
    assert!(matches!(result, ScheduleResult::HasFailures(_)));
}

#[test]
fn escalation() {
    let (graph, mut flow) = setup_test();
    let t1 = graph.root_tasks()[0];

    let mut scheduler = Scheduler::new(&graph, &mut flow);
    scheduler.update_readiness();
    scheduler.start_task(t1).unwrap();
    scheduler.complete_execution(t1).unwrap();
    scheduler.record_verification(t1, false, false).unwrap();
    scheduler.escalate(t1).unwrap();

    let exec = flow.get_task_execution(t1).unwrap();
    assert_eq!(exec.state, TaskExecState::Escalated);
}

#[test]
fn attempt_tracking() {
    let task_id = Uuid::new_v4();
    let mut attempt = Attempt::new(task_id, 1);

    assert_eq!(attempt.attempt_number, 1);
    assert!(attempt.ended_at.is_none());
    assert!(attempt.outcome.is_none());

    attempt.complete(AttemptOutcome::Success);
    assert!(attempt.ended_at.is_some());
    assert_eq!(attempt.outcome, Some(AttemptOutcome::Success));
}
