use tower_sessions::Session;

pub async fn set_flash(session: &Session, msg: &str, kind: &str) {
    let _ = session.insert("flash", format!("{}:{}", kind, msg)).await;
}

pub async fn get_flash(session: &Session) -> (String, String) {
    if let Ok(Some(flash)) = session.get::<String>("flash").await {
        let _ = session.remove::<String>("flash").await;
        if let Some((kind, msg)) = flash.split_once(':') {
            return (kind.to_string(), msg.to_string());
        }
        ("info".to_string(), flash)
    } else {
        ("info".to_string(), String::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_sessions::MemoryStore;
    use tower_sessions::session::{Id, Session};

    fn make_session() -> Session {
        let store = MemoryStore::default();
        Session::new(Some(Id::default()), std::sync::Arc::new(store), None)
    }

    #[tokio::test]
    async fn test_flash_roundtrip() {
        let session = make_session();
        set_flash(&session, "It worked!", "success").await;
        let (kind, msg) = get_flash(&session).await;
        assert_eq!(kind, "success");
        assert_eq!(msg, "It worked!");
    }

    #[tokio::test]
    async fn test_flash_get_empty() {
        let session = make_session();
        let (kind, msg) = get_flash(&session).await;
        assert_eq!(kind, "info");
        assert_eq!(msg, "");
    }
}
