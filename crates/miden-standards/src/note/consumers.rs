// NOTE CONSUMERS
// ================================================================================================

/// Declares which accounts a note allows to consume it.
///
/// Every note script states the same rule on the `Consumers:` line of the doc comment of its
/// `@note_script` procedure and, unless it is [`Unrestricted`](NoteConsumers::Unrestricted),
/// enforces it with the `miden::standards::note::consumer` procedures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NoteConsumers {
    /// Only the single account the note commits to may consume it.
    TargetAccount,

    /// Only one of a fixed set of accounts the note commits to may consume it, and the note script
    /// decides which of them a given consumption belongs to.
    CommittedAccounts,

    /// Any account may consume the note.
    Unrestricted { rationale: &'static str },
}

impl NoteConsumers {
    /// Returns whether consumption is restricted to accounts the note commits to.
    pub const fn is_restricted(&self) -> bool {
        !matches!(self, Self::Unrestricted { .. })
    }

    /// Returns the name of the rule, which is the class named on the note script's `Consumers:`
    /// line.
    pub const fn name(&self) -> &'static str {
        match self {
            Self::TargetAccount => "target account",
            Self::CommittedAccounts => "committed accounts",
            Self::Unrestricted { .. } => "unrestricted",
        }
    }
}
