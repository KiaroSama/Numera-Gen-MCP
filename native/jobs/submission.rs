use super::{Receipt, Store};
use crate::{
    compat::value::json,
    error::{NumeraError, Result},
};

#[derive(Clone, Copy)]
pub struct SubmissionLimits {
    pub queued: usize,
    pub global: usize,
    pub connection: usize,
    pub bytes: i64,
    pub journal: i64,
}

pub struct PreparedSubmission {
    pub fresh: bool,
    pub receipt: Receipt,
    pub admission: Result<bool>,
}

impl Store {
    /// Commit a new intent and, when capacity is available, its reservation together.
    /// The caller may send only after this method returns successful admission.
    pub fn prepare_submission(
        &mut self,
        receipt: Receipt,
        hash: &str,
        identity: &str,
        limits: SubmissionLimits,
    ) -> Result<PreparedSubmission> {
        let id = receipt.id()?.to_owned();
        let previous_generation = self.generations.get(&id).copied();
        let result = self.transaction(|store| {
            let (fresh, mut receipt) = store.prepare_in_transaction(receipt, hash, identity)?;
            let admission = if fresh {
                store.submission_savepoint(&mut receipt, limits, true)?
            } else {
                Ok(false)
            };
            Ok(PreparedSubmission {
                fresh,
                receipt,
                admission,
            })
        });
        // Restore the owner cache as well as SQL when the outer transaction fails.
        if result.is_err() {
            if let Some(generation) = previous_generation {
                self.generations.insert(id, generation);
            } else {
                self.generations.remove(&id);
            }
        }
        result
    }

    /// Retry only local admission for a persisted queued intent, never the POST.
    pub fn admit_submission(&mut self, receipt: &mut Receipt, limits: SubmissionLimits) -> Result<bool> {
        let mut candidate = receipt.clone();
        let admitted =
            self.transaction(|store| store.submission_inner(&mut candidate, limits, false))?;
        if admitted {
            *receipt = candidate;
        }
        Ok(admitted)
    }

    fn submission_savepoint(
        &mut self,
        receipt: &mut Receipt,
        limits: SubmissionLimits,
        check_queue: bool,
    ) -> Result<Result<bool>> {
        let prepared = receipt.clone();
        self.db
            .execute_batch("SAVEPOINT submission_admission")
            .map_err(|_| NumeraError::storage())?;
        let result = self.submission_inner(receipt, limits, check_queue);
        if result.is_err() {
            self.db
                .execute_batch("ROLLBACK TO submission_admission")
                .map_err(|_| NumeraError::storage())?;
            *receipt = prepared;
        }
        self.db
            .execute_batch("RELEASE submission_admission")
            .map_err(|_| NumeraError::storage())?;
        Ok(result)
    }

    fn submission_inner(
        &mut self,
        receipt: &mut Receipt,
        limits: SubmissionLimits,
        check_queue: bool,
    ) -> Result<bool> {
        let id = receipt.id()?.to_owned();
        let connection = receipt.value["connection"]
            .as_str()
            .ok_or_else(NumeraError::storage)?
            .to_owned();
        if check_queue && self.queued_in_transaction()? > limits.queued {
            return Err(NumeraError::new("rate_limited", "Local bounded queue is full."));
        }
        if !self.admit_in_transaction(&id, &connection, limits.global, limits.connection)? {
            return Ok(false);
        }
        self.reserve_in_transaction(&id, limits.bytes, limits.journal)?;
        receipt.value["status"] = json!("submitting");
        receipt.value["generation_outcome"] = json!("unknown");
        receipt.value["another_submission_may_charge"] = json!(true);
        self.update(receipt)?;
        Ok(true)
    }
}
