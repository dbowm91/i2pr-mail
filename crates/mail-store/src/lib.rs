//! SQLite metadata plus immutable content-addressed raw entities.
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("database error")]
    Database(#[from] rusqlite::Error),
    #[error("filesystem error")]
    Filesystem(#[from] std::io::Error),
    #[error("stored blob is missing or corrupt")]
    CorruptBlob,
}

#[derive(Debug)]
pub struct Store {
    db: Connection,
    root: PathBuf,
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
        let digest = Sha256::digest(bytes);
        let id = digest
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let final_path = self.root.join("blobs").join(&id);
        if !final_path.exists() {
            let tmp = self.root.join("blobs").join(format!(".{id}.tmp"));
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&tmp)?;
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
            assert_eq!(s.schema_version().unwrap(), 1);
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
}
