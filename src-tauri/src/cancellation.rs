use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub struct CancelToken {
    flag: AtomicBool,
    pids: Mutex<Vec<u32>>,
}

impl CancelToken {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            flag: AtomicBool::new(false),
            pids: Mutex::new(Vec::new()),
        })
    }

    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::Relaxed)
    }

    pub fn cancel(&self) {
        self.flag.store(true, Ordering::Relaxed);
        let pids: Vec<u32> = self.pids.lock().unwrap().drain(..).collect();
        for pid in pids {
            unsafe {
                libc::kill(pid as libc::pid_t, libc::SIGTERM);
            }
        }
    }

    pub fn register_pid(&self, pid: u32) {
        self.pids.lock().unwrap().push(pid);
    }

    pub fn unregister_pid(&self, pid: u32) {
        self.pids.lock().unwrap().retain(|p| *p != pid);
    }
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

    pub fn remove(&self, id: &str) {
        self.inner.lock().unwrap().remove(id);
    }
}
