use libsql::{Builder, Connection};
use std::error::Error;

use crate::config::Config;

const MIGRATIONS: &[(&str, &str)] = &[(
    "20260803000000_init.sql",
    include_str!("../migrations/20260803000000_init.sql"),
)];

#[derive(Debug)]
enum DbKind {
    Local,
    Remote,
}

fn classify_db_url(url: &str, token: Option<&str>) -> Result<DbKind, String> {
    if url == ":memory:" || url.starts_with("file:") {
        return Ok(DbKind::Local);
    }
    if url.starts_with("libsql://") || url.starts_with("https://") {
        if token.is_none() {
            return Err(
                "APP_DATABASE_URL is a remote libsql/Turso URL but APP_TURSO_TOKEN is not set"
                    .to_string(),
            );
        }
        return Ok(DbKind::Remote);
    }
    Err(format!(
        "unknown APP_DATABASE_URL scheme: '{}' (expected ':memory:', a 'file:' path, or a remote \
         'libsql://'/'https://' URL)",
        url
    ))
}

pub async fn init_database(config: &Config) -> Result<Connection, Box<dyn Error>> {
    let db_url = config.database_url.clone();
    let db_token = config.database_token.clone();

    let db = match classify_db_url(&db_url, db_token.as_deref()) {
        Ok(DbKind::Local) => Builder::new_local(&db_url).build().await?,
        Ok(DbKind::Remote) => {
            Builder::new_remote(db_url, db_token.unwrap_or_default())
                .build()
                .await?
        }
        Err(msg) => return Err(msg.into()),
    };

    let conn = db.connect()?;

    run_migrations(&conn).await?;

    conn.execute("PRAGMA foreign_keys = ON", ()).await?;

    Ok(conn)
}

pub async fn init_test_database() -> Result<Connection, Box<dyn Error>> {
    let db = Builder::new_local(":memory:").build().await?;
    let conn = db.connect()?;
    run_migrations(&conn).await?;
    conn.execute("PRAGMA foreign_keys = ON", ()).await?;
    Ok(conn)
}

async fn run_migrations(conn: &Connection) -> Result<(), Box<dyn Error>> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS _migrations (
            filename TEXT PRIMARY KEY,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        )",
        (),
    )
    .await?;

    let mut applied = Vec::new();
    let mut rows = conn.query("SELECT filename FROM _migrations", ()).await?;
    while let Some(row) = rows.next().await? {
        if let Ok(filename) = row.get::<String>(0) {
            applied.push(filename);
        }
    }

    for (filename, sql) in MIGRATIONS {
        if !applied.contains(&filename.to_string()) {
            tracing::info!("Running migration: {}", filename);

            conn.execute("BEGIN TRANSACTION", ()).await?;
            for statement in sql.split(';') {
                let trimmed = statement.trim();
                if !trimmed.is_empty() {
                    conn.execute(trimmed, ()).await?;
                }
            }
            conn.execute(
                "INSERT INTO _migrations (filename) VALUES (?1)",
                [filename.to_string()],
            )
            .await?;
            conn.execute("COMMIT", ()).await?;

            tracing::info!("Migration applied: {}", filename);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_db() -> Connection {
        let db = Builder::new_local(":memory:").build().await.unwrap();
        let conn = db.connect().unwrap();
        run_migrations(&conn).await.unwrap();
        conn.execute("PRAGMA foreign_keys = ON", ()).await.unwrap();
        conn
    }

    #[test]
    fn test_classify_db_url_local() {
        for url in [":memory:", "file:local.db", "file:/var/lib/app.db"] {
            match classify_db_url(url, None) {
                Ok(DbKind::Local) => {}
                other => panic!("expected Local for '{}', got {:?}", url, other),
            }
        }
    }

    #[test]
    fn test_classify_db_url_remote_with_token() {
        for url in ["libsql://db-org.turso.io", "https://db.example.com"] {
            match classify_db_url(url, Some("token")) {
                Ok(DbKind::Remote) => {}
                other => panic!("expected Remote for '{}', got {:?}", url, other),
            }
        }
    }

    #[test]
    fn test_classify_db_url_remote_requires_token() {
        for url in ["libsql://db-org.turso.io", "https://db.example.com"] {
            let err = classify_db_url(url, None).unwrap_err();
            assert!(err.contains("APP_TURSO_TOKEN"), "for '{}': {}", url, err);
        }
    }

    #[test]
    fn test_classify_db_url_unknown_scheme() {
        for url in ["/tmp/db.sqlite", "foo:bar", "db.sqlite"] {
            let err = classify_db_url(url, Some("token")).unwrap_err();
            assert!(
                err.contains("unknown APP_DATABASE_URL scheme"),
                "for '{}': {}",
                url,
                err
            );
        }
    }

    async fn table_names(conn: &Connection) -> Vec<String> {
        let mut names = Vec::new();
        let mut rows = conn
            .query(
                "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name",
                (),
            )
            .await
            .unwrap();
        while let Ok(Some(row)) = rows.next().await {
            if let Ok(name) = row.get::<String>(0) {
                names.push(name);
            }
        }
        names
    }

    #[tokio::test]
    async fn test_migration_creates_tables() {
        let conn = test_db().await;
        let tables = table_names(&conn).await;
        for expected in &["settings", "session_store"] {
            assert!(
                tables.contains(&expected.to_string()),
                "Missing table: {}",
                expected
            );
        }
    }

    #[tokio::test]
    async fn test_migration_idempotency() {
        let db = Builder::new_local(":memory:").build().await.unwrap();
        let conn = db.connect().unwrap();
        run_migrations(&conn).await.unwrap();
        run_migrations(&conn).await.unwrap();

        let mut rows = conn
            .query("SELECT COUNT(*) FROM _migrations", ())
            .await
            .unwrap();
        let count = if let Ok(Some(row)) = rows.next().await {
            row.get::<i64>(0).unwrap_or(0)
        } else {
            0
        };
        assert_eq!(count, MIGRATIONS.len() as i64);
    }

    #[tokio::test]
    async fn test_foreign_keys_enabled() {
        let conn = test_db().await;
        let mut rows = conn.query("PRAGMA foreign_keys", ()).await.unwrap();
        if let Ok(Some(row)) = rows.next().await {
            let enabled: i64 = row.get(0).unwrap_or(0);
            assert_eq!(enabled, 1);
        } else {
            panic!("Could not query PRAGMA foreign_keys");
        }
    }
}
