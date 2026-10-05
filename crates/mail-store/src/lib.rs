//! SQLite metadata plus immutable content-addressed raw entities.
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
use thiserror::Error;

fn visible(value: &str, max: usize) -> bool {
    !value.is_empty() && value.len() <= max && value.bytes().all(|b| (0x21..=0x7e).contains(&b))
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("database error")]
    Database(#[from] rusqlite::Error),
    #[error("filesystem error")]
    Filesystem(#[from] std::io::Error),
    #[error("stored blob is missing or corrupt")]
    CorruptBlob,
    #[error("database schema version is newer than this store supports")]
    UnsupportedSchema,
    #[error("outbox id already refers to different content, recipients, or a non-retryable state")]
    OutboxConflict,
    #[error("content exceeds the configured size limit")]
    ContentTooLarge,
    #[error("metadata exceeds the configured size or character limit")]
    InvalidMetadata,
}

#[derive(Debug)]
pub struct Store {
    db: Connection,
    root: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredMessage {
    pub id: String,
    pub account_id: String,
    pub uidl: Option<String>,
    pub blob_id: Option<String>,
    pub receive_state: String,
}

impl Store {
    pub fn open(root: impl AsRef<Path>) -> Result<Self, StoreError> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(root.join("blobs"))?;
        let db = Connection::open(root.join("mail.sqlite3"))?;
        db.pragma_update(None, "foreign_keys", "ON")?;
        db.execute_batch("BEGIN IMMEDIATE;
            CREATE TABLE IF NOT EXISTS schema_version(version INTEGER NOT NULL);
            INSERT INTO schema_version(version) SELECT 1 WHERE NOT EXISTS (SELECT 1 FROM schema_version);
            CREATE TABLE IF NOT EXISTS raw_entities(id TEXT PRIMARY KEY, size INTEGER NOT NULL, sha256 BLOB NOT NULL);
            CREATE TABLE IF NOT EXISTS messages(id TEXT PRIMARY KEY, account_id TEXT NOT NULL, uidl TEXT, blob_id TEXT REFERENCES raw_entities(id), receive_state TEXT NOT NULL);
            CREATE UNIQUE INDEX IF NOT EXISTS messages_account_uidl ON messages(account_id, uidl) WHERE uidl IS NOT NULL;
            CREATE TABLE IF NOT EXISTS drafts(id TEXT PRIMARY KEY, blob_id TEXT REFERENCES raw_entities(id));
            CREATE TABLE IF NOT EXISTS outbox(id TEXT PRIMARY KEY, blob_id TEXT REFERENCES raw_entities(id), state TEXT NOT NULL, stage INTEGER NOT NULL DEFAULT 0);
            COMMIT;")?;
        let version: i64 = db.query_row("SELECT version FROM schema_version", [], |r| r.get(0))?;
        if version > 3 {
            return Err(StoreError::UnsupportedSchema);
        }
        if version == 1 {
            db.execute_batch("BEGIN IMMEDIATE;
                CREATE TABLE IF NOT EXISTS outbox_recipients(outbox_id TEXT NOT NULL REFERENCES outbox(id) ON DELETE CASCADE, ordinal INTEGER NOT NULL, address TEXT NOT NULL, kind TEXT NOT NULL, PRIMARY KEY(outbox_id,ordinal));
                CREATE TABLE IF NOT EXISTS sent_entities(id TEXT PRIMARY KEY, account_id TEXT NOT NULL, blob_id TEXT NOT NULL REFERENCES raw_entities(id));
                CREATE TABLE IF NOT EXISTS outbox_senders(outbox_id TEXT PRIMARY KEY REFERENCES outbox(id) ON DELETE CASCADE, sender TEXT NOT NULL);
                UPDATE schema_version SET version=2 WHERE version=1;
                COMMIT;")?;
        }
        let version: i64 = db.query_row("SELECT version FROM schema_version", [], |r| r.get(0))?;
        if version == 2 {
            db.execute_batch("BEGIN IMMEDIATE; CREATE TABLE IF NOT EXISTS local_deletions(entity_id TEXT PRIMARY KEY); UPDATE schema_version SET version=3 WHERE version=2; COMMIT;")?;
        }
        let store = Self { db, root };
        store.recover_temps()?;
        Ok(store)
    }

    pub fn schema_version(&self) -> Result<i64, StoreError> {
        Ok(self
            .db
            .query_row("SELECT version FROM schema_version", [], |r| r.get(0))?)
    }

    /// Writes and syncs a temporary content file, atomically publishes it, then
    /// records metadata. A failed DB commit may leave an unreferenced immutable blob.
    pub fn put_raw(&mut self, bytes: &[u8]) -> Result<String, StoreError> {
        if bytes.len() > 25 * 1024 * 1024 {
            return Err(StoreError::ContentTooLarge);
        }
        let digest = Sha256::digest(bytes);
        let id = digest
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let final_path = self.root.join("blobs").join(&id);
        if final_path.exists() {
            let existing = fs::read(&final_path)?;
            if existing.len() != bytes.len()
                || Sha256::digest(&existing).as_slice() != digest.as_slice()
            {
                return Err(StoreError::CorruptBlob);
            }
        } else {
            let (tmp, mut file) = (0_u64..)
                .find_map(|n| {
                    let candidate = self.root.join("blobs").join(format!(".{id}.{n}.tmp"));
                    match fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&candidate)
                    {
                        Ok(file) => Some(Ok((candidate, file))),
                        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => None,
                        Err(error) => Some(Err(error)),
                    }
                })
                .expect("unbounded temporary suffix iterator")?;
            file.write_all(bytes)?;
            file.sync_all()?;
            fs::rename(&tmp, &final_path)?;
            fs::File::open(self.root.join("blobs"))?.sync_all()?;
        }
        let tx = self.db.transaction()?;
        tx.execute(
            "INSERT OR IGNORE INTO raw_entities(id,size,sha256) VALUES(?1,?2,?3)",
            params![id, bytes.len() as i64, digest.as_slice()],
        )?;
        tx.commit()?;
        Ok(id)
    }

    pub fn get_raw(&self, id: &str) -> Result<Vec<u8>, StoreError> {
        if id.len() != 64 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(StoreError::CorruptBlob);
        }
        let meta: Option<(i64, Vec<u8>)> = self
            .db
            .query_row(
                "SELECT size,sha256 FROM raw_entities WHERE id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let (size, expected) = meta.ok_or(StoreError::CorruptBlob)?;
        let bytes = fs::read(self.root.join("blobs").join(id))?;
        if bytes.len() as i64 != size || Sha256::digest(&bytes).as_slice() != expected {
            return Err(StoreError::CorruptBlob);
        }
        Ok(bytes)
    }

    pub fn save_message(
        &mut self,
        id: &str,
        account: &str,
        uidl: Option<&str>,
        state: &str,
        raw: &[u8],
    ) -> Result<String, StoreError> {
        if !visible(id, 128)
            || !visible(account, 128)
            || !matches!(
                state,
                "RemoteKnown"
                    | "HeaderCached"
                    | "BodyCached"
                    | "DeletePending"
                    | "DeleteMarkedSession"
                    | "RemoteDeletionCommitted"
                    | "Sent"
            )
            || uidl.is_some_and(|u| !visible(u, 1024))
        {
            return Err(StoreError::InvalidMetadata);
        }
        let blob_id = self.put_raw(raw)?;
        self.db.execute("INSERT INTO messages(id,account_id,uidl,blob_id,receive_state) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(id) DO UPDATE SET uidl=excluded.uidl,blob_id=excluded.blob_id,receive_state=excluded.receive_state", params![id,account,uidl,blob_id,state])?;
        Ok(blob_id)
    }

    pub fn message_by_uidl(
        &self,
        account: &str,
        uidl: &str,
    ) -> Result<Option<StoredMessage>, StoreError> {
        Ok(self.db.query_row("SELECT id,account_id,uidl,blob_id,receive_state FROM messages WHERE account_id=?1 AND uidl=?2", params![account,uidl], |r| Ok(StoredMessage { id:r.get(0)?, account_id:r.get(1)?, uidl:r.get(2)?, blob_id:r.get(3)?, receive_state:r.get(4)? })).optional()?)
    }

    pub fn messages_for_account(&self, account: &str) -> Result<Vec<StoredMessage>, StoreError> {
        let mut stmt=self.db.prepare("SELECT id,account_id,uidl,blob_id,receive_state FROM messages WHERE account_id=?1 UNION ALL SELECT id,account_id,NULL,blob_id,'Sent' FROM sent_entities WHERE account_id=?1 ORDER BY id")?;
        Ok(stmt
            .query_map([account], |r| {
                Ok(StoredMessage {
                    id: r.get(0)?,
                    account_id: r.get(1)?,
                    uidl: r.get(2)?,
                    blob_id: r.get(3)?,
                    receive_state: r.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?)
    }

    pub fn raw_for_message(&self, id: &str) -> Result<Option<Vec<u8>>, StoreError> {
        let blob: Option<String> = self.db.query_row("SELECT blob_id FROM messages WHERE id=?1 UNION ALL SELECT blob_id FROM sent_entities WHERE id=?1 LIMIT 1",[id],|r|r.get(0)).optional()?;
        blob.map(|b| self.get_raw(&b)).transpose()
    }

    pub fn mark_delete_pending(&mut self, account: &str, uidl: &str) -> Result<bool, StoreError> {
        Ok(self.db.execute("UPDATE messages SET receive_state='DeletePending' WHERE account_id=?1 AND uidl=?2 AND receive_state IN ('HeaderCached','BodyCached','DeleteMarkedSession')",params![account,uidl])?==1)
    }

    pub fn set_receive_state(
        &mut self,
        account: &str,
        uidl: &str,
        state: &str,
    ) -> Result<bool, StoreError> {
        Ok(self.db.execute(
            "UPDATE messages SET receive_state=?3 WHERE account_id=?1 AND uidl=?2",
            params![account, uidl, state],
        )? == 1)
    }

    pub fn deletion_states(&self, account: &str) -> Result<Vec<(String, String)>, StoreError> {
        let mut stmt=self.db.prepare("SELECT uidl,receive_state FROM messages WHERE account_id=?1 AND uidl IS NOT NULL AND receive_state IN ('DeletePending','DeleteMarkedSession')")?;
        Ok(stmt
            .query_map([account], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?)
    }

    pub fn save_draft(&mut self, id: &str, raw: &[u8]) -> Result<String, StoreError> {
        if !visible(id, 128) {
            return Err(StoreError::InvalidMetadata);
        }
        let blob_id = self.put_raw(raw)?;
        self.db.execute("INSERT INTO drafts(id,blob_id) VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET blob_id=excluded.blob_id", params![id,blob_id])?;
        Ok(blob_id)
    }
    pub fn delete_draft(&mut self, id: &str) -> Result<bool, StoreError> {
        Ok(self.db.execute("DELETE FROM drafts WHERE id=?1", [id])? == 1)
    }
    pub fn local_delete(&mut self, id: &str) -> Result<bool, StoreError> {
        let exists:i64=self.db.query_row("SELECT COUNT(*) FROM (SELECT id FROM messages UNION ALL SELECT id FROM sent_entities) WHERE id=?1",[id],|r|r.get(0))?;
        if exists == 0 {
            return Ok(false);
        }
        self.db.execute(
            "INSERT OR IGNORE INTO local_deletions(entity_id) VALUES(?1)",
            [id],
        )?;
        Ok(true)
    }
    pub fn is_locally_deleted(&self, id: &str) -> Result<bool, StoreError> {
        Ok(self.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM local_deletions WHERE entity_id=?1)",
            [id],
            |r| r.get(0),
        )?)
    }

    pub fn queue_outbox(&mut self, id: &str, raw: &[u8]) -> Result<String, StoreError> {
        self.queue_outbox_with_recipients(id, raw, &[])
    }

    pub fn queue_outbox_with_recipients(
        &mut self,
        id: &str,
        raw: &[u8],
        recipients: &[(String, String)],
    ) -> Result<String, StoreError> {
        self.queue_outbox_full(id, "", raw, recipients)
    }

    pub fn queue_outbox_full(
        &mut self,
        id: &str,
        sender: &str,
        raw: &[u8],
        recipients: &[(String, String)],
    ) -> Result<String, StoreError> {
        if !visible(id, 128)
            || (!sender.is_empty() && !visible(sender, 320))
            || recipients.len() > 100
            || recipients
                .iter()
                .any(|(a, k)| !visible(a, 320) || !matches!(k.as_str(), "To" | "Cc" | "Bcc"))
        {
            return Err(StoreError::InvalidMetadata);
        }
        let blob_id = self.put_raw(raw)?;
        let tx = self.db.transaction()?;
        tx.execute("INSERT INTO outbox(id,blob_id,state,stage) VALUES(?1,?2,'Queued',0) ON CONFLICT(id) DO NOTHING",params![id,blob_id])?;
        tx.execute(
            "INSERT OR IGNORE INTO outbox_senders(outbox_id,sender) VALUES(?1,?2)",
            params![id, sender],
        )?;
        let mut insert=tx.prepare("INSERT OR IGNORE INTO outbox_recipients(outbox_id,ordinal,address,kind) VALUES(?1,?2,?3,?4)")?;
        for (ordinal, (address, kind)) in recipients.iter().enumerate() {
            insert.execute(params![id, ordinal as i64, address, kind])?;
        }
        drop(insert);
        let stored: (String, String) =
            tx.query_row("SELECT blob_id,state FROM outbox WHERE id=?1", [id], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?;
        if stored.0 != blob_id || !matches!(stored.1.as_str(), "Queued" | "FailedSafeToRetry") {
            return Err(StoreError::OutboxConflict);
        }
        let stored_sender: String = tx.query_row(
            "SELECT sender FROM outbox_senders WHERE outbox_id=?1",
            [id],
            |r| r.get(0),
        )?;
        if stored_sender != sender {
            return Err(StoreError::OutboxConflict);
        }
        let existing: Vec<(String, String)> = {
            let mut q = tx.prepare(
                "SELECT address,kind FROM outbox_recipients WHERE outbox_id=?1 ORDER BY ordinal",
            )?;
            q.query_map([id], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<Result<Vec<_>, _>>()?
        };
        if existing != recipients {
            return Err(StoreError::OutboxConflict);
        }
        tx.commit()?;
        Ok(blob_id)
    }

    pub fn outbox_recipients(&self, id: &str) -> Result<Vec<(String, String)>, StoreError> {
        let mut stmt = self.db.prepare(
            "SELECT address,kind FROM outbox_recipients WHERE outbox_id=?1 ORDER BY ordinal",
        )?;
        Ok(stmt
            .query_map([id], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?)
    }

    pub fn outbox_state(&self, id: &str) -> Result<Option<(String, i64)>, StoreError> {
        Ok(self
            .db
            .query_row("SELECT state,stage FROM outbox WHERE id=?1", [id], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .optional()?)
    }

    pub fn resolve_delivery_unknown(
        &mut self,
        outbox_id: &str,
        account_id: &str,
        mark_sent: bool,
    ) -> Result<bool, StoreError> {
        let tx = self.db.transaction()?;
        let blob: Option<String> = tx
            .query_row(
                "SELECT blob_id FROM outbox WHERE id=?1 AND state='DeliveryUnknown'",
                [outbox_id],
                |r| r.get(0),
            )
            .optional()?;
        let Some(blob) = blob else { return Ok(false) };
        if mark_sent {
            tx.execute(
                "UPDATE outbox SET state='Sent',stage=2 WHERE id=?1",
                [outbox_id],
            )?;
            tx.execute(
                "INSERT INTO sent_entities(id,account_id,blob_id) VALUES(?1,?2,?3)",
                params![format!("sent-{outbox_id}"), account_id, blob],
            )?;
        } else {
            tx.execute(
                "UPDATE outbox SET state='FailedSafeToRetry',stage=0 WHERE id=?1",
                [outbox_id],
            )?;
        }
        tx.commit()?;
        Ok(true)
    }

    pub fn finalize_sent(
        &mut self,
        outbox_id: &str,
        account_id: &str,
        sent_id: &str,
    ) -> Result<bool, StoreError> {
        let tx = self.db.transaction()?;
        let blob: Option<String> = tx
            .query_row(
                "SELECT blob_id FROM outbox WHERE id=?1 AND state='Submitting' AND stage>=1",
                [outbox_id],
                |r| r.get(0),
            )
            .optional()?;
        let Some(blob) = blob else {
            return Ok(false);
        };
        tx.execute(
            "UPDATE outbox SET state='Sent',stage=2 WHERE id=?1",
            [outbox_id],
        )?;
        tx.execute(
            "INSERT INTO sent_entities(id,account_id,blob_id) VALUES(?1,?2,?3)",
            params![sent_id, account_id, blob],
        )?;
        tx.commit()?;
        Ok(true)
    }

    pub fn sent_raw(&self, sent_id: &str) -> Result<Option<Vec<u8>>, StoreError> {
        let blob: Option<String> = self
            .db
            .query_row(
                "SELECT blob_id FROM sent_entities WHERE id=?1",
                [sent_id],
                |r| r.get(0),
            )
            .optional()?;
        blob.map(|id| self.get_raw(&id)).transpose()
    }

    pub fn outbox_raw(&self, id: &str) -> Result<Option<Vec<u8>>, StoreError> {
        let blob: Option<String> = self
            .db
            .query_row("SELECT blob_id FROM outbox WHERE id=?1", [id], |r| r.get(0))
            .optional()?;
        blob.map(|id| self.get_raw(&id)).transpose()
    }
    pub fn outbox_sender(&self, id: &str) -> Result<Option<String>, StoreError> {
        Ok(self
            .db
            .query_row(
                "SELECT sender FROM outbox_senders WHERE outbox_id=?1",
                [id],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// Compare-and-set prevents two senders acquiring the same outbox row.
    pub fn claim_outbox(&mut self, id: &str) -> Result<bool, StoreError> {
        Ok(self.db.execute("UPDATE outbox SET state='Submitting' WHERE id=?1 AND state IN ('Queued','FailedSafeToRetry')", [id])? == 1)
    }

    /// Durable stage 1 means DATA may have started; recovery must preserve ambiguity.
    pub fn set_outbox_stage(
        &mut self,
        id: &str,
        stage: i64,
        state: &str,
    ) -> Result<bool, StoreError> {
        Ok(self.db.execute(
            "UPDATE outbox SET stage=?2,state=?3 WHERE id=?1 AND state='Submitting'",
            params![id, stage, state],
        )? == 1)
    }

    pub fn recover_submitting(&mut self) -> Result<usize, StoreError> {
        Ok(self.db.execute("UPDATE outbox SET state=CASE WHEN stage >= 1 THEN 'DeliveryUnknown' ELSE 'FailedSafeToRetry' END WHERE state='Submitting'", [])?)
    }

    fn recover_temps(&self) -> Result<(), StoreError> {
        for entry in fs::read_dir(self.root.join("blobs"))? {
            let path = entry?.path();
            if path
                .file_name()
                .and_then(|x| x.to_str())
                .is_some_and(|x| x.ends_with(".tmp"))
            {
                fs::remove_file(path)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn raw_entity_survives_reopen_and_corruption_is_detected() {
        let dir = tempfile::tempdir().unwrap();
        let id;
        {
            let mut s = Store::open(dir.path()).unwrap();
            id = s.put_raw(b"Subject: x\r\n\r\nbody").unwrap();
            assert_eq!(s.schema_version().unwrap(), 3);
        }
        {
            let s = Store::open(dir.path()).unwrap();
            assert_eq!(s.get_raw(&id).unwrap(), b"Subject: x\r\n\r\nbody");
        }
        fs::write(dir.path().join("blobs").join(&id), b"bad").unwrap();
        assert!(matches!(
            Store::open(dir.path()).unwrap().get_raw(&id),
            Err(StoreError::CorruptBlob)
        ));
    }
    #[test]
    fn uidls_are_unique_per_account() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open(dir.path()).unwrap();
        s.db.execute("INSERT INTO messages(id,account_id,uidl,receive_state) VALUES('m1','a','opaque','RemoteKnown')",[]).unwrap();
        assert!(s.db.execute("INSERT INTO messages(id,account_id,uidl,receive_state) VALUES('m2','a','opaque','RemoteKnown')",[]).is_err());
        s.db.execute("INSERT INTO messages(id,account_id,uidl,receive_state) VALUES('m3','b','opaque','RemoteKnown')",[]).unwrap();
    }

    #[test]
    fn outbox_ownership_and_restart_recovery_keep_ambiguous_delivery() {
        let dir = tempfile::tempdir().unwrap();
        {
            let mut s = Store::open(dir.path()).unwrap();
            s.queue_outbox("out-1", b"synthetic mail").unwrap();
            assert!(s.claim_outbox("out-1").unwrap());
            assert!(!s.claim_outbox("out-1").unwrap());
            s.set_outbox_stage("out-1", 1, "Submitting").unwrap();
            assert_eq!(s.recover_submitting().unwrap(), 1);
        }
        let s = Store::open(dir.path()).unwrap();
        let state: String =
            s.db.query_row("SELECT state FROM outbox WHERE id='out-1'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(state, "DeliveryUnknown");
    }

    #[test]
    fn credentials_never_enter_persistence_apis_or_files() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = Store::open(dir.path()).unwrap();
        s.save_message(
            "m1",
            "a",
            Some("opaque"),
            "HeaderCached",
            b"Subject: safe\r\n\r\nbody",
        )
        .unwrap();
        drop(s);
        for entry in fs::read_dir(dir.path()).unwrap() {
            let path = entry.unwrap().path();
            if path.is_file() {
                assert!(
                    !fs::read(path)
                        .unwrap()
                        .windows(12)
                        .any(|w| w == b"password1234")
                );
            } else {
                for blob in fs::read_dir(path).unwrap() {
                    assert!(
                        !fs::read(blob.unwrap().path())
                            .unwrap()
                            .windows(12)
                            .any(|w| w == b"password1234")
                    );
                }
            }
        }
    }

    #[test]
    fn startup_removes_only_temporary_files() {
        let dir = tempfile::tempdir().unwrap();
        let blobs = dir.path().join("blobs");
        fs::create_dir_all(&blobs).unwrap();
        fs::write(blobs.join("orphan.tmp"), b"partial").unwrap();
        fs::write(blobs.join("opaque-content"), b"unreferenced immutable data").unwrap();
        Store::open(dir.path()).unwrap();
        assert!(!blobs.join("orphan.tmp").exists());
        assert!(blobs.join("opaque-content").exists());
    }

    #[test]
    fn duplicate_uidl_failure_does_not_publish_second_message() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = Store::open(dir.path()).unwrap();
        s.save_message("m1", "a", Some("opaque"), "HeaderCached", b"first")
            .unwrap();
        assert!(
            s.save_message("m2", "a", Some("opaque"), "HeaderCached", b"second")
                .is_err()
        );
        assert!(s.message_by_uidl("a", "opaque").unwrap().is_some());
        assert_eq!(s.messages_for_account("a").unwrap().len(), 1);
    }

    #[test]
    fn v1_database_migrates_to_v3_backend_schema_on_open() {
        let dir = tempfile::tempdir().unwrap();
        {
            let db = Connection::open(dir.path().join("mail.sqlite3")).unwrap();
            db.execute_batch("CREATE TABLE schema_version(version INTEGER NOT NULL); INSERT INTO schema_version VALUES(1); CREATE TABLE raw_entities(id TEXT PRIMARY KEY,size INTEGER NOT NULL,sha256 BLOB NOT NULL); CREATE TABLE outbox(id TEXT PRIMARY KEY,blob_id TEXT REFERENCES raw_entities(id),state TEXT NOT NULL,stage INTEGER NOT NULL DEFAULT 0);").unwrap();
        }
        let s = Store::open(dir.path()).unwrap();
        assert_eq!(s.schema_version().unwrap(), 3);
        let count:i64=s.db.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('outbox_recipients','sent_entities','outbox_senders','local_deletions')",[],|r|r.get(0)).unwrap();
        assert_eq!(count, 4);
        drop(s);
        assert_eq!(
            Store::open(dir.path()).unwrap().schema_version().unwrap(),
            3
        );
    }

    #[test]
    fn outbox_retry_cannot_replace_canonical_entity_or_retry_unknown_delivery() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = Store::open(dir.path()).unwrap();
        let recipients = vec![("to@i2p".to_owned(), "To".to_owned())];
        s.queue_outbox_with_recipients("retry", b"same", &recipients)
            .unwrap();
        assert!(s.claim_outbox("retry").unwrap());
        s.set_outbox_stage("retry", 0, "FailedSafeToRetry").unwrap();
        assert!(matches!(
            s.queue_outbox_with_recipients("retry", b"changed", &recipients),
            Err(StoreError::OutboxConflict)
        ));
        assert!(s.claim_outbox("retry").unwrap());
        s.set_outbox_stage("retry", 1, "DeliveryUnknown").unwrap();
        assert!(matches!(
            s.queue_outbox_with_recipients("retry", b"same", &recipients),
            Err(StoreError::OutboxConflict)
        ));
    }
}
