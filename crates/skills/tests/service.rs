//! Enablement (persisted per workspace, with events) and drive skill authoring.

mod common;

use std::{fs, sync::Arc};

use aidex_events::{EventKind, ExtensionKind, MemoryEventSink};
use aidex_skills::{SkillDraft, SkillError, SkillId, SkillService, SkillStateFile};
use common::{drive, fake_token, skill, skill_text, workspace, write};

fn id(text: &str) -> SkillId {
    SkillId::parse(text).unwrap()
}

fn service(paths: &aidex_drive::DrivePaths) -> (SkillService, Arc<MemoryEventSink>) {
    let events = Arc::new(MemoryEventSink::new());
    (SkillService::new(paths.clone(), events.clone()), events)
}

fn draft(name: &str, description: &str, body: &str) -> SkillDraft {
    SkillDraft {
        name: name.into(),
        description: description.into(),
        body: body.into(),
        version: None,
    }
}

/// `(event name, extension id, workspace id)` of everything emitted so far.
fn emitted(events: &MemoryEventSink) -> Vec<(&'static str, String, Option<String>)> {
    events
        .take()
        .into_iter()
        .map(|envelope| {
            let workspace = envelope
                .scope
                .workspace_id
                .as_ref()
                .map(ToString::to_string);
            let name = envelope.name();
            match envelope.event {
                EventKind::ExtensionEnabled { extension_id, kind }
                | EventKind::ExtensionDisabled { extension_id, kind } => {
                    assert_eq!(kind, ExtensionKind::Skill);
                    (name, extension_id.to_string(), workspace)
                }
                other => panic!("unexpected event {other:?}"),
            }
        })
        .collect()
}

#[test]
fn enablement_is_persisted_per_workspace_and_emits_events() {
    let drive = drive();
    let (service, events) = service(&drive.paths);
    skill(&drive.paths.skills(), "notes", "Drive notes");
    write(
        &drive.paths.skills(),
        "broken",
        "SKILL.md",
        "---\nname: broken\n---\n",
    );
    let (one_dir, one) = workspace("wsp_one");
    let (two_dir, two) = workspace("wsp_two");
    for root in [one_dir.path(), two_dir.path()] {
        skill(&root.join(".claude/skills"), "deploy", "Repo deploy");
    }

    // Drive skills start enabled; disabling records the decision and an event.
    let summary = service
        .set_enabled(None, &[], &id("drive:notes"), false)
        .unwrap();
    assert!(!summary.enabled);
    assert_eq!(
        emitted(&events),
        [("extension.disabled", "drive:notes".into(), None)]
    );
    // Repeating the same decision is not a change: no event.
    service
        .set_enabled(None, &[], &id("drive:notes"), false)
        .unwrap();
    assert!(events.is_empty());
    let catalog = service.catalog(None, &[]).unwrap();
    assert!(!catalog.summary(&id("drive:notes")).unwrap().enabled);
    assert_eq!(
        catalog.load_body(&id("drive:notes")).unwrap_err(),
        SkillError::Unavailable {
            id: "drive:notes".into(),
            reason: "it is disabled".into()
        }
    );

    // Repo skills start disabled and are enabled per workspace.
    assert_eq!(
        service
            .set_enabled(None, &[], &id("repo:deploy"), true)
            .unwrap_err(),
        SkillError::WorkspaceRequired
    );
    let catalog = service.catalog(Some(&one), &[]).unwrap();
    assert!(!catalog.summary(&id("repo:deploy")).unwrap().enabled);
    assert_eq!(
        catalog.load_body(&id("repo:deploy")).unwrap_err().reason(),
        "skill_unavailable"
    );
    let summary = service
        .set_enabled(Some(&one), &[], &id("repo:deploy"), true)
        .unwrap();
    assert!(summary.enabled);
    assert_eq!(
        emitted(&events),
        [(
            "extension.enabled",
            "repo:deploy".into(),
            Some("wsp_one".into())
        )]
    );
    let body = service
        .catalog(Some(&one), &[])
        .unwrap()
        .load_body(&id("repo:deploy"))
        .unwrap();
    assert_eq!(body.body, "Instructions of deploy.");
    assert!(
        !service
            .catalog(Some(&two), &[])
            .unwrap()
            .summary(&id("repo:deploy"))
            .unwrap()
            .enabled,
        "decisions are per workspace"
    );

    // Skills with errors can be disabled but not enabled.
    let error = service
        .set_enabled(None, &[], &id("drive:broken"), true)
        .unwrap_err();
    assert_eq!(error.reason(), "invalid_skill");
    service
        .set_enabled(None, &[], &id("drive:broken"), false)
        .unwrap();
    assert_eq!(emitted(&events).len(), 1);
    assert!(matches!(
        service.set_enabled(None, &[], &id("drive:missing"), true),
        Err(SkillError::NotFound(_))
    ));

    // The state file is closed-schema TOML holding only explicit decisions.
    let text = fs::read_to_string(drive.paths.config().join("skills.toml")).unwrap();
    let state: SkillStateFile = toml::from_str(&text).unwrap();
    assert_eq!(state.version, 1);
    let entries: Vec<(String, Option<String>, bool)> = state
        .skills
        .iter()
        .map(|entry| {
            (
                format!("{}:{}", entry.scope, entry.name),
                entry.workspace_id.as_ref().map(ToString::to_string),
                entry.enabled,
            )
        })
        .collect();
    assert_eq!(
        entries,
        [
            ("repo:deploy".into(), Some("wsp_one".into()), true),
            ("drive:broken".into(), None, false),
            ("drive:notes".into(), None, false),
        ]
    );
}

#[test]
fn drive_skills_are_saved_replaced_and_deleted() {
    let drive = drive();
    let (service, events) = service(&drive.paths);

    let saved = service
        .save_drive_skill(&SkillDraft {
            version: Some("1.0.0".into()),
            ..draft(
                "release-notes",
                "Write release notes",
                "# Steps\n\n1. Read the log",
            )
        })
        .unwrap();
    assert_eq!(saved.id, "drive:release-notes");
    assert_eq!(saved.location, "skills/release-notes/SKILL.md");
    assert_eq!(saved.version.as_deref(), Some("1.0.0"));
    assert!(saved.enabled && saved.error.is_none());
    let file = drive.paths.skills().join("release-notes").join("SKILL.md");
    let text = fs::read_to_string(&file).unwrap();
    assert!(text.starts_with("---\nname: release-notes\n"), "{text}");

    let replaced = service
        .save_drive_skill(&draft("release-notes", "Updated", "New body"))
        .unwrap();
    assert_eq!(
        (replaced.description.as_str(), replaced.version),
        ("Updated", None)
    );
    let body = service
        .catalog(None, &[])
        .unwrap()
        .load_body(&id("drive:release-notes"))
        .unwrap();
    assert_eq!(body.body, "New body");

    // A drive skill declared in a differently named directory is replaced in place.
    write(
        &drive.paths.skills(),
        "Odd Dir",
        "skill.md",
        &skill_text("odd", "Old", "old"),
    );
    let odd = service
        .save_drive_skill(&draft("odd", "New", "new"))
        .unwrap();
    assert_eq!(odd.location, "skills/Odd Dir/skill.md");
    assert!(!drive.paths.skills().join("odd").exists());

    // A directory holding another skill is not overwritten.
    write(
        &drive.paths.skills(),
        "taken",
        "SKILL.md",
        &skill_text("other", "d", "b"),
    );
    let error = service
        .save_drive_skill(&draft("taken", "d", "b"))
        .unwrap_err();
    assert_eq!(error.reason(), "skill_conflict", "{error}");

    // Credentials are refused, not stored.
    let token = fake_token();
    let error = service
        .save_drive_skill(&draft("leaky", "d", &format!("use {token}")))
        .unwrap_err();
    assert_eq!(error, SkillError::CredentialDetected);
    assert!(!drive.paths.skills().join("leaky").exists());

    // Deleting removes the directory and forgets the enablement decision.
    service
        .set_enabled(None, &[], &id("drive:release-notes"), false)
        .unwrap();
    events.take();
    let deleted = service.delete_drive_skill("release-notes").unwrap();
    assert_eq!(deleted.id, "drive:release-notes");
    assert!(!drive.paths.skills().join("release-notes").exists());
    let text = fs::read_to_string(drive.paths.config().join("skills.toml")).unwrap();
    assert!(!text.contains("release-notes"), "{text}");
    assert!(events.is_empty());
    assert!(matches!(
        service.delete_drive_skill("release-notes"),
        Err(SkillError::NotFound(_))
    ));
    assert_eq!(
        service.delete_drive_skill("../x").unwrap_err().reason(),
        "invalid_skill_id"
    );
}

#[test]
fn saving_refuses_a_linked_skill_directory() {
    let drive = drive();
    let (service, _events) = service(&drive.paths);
    let outside = tempfile::tempdir().unwrap();
    skill(outside.path(), "linked", "Outside");
    fs::create_dir_all(drive.paths.skills()).unwrap();
    if !common::dir_link(
        &outside.path().join("linked"),
        &drive.paths.skills().join("linked"),
    ) {
        return;
    }
    let error = service
        .save_drive_skill(&draft("linked", "d", "b"))
        .unwrap_err();
    assert_eq!(error.reason(), "unsafe_path", "{error}");
    let error = service.delete_drive_skill("linked").unwrap_err();
    assert_eq!(error.reason(), "unsafe_path", "{error}");
    let outside_text = fs::read_to_string(outside.path().join("linked").join("SKILL.md")).unwrap();
    assert!(outside_text.contains("Outside"));
}
