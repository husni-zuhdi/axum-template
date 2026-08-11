#[derive(Clone, Debug)]
pub struct Config {
    pub address: String,
    pub port: String,
    pub database_url: String,
    pub database_token: Option<String>,
    pub password_hash: Option<String>,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            address: std::env::var("APP_ADDRESS").unwrap_or_else(|_| "0.0.0.0".to_string()),
            port: std::env::var("APP_PORT").unwrap_or_else(|_| "3000".to_string()),
            database_url: std::env::var("APP_DATABASE_URL")
                .unwrap_or_else(|_| ":memory:".to_string()),
            database_token: std::env::var("APP_TURSO_TOKEN").ok(),
            password_hash: std::env::var("APP_PASSWORD_HASH").ok(),
        }
    }

    pub fn bind_addr(&self) -> String {
        format!("{}:{}", self.address, self.port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_defaults_when_unset() {
        let config = Config::from_env();
        assert_eq!(config.address, "0.0.0.0");
        assert_eq!(config.port, "3000");
        assert_eq!(config.database_url, ":memory:");
        assert!(config.database_token.is_none());
        assert!(config.password_hash.is_none());
    }

    #[test]
    fn test_bind_addr() {
        let config = Config {
            address: "127.0.0.1".into(),
            port: "8080".into(),
            database_url: ":memory:".into(),
            database_token: None,
            password_hash: None,
        };
        assert_eq!(config.bind_addr(), "127.0.0.1:8080");
    }
}
