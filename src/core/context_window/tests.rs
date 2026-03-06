use super::*;
use proptest::prelude::*;

fn default_policy() -> ContextBudgetPolicy {
    ContextBudgetPolicy {
        total_budget_bytes: 64,
        default_section_budget_bytes: 32,
        per_section_budget_bytes: BTreeMap::new(),
        max_expand_depth: 2,
        deduplicate: true,
        truncation_policy: "ordered_section_then_total_budget".to_string(),
    }
}

fn actor() -> ContextOperationActor {
    ContextOperationActor {
        actor: "test".to_string(),
        runtime: None,
        tool: None,
    }
}

#[test]
fn prune_enforces_depth_dedup_and_budgets() {
    let mut window = ContextWindow::new(
        "w1",
        vec!["a".to_string(), "b".to_string()],
        default_policy(),
    );
    let _ = window.add_entry(
        ContextEntryCandidate {
            entry_id: "a0".to_string(),
            section: "a".to_string(),
            content: "header".to_string(),
            source: "s".to_string(),
            depth: 0,
        },
        "seed",
        actor(),
    );
    let _ = window.expand_entries(
        vec![
            ContextEntryCandidate {
                entry_id: "a1".to_string(),
                section: "a".to_string(),
                content: "aaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
                source: "s".to_string(),
                depth: 1,
            },
            ContextEntryCandidate {
                entry_id: "a2".to_string(),
                section: "a".to_string(),
                content: "bbbbbbbbbbbbbbbbbbbbbbbbb".to_string(),
                source: "s".to_string(),
                depth: 1,
            },
            ContextEntryCandidate {
                entry_id: "b3".to_string(),
                section: "b".to_string(),
                content: "ccccccccccccccccccccccccc".to_string(),
                source: "s".to_string(),
                depth: 1,
            },
        ],
        "expand",
        actor(),
        true,
    );
    let prune = window.prune("prune", actor());
    assert!(!prune.removed_ids.is_empty());
    assert!(prune
        .section_reasons
        .values()
        .flat_map(|v| v.iter())
        .any(|reason| reason == "section_budget_exceeded" || reason == "total_budget_exceeded"));
}

#[test]
fn snapshot_hash_stable_for_same_inputs() {
    let mut w1 = ContextWindow::new("w", vec!["s".to_string()], default_policy());
    let mut w2 = ContextWindow::new("w", vec!["s".to_string()], default_policy());
    for window in [&mut w1, &mut w2] {
        let _ = window.add_entry(
            ContextEntryCandidate {
                entry_id: "e1".to_string(),
                section: "s".to_string(),
                content: "content".to_string(),
                source: "x".to_string(),
                depth: 0,
            },
            "seed",
            actor(),
        );
        let _ = window.prune("prune", actor());
    }
    let (s1, _) = w1.snapshot("snapshot", actor());
    let (s2, _) = w2.snapshot("snapshot", actor());
    assert_eq!(s1.state_hash, s2.state_hash);
    assert_eq!(s1.rendered_prompt_hash, s2.rendered_prompt_hash);
}

#[test]
fn snapshot_matches_golden_shape() {
    let mut window = ContextWindow::new(
        "golden",
        vec!["s".to_string(), "t".to_string()],
        default_policy(),
    );
    let _ = window.add_entry(
        ContextEntryCandidate {
            entry_id: "s-1".to_string(),
            section: "s".to_string(),
            content: "S1".to_string(),
            source: "seed".to_string(),
            depth: 0,
        },
        "seed",
        actor(),
    );
    let _ = window.expand_entries(
        vec![
            ContextEntryCandidate {
                entry_id: "s-2".to_string(),
                section: "s".to_string(),
                content: "S2".to_string(),
                source: "expand".to_string(),
                depth: 1,
            },
            ContextEntryCandidate {
                entry_id: "t-1".to_string(),
                section: "t".to_string(),
                content: "T1".to_string(),
                source: "expand".to_string(),
                depth: 1,
            },
        ],
        "expand",
        actor(),
        true,
    );
    let _ = window.prune("prune", actor());
    let (snapshot, _) = window.snapshot("snapshot", actor());
    insta::assert_json_snapshot!(snapshot, @r###"
    {
      "window_id": "golden",
      "state_hash": "7352e41c13b3b73d73986042820404e9a2a5dad5bcad3edbe59338dc3ff0251b",
      "rendered_prompt_hash": "550e8dab8c77c70ec018005217639e92980d0299b32513e759b6fc9a8329e3c1",
      "total_size_bytes": 8,
      "sections": [
        {
          "section": "s",
          "content": "S1\n\nS2",
          "size_bytes": 6,
          "entry_ids": [
            "s-1",
            "s-2"
          ]
        },
        {
          "section": "t",
          "content": "T1",
          "size_bytes": 2,
          "entry_ids": [
            "t-1"
          ]
        }
      ],
      "rendered_prompt": "S1\n\nS2\n\nT1"
    }
    "###);
}

proptest! {
    #[test]
    fn expand_is_order_independent_for_hash(
        ids in prop::collection::vec("[a-z]{1,8}", 1..16)
    ) {
        let mut forward = ContextWindow::new("p", vec!["s".to_string()], default_policy());
        let mut backward = ContextWindow::new("p", vec!["s".to_string()], default_policy());

        let candidates_forward: Vec<ContextEntryCandidate> = ids.iter().map(|id| ContextEntryCandidate {
            entry_id: id.clone(),
            section: "s".to_string(),
            content: format!("content-{id}"),
            source: "seed".to_string(),
            depth: 1,
        }).collect();
        let candidates_backward: Vec<ContextEntryCandidate> = ids.iter().rev().map(|id| ContextEntryCandidate {
            entry_id: id.clone(),
            section: "s".to_string(),
            content: format!("content-{id}"),
            source: "seed".to_string(),
            depth: 1,
        }).collect();

        let _ = forward.expand_entries(candidates_forward, "expand", actor(), true);
        let _ = backward.expand_entries(candidates_backward, "expand", actor(), true);
        let _ = forward.prune("prune", actor());
        let _ = backward.prune("prune", actor());

        prop_assert_eq!(forward.state_hash(), backward.state_hash());
    }
}
