//! Subscriptions: history then live, reconnect exactness, bounded backpressure.

mod common;

use aidex_events::{
    Cursor, EventEnvelope, EventError, EventFilter, EventKind, RetentionPolicy, Subscription,
    subscription::MAX_QUEUED_BYTES,
};
use common::{id, session_scope, task_started, temp_log, ws};
use std::{
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

const PATIENCE: Duration = Duration::from_secs(10);

fn next(subscription: &mut Subscription) -> EventEnvelope {
    subscription
        .next_timeout(PATIENCE)
        .unwrap()
        .expect("an event within the timeout")
}

fn drain(subscription: &mut Subscription) -> Vec<u64> {
    let mut seen = Vec::new();
    while let Some(event) = subscription
        .next_timeout(Duration::from_millis(50))
        .unwrap()
    {
        seen.push(event.cursor.get());
    }
    seen
}

#[test]
fn subscription_delivers_history_then_live_events_in_order() {
    let fixture = temp_log(RetentionPolicy::UNBOUNDED);
    for index in 1..=600 {
        fixture
            .log
            .append(session_scope(), task_started(&format!("t{index}")))
            .unwrap();
    }
    let mut subscription = fixture
        .log
        .subscribe(EventFilter::all(), Cursor::new(100))
        .unwrap();
    let writer = {
        let log = fixture.log.clone();
        thread::spawn(move || {
            for index in 601..=700 {
                log.append(session_scope(), task_started(&format!("t{index}")))
                    .unwrap();
            }
        })
    };
    let mut seen = Vec::new();
    while seen.len() < 600 {
        seen.push(next(&mut subscription).cursor.get());
    }
    writer.join().unwrap();
    assert_eq!(seen, (101..=700).collect::<Vec<_>>());
    assert_eq!(subscription.try_next().unwrap(), None);
}

#[test]
fn reconnecting_after_the_last_acknowledged_cursor_has_no_duplicates_or_gaps() {
    let fixture = temp_log(RetentionPolicy::UNBOUNDED);
    for index in 1..=5 {
        fixture
            .log
            .append(session_scope(), task_started(&format!("t{index}")))
            .unwrap();
    }
    let mut first = fixture
        .log
        .subscribe(EventFilter::all(), Cursor::ZERO)
        .unwrap();
    let mut acknowledged = Cursor::ZERO;
    for _ in 0..3 {
        acknowledged = next(&mut first).cursor;
    }
    drop(first);
    assert_eq!(fixture.log.subscriber_count(), 0);
    // Events keep arriving while the consumer is away.
    for index in 6..=8 {
        fixture
            .log
            .append(session_scope(), task_started(&format!("t{index}")))
            .unwrap();
    }
    let mut second = fixture
        .log
        .subscribe(EventFilter::all(), acknowledged)
        .unwrap();
    fixture
        .log
        .append(session_scope(), task_started("t9"))
        .unwrap();
    assert_eq!(drain(&mut second), [4, 5, 6, 7, 8, 9]);
}

#[test]
fn a_subscriber_that_never_reads_cannot_block_appends_or_grow_without_bound() {
    let fixture = temp_log(RetentionPolicy::UNBOUNDED);
    let mut stalled = fixture
        .log
        .subscribe_with_capacity(EventFilter::all(), Cursor::ZERO, 8)
        .unwrap();
    // Become live: catch up on the (empty) history.
    assert_eq!(stalled.try_next().unwrap(), None);
    let started = Instant::now();
    for index in 1..=2000 {
        fixture
            .log
            .append(session_scope(), task_started(&format!("t{index}")))
            .unwrap();
        assert!(stalled.queued() <= 8, "queue grew to {}", stalled.queued());
    }
    assert!(started.elapsed() < Duration::from_secs(60));
    assert!(stalled.lag_count() >= 1);
    // It catches up from disk, in order, without duplicates or gaps.
    assert_eq!(drain(&mut stalled), (1..=2000).collect::<Vec<_>>());
}

#[test]
fn a_lagging_subscriber_rejoins_the_live_stream_while_writers_continue() {
    let fixture = temp_log(RetentionPolicy::UNBOUNDED);
    let log = Arc::new(fixture.log.clone());
    let mut subscription = fixture
        .log
        .subscribe_with_capacity(EventFilter::all(), Cursor::ZERO, 4)
        .unwrap();
    let writer = {
        let log = Arc::clone(&log);
        thread::spawn(move || {
            for index in 1..=1500 {
                log.append(session_scope(), task_started(&format!("t{index}")))
                    .unwrap();
                if index % 100 == 0 {
                    thread::sleep(Duration::from_millis(2));
                }
            }
        })
    };
    let mut seen = Vec::new();
    while seen.len() < 1500 {
        let event = next(&mut subscription);
        seen.push(event.cursor.get());
        if seen.len() % 97 == 0 {
            thread::sleep(Duration::from_millis(3));
        }
    }
    writer.join().unwrap();
    assert_eq!(seen, (1..=1500).collect::<Vec<_>>());
}

#[test]
fn the_live_queue_is_also_bounded_by_bytes() {
    let fixture = temp_log(RetentionPolicy::UNBOUNDED);
    let mut stalled = fixture
        .log
        .subscribe_with_capacity(EventFilter::all(), Cursor::ZERO, 16 * 1024)
        .unwrap();
    assert_eq!(stalled.try_next().unwrap(), None);
    let big = "y".repeat(15 * 1024);
    let count = MAX_QUEUED_BYTES / big.len() + 50;
    for _ in 0..count {
        fixture
            .log
            .append(
                session_scope(),
                EventKind::TaskFailed {
                    task_id: id("t"),
                    reason: big.clone(),
                },
            )
            .unwrap();
    }
    assert!(stalled.queued() * big.len() <= MAX_QUEUED_BYTES);
    assert!(stalled.lag_count() >= 1);
    assert_eq!(drain(&mut stalled).len(), count);
}

#[test]
fn filtered_subscriptions_and_ephemeral_events() {
    let fixture = temp_log(RetentionPolicy::UNBOUNDED);
    let log = &fixture.log;
    let mut tasks = log
        .subscribe(
            EventFilter::for_workspace(ws()).with_kind("task"),
            Cursor::ZERO,
        )
        .unwrap();
    let mut everything = log.subscribe(EventFilter::all(), Cursor::ZERO).unwrap();
    log.append(session_scope(), task_started("a")).unwrap(); // 1
    log.append(
        aidex_events::EventScope::workspace(id("ws_other")),
        task_started("b"),
    )
    .unwrap(); // 2
    log.append(
        session_scope(),
        EventKind::AgentDelta {
            text: "partial".into(),
        },
    )
    .unwrap();
    log.append(
        session_scope(),
        EventKind::TaskCompleted { task_id: id("a") },
    )
    .unwrap(); // 3
    assert_eq!(drain(&mut tasks), [1, 3]);
    let names: Vec<_> =
        std::iter::from_fn(|| everything.next_timeout(Duration::from_millis(50)).unwrap())
            .map(|event| (event.cursor.get(), event.name()))
            .collect();
    assert_eq!(
        names,
        [
            (1, "task.started"),
            (2, "task.started"),
            (2, "agent.delta"),
            (3, "task.completed")
        ]
    );
    // A late subscriber never sees ephemeral events again.
    let mut late = log.subscribe(EventFilter::all(), Cursor::ZERO).unwrap();
    assert_eq!(drain(&mut late), [1, 2, 3]);
}

#[test]
fn dropping_subscriptions_releases_their_registration() {
    let fixture = temp_log(RetentionPolicy::UNBOUNDED);
    let first = fixture
        .log
        .subscribe(EventFilter::all(), Cursor::ZERO)
        .unwrap();
    let second = fixture
        .log
        .subscribe(EventFilter::all(), Cursor::ZERO)
        .unwrap();
    assert_eq!(fixture.log.subscriber_count(), 2);
    drop(first);
    assert_eq!(fixture.log.subscriber_count(), 1);
    drop(second);
    assert_eq!(fixture.log.subscriber_count(), 0);
    fixture
        .log
        .append(session_scope(), task_started("a"))
        .unwrap();
}

#[test]
fn subscription_reports_history_removed_before_it_was_delivered() {
    let fixture = temp_log(RetentionPolicy::max_events(5));
    let mut subscription = fixture
        .log
        .subscribe_with_capacity(EventFilter::all(), Cursor::ZERO, 2)
        .unwrap();
    assert_eq!(subscription.try_next().unwrap(), None);
    for index in 1..=20 {
        fixture
            .log
            .append(session_scope(), task_started(&format!("t{index}")))
            .unwrap();
    }
    assert_eq!(
        subscription.next_timeout(PATIENCE),
        Err(EventError::CursorExpired {
            requested: Cursor::ZERO,
            oldest_available: Cursor::new(16)
        })
    );
    assert!(matches!(
        fixture
            .log
            .subscribe_with_capacity(EventFilter::all(), Cursor::ZERO, 0),
        Err(EventError::InvalidLimit { .. })
    ));
}
