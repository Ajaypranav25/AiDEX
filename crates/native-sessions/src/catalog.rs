//! Thread-safe in-process session lifecycle catalog; persistence is a future domain concern.

use aidex_core::EntityId;
use std::{
    collections::BTreeMap,
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::SystemTime,
};

static NEXT_SESSION: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Session {
    pub id: EntityId,
    pub created_at: SystemTime,
    pub closed_at: Option<SystemTime>,
}

impl Session {
    pub fn is_open(&self) -> bool {
        self.closed_at.is_none()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionError {
    NotFound(EntityId),
}

#[derive(Default)]
pub struct SessionCatalog {
    sessions: Mutex<BTreeMap<EntityId, Session>>,
}

impl SessionCatalog {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create and register a new open session with a process-local unique ID.
    pub fn create(&self) -> Session {
        let sequence = NEXT_SESSION.fetch_add(1, Ordering::Relaxed);
        let id = EntityId::new(format!("session:{sequence}"))
            .expect("generated session identifier is valid");
        let session = Session {
            id: id.clone(),
            created_at: SystemTime::now(),
            closed_at: None,
        };
        self.sessions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id, session.clone());
        session
    }

    pub fn get(&self, id: &EntityId) -> Option<Session> {
        self.sessions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(id)
            .cloned()
    }

    pub fn list(&self) -> Vec<Session> {
        self.sessions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .cloned()
            .collect()
    }

    /// Close a session once; repeated closes preserve the original close time.
    pub fn close(&self, id: &EntityId) -> Result<Session, SessionError> {
        let mut sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
        let session = sessions
            .get_mut(id)
            .ok_or_else(|| SessionError::NotFound(id.clone()))?;
        if session.closed_at.is_none() {
            session.closed_at = Some(SystemTime::now());
        }
        Ok(session.clone())
    }
}
