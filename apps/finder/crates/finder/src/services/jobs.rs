//! File-operation control shared by the worker and its window. Never blocks the UI.
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    path::PathBuf,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Change {
    RecoveryCopy {
        source: PathBuf,
        destination: PathBuf,
    },
    Created(PathBuf),
    Moved {
        from: PathBuf,
        to: PathBuf,
    },
    Trashed {
        id: PathBuf,
        name: PathBuf,
        parent: PathBuf,
        deleted: i64,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Decision {
    Skip,
    KeepBoth,
    Replace,
    Merge,
    Cancel,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Conflict {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub directories: bool,
}
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Progress {
    pub path: PathBuf,
    pub bytes: u64,
    pub completed: usize,
    pub conflict: Option<Conflict>,
    pub warnings: Vec<String>,
}
#[derive(Default)]
pub struct Job {
    pub cancelled: AtomicBool,
    pub progress: Mutex<Progress>,
    pub changes: Mutex<Vec<Change>>,
    answer: Mutex<Option<(Decision, bool)>>,
    policy: Mutex<Option<Decision>>,
    wake: Condvar,
}
thread_local! { static CURRENT: RefCell<Option<Arc<Job>>> = const { RefCell::new(None) }; }
pub fn scoped<T>(job: Arc<Job>, f: impl FnOnce() -> T) -> T {
    struct Reset(Option<Arc<Job>>);
    impl Drop for Reset {
        fn drop(&mut self) {
            CURRENT.with(|v| *v.borrow_mut() = self.0.take());
        }
    }
    let _reset = Reset(CURRENT.with(|v| v.replace(Some(job))));
    f()
}
pub fn current() -> Option<Arc<Job>> {
    CURRENT.with(|v| v.borrow().clone())
}
pub fn checkpoint() -> Result<(), String> {
    if current().is_some_and(|j| j.cancelled.load(Ordering::Relaxed)) {
        Err("Operation cancelled".into())
    } else {
        Ok(())
    }
}
pub fn record(change: Change) {
    if let Some(j) = current() {
        j.changes.lock().unwrap().push(change);
    }
}
pub fn advance(path: &std::path::Path, bytes: u64) {
    if let Some(j) = current() {
        let mut p = j.progress.lock().unwrap();
        p.path = path.into();
        p.bytes += bytes;
    }
}
pub fn completed() {
    if let Some(j) = current() {
        j.progress.lock().unwrap().completed += 1;
    }
}
pub fn warning(message: String) {
    if let Some(j) = current() {
        j.progress.lock().unwrap().warnings.push(message);
    }
}
pub fn conflict(c: Conflict) -> Result<Decision, String> {
    let Some(j) = current() else {
        return Err(format!("{} already exists", c.destination.display()));
    };
    if let Some(policy) = *j.policy.lock().unwrap() {
        if policy != Decision::Merge || c.directories {
            return Ok(policy);
        }
    }
    j.progress.lock().unwrap().conflict = Some(c);
    let mut answer = j.answer.lock().unwrap();
    loop {
        checkpoint()?;
        if let Some((decision, all)) = answer.take() {
            j.progress.lock().unwrap().conflict = None;
            if all {
                *j.policy.lock().unwrap() = Some(decision);
            }
            if decision == Decision::Cancel {
                return Err("Operation cancelled".into());
            }
            return Ok(decision);
        }
        answer = j
            .wake
            .wait_timeout(answer, Duration::from_millis(100))
            .unwrap()
            .0;
    }
}
impl Job {
    pub fn answer(&self, decision: Decision, all: bool) {
        *self.answer.lock().unwrap() = Some((decision, all));
        self.wake.notify_all();
    }
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
        self.wake.notify_all();
    }
}
