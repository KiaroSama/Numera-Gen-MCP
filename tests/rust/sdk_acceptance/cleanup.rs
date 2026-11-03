use crate::common;
use std::{
    any::Any,
    future::{Future, poll_fn},
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    pin::Pin,
    sync::{Arc, Mutex},
    task::Poll,
    time::Duration,
};
use tokio::{task::JoinSet, time::timeout};
use tokio_util::task::AbortOnDropHandle;

type Failure = Box<dyn Any + Send>;
type Cleanup = Pin<Box<dyn Future<Output = Result<(), String>> + Send>>;

#[derive(Default)]
struct Pending {
    jobs: Vec<Cleanup>,
    workspaces: Vec<common::Workspace>,
}

tokio::task_local! {
    static PENDING: Arc<Mutex<Pending>>;
}

pub fn active() -> bool {
    PENDING.try_with(|_| ()).is_ok()
}

pub fn defer(operation: impl Future<Output = Result<(), String>> + Send + 'static) {
    let mut operation: Option<Cleanup> = Some(Box::pin(operation));
    if PENDING
        .try_with(|pending| {
            pending.lock().unwrap().jobs.push(operation.take().unwrap());
        })
        .is_err()
    {
        if std::thread::panicking() {
            eprintln!("ERROR: SDK cleanup was dropped outside its awaited scenario scope");
        } else {
            panic!("SDK cleanup requires an awaited scenario scope");
        }
    }
}

pub async fn join<T>(
    mut task: AbortOnDropHandle<T>,
    ms: u64,
    cancelled_ok: bool,
) -> Result<Option<T>, String> {
    match timeout(Duration::from_millis(ms), &mut task).await {
        Ok(Ok(value)) => Ok(Some(value)),
        Ok(Err(error)) if cancelled_ok && error.is_cancelled() => Ok(None),
        Ok(Err(_)) => Err("Owned SDK cleanup task panicked or was cancelled".into()),
        Err(_) => {
            task.abort();
            let _ = task.await;
            Err("Owned SDK cleanup task exceeded its deadline".into())
        }
    }
}

async fn outcome<T>(operation: impl Future<Output = T>) -> Result<T, Failure> {
    let mut operation = Box::pin(operation);
    poll_fn(
        |context| match catch_unwind(AssertUnwindSafe(|| operation.as_mut().poll(context))) {
            Ok(Poll::Ready(value)) => Poll::Ready(Ok(value)),
            Ok(Poll::Pending) => Poll::Pending,
            Err(error) => Poll::Ready(Err(error)),
        },
    )
    .await
}

/// Drop the body inside its scope, await every registered owner, then remove fixture files.
/// Cleanup failures never replace or silently approve an original scenario failure.
pub fn run<T>(operation: impl Future<Output = T>) -> impl Future<Output = T> {
    // Allocate before constructing the scope future, which would otherwise embed the scenario.
    run_boxed(Box::pin(operation))
}

async fn run_boxed<T>(operation: Pin<Box<impl Future<Output = T>>>) -> T {
    let pending = Arc::new(Mutex::new(Pending::default()));
    let result = PENDING.scope(pending.clone(), outcome(operation)).await;
    let Pending { jobs, workspaces } = std::mem::take(&mut *pending.lock().unwrap());
    let mut tasks = JoinSet::new();
    for job in jobs {
        tasks.spawn(outcome(job));
    }
    let mut errors = Vec::new();
    let drained = timeout(Duration::from_secs(8), async {
        while let Some(result) = tasks.join_next().await {
            match result {
                Ok(Ok(Ok(()))) => {}
                Ok(Ok(Err(error))) => errors.push(error),
                _ => errors.push("Owned SDK cleanup panicked".into()),
            }
        }
    })
    .await;
    if drained.is_err() {
        tasks.abort_all();
        while tasks.join_next().await.is_some() {}
        errors.push("SDK scenario cleanup exceeded eight seconds".into());
    }
    // A failed cleanup remains a failed test, but all owned joins settle before file deletion.
    for work in workspaces {
        if catch_unwind(AssertUnwindSafe(|| drop(work))).is_err() {
            errors.push("SDK fixture workspace cleanup failed".into());
        }
    }
    if !errors.is_empty() {
        let original = result.as_ref().err().map(|error| {
            error
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| error.downcast_ref::<&str>().map(|text| text.to_string()))
                .unwrap_or_else(|| "Original SDK scenario panic".into())
        });
        panic!(
            "{}cleanup: {}",
            original.map(|text| text + "; ").unwrap_or_default(),
            errors.join("; ")
        );
    }
    match result {
        Ok(value) => value,
        Err(error) => resume_unwind(error),
    }
}

pub struct Workspace(pub std::path::PathBuf, Option<common::Workspace>);
impl Workspace {
    pub fn new() -> Self {
        let work = common::Workspace::new();
        Self(work.0.clone(), Some(work))
    }
}
impl std::ops::Deref for Workspace {
    type Target = common::Workspace;
    fn deref(&self) -> &common::Workspace {
        self.1.as_ref().expect("owned SDK fixture workspace")
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = PENDING.try_with(|pending| {
            pending
                .lock()
                .unwrap()
                .workspaces
                .push(self.1.take().unwrap());
        });
    }
}
