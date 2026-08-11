/// Parses a string value and validates it against a predicate.
/// Returns the parsed value on success, or the error message on failure.
pub fn parse_and_validate<T: std::str::FromStr>(
    value: &str,
    error_msg: &str,
    predicate: impl FnOnce(&T) -> bool,
) -> Result<T, String> {
    value
        .parse::<T>()
        .ok()
        .filter(|v| predicate(v))
        .ok_or_else(|| error_msg.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_and_validate_int_valid() {
        let result = parse_and_validate::<i64>("42", "Must be positive", |v| *v > 0);
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn test_parse_and_validate_int_invalid() {
        let result = parse_and_validate::<i64>("abc", "Must be positive", |v| *v > 0);
        assert!(result.is_err());
    }
}
