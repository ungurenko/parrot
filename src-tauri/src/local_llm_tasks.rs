use std::sync::{Arc, Mutex};

use crate::cancellation::CancelToken;

struct ActiveTask {
    id: String,
    token: Arc<CancelToken>,
}

#[derive(Clone, Default)]
pub struct LocalLlmTasks {
    active: Arc<Mutex<Option<ActiveTask>>>,
}

impl LocalLlmTasks {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn try_start(&self, id: &str) -> Option<LocalLlmLease> {
        let mut active = self.active.lock().unwrap();
        if active.is_some() {
            return None;
        }
        let token = CancelToken::new();
        *active = Some(ActiveTask {
            id: id.to_string(),
            token: token.clone(),
        });
        Some(LocalLlmLease {
            tasks: self.clone(),
            id: id.to_string(),
            token,
        })
    }

    #[cfg(test)]
    pub fn is_busy(&self) -> bool {
        self.active.lock().unwrap().is_some()
    }

    pub fn cancel(&self, id: &str) -> bool {
        let active = self.active.lock().unwrap();
        if let Some(task) = active.as_ref().filter(|task| task.id == id) {
            task.token.cancel();
            true
        } else {
            false
        }
    }

    pub fn cancel_all(&self) {
        let token = self
            .active
            .lock()
            .unwrap()
            .as_ref()
            .map(|task| task.token.clone());
        if let Some(token) = token {
            token.cancel();
        }
    }

    fn finish(&self, id: &str, token: &Arc<CancelToken>) {
        let mut active = self.active.lock().unwrap();
        if active
            .as_ref()
            .is_some_and(|task| task.id == id && Arc::ptr_eq(&task.token, token))
        {
            *active = None;
        }
    }

    fn begin_commit(&self, id: &str, token: &Arc<CancelToken>) -> bool {
        let mut active = self.active.lock().unwrap();
        let matches = active
            .as_ref()
            .is_some_and(|task| task.id == id && Arc::ptr_eq(&task.token, token));
        if !matches {
            return false;
        }
        let canceled = token.is_cancelled();
        *active = None;
        !canceled
    }
}

pub struct LocalLlmLease {
    tasks: LocalLlmTasks,
    id: String,
    token: Arc<CancelToken>,
}

impl LocalLlmLease {
    pub fn token(&self) -> Arc<CancelToken> {
        self.token.clone()
    }

    pub fn begin_commit(&mut self) -> bool {
        self.tasks.begin_commit(&self.id, &self.token)
    }
}

impl Drop for LocalLlmLease {
    fn drop(&mut self) {
        self.tasks.finish(&self.id, &self.token);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_one_local_model_task_can_run() {
        let tasks = LocalLlmTasks::new();
        let first = tasks.try_start("summary:one").expect("first task");

        assert!(tasks.is_busy());
        assert!(tasks.try_start("translation:two").is_none());

        drop(first);
        assert!(!tasks.is_busy());
        assert!(tasks.try_start("translation:two").is_some());
    }

    #[test]
    fn dropping_generation_lease_closes_cancellation_window() {
        let tasks = LocalLlmTasks::new();
        let lease = tasks.try_start("translation:one").expect("task");

        assert!(tasks.cancel("translation:one"));
        assert!(lease.token().is_cancelled());

        drop(lease);
        assert!(!tasks.cancel("translation:one"));
    }

    #[test]
    fn cancellation_and_commit_have_one_atomic_winner() {
        let tasks = LocalLlmTasks::new();
        let mut canceled = tasks.try_start("translation:cancel").expect("task");
        assert!(tasks.cancel("translation:cancel"));
        assert!(!canceled.begin_commit());

        let mut committed = tasks.try_start("translation:commit").expect("task");
        assert!(committed.begin_commit());
        assert!(!tasks.cancel("translation:commit"));
    }
}
