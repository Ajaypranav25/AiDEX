//! Privacy modes (§82), host trust (§61), and network modes (§12.5) only tighten.

mod common;

use aidex_policy::{
    Action, Decision, EvaluationContext, HostTrust, ModelLocality, NetworkMode, PolicyDocument,
    PolicySet, PrivacyMode, Profile, RuleOrigin, SandboxLevel, Verdict, classify_network,
};
use common::*;

fn ctx(
    profile: Profile,
    privacy: PrivacyMode,
    trust: HostTrust,
    mode: NetworkMode,
) -> EvaluationContext {
    EvaluationContext::new(profile, privacy, trust, mode).with_host(host())
}

fn network_policy(body: &str) -> PolicySet {
    let text = format!("schema_version = 1\nscope = \"global\"\n[network]\n{body}");
    PolicySet::from_documents([PolicyDocument::parse(&text, "network.toml").unwrap()]).unwrap()
}

#[test]
fn air_gapped_denies_every_network_destination_even_loopback_and_explicit_allows() {
    let policies = allow_everything();
    let air_gapped = ctx(
        Profile::Autonomous,
        PrivacyMode::AirGapped,
        HostTrust::Trusted,
        NetworkMode::Unrestricted,
    );
    assert_eq!(air_gapped.effective_network_mode(), NetworkMode::Offline);
    let networked = [
        request("localhost", 8080),
        request("127.0.0.1", 11434),
        request("192.168.1.1", 80),
        request("example.com", 443),
        model(ModelLocality::Remote),
        push_to("github.com"),
        shell("curl http://127.0.0.1:8080/health"),
        intent(Action::McpCall).with_network(classify_network("localhost", 3000).unwrap()),
    ];
    for intent in &networked {
        let decision = decide(&policies, &air_gapped, intent);
        assert_eq!(decision.verdict, Verdict::Deny, "{}", intent.effect_hint());
    }
    let decision = decide(&policies, &air_gapped, &request("localhost", 1));
    assert_eq!(decision.rule.origin, RuleOrigin::Privacy);
    for local in [
        model(ModelLocality::Local),
        write_inside("a.txt"),
        shell("ls"),
    ] {
        assert!(
            decide(&policies, &air_gapped, &local)
                .verdict
                .permits_execution()
        );
    }
}

trait EffectHint {
    fn effect_hint(&self) -> String;
}

impl EffectHint for aidex_policy::Intent {
    fn effect_hint(&self) -> String {
        format!("{} {:?}", self.action, self.attributes.network)
    }
}

#[test]
fn local_only_denies_external_services_but_keeps_loopback_and_lan() {
    let policies = allow_everything();
    let local_only = ctx(
        Profile::Balanced,
        PrivacyMode::LocalOnly,
        HostTrust::Trusted,
        NetworkMode::Unrestricted,
    );
    let decision = decide(&policies, &local_only, &model(ModelLocality::Remote));
    assert_eq!(decision.verdict, Verdict::Deny);
    assert_eq!(decision.rule.id, "privacy.local-only");
    assert_eq!(
        decide(&policies, &local_only, &request("example.com", 443)).verdict,
        Verdict::Deny
    );
    assert_eq!(
        decide(&policies, &local_only, &request("localhost", 8080)).verdict,
        Verdict::Allow
    );
    assert_eq!(
        decide(&policies, &local_only, &request("10.0.0.2", 80)).verdict,
        Verdict::Allow
    );
    assert_eq!(
        decide(&policies, &local_only, &model(ModelLocality::Local)).verdict,
        Verdict::Allow
    );
}

#[test]
fn new_hosts_require_approval_for_secrets_and_remote_provider_tokens() {
    let policies = allow_everything();
    let new_host = ctx(
        Profile::Autonomous,
        PrivacyMode::Hybrid,
        HostTrust::New,
        NetworkMode::Unrestricted,
    );
    let decision = decide(&policies, &new_host, &secret());
    assert_eq!(decision.verdict, once());
    assert_eq!(decision.rule.origin, RuleOrigin::HostTrust);
    assert_eq!(
        decide(&policies, &new_host, &model(ModelLocality::Remote)).verdict,
        once()
    );
    assert_eq!(
        decide(&policies, &new_host, &model(ModelLocality::Local)).verdict,
        Verdict::Allow
    );
}

#[test]
fn untrusted_hosts_lose_autonomy_and_secrets() {
    let untrusted = ctx(
        Profile::Autonomous,
        PrivacyMode::Hybrid,
        HostTrust::Untrusted,
        NetworkMode::Unrestricted,
    );
    assert_eq!(untrusted.effective_profile(), Profile::Safe);
    let policies = PolicySet::new();
    assert_eq!(
        decide(&policies, &untrusted, &shell("cargo test")).verdict,
        once_or_session()
    );
    assert_eq!(
        decide(&allow_everything(), &untrusted, &secret()).verdict,
        Verdict::Deny
    );
    assert_eq!(
        decide(&allow_everything(), &untrusted, &intent(Action::HostMount)).verdict,
        Verdict::Deny
    );

    let restricted = EvaluationContext {
        host_trust: HostTrust::Restricted,
        ..untrusted
    };
    assert_eq!(restricted.effective_profile(), Profile::Balanced);
    assert_eq!(
        decide(&policies, &restricted, &shell("cargo test")).verdict,
        Verdict::sandbox(SandboxLevel::Restricted)
    );
}

#[test]
fn allowlist_mode_permits_only_listed_domains() {
    let policies = network_policy(r#"allow_domains = ["github.com", "*.githubusercontent.com"]"#);
    let allowlist = ctx(
        Profile::Balanced,
        PrivacyMode::Hybrid,
        HostTrust::Trusted,
        NetworkMode::Allowlist,
    );
    let allowed = |host: &str| decide(&policies, &allowlist, &request(host, 443)).verdict;
    assert_eq!(allowed("github.com"), Verdict::Allow);
    assert_eq!(allowed("GitHub.com."), Verdict::Allow);
    assert_eq!(allowed("raw.githubusercontent.com"), Verdict::Allow);
    assert_eq!(allowed("api.github.com"), Verdict::Deny);
    assert_eq!(allowed("githubusercontent.com"), Verdict::Deny);
    assert_eq!(allowed("evil-github.com"), Verdict::Deny);
    assert_eq!(allowed("localhost"), Verdict::Allow);
    assert_eq!(allowed("8.8.8.8"), once_or_session());
    let unknown = decide(&policies, &allowlist, &shell("curl $URL"));
    assert_eq!(unknown.verdict, Verdict::Deny);
    assert_eq!(unknown.rule.id, "network.allowlist.unknown");
}

#[test]
fn offline_and_normal_modes_apply_local_network_and_ip_settings() {
    let offline = ctx(
        Profile::Balanced,
        PrivacyMode::Hybrid,
        HostTrust::Trusted,
        NetworkMode::Offline,
    );
    let normal = ctx(
        Profile::Balanced,
        PrivacyMode::Hybrid,
        HostTrust::Trusted,
        NetworkMode::Normal,
    );
    let defaults = PolicySet::new();
    assert_eq!(
        decide(&defaults, &offline, &request("example.com", 443)).verdict,
        Verdict::Deny
    );
    assert_eq!(
        decide(&defaults, &offline, &request("localhost", 8080)).verdict,
        Verdict::Allow
    );
    assert_eq!(
        decide(&defaults, &offline, &request("192.168.1.5", 80)).verdict,
        once_or_session()
    );
    let lan_allowed = network_policy(r#"local_network = "allow""#);
    assert_eq!(
        decide(&lan_allowed, &offline, &request("nas.local", 445)).verdict,
        Verdict::Allow
    );
    let lan_denied = network_policy(r#"local_network = "deny""#);
    assert_eq!(
        decide(&lan_denied, &offline, &request("192.168.1.5", 80)).verdict,
        Verdict::Deny
    );

    assert_eq!(
        decide(&defaults, &normal, &request("example.com", 443)).verdict,
        Verdict::Allow
    );
    assert_eq!(
        decide(&defaults, &normal, &request("8.8.8.8", 53)).verdict,
        once_or_session()
    );
    let no_ips = network_policy(r#"arbitrary_ip = "deny""#);
    assert_eq!(
        decide(&no_ips, &normal, &request("8.8.8.8", 53)).verdict,
        Verdict::Deny
    );
    assert_eq!(
        decide(&defaults, &normal, &shell("curl $URL")).verdict,
        once_or_session()
    );
}

const PRIVACY: [PrivacyMode; 4] = [
    PrivacyMode::Hybrid,
    PrivacyMode::CloudPreferred,
    PrivacyMode::LocalOnly,
    PrivacyMode::AirGapped,
];
const TRUST: [HostTrust; 5] = [
    HostTrust::Trusted,
    HostTrust::Known,
    HostTrust::New,
    HostTrust::Restricted,
    HostTrust::Untrusted,
];
const MODES: [NetworkMode; 4] = [
    NetworkMode::Unrestricted,
    NetworkMode::Normal,
    NetworkMode::Allowlist,
    NetworkMode::Offline,
];

fn rank(decision: &Decision) -> u8 {
    decision.verdict.rank()
}

#[test]
fn privacy_trust_and_network_mode_never_loosen_a_decision() {
    let samples = sample();
    for policies in [PolicySet::new(), allow_everything()] {
        for profile in Profile::ALL {
            let open = ctx(
                *profile,
                PrivacyMode::Hybrid,
                HostTrust::Trusted,
                NetworkMode::Unrestricted,
            );
            for intent in &samples {
                let baseline = rank(&decide(&policies, &open, intent));
                for privacy in PRIVACY {
                    for trust in TRUST {
                        for mode in MODES {
                            let tightened = ctx(*profile, privacy, trust, mode);
                            let decision = decide(&policies, &tightened, intent);
                            assert!(
                                rank(&decision) >= baseline,
                                "{} under {profile}/{privacy}/{trust}/{mode} loosened to {:?}",
                                intent.action,
                                decision.verdict
                            );
                        }
                    }
                }
            }
        }
    }
}

fn sample() -> Vec<aidex_policy::Intent> {
    vec![
        write_inside("src/lib.rs"),
        write_outside("/etc/hosts"),
        shell("ls"),
        shell("cargo test"),
        shell("npm install x"),
        shell("curl https://example.com"),
        request("example.com", 443),
        request("8.8.8.8", 53),
        request("localhost", 80),
        request("10.1.2.3", 80),
        push_to("github.com"),
        intent(Action::GitPull),
        secret(),
        model(ModelLocality::Remote),
        model(ModelLocality::Local),
        intent(Action::HostMount),
        intent(Action::PackageInstall),
        intent(Action::McpCall),
        intent(Action::RemoteAction),
    ]
}
