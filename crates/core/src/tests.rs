use super::*;

#[test]
fn generated_v1_schema_matches_checked_in_client_boundary() {
    let generated = format!(
        "{}\n",
        serde_json::to_string_pretty(&schemars::schema_for!(Versioned<ContractPayload>)).unwrap()
    );
    assert_eq!(
        generated,
        include_str!("../../../protocol/schemas/contract-v1.schema.json")
    );
}

#[test]
fn validated_types_reject_malformed_wire_values() {
    assert!(serde_json::from_str::<EntityId>("\"bad/id\"").is_err());
    assert!(serde_json::from_str::<WorkspacePath>("\"../outside\"").is_err());
    assert!(serde_json::from_str::<WorkspacePath>("\"C:/outside\"").is_err());
    assert!(serde_json::from_str::<WorkspacePath>("\"line\\nbreak\"").is_err());
    assert!(serde_json::from_str::<Timestamp>("\"2024-02-30T12:00:00Z\"").is_err());
    assert!(serde_json::from_str::<Timestamp>("\"not-a-time\"").is_err());
    assert!(serde_json::from_str::<ContractVersion>("0").is_err());
    assert_eq!(ContractVersion::new(0), Err(ContractVersionError));
    assert_eq!(ContractVersion::try_from(2).unwrap().get(), 2);
}

#[test]
fn offers_negotiate_highest_common_version_and_reject_invalid_ranges() {
    let offer = |minimum, maximum| ContractOffer {
        minimum: ContractVersion::new(minimum).unwrap(),
        maximum: ContractVersion::new(maximum).unwrap(),
    };
    assert_eq!(offer(1, 5).negotiate(&offer(3, 7)).unwrap().get(), 5);
    assert_eq!(
        offer(4, 2).negotiate(&offer(1, 5)),
        Err(ContractNegotiationError::InvalidRange)
    );
    assert_eq!(
        offer(1, 2).negotiate(&offer(3, 4)),
        Err(ContractNegotiationError::IncompatibleVersion)
    );
    assert_eq!(
        ContractNegotiationError::IncompatibleVersion.to_string(),
        "no compatible contract version"
    );
}
#[test]
fn versioned_requests_and_events_round_trip_and_check_version() {
    let request = Versioned::new(ToolIntentRequest {
        session_id: EntityId::new("session-1").unwrap(),
        tool_id: EntityId::new("shell").unwrap(),
        input: serde_json::json!({"command":"pwd"}),
        requested_at: Timestamp::new("2026-10-10T07:51:01Z").unwrap(),
    });
    let json = serde_json::to_string(&request).unwrap();
    let decoded: Versioned<ToolIntentRequest> = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded, request);
    assert_eq!(decoded.validate_version(), Ok(()));
    let incompatible = serde_json::from_str::<Versioned<ToolIntentRequest>>(
        r#"{"version":2,"payload":{"session_id":"session-1","tool_id":"shell","input":{},"requested_at":"2026-10-10T07:51:01Z"}}"#,
    ).unwrap();
    assert_eq!(
        incompatible.validate_version(),
        Err(ServiceErrorCode::IncompatibleVersion)
    );
    let event = ContractEvent::ApprovalRecorded {
        approval_id: EntityId::new("approval-1").unwrap(),
        intent_id: EntityId::new("intent-1").unwrap(),
        decision: ApprovalDecision::Approve,
        occurred_at: Timestamp::new("2026-10-10T07:51:01Z").unwrap(),
    };
    assert_eq!(
        serde_json::from_str::<ContractEvent>(&serde_json::to_string(&event).unwrap()).unwrap(),
        event
    );
}

#[test]
fn secret_contract_contains_references_only() {
    let secret = SecretRef {
        vault_id: EntityId::new("vault-1").unwrap(),
        secret_id: EntityId::new("token").unwrap(),
    };
    let json = serde_json::to_value(secret).unwrap();
    assert_eq!(
        json,
        serde_json::json!({"vault_id":"vault-1","secret_id":"token"})
    );
    assert!(
        serde_json::from_str::<SecretRef>(
            r#"{"vault_id":"v","secret_id":"s","value":"raw-secret"}"#
        )
        .is_err()
    );
}
