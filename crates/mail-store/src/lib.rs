//! SQLite metadata plus immutable content-addressed raw entities.
use i2pr_mail_domain::{ReceiveState, StoredMessageState, SubmissionProgress, SubmissionState};
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
use thiserror::Error;

/// Current schema version. v4 adds receive/submission state vocabulary plus
/// state/progress CHECK constraints; v1-v3 databases still migrate.
const SCHEMA_VERSION: i64 = 4;

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
    #[error("persisted state is not a value this backend version supports")]
    UnknownState,
    #[error("submission state and progress are not a representable combination")]
    InvalidStateCombination,
}

/// `CHECK` constraint listing every receive state this version can persist.
/// Derived from the domain vocabulary so SQL and Rust cannot drift.
fn receive_state_constraint() -> String {
    let tokens = ReceiveState::ALL
        .map(|state| format!("'{}'", state.as_storage_str()))
        .join(", ");
    format!("receive_state TEXT NOT NULL CHECK(receive_state IN ({tokens}))")
}

/// `CHECK` constraint admitting only state/progress pairs the domain model can
/// represent, so an impossible combination cannot be written at all.
fn submission_state_constraint() -> String {
    let clauses = SubmissionState::ALL
        .iter()
        .map(|state| {
            let stages = state
                .allowed_progress()
                .iter()
                .map(|progress| progress.as_storage_i64().to_string())
                .collect::<Vec<_>>()
                .join(", ");
            let progress = if state.allowed_progress().len() == 1 {
                format!("stage={stages}")
            } else {
                format!("stage IN ({stages})")
            };
            format!("(state='{}' AND {progress})", state.as_storage_str())
        })
        .collect::<Vec<_>>()
        .join(" OR ");
    format!("state TEXT NOT NULL, stage INTEGER NOT NULL DEFAULT 0, CHECK ({clauses})")
}

/// Decodes a persisted `messages.receive_state` value. Only receive states are
/// stored there; the `Sent` label is a projection of delivered submissions and is
/// rejected here so a corrupt or unsupported value fails closed.
fn decode_receive_state(value: &str) -> Result<ReceiveState, StoreError> {
    ReceiveState::from_storage_str(value).ok_or(StoreError::UnknownState)
}

/// Decodes the message-list projection, which also carries the `Sent` label that
/// `sent_entities` contributes to a listing.
fn decode_stored_message_state(value: &str) -> Result<StoredMessageState, StoreError> {
    StoredMessageState::from_storage_str(value).ok_or(StoreError::UnknownState)
}

fn decode_submission(
    state: &str,
    stage: i64,
) -> Result<(SubmissionState, SubmissionProgress), StoreError> {
    let state = SubmissionState::from_storage_str(state).ok_or(StoreError::UnknownState)?;
    let progress = SubmissionProgress::from_storage_i64(stage).ok_or(StoreError::UnknownState)?;
    if !state.allows(progress) {
        return Err(StoreError::UnknownState);
    }
    Ok((state, progress))
}

#[derive(Debug)]
pub struct Store {
    db: Connection,
    root: PathBuf,
}

/// Raw persisted message row, before typed state decoding.
type MessageRow = (String, String, Option<String>, Option<String>, String);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredMessage {
    pub id: String,
    pub account_id: String,
    pub uidl: Option<String>,
    pub blob_id: Option<String>,
    pub state: StoredMessageState,
}

impl Store {
    pub fn open(root: impl AsRef<Path>) -> Result<Self, StoreError> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(root.join("blobs"))?;
        let db = Connection::open(root.join("mail.sqlite3"))?;
        db.pragma_update(None, "foreign_keys", "ON")?;
        db.execute_batch(
            "BEGIN IMMEDIATE;
            CREATE TABLE IF NOT EXISTS schema_version(version INTEGER NOT NULL);
            COMMIT;",
        )?;
        let version: Option<i64> = db
            .query_row("SELECT version FROM schema_version LIMIT 1", [], |r| {
                r.get(0)
            })
            .optional()?;
        let mut fresh = false;
        match version {
            // A brand new store is created directly at the current version.
            None => fresh = true,
            Some(version) if version > SCHEMA_VERSION => {
                return Err(StoreError::UnsupportedSchema);
            }
            Some(version) => self::migrate(&db, version)?,
        }
        let store = Self { db, root };
        if fresh {
            store.initialize()?;
        }
        store.recover_temps()?;
        Ok(store)
    }

    /// Creates the current-version schema for a new store.
    fn initialize(&self) -> Result<(), StoreError> {
        self.db.execute_batch(&format!(
            "BEGIN IMMEDIATE;
            CREATE TABLE IF NOT EXISTS raw_entities(id TEXT PRIMARY KEY, size INTEGER NOT NULL, sha256 BLOB NOT NULL);
            CREATE TABLE IF NOT EXISTS messages(id TEXT PRIMARY KEY, account_id TEXT NOT NULL, uidl TEXT, blob_id TEXT REFERENCES raw_entities(id), {});
            CREATE UNIQUE INDEX IF NOT EXISTS messages_account_uidl ON messages(account_id, uidl) WHERE uidl IS NOT NULL;
            CREATE TABLE IF NOT EXISTS drafts(id TEXT PRIMARY KEY, blob_id TEXT REFERENCES raw_entities(id));
            CREATE TABLE IF NOT EXISTS outbox(id TEXT PRIMARY KEY, blob_id TEXT REFERENCES raw_entities(id), {});
            CREATE TABLE IF NOT EXISTS outbox_recipients(outbox_id TEXT NOT NULL REFERENCES outbox(id) ON DELETE CASCADE, ordinal INTEGER NOT NULL, address TEXT NOT NULL, kind TEXT NOT NULL, PRIMARY KEY(outbox_id,ordinal));
            CREATE TABLE IF NOT EXISTS sent_entities(id TEXT PRIMARY KEY, account_id TEXT NOT NULL, blob_id TEXT NOT NULL REFERENCES raw_entities(id));
            CREATE TABLE IF NOT EXISTS outbox_senders(outbox_id TEXT PRIMARY KEY REFERENCES outbox(id) ON DELETE CASCADE, sender TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS local_deletions(entity_id TEXT PRIMARY KEY);
            INSERT INTO schema_version(version) SELECT {} WHERE NOT EXISTS (SELECT 1 FROM schema_version);
            COMMIT;",
            receive_state_constraint(),
            submission_state_constraint(),
            SCHEMA_VERSION
        ))?;
        Ok(())
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
        state: ReceiveState,
        raw: &[u8],
    ) -> Result<String, StoreError> {
        if !visible(id, 128) || !visible(account, 128) || uidl.is_some_and(|u| !visible(u, 1024)) {
            return Err(StoreError::InvalidMetadata);
        }
        let blob_id = self.put_raw(raw)?;
        self.db.execute("INSERT INTO messages(id,account_id,uidl,blob_id,receive_state) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(id) DO UPDATE SET uidl=excluded.uidl,blob_id=excluded.blob_id,receive_state=excluded.receive_state", params![id,account,uidl,blob_id,state.as_storage_str()])?;
        Ok(blob_id)
    }

    pub fn message_by_uidl(
        &self,
        account: &str,
        uidl: &str,
    ) -> Result<Option<StoredMessage>, StoreError> {
        let row: Option<MessageRow> = self
            .db
            .query_row("SELECT id,account_id,uidl,blob_id,receive_state FROM messages WHERE account_id=?1 AND uidl=?2", params![account,uidl], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
            .optional()?;
        row.map(|(id, account_id, uidl, blob_id, receive_state)| {
            Ok(StoredMessage {
                id,
                account_id,
                uidl,
                blob_id,
                state: StoredMessageState::Receive(decode_receive_state(&receive_state)?),
            })
        })
        .transpose()
    }

    pub fn messages_for_account(&self, account: &str) -> Result<Vec<StoredMessage>, StoreError> {
        // Decoding happens outside the row closure so a corrupt persisted value
        // surfaces as a fail-closed store error, not a generic database error.
        let rows: Vec<MessageRow> = {
            let mut stmt=self.db.prepare("SELECT id,account_id,uidl,blob_id,receive_state FROM messages WHERE account_id=?1 UNION ALL SELECT id,account_id,NULL,blob_id,'Sent' FROM sent_entities WHERE account_id=?1 ORDER BY id")?;
            stmt.query_map([account], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })?
            .collect::<Result<Vec<_>, _>>()?
        };
        rows.into_iter()
            .map(|(id, account_id, uidl, blob_id, receive_state)| {
                Ok(StoredMessage {
                    id,
                    account_id,
                    uidl,
                    blob_id,
                    state: decode_stored_message_state(&receive_state)?,
                })
            })
            .collect()
    }

    pub fn raw_for_message(&self, id: &str) -> Result<Option<Vec<u8>>, StoreError> {
        let blob: Option<String> = self.db.query_row("SELECT blob_id FROM messages WHERE id=?1 UNION ALL SELECT blob_id FROM sent_entities WHERE id=?1 LIMIT 1",[id],|r|r.get(0)).optional()?;
        blob.map(|b| self.get_raw(&b)).transpose()
    }

    pub fn mark_delete_pending(&mut self, account: &str, uidl: &str) -> Result<bool, StoreError> {
        let allowed = [
            ReceiveState::HeaderCached,
            ReceiveState::BodyCached,
            ReceiveState::DeleteMarkedSession,
        ]
        .map(|state| state.as_storage_str());
        Ok(self.db.execute("UPDATE messages SET receive_state=?3 WHERE account_id=?1 AND uidl=?2 AND receive_state IN (?4,?5,?6)",params![account,uidl,ReceiveState::DeletePending.as_storage_str(),allowed[0],allowed[1],allowed[2]])?==1)
    }

    pub fn set_receive_state(
        &mut self,
        account: &str,
        uidl: &str,
        state: ReceiveState,
    ) -> Result<bool, StoreError> {
        Ok(self.db.execute(
            "UPDATE messages SET receive_state=?3 WHERE account_id=?1 AND uidl=?2",
            params![account, uidl, state.as_storage_str()],
        )? == 1)
    }

    pub fn deletion_states(
        &self,
        account: &str,
    ) -> Result<Vec<(String, ReceiveState)>, StoreError> {
        let rows: Vec<(String, String)> = {
            let mut stmt=self.db.prepare("SELECT uidl,receive_state FROM messages WHERE account_id=?1 AND uidl IS NOT NULL AND receive_state IN (?2,?3)")?;
            stmt.query_map(
                params![
                    account,
                    ReceiveState::DeletePending.as_storage_str(),
                    ReceiveState::DeleteMarkedSession.as_storage_str()
                ],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?
            .collect::<Result<Vec<_>, _>>()?
        };
        rows.into_iter()
            .map(|(uidl, state)| {
                ReceiveState::from_storage_str(&state)
                    .map(|state| (uidl, state))
                    .ok_or(StoreError::UnknownState)
            })
            .collect()
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
        tx.execute("INSERT INTO outbox(id,blob_id,state,stage) VALUES(?1,?2,?3,?4) ON CONFLICT(id) DO NOTHING",params![id,blob_id,SubmissionState::Queued.as_storage_str(),SubmissionProgress::NotStarted.as_storage_i64()])?;
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
        let retryable = [SubmissionState::Queued, SubmissionState::FailedSafeToRetry]
            .map(|state| state.as_storage_str());
        if stored.0 != blob_id || !retryable.contains(&stored.1.as_str()) {
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

    pub fn outbox_state(
        &self,
        id: &str,
    ) -> Result<Option<(SubmissionState, SubmissionProgress)>, StoreError> {
        let row: Option<(String, i64)> = self
            .db
            .query_row("SELECT state,stage FROM outbox WHERE id=?1", [id], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .optional()?;
        row.map(|(state, stage)| decode_submission(&state, stage))
            .transpose()
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
                "SELECT blob_id FROM outbox WHERE id=?1 AND state=?2",
                params![outbox_id, SubmissionState::DeliveryUnknown.as_storage_str()],
                |r| r.get(0),
            )
            .optional()?;
        let Some(blob) = blob else { return Ok(false) };
        if mark_sent {
            tx.execute(
                "UPDATE outbox SET state=?2,stage=?3 WHERE id=?1",
                params![
                    outbox_id,
                    SubmissionState::Sent.as_storage_str(),
                    SubmissionProgress::DeliveryAccepted.as_storage_i64()
                ],
            )?;
            tx.execute(
                "INSERT INTO sent_entities(id,account_id,blob_id) VALUES(?1,?2,?3)",
                params![format!("sent-{outbox_id}"), account_id, blob],
            )?;
        } else {
            tx.execute(
                "UPDATE outbox SET state=?2,stage=?3 WHERE id=?1",
                params![
                    outbox_id,
                    SubmissionState::FailedSafeToRetry.as_storage_str(),
                    SubmissionProgress::NotStarted.as_storage_i64()
                ],
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
        // Ambiguity is preserved: only a submission that reached DATA may finalize.
        let blob: Option<String> = tx
            .query_row(
                "SELECT blob_id FROM outbox WHERE id=?1 AND state=?2 AND stage=?3",
                params![
                    outbox_id,
                    SubmissionState::Submitting.as_storage_str(),
                    SubmissionProgress::DataMayHaveStarted.as_storage_i64()
                ],
                |r| r.get(0),
            )
            .optional()?;
        let Some(blob) = blob else {
            return Ok(false);
        };
        tx.execute(
            "UPDATE outbox SET state=?2,stage=?3 WHERE id=?1",
            params![
                outbox_id,
                SubmissionState::Sent.as_storage_str(),
                SubmissionProgress::DeliveryAccepted.as_storage_i64()
            ],
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
        let claimable = [SubmissionState::Queued, SubmissionState::FailedSafeToRetry]
            .map(|state| state.as_storage_str());
        Ok(self.db.execute(
            "UPDATE outbox SET state=?2 WHERE id=?1 AND state IN (?3,?4)",
            params![
                id,
                SubmissionState::Submitting.as_storage_str(),
                claimable[0],
                claimable[1]
            ],
        )? == 1)
    }

    /// Records submission progress under an explicit typed state. A state/progress
    /// pair the domain model cannot represent is rejected before any write, so no
    /// impossible combination can reach the table or its CHECK constraint.
    pub fn set_outbox_stage(
        &mut self,
        id: &str,
        progress: SubmissionProgress,
        state: SubmissionState,
    ) -> Result<bool, StoreError> {
        if !state.allows(progress) {
            return Err(StoreError::InvalidStateCombination);
        }
        Ok(self.db.execute(
            "UPDATE outbox SET stage=?2,state=?3 WHERE id=?1 AND state=?4",
            params![
                id,
                progress.as_storage_i64(),
                state.as_storage_str(),
                SubmissionState::Submitting.as_storage_str()
            ],
        )? == 1)
    }

    /// Restart recovery. A submission that never reached DATA is retry-safe; one
    /// that may have started is ambiguous and stays non-retryable until an explicit
    /// resolution decision.
    pub fn recover_submitting(&mut self) -> Result<usize, StoreError> {
        Ok(self.db.execute(
            "UPDATE outbox SET state=CASE WHEN stage>=?1 THEN ?2 ELSE ?3 END WHERE state=?4",
            params![
                SubmissionProgress::DataMayHaveStarted.as_storage_i64(),
                SubmissionState::DeliveryUnknown.as_storage_str(),
                SubmissionState::FailedSafeToRetry.as_storage_str(),
                SubmissionState::Submitting.as_storage_str(),
            ],
        )?)
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

/// Advances an existing database from `version` to `SCHEMA_VERSION`. Each step is
/// transactional, and v4 validates every persisted state value before rebuilding so
/// an unknown or unrepresentable value fails closed instead of being coerced.
fn migrate(db: &Connection, version: i64) -> Result<(), StoreError> {
    if version == 1 {
        db.execute_batch("BEGIN IMMEDIATE;
            CREATE TABLE IF NOT EXISTS outbox_recipients(outbox_id TEXT NOT NULL REFERENCES outbox(id) ON DELETE CASCADE, ordinal INTEGER NOT NULL, address TEXT NOT NULL, kind TEXT NOT NULL, PRIMARY KEY(outbox_id,ordinal));
            CREATE TABLE IF NOT EXISTS sent_entities(id TEXT PRIMARY KEY, account_id TEXT NOT NULL, blob_id TEXT NOT NULL REFERENCES raw_entities(id));
            CREATE TABLE IF NOT EXISTS outbox_senders(outbox_id TEXT PRIMARY KEY REFERENCES outbox(id) ON DELETE CASCADE, sender TEXT NOT NULL);
            UPDATE schema_version SET version=2 WHERE version=1;
            COMMIT;")?;
    }
    if version <= 2 {
        db.execute_batch(
            "BEGIN IMMEDIATE;
            CREATE TABLE IF NOT EXISTS local_deletions(entity_id TEXT PRIMARY KEY);
            UPDATE schema_version SET version=3 WHERE version<=2;
            COMMIT;",
        )?;
    }
    // A v1-v3 database may be missing tables that a current store owns, so the
    // pre-v4 shape is materialized before the rebuild copies rows out of it.
    db.execute_batch(&format!(
        "BEGIN IMMEDIATE;
        CREATE TABLE IF NOT EXISTS raw_entities(id TEXT PRIMARY KEY, size INTEGER NOT NULL, sha256 BLOB NOT NULL);
        CREATE TABLE IF NOT EXISTS messages(id TEXT PRIMARY KEY, account_id TEXT NOT NULL, uidl TEXT, blob_id TEXT REFERENCES raw_entities(id), {});
        CREATE TABLE IF NOT EXISTS drafts(id TEXT PRIMARY KEY, blob_id TEXT REFERENCES raw_entities(id));
        CREATE TABLE IF NOT EXISTS outbox(id TEXT PRIMARY KEY, blob_id TEXT REFERENCES raw_entities(id), {});
        COMMIT;",
        receive_state_constraint(),
        submission_state_constraint(),
    ))?;
    install_state_constraints(db)?;
    db.execute_batch(&format!(
        "BEGIN IMMEDIATE;
        UPDATE schema_version SET version={SCHEMA_VERSION} WHERE version<{SCHEMA_VERSION};
        COMMIT;"
    ))?;
    Ok(())
}

/// Rebuilds `messages` and `outbox` with v4 CHECK constraints. Existing rows are
/// decoded first; an unknown state, stage, or state/progress pair aborts the
/// migration with `UnknownState` rather than being silently repaired. Raw message
/// blobs are never rewritten.
fn install_state_constraints(db: &Connection) -> Result<(), StoreError> {
    {
        let mut stmt = db.prepare("SELECT DISTINCT receive_state FROM messages")?;
        let states = stmt
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        for state in states {
            decode_receive_state(&state)?;
        }
    }
    {
        let mut stmt = db.prepare("SELECT state,stage FROM outbox")?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        for (state, stage) in rows {
            decode_submission(&state, stage)?;
        }
    }
    // `outbox_recipients` and `outbox_senders` reference `outbox`, so foreign key
    // enforcement is suspended for the rebuild and checked immediately afterwards.
    db.execute_batch("PRAGMA foreign_keys=OFF")?;
    let rebuilt = (|| -> Result<(), StoreError> {
        db.execute_batch(&format!(
            "BEGIN IMMEDIATE;
            CREATE TABLE messages_v4(id TEXT PRIMARY KEY, account_id TEXT NOT NULL, uidl TEXT, blob_id TEXT REFERENCES raw_entities(id), {});
            INSERT INTO messages_v4(id,account_id,uidl,blob_id,receive_state) SELECT id,account_id,uidl,blob_id,receive_state FROM messages;
            DROP TABLE messages;
            ALTER TABLE messages_v4 RENAME TO messages;
            CREATE UNIQUE INDEX IF NOT EXISTS messages_account_uidl ON messages(account_id, uidl) WHERE uidl IS NOT NULL;
            CREATE TABLE outbox_v4(id TEXT PRIMARY KEY, blob_id TEXT REFERENCES raw_entities(id), {});
            INSERT INTO outbox_v4(id,blob_id,state,stage) SELECT id,blob_id,state,stage FROM outbox;
            DROP TABLE outbox;
            ALTER TABLE outbox_v4 RENAME TO outbox;
            COMMIT;",
            receive_state_constraint(),
            submission_state_constraint(),
        ))?;
        Ok(())
    })();
    if rebuilt.is_err() {
        let _ = db.execute_batch("ROLLBACK");
    }
    db.execute_batch("PRAGMA foreign_keys=ON")?;
    rebuilt?;
    let violations: i64 =
        db.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| {
            r.get(0)
        })?;
    if violations != 0 {
        return Err(StoreError::UnknownState);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn legacy_store(dir: &std::path::Path, version: i64) {
        let db = Connection::open(dir.join("mail.sqlite3")).unwrap();
        let mut ddl = format!(
            "CREATE TABLE schema_version(version INTEGER NOT NULL);
             INSERT INTO schema_version VALUES({version});
             CREATE TABLE raw_entities(id TEXT PRIMARY KEY,size INTEGER NOT NULL,sha256 BLOB NOT NULL);
             CREATE TABLE messages(id TEXT PRIMARY KEY, account_id TEXT NOT NULL, uidl TEXT, blob_id TEXT REFERENCES raw_entities(id), receive_state TEXT NOT NULL);
             CREATE UNIQUE INDEX messages_account_uidl ON messages(account_id, uidl) WHERE uidl IS NOT NULL;
             CREATE TABLE drafts(id TEXT PRIMARY KEY, blob_id TEXT REFERENCES raw_entities(id));
             CREATE TABLE outbox(id TEXT PRIMARY KEY, blob_id TEXT REFERENCES raw_entities(id), state TEXT NOT NULL, stage INTEGER NOT NULL DEFAULT 0);"
        );
        if version >= 2 {
            ddl.push_str(
                "CREATE TABLE outbox_recipients(outbox_id TEXT NOT NULL REFERENCES outbox(id) ON DELETE CASCADE, ordinal INTEGER NOT NULL, address TEXT NOT NULL, kind TEXT NOT NULL, PRIMARY KEY(outbox_id,ordinal));
                 CREATE TABLE sent_entities(id TEXT PRIMARY KEY, account_id TEXT NOT NULL, blob_id TEXT NOT NULL REFERENCES raw_entities(id));
                 CREATE TABLE outbox_senders(outbox_id TEXT PRIMARY KEY REFERENCES outbox(id) ON DELETE CASCADE, sender TEXT NOT NULL);",
            );
        }
        if version >= 3 {
            ddl.push_str("CREATE TABLE local_deletions(entity_id TEXT PRIMARY KEY);");
        }
        db.execute_batch(&ddl).unwrap();
    }

    /// Populates a legacy-shaped database with one row per persisted state kind.
    fn seed_legacy_rows(db: &Connection) {
        db.execute_batch(
            "INSERT INTO messages(id,account_id,uidl,receive_state) VALUES('m1','a','opaque','DeleteMarkedSession');
             INSERT INTO messages(id,account_id,uidl,receive_state) VALUES('m2','a','other','RemoteKnown');
             INSERT INTO outbox(id,state,stage) VALUES('o-queued','Queued',0);
             INSERT INTO outbox(id,state,stage) VALUES('o-unknown','DeliveryUnknown',1);
             INSERT INTO outbox(id,state,stage) VALUES('o-sent','Sent',2);",
        )
        .unwrap();
        if db.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='outbox_recipients'", [], |r| r.get::<_, i64>(0)).unwrap() == 1 {
            db.execute_batch(
                "INSERT INTO outbox_recipients(outbox_id,ordinal,address,kind) VALUES('o-queued',0,'to@i2p','To');
                 INSERT INTO outbox_senders(outbox_id,sender) VALUES('o-queued','from@i2p');",
            )
            .unwrap();
        }
    }

    #[test]
    fn raw_entity_survives_reopen_and_corruption_is_detected() {
        let dir = tempfile::tempdir().unwrap();
        let id;
        {
            let mut s = Store::open(dir.path()).unwrap();
            id = s.put_raw(b"Subject: x\r\n\r\nbody").unwrap();
            assert_eq!(s.schema_version().unwrap(), SCHEMA_VERSION);
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
            s.set_outbox_stage(
                "out-1",
                SubmissionProgress::DataMayHaveStarted,
                SubmissionState::Submitting,
            )
            .unwrap();
            assert_eq!(s.recover_submitting().unwrap(), 1);
        }
        let s = Store::open(dir.path()).unwrap();
        assert_eq!(
            s.outbox_state("out-1").unwrap(),
            Some((
                SubmissionState::DeliveryUnknown,
                SubmissionProgress::DataMayHaveStarted
            ))
        );
    }

    #[test]
    fn credentials_never_enter_persistence_apis_or_files() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = Store::open(dir.path()).unwrap();
        s.save_message(
            "m1",
            "a",
            Some("opaque"),
            ReceiveState::HeaderCached,
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
        s.save_message(
            "m1",
            "a",
            Some("opaque"),
            ReceiveState::HeaderCached,
            b"first",
        )
        .unwrap();
        assert!(
            s.save_message(
                "m2",
                "a",
                Some("opaque"),
                ReceiveState::HeaderCached,
                b"second"
            )
            .is_err()
        );
        assert!(s.message_by_uidl("a", "opaque").unwrap().is_some());
        assert_eq!(s.messages_for_account("a").unwrap().len(), 1);
    }

    #[test]
    fn every_legacy_schema_version_migrates_to_the_current_schema() {
        for legacy in [1, 2, 3] {
            let dir = tempfile::tempdir().unwrap();
            legacy_store(dir.path(), legacy);
            seed_legacy_rows(&Connection::open(dir.path().join("mail.sqlite3")).unwrap());
            {
                let s = Store::open(dir.path()).unwrap();
                assert_eq!(s.schema_version().unwrap(), SCHEMA_VERSION, "v{legacy}");
                // Persisted rows survive with their typed meaning intact.
                assert_eq!(
                    s.message_by_uidl("a", "opaque").unwrap().unwrap().state,
                    StoredMessageState::Receive(ReceiveState::DeleteMarkedSession)
                );
                assert_eq!(
                    s.deletion_states("a").unwrap(),
                    vec![("opaque".to_owned(), ReceiveState::DeleteMarkedSession)]
                );
                assert_eq!(
                    s.outbox_state("o-unknown").unwrap(),
                    Some((
                        SubmissionState::DeliveryUnknown,
                        SubmissionProgress::DataMayHaveStarted
                    ))
                );
                assert_eq!(
                    s.outbox_state("o-sent").unwrap(),
                    Some((SubmissionState::Sent, SubmissionProgress::DeliveryAccepted))
                );
                if legacy >= 2 {
                    assert_eq!(s.outbox_recipients("o-queued").unwrap().len(), 1);
                    assert_eq!(
                        s.outbox_sender("o-queued").unwrap().as_deref(),
                        Some("from@i2p")
                    );
                }
            }
            // A migrated database reopens at the current version and is already constrained.
            let s = Store::open(dir.path()).unwrap();
            assert_eq!(
                s.schema_version().unwrap(),
                SCHEMA_VERSION,
                "reopen v{legacy}"
            );
            assert!(
                s.db
                    .execute(
                        "INSERT INTO messages(id,account_id,uidl,receive_state) VALUES('bad','a','bad','NotAState')",
                        []
                    )
                    .is_err(),
                "v{legacy} must not accept an unknown receive state"
            );
            assert!(
                s.db.execute(
                    "INSERT INTO outbox(id,state,stage) VALUES('bad','DeliveryUnknown',0)",
                    []
                )
                .is_err(),
                "v{legacy} must not accept an unrepresentable state/progress pair"
            );
        }
    }

    #[test]
    fn unsupported_persisted_state_fails_closed_instead_of_being_repaired() {
        for (table, row, value) in [
            (
                "messages",
                "INSERT INTO messages(id,account_id,uidl,receive_state) VALUES('bad','a','bad',?)",
                "Sent",
            ),
            (
                "messages",
                "INSERT INTO messages(id,account_id,uidl,receive_state) VALUES('bad','a','bad',?)",
                "remoteknown",
            ),
            (
                "outbox",
                "INSERT INTO outbox(id,state,stage) VALUES('bad',?,0)",
                "Unknown",
            ),
            (
                "outbox",
                "INSERT INTO outbox(id,state,stage) VALUES('bad','DeliveryUnknown',?)",
                "0",
            ),
        ] {
            let dir = tempfile::tempdir().unwrap();
            legacy_store(dir.path(), 3);
            let db = Connection::open(dir.path().join("mail.sqlite3")).unwrap();
            db.execute(row, [value]).unwrap();
            drop(db);
            assert!(
                matches!(Store::open(dir.path()), Err(StoreError::UnknownState)),
                "{table} {value} must fail closed"
            );
        }
    }

    #[test]
    fn a_newer_schema_is_rejected_without_modification() {
        let dir = tempfile::tempdir().unwrap();
        legacy_store(dir.path(), SCHEMA_VERSION + 1);
        assert!(matches!(
            Store::open(dir.path()),
            Err(StoreError::UnsupportedSchema)
        ));
    }

    #[test]
    fn every_valid_state_round_trips_through_the_typed_api() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = Store::open(dir.path()).unwrap();
        // Round-trip every receive state under a dedicated account so the
        // deletion-state assertions below stay scoped to one message.
        for state in ReceiveState::ALL {
            let id = format!("m-{}", state.as_storage_str());
            s.save_message(&id, "all-states", Some(&id), state, b"entity")
                .unwrap();
            assert_eq!(
                s.message_by_uidl("all-states", &id).unwrap().unwrap().state,
                StoredMessageState::Receive(state)
            );
        }
        assert_eq!(
            s.messages_for_account("all-states").unwrap().len(),
            ReceiveState::ALL.len()
        );
        // Transitions use the typed API only, and every read decodes.
        s.save_message("m1", "a", Some("m1"), ReceiveState::HeaderCached, b"x")
            .unwrap();
        assert!(s.mark_delete_pending("a", "m1").unwrap());
        assert!(
            s.set_receive_state("a", "m1", ReceiveState::DeletePending)
                .unwrap()
        );
        assert_eq!(
            s.deletion_states("a").unwrap(),
            vec![("m1".to_owned(), ReceiveState::DeletePending)]
        );
        assert!(
            s.set_receive_state("a", "m1", ReceiveState::DeleteMarkedSession)
                .unwrap()
        );
        assert_eq!(
            s.deletion_states("a").unwrap(),
            vec![("m1".to_owned(), ReceiveState::DeleteMarkedSession)]
        );
        assert!(
            s.set_receive_state("a", "m1", ReceiveState::RemoteDeletionCommitted)
                .unwrap()
        );
        assert!(s.deletion_states("a").unwrap().is_empty());
        // Sent rows are a projection of delivered submissions, not a receive state.
        s.queue_outbox("sent-out", b"entity").unwrap();
        s.claim_outbox("sent-out").unwrap();
        s.set_outbox_stage(
            "sent-out",
            SubmissionProgress::DataMayHaveStarted,
            SubmissionState::Submitting,
        )
        .unwrap();
        assert!(s.finalize_sent("sent-out", "a", "sent-sent-out").unwrap());
        let listed = s.messages_for_account("a").unwrap();
        assert!(listed.iter().any(|m| m.state == StoredMessageState::Sent));
        assert!(listed.iter().all(|m| match m.state {
            StoredMessageState::Sent => m.uidl.is_none(),
            StoredMessageState::Receive(_) => m.uidl.is_some(),
        }));
    }

    #[test]
    fn impossible_submission_state_progress_pairs_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = Store::open(dir.path()).unwrap();
        s.queue_outbox("out", b"entity").unwrap();
        assert!(s.claim_outbox("out").unwrap());
        for (progress, state) in [
            (
                SubmissionProgress::NotStarted,
                SubmissionState::DeliveryUnknown,
            ),
            (
                SubmissionProgress::DeliveryAccepted,
                SubmissionState::Submitting,
            ),
            (
                SubmissionProgress::DataMayHaveStarted,
                SubmissionState::FailedSafeToRetry,
            ),
            (
                SubmissionProgress::DeliveryAccepted,
                SubmissionState::Queued,
            ),
            (SubmissionProgress::NotStarted, SubmissionState::Sent),
        ] {
            assert!(
                matches!(
                    s.set_outbox_stage("out", progress, state),
                    Err(StoreError::InvalidStateCombination)
                ),
                "{state:?}/{progress:?} must be rejected"
            );
        }
        // A rejected pair leaves the owned row unchanged.
        assert_eq!(
            s.outbox_state("out").unwrap(),
            Some((SubmissionState::Submitting, SubmissionProgress::NotStarted))
        );
        assert!(
            s.set_outbox_stage(
                "out",
                SubmissionProgress::DataMayHaveStarted,
                SubmissionState::Submitting
            )
            .unwrap()
        );
        assert!(
            s.set_outbox_stage(
                "out",
                SubmissionProgress::DataMayHaveStarted,
                SubmissionState::DeliveryUnknown
            )
            .unwrap()
        );
        assert_eq!(
            s.outbox_state("out").unwrap(),
            Some((
                SubmissionState::DeliveryUnknown,
                SubmissionProgress::DataMayHaveStarted
            ))
        );
        // Progress is only writable by the submission that owns the row.
        assert!(
            !s.set_outbox_stage(
                "out",
                SubmissionProgress::NotStarted,
                SubmissionState::Submitting
            )
            .unwrap()
        );
    }

    #[test]
    fn ambiguous_delivery_requires_explicit_resolution_before_a_retry() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = Store::open(dir.path()).unwrap();
        s.queue_outbox("amb", b"entity").unwrap();
        assert!(s.claim_outbox("amb").unwrap());
        s.set_outbox_stage(
            "amb",
            SubmissionProgress::DataMayHaveStarted,
            SubmissionState::Submitting,
        )
        .unwrap();
        s.set_outbox_stage(
            "amb",
            SubmissionProgress::DataMayHaveStarted,
            SubmissionState::DeliveryUnknown,
        )
        .unwrap();
        assert!(!s.claim_outbox("amb").unwrap());
        assert_eq!(s.recover_submitting().unwrap(), 0);
        assert!(!s.finalize_sent("amb", "a", "sent-amb").unwrap());
        assert!(s.sent_raw("sent-amb").unwrap().is_none());
        // Only an explicit decision may clear ambiguity.
        assert!(s.resolve_delivery_unknown("amb", "a", false).unwrap());
        assert_eq!(
            s.outbox_state("amb").unwrap(),
            Some((
                SubmissionState::FailedSafeToRetry,
                SubmissionProgress::NotStarted
            ))
        );
        assert!(s.claim_outbox("amb").unwrap());
        s.set_outbox_stage(
            "amb",
            SubmissionProgress::DataMayHaveStarted,
            SubmissionState::DeliveryUnknown,
        )
        .unwrap();
        assert!(s.resolve_delivery_unknown("amb", "a", true).unwrap());
        assert_eq!(
            s.outbox_state("amb").unwrap(),
            Some((SubmissionState::Sent, SubmissionProgress::DeliveryAccepted))
        );
        assert_eq!(s.sent_raw("sent-amb").unwrap().unwrap(), b"entity");
    }

    #[test]
    fn outbox_retry_cannot_replace_canonical_entity_or_retry_unknown_delivery() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = Store::open(dir.path()).unwrap();
        let recipients = vec![("to@i2p".to_owned(), "To".to_owned())];
        s.queue_outbox_with_recipients("retry", b"same", &recipients)
            .unwrap();
        assert!(s.claim_outbox("retry").unwrap());
        s.set_outbox_stage(
            "retry",
            SubmissionProgress::NotStarted,
            SubmissionState::FailedSafeToRetry,
        )
        .unwrap();
        assert!(matches!(
            s.queue_outbox_with_recipients("retry", b"changed", &recipients),
            Err(StoreError::OutboxConflict)
        ));
        assert!(s.claim_outbox("retry").unwrap());
        s.set_outbox_stage(
            "retry",
            SubmissionProgress::DataMayHaveStarted,
            SubmissionState::DeliveryUnknown,
        )
        .unwrap();
        assert!(matches!(
            s.queue_outbox_with_recipients("retry", b"same", &recipients),
            Err(StoreError::OutboxConflict)
        ));
        assert!(!s.claim_outbox("retry").unwrap());
    }
}
