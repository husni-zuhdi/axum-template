use async_trait::async_trait;
use libsql::Connection;
use time::OffsetDateTime;
use tower_sessions::{
    session::{Id, Record},
    session_store::{self, ExpiredDeletion, SessionStore},
};

#[derive(Debug, Clone)]
pub struct SqliteSessionStore {
    db: Connection,
}

impl SqliteSessionStore {
    pub const fn new(db: Connection) -> Self {
        Self { db }
    }
}

#[async_trait]
impl SessionStore for SqliteSessionStore {
    async fn create(&self, record: &mut Record) -> session_store::Result<()> {
        loop {
            let existing = self.load(&record.id).await?;
            if existing.is_none() {
                break;
            }
            record.id = Id::default();
        }
        self.save(record).await
    }

    async fn save(&self, record: &Record) -> session_store::Result<()> {
        let id_str = record.id.to_string();
        let data_str = serde_json::to_string(record)
            .map_err(|e| session_store::Error::Encode(e.to_string()))?;
        let expiry_str = record
            .expiry_date
            .format(&time::format_description::well_known::Rfc3339)
            .map_err(|e| session_store::Error::Encode(e.to_string()))?;

        self.db
            .execute(
                "INSERT INTO session_store (id, data, expiry_date) VALUES (?1, ?2, ?3)
                 ON CONFLICT(id) DO UPDATE SET data = excluded.data, expiry_date = excluded.expiry_date",
                (id_str.as_str(), data_str.as_str(), expiry_str.as_str()),
            )
            .await
            .map_err(|e| session_store::Error::Backend(e.to_string()))?;

        Ok(())
    }

    async fn load(&self, session_id: &Id) -> session_store::Result<Option<Record>> {
        let id_str = session_id.to_string();
        let mut rows = self
            .db
            .query(
                "SELECT data, expiry_date FROM session_store WHERE id = ?1",
                [id_str.as_str()],
            )
            .await
            .map_err(|e| session_store::Error::Backend(e.to_string()))?;

        match rows
            .next()
            .await
            .map_err(|e| session_store::Error::Backend(e.to_string()))?
        {
            Some(row) => {
                let data_str: String = row
                    .get::<String>(0)
                    .map_err(|e| session_store::Error::Backend(e.to_string()))?;
                let expiry_str: String = row
                    .get::<String>(1)
                    .map_err(|e| session_store::Error::Backend(e.to_string()))?;

                let mut record: Record = serde_json::from_str(&data_str)
                    .map_err(|e| session_store::Error::Decode(e.to_string()))?;

                let expiry = OffsetDateTime::parse(
                    &expiry_str,
                    &time::format_description::well_known::Rfc3339,
                )
                .map_err(|e| session_store::Error::Decode(e.to_string()))?;

                if expiry > OffsetDateTime::now_utc() {
                    record.expiry_date = expiry;
                    Ok(Some(record))
                } else {
                    Ok(None)
                }
            }
            None => Ok(None),
        }
    }

    async fn delete(&self, session_id: &Id) -> session_store::Result<()> {
        let id_str = session_id.to_string();
        self.db
            .execute("DELETE FROM session_store WHERE id = ?1", [id_str.as_str()])
            .await
            .map_err(|e| session_store::Error::Backend(e.to_string()))?;
        Ok(())
    }
}

#[async_trait]
impl ExpiredDeletion for SqliteSessionStore {
    async fn delete_expired(&self) -> session_store::Result<()> {
        let now = OffsetDateTime::now_utc();
        let now_str = now
            .format(&time::format_description::well_known::Rfc3339)
            .map_err(|e| session_store::Error::Encode(e.to_string()))?;
        self.db
            .execute(
                "DELETE FROM session_store WHERE expiry_date < ?1",
                [now_str.as_str()],
            )
            .await
            .map_err(|e| session_store::Error::Backend(e.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libsql::Builder;
    use std::collections::HashMap;
    use time::Duration;
    use tower_sessions::session::Record;

    async fn test_store() -> SqliteSessionStore {
        let db = Builder::new_local(":memory:").build().await.unwrap();
        let conn = db.connect().unwrap();
        conn.execute(
            "CREATE TABLE IF NOT EXISTS session_store (
                id TEXT PRIMARY KEY,
                data TEXT NOT NULL,
                expiry_date TEXT NOT NULL
            )",
            (),
        )
        .await
        .unwrap();
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_session_store_expiry ON session_store(expiry_date)",
            (),
        )
        .await
        .unwrap();
        SqliteSessionStore::new(conn)
    }

    fn make_record(future_seconds: i64) -> Record {
        let mut data = HashMap::new();
        data.insert("authenticated".to_string(), serde_json::Value::Bool(true));
        Record {
            id: Id::default(),
            data,
            expiry_date: OffsetDateTime::now_utc() + Duration::seconds(future_seconds),
        }
    }

    #[tokio::test]
    async fn test_create_and_load() {
        let store = test_store().await;
        let mut record = make_record(3600);
        let record_id = record.id;

        store.create(&mut record).await.unwrap();

        let loaded = store.load(&record_id).await.unwrap().expect("Should exist");
        assert_eq!(loaded.id, record_id);
        assert_eq!(
            loaded.data.get("authenticated"),
            Some(&serde_json::Value::Bool(true))
        );
    }

    #[tokio::test]
    async fn test_save_overwrites() {
        let store = test_store().await;
        let mut record = make_record(3600);
        let record_id = record.id;
        store.create(&mut record).await.unwrap();

        let mut updated = record.clone();
        updated
            .data
            .insert("foo".to_string(), serde_json::Value::String("bar".into()));
        store.save(&updated).await.unwrap();

        let loaded = store.load(&record_id).await.unwrap().expect("Should exist");
        assert_eq!(
            loaded.data.get("foo"),
            Some(&serde_json::Value::String("bar".into()))
        );
    }

    #[tokio::test]
    async fn test_load_missing() {
        let store = test_store().await;
        let id = Id::default();
        let result = store.load(&id).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_load_expired() {
        let store = test_store().await;
        let mut record = make_record(-1);
        store.create(&mut record).await.unwrap();
        let result = store.load(&record.id).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_delete_removes() {
        let store = test_store().await;
        let mut record = make_record(3600);
        store.create(&mut record).await.unwrap();
        store.delete(&record.id).await.unwrap();
        let result = store.load(&record.id).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_create_id_collision() {
        let store = test_store().await;
        let mut record1 = make_record(3600);
        let mut record2 = make_record(3600);

        store.create(&mut record1).await.unwrap();
        record2.id = record1.id;
        store.create(&mut record2).await.unwrap();

        assert_ne!(record1.id, record2.id);
    }

    #[tokio::test]
    async fn test_delete_expired() {
        let store = test_store().await;
        let mut expired = make_record(-1);
        let mut active = make_record(3600);
        store.create(&mut expired).await.unwrap();
        store.create(&mut active).await.unwrap();

        store.delete_expired().await.unwrap();

        assert!(store.load(&expired.id).await.unwrap().is_none());
        assert!(store.load(&active.id).await.unwrap().is_some());
    }
}
