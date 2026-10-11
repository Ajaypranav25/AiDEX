//! References to broker approvals, bound to the intent fingerprint they approve.
//!
//! The broker owns approvals and emits `approval.*` events; the session store keeps
//! a reference so a checkpoint can list open approvals and a resume can report the
//! ones that must be requested again. An approval is never transferred to a branch.

use aidex_core::{EntityId, Timestamp};
pub use aidex_events::ApprovalScope;
use rusqlite::{Connection, OptionalExtension, Row, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

use crate::{
    SessionError, SessionStore,
    store::rows::{self, load_session},
};

/// Broker decision state of an approval.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalStatus {
    Pending,
    Approved,
    Denied,
    Expired,
    Revoked,
    Cancelled,
}

impl ApprovalStatus {
    pub const ALL: [Self; 6] = [
        Self::Pending,
        Self::Approved,
        Self::Denied,
        Self::Expired,
        Self::Revoked,
        Self::Cancelled,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Approved => "approved",
            Self::Denied => "denied",
            Self::Expired => "expired",
            Self::Revoked => "revoked",
            Self::Cancelled => "cancelled",
        }
    }

    /// Denied, expired, revoked, and cancelled approvals never change again.
    pub const fn is_final(self) -> bool {
        matches!(
            self,
            Self::Denied | Self::Expired | Self::Revoked | Self::Cancelled
        )
    }

    /// Allowed status changes: pending → anything; approved → expired/revoked/cancelled.
    pub const fn can_become(self, next: Self) -> bool {
        match self {
            Self::Pending => true,
            Self::Approved => matches!(
                next,
                Self::Approved | Self::Expired | Self::Revoked | Self::Cancelled
            ),
            _ => false,
        }
    }

    fn parse(name: &str) -> Result<Self, SessionError> {
        Self::ALL
            .into_iter()
            .find(|status| status.as_str() == name)
            .ok_or_else(|| rows::corrupt("invalid approval status"))
    }
}

/// What the broker reports about one approval.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovalRef {
    pub approval_id: EntityId,
    /// `aidex_policy::Intent::fingerprint_hex()` of the approved intent (64 hex).
    pub intent_fingerprint_hex: String,
    pub decision: ApprovalStatus,
    pub scope: ApprovalScope,
    pub expires_at: Option<Timestamp>,
}

/// A stored approval reference.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovalRecord {
    pub session_id: EntityId,
    pub approval: ApprovalRef,
    pub recorded_at: Timestamp,
    pub updated_at: Timestamp,
}

impl ApprovalRecord {
    /// Pending and not past its expiry at `now`.
    pub fn is_open(&self, now: &Timestamp) -> bool {
        self.approval.decision == ApprovalStatus::Pending
            && self.approval.expires_at.as_ref().is_none_or(|at| at > now)
    }
}

fn scope_name(scope: ApprovalScope) -> &'static str {
    match scope {
        ApprovalScope::Once => "once",
        ApprovalScope::Session => "session",
        ApprovalScope::Workspace => "workspace",
        ApprovalScope::Always => "always",
    }
}

fn parse_scope(name: &str) -> Result<ApprovalScope, SessionError> {
    Ok(match name {
        "once" => ApprovalScope::Once,
        "session" => ApprovalScope::Session,
        "workspace" => ApprovalScope::Workspace,
        "always" => ApprovalScope::Always,
        _ => return Err(rows::corrupt("invalid approval scope")),
    })
}

fn valid_fingerprint(hex: &str) -> bool {
    hex.len() == 64
        && hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

type ApprovalRow = (
    String,
    String,
    String,
    String,
    String,
    Option<String>,
    String,
    String,
);

const APPROVAL_COLUMNS: &str = "session_id, approval_id, intent_fingerprint, status, scope, expires_at, recorded_at, updated_at";

fn approval_row(row: &Row<'_>) -> rusqlite::Result<ApprovalRow> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
    ))
}

fn decode(
    (session, approval, fingerprint, status, scope, expires, recorded, updated): ApprovalRow,
) -> Result<ApprovalRecord, SessionError> {
    if !valid_fingerprint(&fingerprint) {
        return Err(rows::corrupt("invalid approval intent fingerprint"));
    }
    Ok(ApprovalRecord {
        session_id: rows::id(session, "approval session id")?,
        approval: ApprovalRef {
            approval_id: rows::id(approval, "approval id")?,
            intent_fingerprint_hex: fingerprint,
            decision: ApprovalStatus::parse(&status)?,
            scope: parse_scope(&scope)?,
            expires_at: rows::opt_time(expires, "approval expiry")?,
        },
        recorded_at: rows::time(recorded, "approval time")?,
        updated_at: rows::time(updated, "approval update time")?,
    })
}

pub(crate) fn load_approvals(
    connection: &Connection,
    session_id: &EntityId,
) -> Result<Vec<ApprovalRecord>, SessionError> {
    let mut statement = connection.prepare(&format!(
        "SELECT {APPROVAL_COLUMNS} FROM approvals WHERE session_id = ?1 ORDER BY rowid"
    ))?;
    let rows = statement
        .query_map([session_id.as_str()], approval_row)?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter().map(decode).collect()
}

/// Cancel every pending approval of `session_id`; returns their ids.
pub(crate) fn cancel_pending(
    connection: &Connection,
    session_id: &EntityId,
) -> Result<Vec<EntityId>, SessionError> {
    let pending: Vec<EntityId> = load_approvals(connection, session_id)?
        .into_iter()
        .filter(|record| record.approval.decision == ApprovalStatus::Pending)
        .map(|record| record.approval.approval_id)
        .collect();
    let now = Timestamp::now();
    for approval in &pending {
        connection.execute(
            "UPDATE approvals SET status = 'cancelled', updated_at = ?3 WHERE session_id = ?1 AND approval_id = ?2",
            params![session_id.as_str(), approval.as_str(), now.as_str()],
        )?;
    }
    Ok(pending)
}

impl SessionStore {
    /// Record or update the broker's view of an approval for this session.
    ///
    /// The intent fingerprint is fixed at first record: an update naming a different
    /// fingerprint is a conflict (an approval never moves to another intent). Status
    /// changes follow [`ApprovalStatus::can_become`].
    pub fn record_approval_ref(
        &self,
        id: &EntityId,
        approval: ApprovalRef,
    ) -> Result<ApprovalRecord, SessionError> {
        if !valid_fingerprint(&approval.intent_fingerprint_hex) {
            return Err(SessionError::InvalidArgument(
                "intent_fingerprint_hex must be 64 lowercase hexadecimal characters".into(),
            ));
        }
        let mut connection = self.lock();
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        load_session(&transaction, id)?;
        let existing = transaction
            .query_row(
                &format!(
                    "SELECT {APPROVAL_COLUMNS} FROM approvals WHERE session_id = ?1 AND approval_id = ?2"
                ),
                params![id.as_str(), approval.approval_id.as_str()],
                approval_row,
            )
            .optional()?
            .map(decode)
            .transpose()?;
        let now = Timestamp::now();
        match existing {
            Some(current) => {
                if current.approval.intent_fingerprint_hex != approval.intent_fingerprint_hex {
                    return Err(SessionError::Conflict(format!(
                        "approval `{}` is bound to a different intent",
                        approval.approval_id
                    )));
                }
                if !current.approval.decision.can_become(approval.decision) {
                    return Err(SessionError::Conflict(format!(
                        "approval `{}` is {} and cannot become {}",
                        approval.approval_id,
                        current.approval.decision.as_str(),
                        approval.decision.as_str()
                    )));
                }
                transaction.execute(
                    "UPDATE approvals SET status = ?3, scope = ?4, expires_at = ?5, updated_at = ?6
                     WHERE session_id = ?1 AND approval_id = ?2",
                    params![
                        id.as_str(),
                        approval.approval_id.as_str(),
                        approval.decision.as_str(),
                        scope_name(approval.scope),
                        approval.expires_at.as_ref().map(Timestamp::as_str),
                        now.as_str(),
                    ],
                )?;
            }
            None => {
                transaction.execute(
                    "INSERT INTO approvals (session_id, approval_id, intent_fingerprint, status, scope, expires_at, recorded_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
                    params![
                        id.as_str(),
                        approval.approval_id.as_str(),
                        approval.intent_fingerprint_hex,
                        approval.decision.as_str(),
                        scope_name(approval.scope),
                        approval.expires_at.as_ref().map(Timestamp::as_str),
                        now.as_str(),
                    ],
                )?;
            }
        }
        let stored = transaction
            .query_row(
                &format!(
                    "SELECT {APPROVAL_COLUMNS} FROM approvals WHERE session_id = ?1 AND approval_id = ?2"
                ),
                params![id.as_str(), approval.approval_id.as_str()],
                approval_row,
            )
            .map_err(SessionError::from)
            .and_then(decode)?;
        transaction.commit()?;
        Ok(stored)
    }

    /// Every approval reference of the session in recording order.
    pub fn approvals(&self, id: &EntityId) -> Result<Vec<ApprovalRecord>, SessionError> {
        let connection = self.lock();
        load_session(&connection, id)?;
        load_approvals(&connection, id)
    }
}
