//! Memory: three inspectable, editable scopes; secret-like text is rejected.

mod common;

use std::fs;

use aidex_context::{
    ContextSource, DropReason, FileMemoryStore, MemoryEdit, MemoryError, MemoryScope,
    MemorySource, MemoryStore, NewMemory, PackRequest, Trust,
};
use aidex_core::EntityId;
use common::*;

fn id(value: &str) -> EntityId {
    EntityId::new(value).unwrap()
}

#[test]
fn every_scope_is_listable_editable_and_removable() {
    let drive = Drive::new();
    let paths = drive.paths();
    let store = FileMemoryStore::open(&paths).unwrap();
    let workspace = MemoryScope::Workspace {
        workspace_id: id("wsp_one"),
    };
    let session = MemoryScope::Session {
        session_id: id("ses_one"),
    };
    let user = store
        .add(&MemoryScope::User, NewMemory::user("Prefer tabs").with_tags(["Style"]))
        .unwrap();
    assert_eq!(user.tags, ["style"]);
    assert_eq!(user.source, MemorySource::User);
    let build = store
        .add(&workspace, NewMemory::user("Build with `bun run build`"))
        .unwrap();
    let agent = store
        .add(
            &workspace,
            NewMemory::agent(id("ses_one"), "Tests flake on Windows when run in parallel"),
        )
        .unwrap();
    assert!(agent.source.is_agent());
    store
        .add(&session, NewMemory::agent(id("ses_one"), "Halfway through the refactor"))
        .unwrap();

    // Human-editable files in the documented locations.
    let memory = paths.memory();
    assert!(memory.join("user.toml").is_file());
    let text = fs::read_to_string(memory.join("workspaces").join("wsp_one.toml")).unwrap();
    assert!(text.contains("schema_version = 1"));
    assert!(text.contains("bun run build"));
    assert!(memory.join("sessions").join("ses_one.toml").is_file());

    let listed = store.list(&workspace).unwrap();
    assert_eq!(listed, [build.clone(), agent.clone()]);

    let edited = store
        .edit(&workspace, &build.id, MemoryEdit::by_user().text("Build with `bun run build:all`"))
        .unwrap();
    assert_eq!(edited.text, "Build with `bun run build:all`");
    assert_eq!(edited.created_at, build.created_at);
    // An agent edit of a user memory marks it agent-written.
    let tainted = store
        .edit(
            &workspace,
            &build.id,
            MemoryEdit {
                text: None,
                tags: Some(vec!["build".into()]),
                by: MemorySource::Agent {
                    session_id: id("ses_one"),
                },
            },
        )
        .unwrap();
    assert!(tainted.source.is_agent());

    let removed = store.remove(&workspace, &agent.id).unwrap();
    assert_eq!(removed.id, agent.id);
    assert_eq!(store.list(&workspace).unwrap().len(), 1);
    assert_eq!(
        store.remove(&workspace, &agent.id),
        Err(MemoryError::NotFound(agent.id.clone()))
    );
    assert_eq!(store.clear(&session).unwrap(), 1);
    assert!(store.list(&session).unwrap().is_empty());
    // Ids that cannot name files are refused.
    let bad = MemoryScope::Workspace {
        workspace_id: id(".hidden"),
    };
    assert!(matches!(
        store.list(&bad),
        Err(MemoryError::InvalidScope(_))
    ));
}

#[test]
fn secret_like_memories_are_rejected_and_never_stored() {
    let drive = Drive::new();
    let store = FileMemoryStore::open(&drive.paths()).unwrap();
    let scope = MemoryScope::User;
    let token = fake_token();
    for text in [
        format!("deploy token {token}"),
        format!("{}=correct-horse", "password"),
        fake_private_key(),
        format!("connect to https://admin:{}@db.internal/app", "s3cretpw"),
    ] {
        assert_eq!(
            store.add(&scope, NewMemory::user(text.clone())),
            Err(MemoryError::SecretLike),
            "{text}"
        );
    }
    let entry = store.add(&scope, NewMemory::user("harmless")).unwrap();
    assert_eq!(
        store.edit(&scope, &entry.id, MemoryEdit::by_user().text(format!("now {token}"))),
        Err(MemoryError::SecretLike)
    );
    let file = fs::read_to_string(drive.paths().memory().join("user.toml")).unwrap();
    assert!(!file.contains(&token));
    assert_eq!(store.list(&scope).unwrap(), [entry]);
    assert_eq!(MemoryError::SecretLike.reason(), "secret_like");
}

#[test]
fn hand_edited_memory_files_are_validated_and_secrets_are_not_packed() {
    let fixture = Fixture::new(&[("src/lib.rs", "pub fn deploy() {}\n")]);
    fixture.build();
    let memory = fixture.service.memory();
    let workspace = MemoryScope::Workspace {
        workspace_id: fixture.workspace.clone(),
    };
    let kept = memory
        .add(&workspace, NewMemory::user("deploy runs from the release branch"))
        .unwrap();
    let agent = memory
        .add(
            &workspace,
            NewMemory::agent(id("ses_x"), "deploy needs the staging flag"),
        )
        .unwrap();
    // The user edits the file by hand and pastes a credential into an entry.
    let path = fixture
        .drive
        .paths()
        .memory()
        .join("workspaces")
        .join(format!("{}.toml", fixture.workspace.as_str()));
    let text = fs::read_to_string(&path).unwrap();
    let edited = text.replacen(
        "deploy needs the staging flag",
        &format!("deploy needs {}", fake_token()),
        1,
    );
    fs::write(&path, edited).unwrap();
    let pack = fixture
        .service
        .assemble(
            &fixture.target(),
            &PackRequest::new(fixture.workspace.clone(), "deploy", 5_000),
        )
        .unwrap();
    assert!(!pack.render().contains(&fake_token()));
    let memory_items: Vec<_> = pack
        .items
        .iter()
        .filter(|item| matches!(item.source, ContextSource::Memory { .. }))
        .collect();
    assert_eq!(memory_items.len(), 1);
    assert_eq!(memory_items[0].trust, Trust::User);
    assert!(memory_items[0].text.contains(&kept.text));
    assert!(pack.dropped.iter().any(|dropped| dropped.reason == DropReason::SecretLike
        && dropped.source
            == ContextSource::Memory {
                scope: aidex_context::MemoryScopeKind::Workspace,
                id: agent.id.clone()
            }));

    // An invalid file is reported, never silently replaced.
    fs::write(&path, "schema_version = 1\n[[entries]]\nid = 3\n").unwrap();
    assert!(matches!(
        memory.list(&workspace),
        Err(MemoryError::Corrupt { .. })
    ));
    assert!(matches!(
        memory.add(&workspace, NewMemory::user("x")),
        Err(MemoryError::Corrupt { .. })
    ));
}
