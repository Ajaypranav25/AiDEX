//! Workspace and vault methods record their typed events in the drive log, scoped
//! to the workspace (or drive-wide for the vault), and never carry secret values.

mod common;

use aidex_core::EntityId;
use aidex_events::{Cursor, EventFilter, EventKind, EventScope};
use aidex_workspace::{EphemeralCreated, Workspace};
use aidexd::{
    Client,
    methods::{
        events::{REPLAY, ReplayPage},
        vault, workspace,
    },
};
use common::{options, start};
use serde_json::json;
use std::fs;

const PASSPHRASE: &str = "correct horse battery staple";
const VALUE: &str = "PLAINTEXT-MARKER-VALUE-77cd";

fn replay(client: &mut Client, filter: EventFilter) -> ReplayPage {
    client
        .call_as(REPLAY, json!({"after": Cursor::ZERO, "filter": filter}))
        .unwrap()
}

#[test]
fn workspace_methods_emit_workspace_scoped_events() {
    let root = common::temp_drive();
    let project = tempfile::tempdir().unwrap();
    fs::write(project.path().join("a.txt"), "a\n").unwrap();
    let daemon = start(root.path(), options(), |_| {});
    let mut client = Client::connect(root.path(), "test-domain-events").unwrap();
    let host: Workspace = client
        .call_as(
            workspace::ADD,
            json!({"path": project.path().to_str().unwrap(), "name": "proj"}),
        )
        .unwrap();
    let drive: Workspace = client
        .call_as(workspace::CREATE_DRIVE, json!({"name": "scratch"}))
        .unwrap();
    client
        .call(workspace::SELECT, json!({"id_or_name": "proj"}))
        .unwrap();
    let created: EphemeralCreated = client
        .call_as(workspace::EPHEMERAL_CREATE, json!({"source": "proj"}))
        .unwrap();
    let ephemeral = created.workspace.id.clone();
    fs::write(
        std::path::PathBuf::from(&created.workspace.path).join("a.txt"),
        "changed\n",
    )
    .unwrap();
    client
        .call(
            workspace::EPHEMERAL_APPLY,
            json!({"id": ephemeral.as_str()}),
        )
        .unwrap();
    client
        .call(
            workspace::EPHEMERAL_DISCARD,
            json!({"id": ephemeral.as_str()}),
        )
        .unwrap();
    client
        .call(workspace::REMOVE, json!({"id": drive.id.as_str()}))
        .unwrap();

    let page = replay(&mut client, EventFilter::all().with_kind("workspace"));
    let events: Vec<(EventScope, EventKind)> = page
        .events
        .into_iter()
        .map(|envelope| (envelope.scope, envelope.event))
        .collect();
    let scope = |id: &EntityId| EventScope::workspace(id.clone());
    assert_eq!(
        events,
        [
            (
                scope(&host.id),
                EventKind::WorkspaceAdded {
                    workspace_id: host.id.clone(),
                    kind: aidex_events::WorkspaceKind::Host,
                    name: "proj".into(),
                }
            ),
            (
                scope(&drive.id),
                EventKind::WorkspaceAdded {
                    workspace_id: drive.id.clone(),
                    kind: aidex_events::WorkspaceKind::Drive,
                    name: "scratch".into(),
                }
            ),
            (
                scope(&host.id),
                EventKind::WorkspaceSelected {
                    workspace_id: host.id.clone()
                }
            ),
            (
                scope(&ephemeral),
                EventKind::WorkspaceEphemeralCreated {
                    workspace_id: ephemeral.clone(),
                    source_workspace_id: host.id.clone(),
                }
            ),
            (
                scope(&ephemeral),
                EventKind::WorkspaceEphemeralApplied {
                    workspace_id: ephemeral.clone(),
                    files_changed: 1,
                }
            ),
            (
                scope(&ephemeral),
                EventKind::WorkspaceEphemeralDiscarded {
                    workspace_id: ephemeral.clone()
                }
            ),
            (
                scope(&drive.id),
                EventKind::WorkspaceRemoved {
                    workspace_id: drive.id.clone()
                }
            ),
        ]
    );
    let only_host = replay(&mut client, EventFilter::for_workspace(host.id.clone()));
    assert_eq!(only_host.events.len(), 2);
    daemon.stop();
    daemon.wait().unwrap();
}

#[test]
fn vault_methods_emit_drive_wide_audit_events_without_secrets() {
    let root = common::temp_drive();
    let daemon = start(root.path(), options(), |daemon| {
        daemon
            .services()
            .vault()
            .set_kdf_params(aidex_vault::KdfParams::new(8, 1, 1).unwrap());
    });
    let mut client = Client::connect(root.path(), "test-domain-events").unwrap();
    let init: vault::InitResult = client
        .call_as(vault::INIT, json!({"passphrase": PASSPHRASE}))
        .unwrap();
    client
        .call(
            vault::SECRET_SET,
            json!({"name": "openai", "value": VALUE, "consumers": ["provider:openai"]}),
        )
        .unwrap();
    client.call(vault::LOCK, json!({})).unwrap();
    client.call(vault::LOCK, json!({})).unwrap();
    client
        .call(vault::UNLOCK, json!({"passphrase": PASSPHRASE}))
        .unwrap();

    let page = replay(&mut client, EventFilter::all().with_kind("vault"));
    let vault_id = EntityId::new(init.vault_id).unwrap();
    let events: Vec<(EventScope, EventKind)> = page
        .events
        .into_iter()
        .map(|envelope| (envelope.scope, envelope.event))
        .collect();
    assert_eq!(
        events,
        [
            (
                EventScope::global(),
                EventKind::VaultUnlocked {
                    vault_id: vault_id.clone()
                }
            ),
            (
                EventScope::global(),
                EventKind::VaultLocked {
                    vault_id: vault_id.clone()
                }
            ),
            (EventScope::global(), EventKind::VaultUnlocked { vault_id }),
        ],
        "locking an already locked vault records nothing"
    );
    let everything = replay(&mut client, EventFilter::all());
    let text = serde_json::to_string(&everything).unwrap();
    assert!(!text.contains(VALUE));
    assert!(!text.contains(PASSPHRASE));
    daemon.stop();
    daemon.wait().unwrap();
}
