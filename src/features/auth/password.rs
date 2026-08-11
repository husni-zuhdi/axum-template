use libsql::Connection;

use crate::services::settings::SettingsRepository;

/// The settings key holding the hash of a password changed via the UI. When
/// present it wins over the bootstrap `APP_PASSWORD_HASH`; the env value remains
/// the fallback (and the boot-time requirement).
pub const OVERRIDE_KEY: &str = "password_hash_override";

pub const MIN_PASSWORD_LENGTH: usize = 8;

/// Verifies a plaintext password against an argon2id PHC hash.
pub fn verify_password(password: &str, hash: &str) -> bool {
    use argon2::Argon2;
    use argon2::password_hash::{PasswordHash, PasswordVerifier};

    let parsed_hash = match PasswordHash::new(hash) {
        Ok(h) => h,
        Err(_) => return false,
    };

    Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok()
}

/// Hashes a plaintext password with argon2id and a fresh random salt.
pub fn hash_password(password: &str) -> Result<String, argon2::password_hash::Error> {
    use argon2::Argon2;
    use argon2::password_hash::{PasswordHasher, SaltString, rand_core::OsRng};

    let salt = SaltString::generate(&mut OsRng);
    Ok(Argon2::default()
        .hash_password(password.as_bytes(), &salt)?
        .to_string())
}

/// The effective verification hash: a `password_hash_override` row in
/// `settings` wins; otherwise the bootstrap `APP_PASSWORD_HASH` is used.
pub async fn effective_hash(db: &Connection, bootstrap_hash: &str) -> String {
    SettingsRepository::new(db)
        .get(OVERRIDE_KEY)
        .await
        .filter(|h| !h.is_empty())
        .unwrap_or_else(|| bootstrap_hash.to_string())
}

#[derive(Debug, PartialEq, Eq)]
pub enum PasswordError {
    IncorrectCurrent,
    Mismatch,
    TooShort,
    MissingUppercase,
    MissingNumber,
    MissingSpecial,
    HashFailed,
}

impl PasswordError {
    pub fn message(&self) -> &'static str {
        match self {
            Self::IncorrectCurrent => "Current password is incorrect.",
            Self::Mismatch => "New password and confirmation do not match.",
            Self::TooShort => "New password must be at least 8 characters.",
            Self::MissingUppercase => "New password must include at least one uppercase letter.",
            Self::MissingNumber => "New password must include at least one number.",
            Self::MissingSpecial => "New password must include at least one special character.",
            Self::HashFailed => "Could not save the new password. Please try again.",
        }
    }
}

/// Enforces the password rules: minimum length plus at least one uppercase
/// letter, one digit, and one non-alphanumeric (non-whitespace) character.
pub fn validate_new_password(password: &str) -> Result<(), PasswordError> {
    if password.chars().count() < MIN_PASSWORD_LENGTH {
        return Err(PasswordError::TooShort);
    }
    if !password.chars().any(|c| c.is_ascii_uppercase()) {
        return Err(PasswordError::MissingUppercase);
    }
    if !password.chars().any(|c| c.is_ascii_digit()) {
        return Err(PasswordError::MissingNumber);
    }
    if !password
        .chars()
        .any(|c| !c.is_alphanumeric() && !c.is_whitespace())
    {
        return Err(PasswordError::MissingSpecial);
    }
    Ok(())
}

/// Verifies `current` against the effective hash, then validates and stores a
/// fresh argon2id hash of `new` under `OVERRIDE_KEY`.
pub async fn change_password(
    db: &Connection,
    bootstrap_hash: &str,
    current: &str,
    new: &str,
    confirm: &str,
) -> Result<(), PasswordError> {
    let current_hash = effective_hash(db, bootstrap_hash).await;
    if !verify_password(current, &current_hash) {
        return Err(PasswordError::IncorrectCurrent);
    }
    if new != confirm {
        return Err(PasswordError::Mismatch);
    }
    validate_new_password(new)?;
    let hash = hash_password(new).map_err(|_| PasswordError::HashFailed)?;
    SettingsRepository::new(db)
        .set(OVERRIDE_KEY, &hash)
        .await
        .map_err(|_| PasswordError::HashFailed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_test_database;

    /// Deterministic argon2id hash of `secret` (fixed salt, matching the
    /// test harness); avoids hardcoding a brittle PHC string.
    fn hash_of_secret() -> String {
        use argon2::Argon2;
        use argon2::password_hash::{PasswordHasher, SaltString};
        Argon2::default()
            .hash_password(
                b"secret",
                &SaltString::from_b64("c2FsdHNhbHRzYWx0c2FsdA").unwrap(),
            )
            .unwrap()
            .to_string()
    }

    #[test]
    fn test_verify_password() {
        let hash = hash_of_secret();
        assert!(verify_password("secret", &hash));
        assert!(!verify_password("wrong", &hash));
        assert!(!verify_password("secret", "not-a-hash"));
    }

    #[test]
    fn test_hash_password_roundtrip() {
        let hash = hash_password("Sup3r$ecret!").unwrap();
        assert!(verify_password("Sup3r$ecret!", &hash));
        assert!(!verify_password("nope", &hash));
        assert!(hash.starts_with("$argon2id$"));
    }

    #[test]
    fn test_validate_new_password() {
        assert_eq!(validate_new_password("short"), Err(PasswordError::TooShort));
        assert_eq!(
            validate_new_password("lowercase1!"),
            Err(PasswordError::MissingUppercase)
        );
        assert_eq!(
            validate_new_password("Uppercase!"),
            Err(PasswordError::MissingNumber)
        );
        assert_eq!(
            validate_new_password("Uppercase1"),
            Err(PasswordError::MissingSpecial)
        );
        assert_eq!(validate_new_password("Uppercase1!"), Ok(()));
        assert_eq!(
            validate_new_password("Uppercase1 "),
            Err(PasswordError::MissingSpecial)
        );
    }

    #[tokio::test]
    async fn test_effective_hash_uses_override_when_present() {
        let db = init_test_database().await.unwrap();
        let repo = SettingsRepository::new(&db);
        repo.set(OVERRIDE_KEY, "override").await.unwrap();

        assert_eq!(effective_hash(&db, "bootstrap").await, "override");
        repo.set(OVERRIDE_KEY, "").await.unwrap();
        assert_eq!(effective_hash(&db, "bootstrap").await, "bootstrap");
    }

    #[tokio::test]
    async fn test_change_password_writes_override_and_verifies() {
        let db = init_test_database().await.unwrap();
        let bootstrap = hash_of_secret();

        change_password(&db, &bootstrap, "secret", "New$ecret1", "New$ecret1")
            .await
            .unwrap();

        let stored = SettingsRepository::new(&db)
            .get(OVERRIDE_KEY)
            .await
            .unwrap();
        assert!(verify_password("New$ecret1", &stored));
        assert_eq!(effective_hash(&db, &bootstrap).await, stored);
        assert_eq!(
            change_password(&db, &bootstrap, "secret", "Another$1", "Another$1").await,
            Err(PasswordError::IncorrectCurrent)
        );
    }

    #[tokio::test]
    async fn test_change_password_rejects_bad_input() {
        let db = init_test_database().await.unwrap();
        let bootstrap = hash_of_secret();

        let too_short = change_password(&db, &bootstrap, "secret", "Short1!", "Short1!").await;
        assert_eq!(too_short, Err(PasswordError::TooShort));
        let mismatch =
            change_password(&db, &bootstrap, "secret", "New$ecret1", "Different!1").await;
        assert_eq!(mismatch, Err(PasswordError::Mismatch));
        assert!(
            SettingsRepository::new(&db)
                .get(OVERRIDE_KEY)
                .await
                .is_none()
        );
    }
}
