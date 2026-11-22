//! The store, exercised through its public surface.

use super::*;

async fn add(store: &StateStore, pane_id: &str, project: &str, agent: &str) {
    store
        .add_agent(NewAgent {
            pane_id: pane_id.to_string(),
            workspace_id: "w1".to_string(),
            project: project.to_string(),
            agent: agent.to_string(),
            cwd: "C:\\p".to_string(),
            label: None,
            restored: false,
            last_active_ms: None,
        })
        .await;
}

async fn last_active(store: &StateStore, pane_id: &str) -> Option<u64> {
    store
        .row_of(pane_id)
        .await
        .and_then(|row| row.last_active_ms)
}

#[tokio::test]
async fn a_new_agent_has_its_last_movement_and_a_status_change_moves_it() {
    let store = StateStore::new("test");
    add(&store, "w1:p1", "p", "fake").await;
    let created = last_active(&store, "w1:p1")
        .await
        .expect("known at creation");
    assert!(created > 0);

    tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    store.set_status("w1:p1", Status::Working).await;
    let changed = last_active(&store, "w1:p1")
        .await
        .expect("known after change");
    assert!(changed > created, "a status change must move the timestamp");
}

#[tokio::test]
async fn a_restored_agent_keeps_the_snapshot_value_or_stays_unknown() {
    let store = StateStore::new("test");
    for (pane_id, value) in [("w1:p1", Some(1_793_880_000_000)), ("w1:p2", None)] {
        store
            .add_agent(NewAgent {
                pane_id: pane_id.to_string(),
                workspace_id: "w1".to_string(),
                project: "p".to_string(),
                agent: "fake".to_string(),
                cwd: "C:\\p".to_string(),
                label: None,
                restored: true,
                last_active_ms: value,
            })
            .await;
    }
    assert_eq!(last_active(&store, "w1:p1").await, Some(1_793_880_000_000));
    assert_eq!(last_active(&store, "w1:p2").await, None);
}

#[tokio::test]
async fn ranking_counts_and_done_derivation() {
    let store = StateStore::new("test");
    add(&store, "w1:p1", "proyecto-b", "opencode").await;
    add(&store, "w1:p2", "proyecto-a", "claude").await;

    store.set_status("w1:p1", Status::Working).await;
    store.set_status("w1:p2", Status::Blocked).await;

    let payload = store.payload().await;
    assert_eq!(payload.counts.get("blocked"), Some(&1));
    assert_eq!(payload.counts.get("working"), Some(&1));
    assert_eq!(payload.agents[0].pane_id, "w1:p2"); // blocked first

    // Finish p1 without the operator seeing it: idle + !seen -> done.
    store.set_status("w1:p1", Status::Idle).await;
    let payload = store.payload().await;
    let p1 = payload
        .agents
        .iter()
        .find(|row| row.pane_id == "w1:p1")
        .unwrap();
    assert_eq!(p1.status, Status::Done);
    // Blocked still outranks done; the finished pane comes second.
    assert_eq!(payload.agents[0].pane_id, "w1:p2");
    assert_eq!(payload.agents[1].pane_id, "w1:p1");

    // Seen clears the badge.
    assert!(store.mark_seen("w1:p1").await);
    assert_eq!(store.status_of("w1:p1").await, Some(Status::Idle));
    let payload = store.payload().await;
    assert_eq!(payload.agents[0].pane_id, "w1:p2"); // only blocked left on top
}

#[tokio::test]
async fn idle_without_work_is_not_done() {
    let store = StateStore::new("test");
    add(&store, "w1:p1", "p", "claude").await;
    store.set_status("w1:p1", Status::Idle).await;
    assert_eq!(store.status_of("w1:p1").await, Some(Status::Idle));
}

#[tokio::test]
async fn a_removed_pane_leaves_the_state_list() {
    let store = StateStore::new("test");
    add(&store, "w1:p1", "p", "fake").await;
    store.set_status("w1:p1", Status::Working).await;
    assert!(store.remove_agent("w1:p1").await);
    assert!(store.payload().await.agents.is_empty());
    assert!(!store.agent_exists("w1:p1").await);
    assert!(store.row_of("w1:p1").await.is_none());
}

#[tokio::test]
async fn revision_and_seq_are_monotonic() {
    let store = StateStore::new("test");
    let (_payload, mut rx) = store.subscribe().await;

    add(&store, "w1:p1", "p", "fake").await;
    store.set_status("w1:p1", Status::Working).await;
    store.set_status("w1:p1", Status::Blocked).await;
    store.set_status("w1:p1", Status::Idle).await;

    let mut revisions = Vec::new();
    let mut seqs = Vec::new();
    while let Ok(frame) = rx.try_recv() {
        let revision = frame.body["revision"].as_u64().unwrap();
        revisions.push(revision);
        if let Some(seq) = frame.body.get("state_change_seq").and_then(|v| v.as_u64()) {
            seqs.push(seq);
        }
    }
    assert!(revisions.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(seqs.windows(2).all(|pair| pair[0] < pair[1]));
    assert_eq!(seqs.len(), 4); // create + working + blocked + idle
}

#[tokio::test]
async fn counters_continue_after_a_restart() {
    let dir = excubia_test_dir::TestDir::new("state-tests-restart");
    let path = dir.join("counters.json");

    let first = StateStore::open("test", Some(path.clone()));
    add(&first, "w1:p1", "p", "fake").await;
    first.set_status("w1:p1", Status::Working).await;
    let revision = first.revision().await;
    assert!(revision >= 2);

    let second = StateStore::open("test", Some(path));
    assert!(
        second.revision().await >= revision,
        "a restart must not rewind the revision"
    );
    assert!(
        second.payload().await.agents.is_empty(),
        "agents are processes"
    );
}
