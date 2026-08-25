use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub struct CancelToken {
    flag: AtomicBool,
    pids: Mutex<Vec<u32>>,
}

impl CancelToken {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            flag: AtomicBool::new(false),
            pids: Mutex::new(Vec::new()),
        })
    }

    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::Relaxed)
    }

    pub fn cancel(&self) {
        // Hold the lock while toggling the flag so register_pid cannot sneak in a
        // PID between flag-set and drain. Otherwise a cancel issued right before
        // register_pid leaves a live subprocess with no one to SIGTERM it.
        let pids: Vec<u32> = {
            let mut pids = self.pids.lock().unwrap();
            self.flag.store(true, Ordering::Relaxed);
            pids.drain(..).collect()
        };
        for pid in pids {
            unsafe {
                libc::kill(pid as libc::pid_t, libc::SIGTERM);
            }
        }
    }

    pub fn register_pid(&self, pid: u32) {
        let mut pids = self.pids.lock().unwrap();
        if self.flag.load(Ordering::Relaxed) {
            drop(pids);
            unsafe {
                libc::kill(pid as libc::pid_t, libc::SIGTERM);
            }
            return;
        }
        pids.push(pid);
    }

    pub fn unregister_pid(&self, pid: u32) {
        self.pids.lock().unwrap().retain(|p| *p != pid);
    }
}

pub(crate) fn is_cancelled(token: &Option<Arc<CancelToken>>) -> bool {
    token.as_ref().is_some_and(|t| t.is_cancelled())
}

#[derive(Clone, Default)]
pub struct CancelRegistry {
    inner: Arc<Mutex<HashMap<String, Arc<CancelToken>>>>,
}

impl CancelRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn create(&self, id: &str) -> Arc<CancelToken> {
        let token = CancelToken::new();
        self.inner
            .lock()
            .unwrap()
            .insert(id.to_string(), token.clone());
        token
    }

    /// Atomic create: returns `None` if a token for `id` already exists.
    /// Use this when the same `id` may be passed concurrently (e.g. summarize on
    /// an already-summarizing job) to avoid silently overwriting the previous token.
    pub fn try_create(&self, id: &str) -> Option<Arc<CancelToken>> {
        let mut guard = self.inner.lock().unwrap();
        if guard.contains_key(id) {
            return None;
        }
        let token = CancelToken::new();
        guard.insert(id.to_string(), token.clone());
        Some(token)
    }

    pub fn get(&self, id: &str) -> Option<Arc<CancelToken>> {
        self.inner.lock().unwrap().get(id).cloned()
    }

    pub fn cancel(&self, id: &str) -> bool {
        let tok = self.inner.lock().unwrap().get(id).cloned();
        if let Some(tok) = tok {
            tok.cancel();
            true
        } else {
            false
        }
    }

    /// Cancel every active token. Used on window close to clean up all running
    /// subprocesses (transcription, summarization).
    pub fn cancel_all(&self) {
        let tokens: Vec<Arc<CancelToken>> = self.inner.lock().unwrap().values().cloned().collect();
        for tok in tokens {
            tok.cancel();
        }
    }

    pub fn remove(&self, id: &str) {
        self.inner.lock().unwrap().remove(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn try_create_should_reject_duplicate_active_token() {
        let registry = CancelRegistry::new();

        assert!(registry.try_create("model:parakeet").is_some());
        assert!(registry.try_create("model:parakeet").is_none());

        registry.remove("model:parakeet");
        assert!(registry.try_create("model:parakeet").is_some());
    }
}
