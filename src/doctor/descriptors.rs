//! Descriptor allowance derived from the declared launch profile.

use crate::launch::{DescriptorBudget, LaunchError};
use serde::Serialize;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DescriptorCheck {
    pub declared_limit: u64,
    pub fixed_reserve: u64,
    pub effective_worker_count: u64,
    pub guest_allowance: u64,
}

/// Return the declared limit, pinned reserve inputs, and resulting guest allowance.
///
/// # Errors
///
/// Returns an error when the reserve overflows or does not leave a positive allowance.
pub fn host_fd_limit_sufficient(budget: DescriptorBudget) -> Result<DescriptorCheck, LaunchError> {
    let guest_allowance = budget.guest_allowance()?;
    Ok(DescriptorCheck {
        declared_limit: budget.limit,
        fixed_reserve: DescriptorBudget::PINNED_FIXED_RESERVE,
        effective_worker_count: budget.effective_worker_count(),
        guest_allowance,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::launch::VIRTIOFSD_RLIMIT_NOFILE;

    #[test]
    fn current_allowance_and_boundaries_are_derived() {
        let current = host_fd_limit_sufficient(DescriptorBudget::default()).unwrap();
        // 610, not 609: the default pool is `0` and the daemon floors the per-worker term
        // at one. See `DescriptorBudget::effective_worker_count`.
        assert_eq!(current.guest_allowance, VIRTIOFSD_RLIMIT_NOFILE - 610);
        assert_eq!(current.effective_worker_count, 1);
        assert!(
            host_fd_limit_sufficient(DescriptorBudget {
                limit: 610,
                worker_pool_size: 1
            })
            .is_err()
        );
        let changed = host_fd_limit_sufficient(DescriptorBudget {
            limit: 1_000,
            worker_pool_size: 4,
        })
        .unwrap();
        assert_eq!(changed.guest_allowance, 387);
    }
}
