//! Built-in policy profiles (spec §49) expressed as ordinary rule tables.

use std::sync::LazyLock;

use crate::{
    Action, Conditions, NetworkKind, NetworkMode, PathClass, PathLocation, PrivacyMode, Rule,
    SandboxLevel, ShellRisk, Verdict, macros::string_enum,
};

string_enum! {
    /// A built-in default rule set. Layers override profile defaults; hard rules,
    /// privacy, network mode, host trust, and project rules still tighten the result.
    pub enum Profile("profile") {
        /// Restricted sandbox, network allowlist, approvals for anything risky.
        Safe => "safe",
        /// Workspace writes and safe shell automatic; remote writes approved.
        Balanced => "balanced",
        /// Broad workspace permissions; destructive actions still protected.
        Autonomous => "autonomous",
        /// Local models only, no Internet, restricted sandbox.
        OfflineSecure => "offline_secure",
    }
}

impl Profile {
    /// 0 (Autonomous) to 3 (Offline Secure). Every built-in table is at least as
    /// strict as the tables below it for every intent.
    pub const fn strictness(self) -> u8 {
        match self {
            Self::Autonomous => 0,
            Self::Balanced => 1,
            Self::Safe => 2,
            Self::OfflineSecure => 3,
        }
    }

    pub fn stricter(a: Self, b: Self) -> Self {
        if b.strictness() > a.strictness() {
            b
        } else {
            a
        }
    }

    /// Isolation every allowed action runs at, at least.
    pub const fn baseline_sandbox(self) -> SandboxLevel {
        match self {
            Self::Safe | Self::OfflineSecure => SandboxLevel::Restricted,
            Self::Balanced | Self::Autonomous => SandboxLevel::Brokered,
        }
    }

    /// The most permissive network mode the profile allows.
    pub const fn network_cap(self) -> NetworkMode {
        match self {
            Self::Safe => NetworkMode::Allowlist,
            Self::OfflineSecure => NetworkMode::Offline,
            Self::Balanced | Self::Autonomous => NetworkMode::Unrestricted,
        }
    }

    /// The least strict privacy mode the profile allows.
    pub const fn minimum_privacy(self) -> PrivacyMode {
        match self {
            Self::OfflineSecure => PrivacyMode::LocalOnly,
            _ => PrivacyMode::Hybrid,
        }
    }

    /// The profile's default rules. Actions no rule covers are denied by default.
    pub fn rules(self) -> &'static [Rule] {
        &TABLES[self as usize]
    }
}

static TABLES: LazyLock<[Vec<Rule>; 4]> = LazyLock::new(|| {
    [
        build(&SAFE),
        build(&BALANCED),
        build(&AUTONOMOUS),
        build(&OFFLINE_SECURE),
    ]
});

/// Verdict choices that differ between profiles.
struct Table {
    read_outside: Choice,
    write_outside: Choice,
    delete_workspace: Choice,
    /// Shell/process verdicts for Low, Medium, High, Destructive risk.
    shell: [Choice; 4],
    /// `None`: only loopback and local-network requests have a rule.
    network: Option<Choice>,
    package: Option<Choice>,
    git_pull: Choice,
    git_push: Choice,
    secret: Choice,
    mcp: Choice,
    agent: Choice,
    model_remote: Option<Choice>,
    host_mount: Option<Choice>,
    remote_action: Option<Choice>,
    remote_write: Choice,
}

#[derive(Clone, Copy)]
enum Choice {
    Allow,
    Restricted,
    Ask,
    Once,
}

impl Choice {
    fn verdict(self) -> Verdict {
        match self {
            Self::Allow => Verdict::Allow,
            Self::Restricted => Verdict::sandbox(SandboxLevel::Restricted),
            Self::Ask => Verdict::approve_once_or_session(),
            Self::Once => Verdict::approve_once(),
        }
    }
}

const SAFE: Table = Table {
    read_outside: Choice::Ask,
    write_outside: Choice::Once,
    delete_workspace: Choice::Ask,
    shell: [Choice::Allow, Choice::Ask, Choice::Once, Choice::Once],
    network: Some(Choice::Allow),
    package: Some(Choice::Ask),
    git_pull: Choice::Ask,
    git_push: Choice::Once,
    secret: Choice::Once,
    mcp: Choice::Ask,
    agent: Choice::Ask,
    model_remote: Some(Choice::Ask),
    host_mount: None,
    remote_action: None,
    remote_write: Choice::Once,
};

const BALANCED: Table = Table {
    read_outside: Choice::Ask,
    write_outside: Choice::Once,
    delete_workspace: Choice::Allow,
    shell: [Choice::Allow, Choice::Restricted, Choice::Ask, Choice::Once],
    network: Some(Choice::Allow),
    package: Some(Choice::Ask),
    git_pull: Choice::Allow,
    git_push: Choice::Ask,
    secret: Choice::Ask,
    mcp: Choice::Allow,
    agent: Choice::Allow,
    model_remote: Some(Choice::Allow),
    host_mount: Some(Choice::Once),
    remote_action: Some(Choice::Ask),
    remote_write: Choice::Ask,
};

const AUTONOMOUS: Table = Table {
    read_outside: Choice::Ask,
    write_outside: Choice::Ask,
    delete_workspace: Choice::Allow,
    shell: [
        Choice::Allow,
        Choice::Allow,
        Choice::Restricted,
        Choice::Once,
    ],
    network: Some(Choice::Allow),
    package: Some(Choice::Restricted),
    git_pull: Choice::Allow,
    git_push: Choice::Ask,
    secret: Choice::Ask,
    mcp: Choice::Allow,
    agent: Choice::Allow,
    model_remote: Some(Choice::Allow),
    host_mount: Some(Choice::Ask),
    remote_action: Some(Choice::Ask),
    remote_write: Choice::Ask,
};

const OFFLINE_SECURE: Table = Table {
    read_outside: Choice::Ask,
    write_outside: Choice::Once,
    delete_workspace: Choice::Ask,
    shell: [Choice::Allow, Choice::Ask, Choice::Once, Choice::Once],
    network: None,
    package: None,
    git_pull: Choice::Ask,
    git_push: Choice::Once,
    secret: Choice::Once,
    mcp: Choice::Ask,
    agent: Choice::Ask,
    model_remote: None,
    host_mount: None,
    remote_action: None,
    remote_write: Choice::Once,
};

fn location(location: PathLocation) -> Conditions {
    Conditions {
        location: Some(location),
        ..Conditions::default()
    }
}

fn sensitive() -> Conditions {
    Conditions {
        path_class: Some(PathClass::Sensitive),
        ..Conditions::default()
    }
}

fn build(table: &Table) -> Vec<Rule> {
    use Action as A;
    use PathLocation::{Outside, Workspace};

    let mut rules = vec![
        Rule::new("fs.read.workspace", A::FsRead, Verdict::Allow).when(location(Workspace)),
        Rule::new("fs.read.outside", A::FsRead, table.read_outside.verdict())
            .when(location(Outside)),
        Rule::new("fs.read.sensitive", A::FsRead, Verdict::approve_once()).when(sensitive()),
        Rule::new("fs.search.workspace", A::FsSearch, Verdict::Allow).when(location(Workspace)),
        Rule::new(
            "fs.search.outside",
            A::FsSearch,
            table.read_outside.verdict(),
        )
        .when(location(Outside)),
        Rule::new("fs.search.sensitive", A::FsSearch, Verdict::approve_once()).when(sensitive()),
        Rule::new("fs.write.workspace", A::FsWrite, Verdict::Allow).when(location(Workspace)),
        Rule::new(
            "fs.write.outside",
            A::FsWrite,
            table.write_outside.verdict(),
        )
        .when(location(Outside)),
        Rule::new("fs.write.sensitive", A::FsWrite, Verdict::approve_once()).when(sensitive()),
        Rule::new(
            "fs.delete.workspace",
            A::FsDelete,
            table.delete_workspace.verdict(),
        )
        .when(location(Workspace)),
        Rule::new("fs.delete.outside", A::FsDelete, Verdict::approve_once())
            .when(location(Outside)),
        Rule::new("fs.delete.sensitive", A::FsDelete, Verdict::approve_once()).when(sensitive()),
        Rule::new("git.read", A::GitRead, Verdict::Allow),
        Rule::new("git.commit", A::GitCommit, Verdict::Allow),
        Rule::new("git.branch", A::GitBranch, Verdict::Allow),
        Rule::new("git.pull", A::GitPull, table.git_pull.verdict()),
        Rule::new("git.push", A::GitPush, table.git_push.verdict()),
        Rule::new("secret.use", A::SecretUse, table.secret.verdict()),
        Rule::new("mcp.call", A::McpCall, table.mcp.verdict()),
        Rule::new("agent.spawn", A::AgentSpawn, table.agent.verdict()),
        Rule::new("model.invoke.local", A::ModelInvoke, Verdict::Allow).when(Conditions {
            model_locality: Some(crate::ModelLocality::Local),
            ..Conditions::default()
        }),
        Rule::new("artifact.create", A::ArtifactCreate, Verdict::Allow),
        Rule::new("policy.change", A::PolicyChange, Verdict::approve_once()),
        Rule::new(
            "remote-write",
            crate::ActionPattern::Any,
            table.remote_write.verdict(),
        )
        .when(Conditions {
            remote_write: Some(true),
            ..Conditions::default()
        }),
    ];
    let risks = [
        ShellRisk::Low,
        ShellRisk::Medium,
        ShellRisk::High,
        ShellRisk::Destructive,
    ];
    for action in [A::ShellExec, A::ProcessSpawn] {
        for (risk, choice) in risks.iter().zip(table.shell) {
            rules.push(
                Rule::new(format!("{action}.{risk}"), action, choice.verdict()).when(Conditions {
                    min_risk: Some(*risk),
                    max_risk: Some(*risk),
                    ..Conditions::default()
                }),
            );
        }
        // High-risk commands run restricted even after a human approves them.
        rules.push(
            Rule::new(
                format!("{action}.isolation"),
                action,
                Choice::Restricted.verdict(),
            )
            .when(Conditions {
                min_risk: Some(ShellRisk::High),
                ..Conditions::default()
            }),
        );
    }
    match table.network {
        Some(choice) => rules.push(Rule::new(
            "network.request",
            A::NetworkRequest,
            choice.verdict(),
        )),
        None => rules.push(
            Rule::new("network.request.local", A::NetworkRequest, Verdict::Allow).when(
                Conditions {
                    network_kinds: vec![NetworkKind::Loopback, NetworkKind::LocalNetwork],
                    ..Conditions::default()
                },
            ),
        ),
    }
    let optional = [
        ("package.install", A::PackageInstall, table.package),
        ("host.mount", A::HostMount, table.host_mount),
        ("remote.action", A::RemoteAction, table.remote_action),
    ];
    for (id, action, choice) in optional {
        if let Some(choice) = choice {
            rules.push(Rule::new(id, action, choice.verdict()));
        }
    }
    if table.package.is_some() {
        // Install scripts are untrusted code: restricted even after approval.
        rules.push(Rule::new(
            "package.install.isolation",
            A::PackageInstall,
            Choice::Restricted.verdict(),
        ));
    }
    if let Some(choice) = table.model_remote {
        rules.push(
            Rule::new("model.invoke.remote", A::ModelInvoke, choice.verdict()).when(Conditions {
                model_locality: Some(crate::ModelLocality::Remote),
                ..Conditions::default()
            }),
        );
    }
    debug_assert!(rules.iter().all(|rule| rule.validate().is_ok()));
    rules
}
