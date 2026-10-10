//! Human-readable rendering of session results (`--json` prints the DTOs).

use aidex_sessions::{
    CheckpointMeta, CheckpointStatus, EventGap, MessagePage, MessageRole, Session, SessionState,
};
use aidexd::methods::sessions::{CheckpointList, ResumePlanView, SessionSummary};

pub fn state(state: &SessionState) -> String {
    match state {
        SessionState::Failed { code, message } if message.is_empty() => format!("failed ({code})"),
        SessionState::Failed { code, message } => format!("failed ({code}: {message})"),
        SessionState::Interrupted {
            reason,
            recoverable,
        } => {
            let recoverable = if *recoverable {
                "recoverable"
            } else {
                "no valid checkpoint"
            };
            format!("interrupted ({recoverable}): {reason}")
        }
        other => other.kind().as_str().to_owned(),
    }
}

pub fn session_details(session: &Session) {
    println!("State: {}", state(&session.state));
    println!("Workspace: {}", session.workspace_id);
    println!("Agent: {}", session.agent_id);
    if let Some(parent) = &session.parent_session_id {
        match &session.branched_from {
            Some(checkpoint) => println!("Branched from: {parent} at {checkpoint}"),
            None => println!("Parent session: {parent}"),
        }
    }
    if let Some(model) = &session.model {
        println!("Model: {}/{}", model.provider_id, model.model_id);
    }
    println!(
        "Host: {} {} ({})",
        session.host_fingerprint.os, session.host_fingerprint.arch, session.host_fingerprint.id
    );
    println!("Created: {}", session.created_at.as_str());
    println!("Updated: {}", session.updated_at.as_str());
}

pub fn session_table(sessions: &[Session]) {
    if sessions.is_empty() {
        println!("No sessions.");
        return;
    }
    for session in sessions {
        println!(
            "{}  {}  {}  {}  {}",
            session.id,
            session.kind().as_str(),
            session.workspace_id,
            session.updated_at.as_str(),
            session.title
        );
    }
}

fn gaps(gaps: &[EventGap]) {
    if gaps.is_empty() {
        return;
    }
    println!("Event gaps (history the event log does not have):");
    for gap in gaps {
        println!(
            "  {}  {}  {}",
            gap.detected_at.as_str(),
            gap.event.as_deref().unwrap_or("-"),
            gap.detail
        );
    }
}

fn checkpoint_line(meta: &CheckpointMeta) -> String {
    let status = match meta.status {
        CheckpointStatus::Valid => "valid".to_owned(),
        CheckpointStatus::Corrupt => format!(
            "CORRUPT ({})",
            meta.corrupt_reason
                .as_deref()
                .unwrap_or("failed verification")
        ),
    };
    format!(
        "#{} {}  {}  {}  after message {}  {}",
        meta.seq,
        meta.id,
        meta.created_at.as_str(),
        status,
        meta.message_seq,
        meta.label.as_deref().unwrap_or("")
    )
}

pub fn summary(summary: &SessionSummary) {
    let session = &summary.session;
    println!("Session: {} ({})", session.title, session.id);
    session_details(session);
    println!(
        "Records: {} messages, {} tool calls ({} open), {} artifacts, {} checkpoints, {} approvals",
        summary.messages,
        summary.tool_calls,
        summary.open_tool_calls,
        summary.artifacts,
        summary.checkpoints,
        summary.approvals
    );
    match &summary.latest_checkpoint {
        Some(meta) => println!("Latest checkpoint: {}", checkpoint_line(meta)),
        None => println!("Latest checkpoint: none"),
    }
    gaps(&summary.event_gaps);
}

fn role(role: MessageRole) -> &'static str {
    match role {
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
        MessageRole::System => "system",
        MessageRole::Tool => "tool",
    }
}

pub fn messages(page: &MessagePage) {
    if page.messages.is_empty() {
        println!("No messages after {}.", page.next_after);
    }
    for message in &page.messages {
        println!(
            "#{} {} {}",
            message.seq,
            role(message.role),
            message.created_at.as_str()
        );
        for line in message.content.lines() {
            println!("  {line}");
        }
    }
    if page.has_more {
        println!("More messages: pass --after {}", page.next_after);
    }
}

pub fn checkpoints(list: &CheckpointList) {
    if list.checkpoints.is_empty() {
        println!("No checkpoints.");
        return;
    }
    if list.total > list.checkpoints.len() {
        println!(
            "Showing the newest {} of {} checkpoints.",
            list.checkpoints.len(),
            list.total
        );
    }
    for meta in &list.checkpoints {
        println!("{}", checkpoint_line(meta));
    }
}

pub fn resume_plan(plan: &ResumePlanView) {
    let session = &plan.session;
    println!("Session: {} ({})", session.title, session.id);
    println!("State: {}", state(&session.state));
    match &plan.checkpoint {
        Some(meta) => {
            println!("Resume from: {}", checkpoint_line(meta));
            if plan.recovered_from_corruption {
                println!("  (the newest checkpoint is corrupt; an older valid one was chosen)");
            }
        }
        None => println!("Resume from: message history only (no valid checkpoint)"),
    }
    if !plan.corrupt_checkpoints.is_empty() {
        let ids: Vec<_> = plan
            .corrupt_checkpoints
            .iter()
            .map(|id| id.as_str())
            .collect();
        println!("Corrupt checkpoints: {}", ids.join(", "));
    }
    println!(
        "Messages since the checkpoint: {} (read with `aidex session messages {} --after {}`)",
        plan.messages_since_checkpoint, session.id, plan.messages_after
    );
    if plan.different_host {
        println!("Different host:");
        for change in &plan.host_changes {
            println!("  {change}");
        }
    }
    if !plan.unrecoverable.is_empty() {
        println!("Not recoverable (recreated or redone on resume):");
        for item in &plan.unrecoverable {
            println!("  - {}", item.detail);
        }
        if plan.unrecoverable_total > plan.unrecoverable.len() {
            println!(
                "  ... and {} more",
                plan.unrecoverable_total - plan.unrecoverable.len()
            );
        }
    }
    gaps(&plan.event_gaps);
    if plan.adopted {
        println!("Adopted on this host; the session is paused and ready to continue.");
    } else if let Some(reason) = &plan.blocked_reason {
        println!("Not resumable: {reason}");
    }
    println!("Continuation: {}", plan.continuation);
}
