//! Encrypted, selected-workspace memory. Names and content never enter SQL metadata.
use crate::{MemoryRecord, SessionStore, StoreError, id, now};
use chacha20poly1305::{
    KeyInit, XChaCha20Poly1305, XNonce,
    aead::{Aead, Payload},
};
use rusqlite::{Connection, params};
use zeroize::Zeroizing;
impl SessionStore {
    fn memories(&self, db: &Connection, workspace: &str) -> Result<Vec<MemoryRecord>, StoreError> {
        let key = self.key()?;
        let cipher =
            XChaCha20Poly1305::new_from_slice(key.expose()).map_err(|_| "invalid memory key")?;
        let mut stmt = db
            .prepare("SELECT id,payload FROM memory WHERE workspace=? ORDER BY id")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([workspace], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?))
            })
            .map_err(|e| e.to_string())?;
        let mut out = vec![];
        for row in rows {
            let (id, bytes) = row.map_err(|e| e.to_string())?;
            if bytes.len() < 40 {
                return Err("truncated encrypted memory".into());
            }
            let aad = format!("aidex-native-v1:memory:{id}:{workspace}");
            let plain = Zeroizing::new(
                cipher
                    .decrypt(
                        XNonce::from_slice(&bytes[..24]),
                        Payload {
                            msg: &bytes[24..],
                            aad: aad.as_bytes(),
                        },
                    )
                    .map_err(|_| "memory authentication failed")?,
            );
            let record: MemoryRecord =
                serde_json::from_slice(&plain).map_err(|_| "invalid encrypted memory")?;
            if record.id != id || record.workspace_id != workspace {
                return Err("memory identity mismatch".into());
            }
            out.push(record);
        }
        Ok(out)
    }
    pub fn memory_list(&self, workspace: &str) -> Result<Vec<MemoryRecord>, StoreError> {
        let db = self.db.lock().map_err(|_| "session store poisoned")?;
        self.memories(&db, workspace)
    }
    pub fn memory_set(
        &self,
        workspace: &str,
        key: &str,
        content: &str,
    ) -> Result<MemoryRecord, StoreError> {
        aidex_core::EntityId::new(workspace).map_err(|e| e.to_string())?;
        if key.trim().is_empty() || key.len() > 256 || content.len() > 64 * 1024 {
            return Err("invalid memory size or key".into());
        }
        let mut db = self.db.lock().map_err(|_| "session store poisoned")?;
        let tx = db.transaction().map_err(|e| e.to_string())?;
        let existing = self
            .memories(&tx, workspace)?
            .into_iter()
            .find(|r| r.key == key);
        let record = MemoryRecord {
            id: existing.map(|r| r.id).unwrap_or(id("memory")?),
            workspace_id: workspace.into(),
            key: key.into(),
            content: content.into(),
            updated_at: now(),
        };
        let secret = self.key()?;
        let cipher =
            XChaCha20Poly1305::new_from_slice(secret.expose()).map_err(|_| "invalid memory key")?;
        let plain = Zeroizing::new(serde_json::to_vec(&record).map_err(|e| e.to_string())?);
        let mut nonce = [0u8; 24];
        getrandom::fill(&mut nonce).map_err(|e| e.to_string())?;
        let aad = format!("aidex-native-v1:memory:{}:{workspace}", record.id);
        let bytes = cipher
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: &plain,
                    aad: aad.as_bytes(),
                },
            )
            .map_err(|_| "memory encryption failed")?;
        let mut payload = nonce.to_vec();
        payload.extend(bytes);
        tx.execute("INSERT INTO memory(id,workspace,payload) VALUES(?,?,?) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload",params![record.id,workspace,payload]).map_err(|e|e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(record)
    }
    pub fn memory_delete(&self, workspace: &str, id: &str) -> Result<bool, StoreError> {
        let db = self.db.lock().map_err(|_| "session store poisoned")?;
        self.key()?;
        Ok(db
            .execute(
                "DELETE FROM memory WHERE workspace=? AND id=?",
                params![workspace, id],
            )
            .map_err(|e| e.to_string())?
            > 0)
    }
}
