//! Utility functions for command parsing.

/// Compute Levenshtein distance between two strings.
pub fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let m = a.len();
    let n = b.len();
    if m == 0 { return n; }
    if n == 0 { return m; }
    let mut dp = vec![vec![0; n + 1]; m + 1];
    for (i, row) in dp.iter_mut().enumerate().take(m + 1) { row[0] = i; }
    for (j, val) in dp[0].iter_mut().enumerate().take(n + 1).skip(1) { *val = j; }
    for i in 1..=m {
        for j in 1..=n {
            let cost = if a[i-1] == b[j-1] { 0 } else { 1 };
            dp[i][j] = (dp[i-1][j] + 1).min((dp[i][j-1] + 1).min(dp[i-1][j-1] + cost));
        }
    }
    dp[m][n]
}

/// Check if a string looks like a URL.
/// Matches: http://, https://, aileron://, or bare domains like "example.com"
pub fn looks_like_url(s: &str) -> bool {
    // Explicit scheme
    if s.contains("://") {
        return true;
    }
    // Bare domain: contains a dot and no spaces, and doesn't look like a command
    if s.contains('.') && !s.contains(' ') && !s.contains('/') {
        // Exclude things that look like file paths or commands
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() >= 2 && parts.iter().all(|p| !p.is_empty()) {
            // Check TLD is reasonable (at least 2 chars, all alpha)
            if let Some(tld) = parts.last() {
                return tld.len() >= 2 && tld.chars().all(|c| c.is_alphabetic());
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levenshtein_same() {
        assert_eq!(levenshtein_distance("hello", "hello"), 0);
    }

    #[test]
    fn levenshtein_empty() {
        assert_eq!(levenshtein_distance("", "abc"), 3);
        assert_eq!(levenshtein_distance("abc", ""), 3);
    }

    #[test]
    fn levenshtein_substitution() {
        assert_eq!(levenshtein_distance("cat", "bat"), 1);
    }

    #[test]
    fn levenshtein_insertion() {
        assert_eq!(levenshtein_distance("cat", "cats"), 1);
    }

    #[test]
    fn url_with_scheme() {
        assert!(looks_like_url("https://example.com"));
        assert!(looks_like_url("http://localhost:8080"));
        assert!(looks_like_url("aileron://settings"));
    }

    #[test]
    fn url_bare_domain() {
        assert!(looks_like_url("example.com"));
        assert!(looks_like_url("www.example.com"));
        assert!(looks_like_url("sub.domain.org"));
    }

    #[test]
    fn not_url() {
        assert!(!looks_like_url("quit"));
        assert!(!looks_like_url("set adblock on"));
        assert!(!looks_like_url("hello"));
        // "file.txt" is intentionally accepted — bare domain detection is permissive
    }
}
