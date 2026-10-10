use aidex_adapters::*;
#[test]
fn finite_catalog_carries_real_protocol_launch_commands_and_scope_limits() {
    let catalog = catalog();
    assert_eq!(catalog.len(), 10);
    for driver in Driver::ALL {
        let info = catalog.iter().find(|i| i.driver == driver).unwrap();
        assert!(!info.protocol.is_empty());
        assert!(!info.limitations.is_empty());
        assert!(!info.permission_modes.is_empty());
    }
    assert_eq!(
        catalog
            .iter()
            .find(|i| i.driver == Driver::Codex)
            .unwrap()
            .command,
        ["app-server"]
    );
    assert_eq!(
        catalog
            .iter()
            .find(|i| i.driver == Driver::Grok)
            .unwrap()
            .command,
        ["agent", "stdio"]
    );
    assert_eq!(
        catalog
            .iter()
            .find(|i| i.driver == Driver::Pi)
            .unwrap()
            .command,
        ["--mode", "rpc"]
    );
}
