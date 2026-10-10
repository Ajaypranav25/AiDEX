//! The `EventSink` contract shared by every implementation.

mod common;

use aidex_events::{
    Cursor, EventError, EventFilter, EventKind, EventScope, EventSink, MemoryEventSink,
    NullEventSink, RetentionPolicy,
};
use common::{id, session_scope, task_started, temp_log};
use std::sync::Arc;

fn emit_sequence(sink: &dyn EventSink) -> Vec<Cursor> {
    vec![
        sink.emit(session_scope(), task_started("a")).unwrap(),
        sink.emit(session_scope(), EventKind::AgentDelta { text: "x".into() })
            .unwrap(),
        sink.emit(session_scope(), task_started("b")).unwrap(),
    ]
}

#[test]
fn event_log_and_memory_sink_assign_the_same_cursors() {
    let fixture = temp_log(RetentionPolicy::UNBOUNDED);
    let expected = [Cursor::new(1), Cursor::new(1), Cursor::new(2)];
    assert_eq!(emit_sequence(&fixture.log), expected);
    let memory = MemoryEventSink::new();
    assert_eq!(emit_sequence(&memory), expected);
    assert_eq!(
        memory.names(),
        ["task.started", "agent.delta", "task.started"]
    );
    assert_eq!(memory.len(), 3);
    let taken = memory.take();
    assert_eq!(taken[2].cursor, Cursor::new(2));
    assert!(memory.is_empty());
    assert_eq!(
        fixture
            .log
            .replay(&EventFilter::all(), Cursor::ZERO, 10)
            .unwrap()
            .events
            .len(),
        2
    );
}

#[test]
fn every_sink_applies_the_log_validation_and_redaction() {
    let memory = Arc::new(MemoryEventSink::new());
    let sinks: [Arc<dyn EventSink>; 2] = [memory.clone(), Arc::new(NullEventSink)];
    for sink in &sinks {
        assert_eq!(
            sink.emit(
                EventScope {
                    workspace_id: None,
                    session_id: Some(id("ses_one")),
                },
                EventKind::SessionStarted {},
            )
            .map_err(|error| matches!(error, EventError::InvalidScope(_))),
            Err(true)
        );
        assert_eq!(
            sink.emit(
                session_scope(),
                EventKind::TaskStarted {
                    task_id: id(&format!("{}{}", "ghp_", "GHFAKE0fake1FAKE2fake3FAKE4fake5")),
                },
            ),
            Err(EventError::SensitiveIdentifier)
        );
        sink.emit(
            session_scope(),
            EventKind::SessionFailed {
                reason: "token=Zz9Yy8Xx7Ww6Vv5Uu4Tt3".into(),
            },
        )
        .unwrap();
    }
    let recorded = memory.events();
    assert_eq!(recorded.len(), 1);
    assert_eq!(
        recorded[0].event,
        EventKind::SessionFailed {
            reason: "token=[REDACTED]".into()
        }
    );
    // Shared references and Arcs are sinks too.
    let by_ref: &dyn EventSink = &*memory;
    assert_eq!(
        (&by_ref).emit(session_scope(), task_started("c")).unwrap(),
        Cursor::new(2)
    );
}
