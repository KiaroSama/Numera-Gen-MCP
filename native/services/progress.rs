//! Best-effort MCP `notifications/progress` reporter. Transport failures never abort a job.
use rmcp::{
    RoleServer,
    model::{ProgressNotificationParam, ProgressToken},
    service::Peer,
};
use std::sync::atomic::{AtomicU64, Ordering};

pub struct ProgressReporter {
    token: ProgressToken,
    peer: Peer<RoleServer>,
    step: AtomicU64,
}

impl ProgressReporter {
    pub fn new(token: ProgressToken, peer: Peer<RoleServer>) -> Self {
        Self {
            token,
            peer,
            step: AtomicU64::new(0),
        }
    }

    /// Emits a static sanitized ASCII phase message; errors are swallowed so jobs never fail.
    pub async fn report(&self, message: &'static str) {
        let progress = self.step.fetch_add(1, Ordering::Relaxed).saturating_add(1);
        let param = ProgressNotificationParam::new(self.token.clone(), progress as f64)
            .with_message(message);
        let _ = self.peer.notify_progress(param).await;
    }
}
