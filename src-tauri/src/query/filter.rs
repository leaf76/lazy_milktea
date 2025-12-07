use crate::error::{LogcatError, Result};
use regex::{Regex, RegexBuilder};
use once_cell::sync::Lazy;

/// Maximum allowed regex pattern length
const REGEX_SIZE_LIMIT: usize = 1024;

/// Maximum DFA size to prevent memory explosion
const DFA_SIZE_LIMIT: usize = 1 << 20; // 1MB

/// Patterns that can cause catastrophic backtracking (ReDoS)
static DANGEROUS_PATTERNS: Lazy<Vec<Regex>> = Lazy::new(|| {
    vec![
        // Nested quantifiers on overlapping groups
        Regex::new(r"\(\.\+\)\+").unwrap(),
        Regex::new(r"\(\.\*\)\*").unwrap(),
        Regex::new(r"\(a\+\)\+").unwrap(),
        Regex::new(r"\(a\*\)\*").unwrap(),
        Regex::new(r"\([^)]+\+\)\+").unwrap(),
        // Overlapping alternations
        Regex::new(r"\(a\|a\+\)\+").unwrap(),
        // Very large repetitions
        Regex::new(r"\.\{[0-9]{4,}\}").unwrap(),      // .{1000,}
        Regex::new(r"\.\{[0-9]+,[0-9]{4,}\}").unwrap(), // .{n,1000}
    ]
});

/// Validate that a regex pattern is safe from ReDoS attacks
pub fn validate_regex_safety(pattern: &str) -> Result<()> {
    // Check length
    if pattern.len() > REGEX_SIZE_LIMIT {
        return Err(LogcatError::InvalidFilter(format!(
            "Regex pattern too long: {} > {} characters",
            pattern.len(),
            REGEX_SIZE_LIMIT
        )));
    }

    // Check for dangerous patterns
    for dangerous in DANGEROUS_PATTERNS.iter() {
        if dangerous.is_match(pattern) {
            return Err(LogcatError::InvalidFilter(
                "Potentially slow regex pattern detected. Avoid nested quantifiers like (a+)+ or (.*)*.".to_string()
            ));
        }
    }

    Ok(())
}

/// Compile a user-provided regex pattern with safety checks
pub fn compile_user_regex(pattern: &str, case_insensitive: bool) -> Result<Regex> {
    // Validate safety first
    validate_regex_safety(pattern)?;

    // Build with size limits
    RegexBuilder::new(pattern)
        .case_insensitive(case_insensitive)
        .size_limit(DFA_SIZE_LIMIT)
        .dfa_size_limit(DFA_SIZE_LIMIT)
        .build()
        .map_err(LogcatError::from)
}

/// Check if a plain text search should be performed instead of regex
pub fn should_use_plain_search(pattern: &str) -> bool {
    // If pattern contains no regex metacharacters, use plain search
    let metacharacters = ['.', '*', '+', '?', '[', ']', '(', ')', '{', '}', '|', '^', '$', '\\'];
    !pattern.chars().any(|c| metacharacters.contains(&c))
}

/// Perform case-insensitive plain text search
pub fn plain_text_contains(text: &str, pattern: &str, case_sensitive: bool) -> bool {
    if case_sensitive {
        text.contains(pattern)
    } else {
        text.to_lowercase().contains(&pattern.to_lowercase())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_safe_pattern() {
        assert!(validate_regex_safety("hello.*world").is_ok());
        assert!(validate_regex_safety(r"\d{4}-\d{2}-\d{2}").is_ok());
        assert!(validate_regex_safety("ActivityManager").is_ok());
    }

    #[test]
    fn test_reject_redos_patterns() {
        // Nested quantifiers
        assert!(validate_regex_safety("(a+)+").is_err());
        assert!(validate_regex_safety("(.*)*").is_err());
        assert!(validate_regex_safety("(.+)+").is_err());
    }

    #[test]
    fn test_reject_long_patterns() {
        let long_pattern = "a".repeat(REGEX_SIZE_LIMIT + 1);
        assert!(validate_regex_safety(&long_pattern).is_err());
    }

    #[test]
    fn test_compile_safe_regex() {
        // case_insensitive = true, so "hello" matches "Hello World"
        let re = compile_user_regex("hello", true).unwrap();
        assert!(re.is_match("Hello World"));
    }

    #[test]
    fn test_compile_case_sensitive() {
        // case_insensitive = false, so "hello" does NOT match "HELLO"
        let re = compile_user_regex("hello", false).unwrap();
        assert!(!re.is_match("HELLO"));
        assert!(re.is_match("hello world"));
    }

    #[test]
    fn test_should_use_plain_search() {
        assert!(should_use_plain_search("hello"));
        assert!(should_use_plain_search("ActivityManager"));
        assert!(!should_use_plain_search("hello.*world"));
        assert!(!should_use_plain_search(r"\d+"));
    }

    #[test]
    fn test_plain_text_contains() {
        assert!(plain_text_contains("Hello World", "world", false));
        assert!(!plain_text_contains("Hello World", "world", true));
        assert!(plain_text_contains("Hello World", "World", true));
    }
}
