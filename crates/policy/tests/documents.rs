//! TOML policy documents and project rules: valid loads and fail-closed errors.

mod common;

use std::fs;

use aidex_core::ServiceErrorCode;
use aidex_drive::{DrivePaths, InitOptions, init_with};
use aidex_policy::{
    Action, ActionPattern, Layer, MAX_DOCUMENT_BYTES, NetworkAccess, PathScope, PolicyDocument,
    PolicyError, PolicySet, Profile, ProjectRules, RuleOrigin, SandboxLevel, Verdict,
};
use common::*;

const GLOBAL: &str = r#"
schema_version = 1
scope = "global"

[network]
allow_domains = ["github.com", "*.crates.io"]
local_network = "deny"
arbitrary_ip = "ask"

[[rules]]
id = "no-force-push"
action = "shell.exec"
verdict = "deny"
reason = "force pushes are reviewed by humans"
[rules.when]
min_risk = "destructive"
any_programs = ["git"]

[[rules]]
id = "sandbox-builds"
action = "shell.exec"
verdict = "sandbox"
sandbox = "ephemeral"
[rules.when]
only_programs = ["cargo"]
"#;

const WORKSPACE: &str = r#"
schema_version = 1
scope = "workspace"
subject = "ws_test"

[[rules]]
id = "review-pushes"
action = "git.*"
verdict = "require_approval"
approval_scopes = ["once"]
expires_in_secs = 120
[rules.when]
remote_write = true
"#;

fn drive() -> (tempfile::TempDir, DrivePaths) {
    let dir = tempfile::tempdir().unwrap();
    init_with(
        dir.path(),
        InitOptions {
            require_removable: false,
        },
    )
    .unwrap();
    let paths = DrivePaths::open(dir.path()).unwrap();
    (dir, paths)
}

fn parse(text: &str) -> Result<PolicyDocument, PolicyError> {
    PolicyDocument::parse(text, "test.toml")
}

#[test]
fn loads_policy_files_from_the_drive_policies_directory() {
    let (_dir, paths) = drive();
    let policies_dir = paths.policies();
    fs::write(policies_dir.join("10-global.toml"), GLOBAL).unwrap();
    fs::write(policies_dir.join("20-workspace.toml"), WORKSPACE).unwrap();
    fs::write(policies_dir.join("README.md"), "not policy").unwrap();
    let policies = PolicySet::load(&policies_dir).unwrap();
    assert_eq!(policies.rule_count(), 3);
    assert_eq!(policies.network().local_network, NetworkAccess::Deny);
    assert!(policies.network().allows_domain("static.crates.io"));
    assert_eq!(
        policies.rules(Layer::Workspace, Some(&workspace())).count(),
        1
    );

    let context = open_context(Profile::Balanced);
    let decision = decide(&policies, &context, &shell("git push --force origin main"));
    assert_eq!(decision.verdict, Verdict::Deny);
    assert_eq!(decision.rule.id, "no-force-push");
    assert_eq!(decision.rule.source.as_deref(), Some("10-global.toml"));
    assert!(decision.reason.contains("reviewed by humans"));

    let decision = decide(&policies, &context, &shell("cargo build"));
    assert_eq!(decision.verdict, Verdict::sandbox(SandboxLevel::Ephemeral));

    let decision = decide(&policies, &context, &push_to("github.com"));
    assert_eq!(
        decision.verdict,
        Verdict::RequireApproval {
            scope_options: vec![aidex_policy::ApprovalScope::Once],
            expires_in: std::time::Duration::from_secs(120),
        }
    );
}

#[test]
fn one_malformed_file_fails_the_whole_load() {
    let (_dir, paths) = drive();
    let dir = paths.policies();
    fs::write(dir.join("a.toml"), GLOBAL).unwrap();
    fs::write(
        dir.join("b.toml"),
        "schema_version = 1\nscope = \"global\"\n[[rules]]\nid = ",
    )
    .unwrap();
    let error = PolicySet::load(&dir).unwrap_err();
    assert!(
        matches!(error, PolicyError::Parse { ref source, .. } if source == "b.toml"),
        "{error}"
    );
    assert_eq!(error.code(), ServiceErrorCode::InvalidArgument);
}

#[test]
fn missing_directories_and_oversized_files_are_errors() {
    let dir = tempfile::tempdir().unwrap();
    let missing = PolicySet::load(dir.path().join("nope")).unwrap_err();
    assert!(matches!(missing, PolicyError::MissingDirectory(_)));
    assert_eq!(missing.code(), ServiceErrorCode::NotFound);

    let big = format!(
        "schema_version = 1\nscope = \"global\"\n# {}\n",
        "x".repeat(usize::try_from(MAX_DOCUMENT_BYTES).unwrap())
    );
    fs::write(dir.path().join("big.toml"), big).unwrap();
    assert!(matches!(
        PolicySet::load(dir.path()),
        Err(PolicyError::TooLarge { .. })
    ));
}

#[test]
fn symlinked_policy_files_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let real = dir.path().join("real.txt");
    fs::write(&real, GLOBAL).unwrap();
    let link = dir.path().join("linked.toml");
    #[cfg(unix)]
    let created = std::os::unix::fs::symlink(&real, &link).is_ok();
    #[cfg(windows)]
    let created = std::os::windows::fs::symlink_file(&real, &link).is_ok();
    if !created {
        // Creating symlinks needs Developer Mode or elevation on Windows.
        return;
    }
    let error = PolicySet::load(dir.path()).unwrap_err();
    assert!(matches!(error, PolicyError::UnsafePath(_)), "{error}");
    assert_eq!(error.code(), ServiceErrorCode::PermissionDenied);
}

#[test]
fn closed_schema_rejects_unknown_fields_values_and_versions() {
    let cases: Vec<(&str, &str)> = vec![
        (
            "schema_version = 1\nscope = \"global\"\ncolour = \"red\"\n",
            "unknown field",
        ),
        (
            "schema_version = 1\nscope = \"global\"\n[[rules]]\nid = \"a\"\naction = \"fs.read\"\nverdict = \"allow\"\npriority = 1\n",
            "unknown field",
        ),
        (
            "schema_version = 1\nscope = \"global\"\n[[rules]]\nid = \"a\"\naction = \"fs.teleport\"\nverdict = \"allow\"\n",
            "unknown action",
        ),
        (
            "schema_version = 1\nscope = \"global\"\n[[rules]]\nid = \"a\"\naction = \"fs.read\"\nverdict = \"rewrite\"\n",
            "unknown variant",
        ),
        (
            "schema_version = 1\nscope = \"global\"\n[[rules]]\nid = \"a\"\naction = \"fs.read\"\nverdict = \"allow\"\n[rules.when]\nlocation = \"moon\"\n",
            "unknown path location",
        ),
        (
            "schema_version = 1\nscope = \"planet\"\n",
            "unknown policy layer",
        ),
        ("scope = \"global\"\n", "missing `schema_version`"),
        (
            "schema_version = \"one\"\nscope = \"global\"\n",
            "invalid type",
        ),
        ("this is = = not toml", "line 1"),
    ];
    for (text, expected) in cases {
        let error = parse(text).unwrap_err();
        assert!(
            matches!(error, PolicyError::Parse { .. }),
            "{text}: {error}"
        );
        assert!(error.to_string().contains(expected), "{text}: {error}");
    }
    let future = parse("schema_version = 2\nscope = \"global\"\nnew_thing = true\n").unwrap_err();
    assert_eq!(
        future,
        PolicyError::UnsupportedSchema {
            source: "test.toml".into(),
            found: 2
        }
    );
    assert_eq!(future.code(), ServiceErrorCode::IncompatibleVersion);
}

#[test]
fn semantic_errors_are_rejected() {
    let rule = |extra: &str| {
        format!(
            "schema_version = 1\nscope = \"global\"\n[[rules]]\nid = \"r\"\naction = \"fs.write\"\n{extra}\n"
        )
    };
    let invalid = [
        "schema_version = 1\nscope = \"workspace\"\n".to_owned(),
        "schema_version = 1\nscope = \"global\"\nsubject = \"ws_test\"\n".to_owned(),
        "schema_version = 1\nscope = \"session\"\nsubject = \"bad id!\"\n".to_owned(),
        "schema_version = 1\nscope = \"workspace\"\nsubject = \"ws_test\"\n[network]\n".to_owned(),
        rule("verdict = \"sandbox\""),
        rule("verdict = \"sandbox\"\nsandbox = \"direct\""),
        rule("verdict = \"allow\"\nsandbox = \"restricted\""),
        rule("verdict = \"allow\"\nexpires_in_secs = 60"),
        rule("verdict = \"require_approval\"\napproval_scopes = []"),
        rule("verdict = \"require_approval\"\nexpires_in_secs = 0"),
        rule("verdict = \"rate_limit\"\nrate_max = 3"),
        rule("verdict = \"rate_limit\"\nrate_max = 0\nrate_per_secs = 60"),
        rule("verdict = \"allow\"\n[rules.when]\nmin_risk = \"high\"\nmax_risk = \"low\""),
        "schema_version = 1\nscope = \"global\"\n[[rules]]\nid = \"Upper\"\naction = \"fs.read\"\nverdict = \"allow\"\n"
            .to_owned(),
    ];
    for text in &invalid {
        let error = parse(text).unwrap_err();
        assert!(
            matches!(error, PolicyError::InvalidDocument { .. }),
            "{text}: {error}"
        );
    }
    for pattern in [
        "../outside",
        "/etc/passwd",
        "C:/Windows",
        "src\\\\lib.rs",
        "",
    ] {
        let text = rule(&format!(
            "verdict = \"deny\"\n[rules.when]\npaths = [\"{pattern}\"]"
        ));
        assert!(
            matches!(parse(&text), Err(PolicyError::InvalidGlob { .. })),
            "{pattern}"
        );
    }
    assert!(
        parse(&rule(
            "verdict = \"deny\"\n[rules.when]\nhosts = [\"10.0.0.1\"]"
        ))
        .is_err()
    );
}

#[test]
fn duplicate_rule_ids_and_repeated_network_settings_are_rejected() {
    let one = parse(GLOBAL).unwrap();
    let error = PolicySet::from_documents([
        one.clone(),
        PolicyDocument {
            source: "copy.toml".into(),
            ..one.clone()
        },
    ])
    .unwrap_err();
    assert!(
        matches!(error, PolicyError::InvalidDocument { .. }),
        "{error}"
    );

    let rules_only = PolicyDocument {
        network: None,
        ..one.clone()
    };
    let error = PolicySet::from_documents([one, rules_only]).unwrap_err();
    assert!(
        matches!(error, PolicyError::DuplicateRule { .. }),
        "{error}"
    );
    assert_eq!(error.code(), ServiceErrorCode::Conflict);
}

const PROJECT: &str = r#"
schema_version = 1

[project]
package_manager = "bun"
tests = "bun test"
lint = "bun lint"

[agent]
never_modify = ["migrations/production", "**/*.lock"]
require_approval = ["git.push", "package.*"]
"#;

fn with_project() -> PolicySet {
    let mut policies = allow_everything();
    policies.set_project_rules(
        workspace(),
        ProjectRules::parse(PROJECT, "rules.toml").unwrap(),
    );
    policies
}

#[test]
fn project_never_modify_denies_writes_and_deletes_despite_allow_layers() {
    let policies = with_project();
    let context = open_context(Profile::Autonomous);
    for relative in [
        "migrations/production",
        "migrations/production/001.sql",
        "Migrations/Production/x",
        "web/bun.lock",
    ] {
        let decision = decide(&policies, &context, &write_inside(relative));
        assert_eq!(decision.verdict, Verdict::Deny, "{relative}");
        assert_eq!(decision.rule.origin, RuleOrigin::Project);
        assert_eq!(decision.rule.id, "project.never-modify");
        let delete = intent(Action::FsDelete).with_path(PathScope::inside("/w/x", relative));
        assert_eq!(decide(&policies, &context, &delete).verdict, Verdict::Deny);
    }
    assert_eq!(
        decide(&policies, &context, &write_inside("migrations/dev/001.sql")).verdict,
        Verdict::Allow
    );
    let read = intent(Action::FsRead)
        .with_path(PathScope::inside("/w/m", "migrations/production/001.sql"));
    assert_eq!(decide(&policies, &context, &read).verdict, Verdict::Allow);
}

#[test]
fn project_require_approval_cannot_be_loosened_by_layers() {
    let policies = with_project();
    let context = open_context(Profile::Autonomous);
    let decision = decide(&policies, &context, &push_to("github.com"));
    assert_eq!(decision.verdict, once_or_session());
    assert_eq!(decision.rule.id, "project.require-approval");
    assert!(
        decide(&policies, &context, &shell("bun add zod"))
            .verdict
            .requires_approval()
    );
    assert!(
        decide(&policies, &context, &shell("git push origin main"))
            .verdict
            .requires_approval()
    );
    // Rules for one workspace never apply to another.
    let mut other = shell("git push origin main");
    other.workspace_id = Some(id("ws_other"));
    assert_eq!(decide(&policies, &context, &other).verdict, Verdict::Allow);
}

#[test]
fn project_rules_parse_metadata_and_reject_unknown_content() {
    let rules = ProjectRules::parse(PROJECT, "rules.toml").unwrap();
    assert_eq!(rules.project.package_manager.as_deref(), Some("bun"));
    assert_eq!(rules.project.tests.as_deref(), Some("bun test"));
    assert_eq!(rules.never_modify(), ["migrations/production", "**/*.lock"]);
    assert_eq!(
        rules.require_approval(),
        [
            ActionPattern::Exact(Action::GitPush),
            ActionPattern::Category("package")
        ]
    );

    let invalid = [
        "schema_version = 1\n[agent]\nallow = [\"*\"]\n",
        "schema_version = 1\n[agent]\nrequire_approval = [\"git.yeet\"]\n",
        "schema_version = 1\n[agent]\nnever_modify = [\"../../etc\"]\n",
        "schema_version = 1\n[agent]\nnever_modify = \"src\"\n",
        "[agent]\nnever_modify = [\"src\"]\n",
    ];
    for text in invalid {
        assert!(ProjectRules::parse(text, "rules.toml").is_err(), "{text}");
    }

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rules.toml");
    fs::write(&path, PROJECT).unwrap();
    assert_eq!(ProjectRules::load(&path).unwrap().never_modify().len(), 2);
    assert!(ProjectRules::load(dir.path()).is_err());
}
