//! Private, numeric-only observation of a contained coverage report. No diagnostic controls execution.
use serde::{Deserialize, Serialize};
use std::{io, path::Path, process::Command};

#[path = "linux_snapshot_owner.rs"]
mod owner;
#[path = "linux_snapshot_proc.rs"]
mod proc;

pub use owner::run_owner;
pub use proc::sample_from_stdio;

pub const RECEIPT_LIMIT: usize = 16384;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub schema: u8,
    pub nonce: String,
    pub owner_pid: u32,
    pub group: i32,
    pub report_pid: u32,
    pub status: u8,
    pub signal_ms: Option<u64>,
    pub sample_ms: Option<u64>,
    pub snapshot: Option<Snapshot>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub first: Vec<Row>,
    pub second: Vec<Row>,
    pub interval_ms: u64,
    pub tools_verified: u8,
    pub clock_ticks: Option<u64>,
    pub page_bytes: Option<u64>,
    pub truncated: bool,
    pub errors: u32,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Row {
    pub pid: u32,
    pub ppid: u32,
    pub start_ticks: u64,
    pub role: u8,
    pub state: u8,
    pub user_ticks: u64,
    pub system_ticks: u64,
    pub rss_pages: u64,
    pub major_faults: u64,
    pub threads: u64,
    pub io: Option<[u64; 3]>,
    pub wait: u8,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Setup {
    owner_pid: u32,
    group: i32,
    report_pid: u32,
    tools: Option<[String; 2]>,
}

fn invalid() -> io::Error {
    io::Error::other("Coverage observation unavailable.")
}
pub fn valid_nonce(nonce: &str) -> bool {
    nonce.len() == 32
        && nonce
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
impl Receipt {
    pub fn validate(&self, nonce: &str) -> io::Result<()> {
        if self.schema != 1
            || !valid_nonce(nonce)
            || self.nonce != nonce
            || self.owner_pid == 0
            || self.group <= 0
            || self.owner_pid != self.group as u32
            || self.report_pid == 0
            || self.report_pid == self.owner_pid
            || self.status > 6
            || self.signal_ms.is_some_and(|n| n > 3600000)
            || self.sample_ms.is_some_and(|n| n > 3600000)
        {
            return Err(invalid());
        }
        if let Some(snapshot) = &self.snapshot {
            if self.status != 0
                || self.signal_ms.is_none()
                || self.sample_ms.is_none()
                || snapshot.first.len() > 8
                || snapshot.second.len() > 8
                || snapshot.interval_ms > 500
                || snapshot.errors > 10000
                || snapshot.tools_verified > 2
                || snapshot.first.is_empty()
                || snapshot.second.is_empty()
                || self.sample_ms < self.signal_ms
                || self
                    .sample_ms
                    .zip(self.signal_ms)
                    .is_some_and(|(sample, signal)| sample - signal > 500)
                || snapshot.clock_ticks.is_some_and(|n| n == 0)
                || snapshot.page_bytes.is_some_and(|n| n == 0)
            {
                return Err(invalid());
            }
            for row in snapshot.first.iter().chain(&snapshot.second) {
                if row.pid == 0
                    || row.ppid == 0
                    || row.role > 2
                    || row.state > 9
                    || row.wait > 4
                    || row.threads == 0
                {
                    return Err(invalid());
                }
            }
        } else if self.status == 0 {
            return Err(invalid());
        }
        Ok(())
    }
}

/// The optional discovery command is a direct owned child, inside the original outer budget.
pub fn observe_report(
    report: &mut Command,
    helper: &mut Command,
    discovery: Option<&mut Command>,
    receipt: &Path,
    nonce: &str,
) -> io::Result<i32> {
    owner::run(report, helper, discovery, receipt, nonce)
}
