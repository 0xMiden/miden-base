use miden_protocol::errors::AccountError;

/// Configures the proposal delay rules used by smart multisig timelock flows.
///
/// `min_delay` (in seconds) defines how long a proposal must wait before execution, while
/// `propose_expiration_delta` (in blocks) controls the transaction expiration delta applied to
/// proposal transactions.
///
/// Both fields must be non-zero.
///
/// # Propose threshold
///
/// `propose_threshold` is the number of approver signatures `propose_transaction` requires. It is
/// optional; when unset, proposing requires the account's default threshold.
///
/// Setting it *below* the default threshold is what makes the timelock actually reduce the number
/// of signatures a delayed action costs. Because a proposal is recorded under the target's
/// transaction summary commitment, and that same commitment is the message verified again at
/// execution time, the signatures collected to propose already count towards the execution
/// threshold. The delayed path therefore costs `max(propose_threshold, delay_threshold)`
/// signatures; leaving `propose_threshold` unset pins that floor at the default threshold, so a
/// procedure's `delay_threshold` can then only ever raise the bar, never lower it.
///
/// Lowering the propose threshold does not weaken execution: the per-procedure `delay_threshold`
/// (or the default threshold for procedures without a policy) is still enforced when the proposed
/// transaction actually runs. A low propose threshold only makes it cheap to *start the timelock*,
/// which in turn means a small set of approvers can act unilaterally once the delay elapses unless
/// the proposal is cancelled within the window - see the security considerations on
/// [`AuthMultisigSmart`](super::AuthMultisigSmart).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DelayedExecutionPolicy {
    min_delay: u32,
    propose_expiration_delta: u16,
    propose_threshold: Option<u32>,
}

impl DelayedExecutionPolicy {
    /// Creates a new policy after validating that both fields are non-zero.
    ///
    /// The propose threshold is left unset, so proposing requires the account's default threshold.
    /// Use [`DelayedExecutionPolicy::with_propose_threshold`] to lower it.
    pub fn new(min_delay: u32, propose_expiration_delta: u16) -> Result<Self, AccountError> {
        if min_delay == 0 {
            return Err(AccountError::other("delayed execution min_delay must be non-zero"));
        }
        if propose_expiration_delta == 0 {
            return Err(AccountError::other(
                "delayed execution propose_expiration_delta must be non-zero",
            ));
        }
        Ok(Self {
            min_delay,
            propose_expiration_delta,
            propose_threshold: None,
        })
    }

    /// Sets the number of signatures `propose_transaction` requires.
    ///
    /// See the type-level documentation for what lowering this below the default threshold means.
    pub fn with_propose_threshold(mut self, propose_threshold: u32) -> Result<Self, AccountError> {
        if propose_threshold == 0 {
            return Err(AccountError::other(
                "delayed execution propose_threshold must be non-zero",
            ));
        }
        self.propose_threshold = Some(propose_threshold);
        Ok(self)
    }

    pub const fn min_delay(&self) -> u32 {
        self.min_delay
    }

    pub const fn propose_expiration_delta(&self) -> u16 {
        self.propose_expiration_delta
    }

    /// Returns the configured propose threshold, or `None` when proposing falls back to the
    /// account's default threshold.
    pub const fn propose_threshold(&self) -> Option<u32> {
        self.propose_threshold
    }
}

#[cfg(test)]
mod tests {
    use alloc::string::ToString;

    use super::DelayedExecutionPolicy;

    #[test]
    fn delayed_execution_policy_rejects_zero_min_delay() {
        let err = DelayedExecutionPolicy::new(0, 5).unwrap_err();
        assert!(err.to_string().contains("min_delay must be non-zero"));
    }

    #[test]
    fn delayed_execution_policy_rejects_zero_propose_expiration_delta() {
        let err = DelayedExecutionPolicy::new(30, 0).unwrap_err();
        assert!(err.to_string().contains("propose_expiration_delta must be non-zero"));
    }

    #[test]
    fn delayed_execution_policy_accepts_valid_values() {
        let policy =
            DelayedExecutionPolicy::new(30, 2).expect("non-zero arguments should be accepted");
        assert_eq!(policy.min_delay(), 30);
        assert_eq!(policy.propose_expiration_delta(), 2);
        assert_eq!(policy.propose_threshold(), None);
    }

    #[test]
    fn delayed_execution_policy_rejects_zero_propose_threshold() {
        let err = DelayedExecutionPolicy::new(30, 2)
            .unwrap()
            .with_propose_threshold(0)
            .unwrap_err();
        assert!(err.to_string().contains("propose_threshold must be non-zero"));
    }

    #[test]
    fn delayed_execution_policy_accepts_propose_threshold() {
        let policy = DelayedExecutionPolicy::new(30, 2)
            .unwrap()
            .with_propose_threshold(1)
            .expect("non-zero propose threshold should be accepted");
        assert_eq!(policy.propose_threshold(), Some(1));
    }
}
