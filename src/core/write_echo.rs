use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct WriteToken {
    pub content_hash: Option<u64>,
    pub registered_at: Instant,
    pub expires_at: Instant,
}

impl WriteToken {
    pub fn new(content_hash: Option<u64>, ttl: Duration) -> Self {
        let now = Instant::now();
        Self {
            content_hash,
            registered_at: now,
            expires_at: now + ttl,
        }
    }

    pub fn is_expired(&self, now: Instant) -> bool {
        now > self.expires_at
    }
}

/// Write-Echo Cancellation Token Cache.
///
/// Prevents filesystem event loops when the application writes a note buffer to disk.
/// Any save registers an entry in this cache, and incoming OS file change events
/// check this cache before triggering full re-indexing or UI reloads.
#[derive(Debug, Clone)]
pub struct WriteEchoCache {
    tokens: Arc<Mutex<HashMap<PathBuf, Vec<WriteToken>>>>,
    default_ttl: Duration,
}

impl Default for WriteEchoCache {
    fn default() -> Self {
        Self::new(Duration::from_millis(1500))
    }
}

impl WriteEchoCache {
    pub fn new(default_ttl: Duration) -> Self {
        Self {
            tokens: Arc::new(Mutex::new(HashMap::new())),
            default_ttl,
        }
    }

    /// Register a self-initiated write event for a file path.
    pub fn register(&self, path: impl AsRef<Path>, content_hash: Option<u64>) {
        let canonical_or_norm = path.as_ref().to_path_buf();
        let token = WriteToken::new(content_hash, self.default_ttl);
        let now = Instant::now();

        if let Ok(mut lock) = self.tokens.lock() {
            let entry = lock.entry(canonical_or_norm).or_default();
            // Drop expired tokens while adding new one
            entry.retain(|t| !t.is_expired(now));
            entry.push(token);
        }
    }

    /// Check if an incoming filesystem event matches a registered write echo.
    /// If an echo is found, it is consumed (removed) and `true` is returned.
    pub fn consume_echo(&self, path: impl AsRef<Path>, content_hash: Option<u64>) -> bool {
        let canonical_or_norm = path.as_ref();
        let now = Instant::now();

        if let Ok(mut lock) = self.tokens.lock() {
            if let Some(tokens) = lock.get_mut(canonical_or_norm) {
                // Remove expired
                tokens.retain(|t| !t.is_expired(now));

                // Find matching token
                let match_index = tokens.iter().position(|t| {
                    if let (Some(expected), Some(actual)) = (t.content_hash, content_hash) {
                        expected == actual
                    } else {
                        true
                    }
                });

                if let Some(idx) = match_index {
                    tokens.remove(idx);
                    if tokens.is_empty() {
                        lock.remove(canonical_or_norm);
                    }
                    return true;
                }

                if tokens.is_empty() {
                    lock.remove(canonical_or_norm);
                }
            }
        }
        false
    }

    /// Periodic cleanup of expired tokens.
    pub fn cleanup(&self) {
        let now = Instant::now();
        if let Ok(mut lock) = self.tokens.lock() {
            lock.retain(|_, tokens| {
                tokens.retain(|t| !t.is_expired(now));
                !tokens.is_empty()
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn test_write_echo_registration_and_consumption() {
        let cache = WriteEchoCache::new(Duration::from_millis(500));
        let path = PathBuf::from("/vault/test.md");
        let hash = Some(123456789);

        // Register
        cache.register(&path, hash);

        // First event consumption matches
        assert!(cache.consume_echo(&path, hash));

        // Subsequent event does not match since token was consumed
        assert!(!cache.consume_echo(&path, hash));
    }

    #[test]
    fn test_write_echo_ttl_expiry() {
        let cache = WriteEchoCache::new(Duration::from_millis(50));
        let path = PathBuf::from("/vault/expired.md");

        cache.register(&path, None);
        sleep(Duration::from_millis(70));

        // After TTL, token should be expired
        assert!(!cache.consume_echo(&path, None));
    }
}
