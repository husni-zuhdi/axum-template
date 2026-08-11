use libsql::Connection;

#[derive(Clone)]
pub struct SettingsRepository<'a> {
    db: &'a Connection,
}

impl<'a> SettingsRepository<'a> {
    pub const fn new(db: &'a Connection) -> Self {
        Self { db }
    }

    /// Loads a single settings key; `None` when the key is absent.
    pub async fn get(&self, key: &str) -> Option<String> {
        let mut rows = self
            .db
            .query("SELECT value FROM settings WHERE key = ?1", [key])
            .await
            .ok()?;
        match rows.next().await {
            Ok(Some(row)) => row.get::<String>(0).ok(),
            _ => None,
        }
    }

    /// Upserts a single settings key.
    pub async fn set(&self, key: &str, value: &str) -> Result<(), libsql::Error> {
        self.db
            .execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                (key, value),
            )
            .await?;
        Ok(())
    }

    /// Lists the table names in the database (used by the health check).
    pub async fn health_tables(&self) -> Vec<String> {
        let mut tables = Vec::new();
        if let Ok(mut rows) = self
            .db
            .query(
                "SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name",
                (),
            )
            .await
        {
            while let Ok(Some(row)) = rows.next().await {
                if let Ok(name) = row.get::<String>(0) {
                    tables.push(name);
                }
            }
        }
        tables
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    async fn repo() -> SettingsRepository<'static> {
        let conn = db::init_test_database().await.unwrap();
        let conn = Box::leak(Box::new(conn));
        SettingsRepository::new(conn)
    }

    #[tokio::test]
    async fn test_set_and_get_roundtrip() {
        let repo = repo().await;
        assert!(repo.get("missing").await.is_none());
        repo.set("theme", "dark").await.unwrap();
        assert_eq!(repo.get("theme").await.as_deref(), Some("dark"));
    }

    #[tokio::test]
    async fn test_set_overwrites() {
        let repo = repo().await;
        repo.set("key", "one").await.unwrap();
        repo.set("key", "two").await.unwrap();
        assert_eq!(repo.get("key").await.as_deref(), Some("two"));
    }

    #[tokio::test]
    async fn test_health_tables() {
        let repo = repo().await;
        let tables = repo.health_tables().await;
        assert!(tables.contains(&"settings".to_string()));
        assert!(tables.contains(&"session_store".to_string()));
    }
}
