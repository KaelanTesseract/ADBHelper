//! Error type for AFK Journey actions.
//!
//! Recovery decisions depend on what went wrong, not on the wording of a
//! translated message. Signals the recovery loop acts on have their own
//! variant; every other failure keeps its readable text in `Message`.

use crate::localization::tr;
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActionError {
    /// The user pressed Stop.
    Cancelled,
    /// No battle result appeared within the battle time limit.
    BattleResultTimeout,
    /// The game's Android process ended while an action was running.
    GameCrashed,
    /// Any other failure, already phrased for the user.
    Message(String),
}

impl ActionError {
    /// ADB lost the connection to the device (as opposed to a game or input problem).
    pub fn is_transport(&self) -> bool {
        matches!(self, Self::Message(message) if crate::adb::is_transport_error(message))
    }

    /// The text shown to the user, in the selected language.
    pub fn as_text(&self) -> &str {
        match self {
            Self::Cancelled => tr!("error-cancelled-by-user"),
            Self::BattleResultTimeout => tr!("error-no-battle-result-appeared-before-the"),
            Self::GameCrashed => tr!("error-afk-journey-exited-unexpectedly"),
            Self::Message(message) => message,
        }
    }
}

impl fmt::Display for ActionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_text())
    }
}

impl std::error::Error for ActionError {}

impl From<String> for ActionError {
    fn from(message: String) -> Self {
        Self::Message(message)
    }
}

impl From<&str> for ActionError {
    fn from(message: &str) -> Self {
        Self::Message(message.to_owned())
    }
}

impl From<ActionError> for String {
    fn from(error: ActionError) -> Self {
        error.to_string()
    }
}

/// Lets tests and callers compare against the text the user would see.
impl PartialEq<&str> for ActionError {
    fn eq(&self, other: &&str) -> bool {
        self.as_text() == *other
    }
}

impl PartialEq<str> for ActionError {
    fn eq(&self, other: &str) -> bool {
        self.as_text() == other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_signals_are_types_not_texts() {
        let crashed: ActionError = "AFK Journey exited unexpectedly.".into();
        assert_ne!(crashed, ActionError::GameCrashed);
        assert_eq!(ActionError::GameCrashed, "AFK Journey exited unexpectedly.");
        assert_eq!(ActionError::Cancelled.to_string(), "Cancelled by user.");
        assert_eq!(String::from(ActionError::from("plain")), "plain");
    }

    #[test]
    fn only_adb_connection_messages_count_as_transport_errors() {
        assert!(ActionError::from("error: device offline").is_transport());
        assert!(!ActionError::from("Template not found").is_transport());
        assert!(!ActionError::GameCrashed.is_transport());
        assert!(!ActionError::Cancelled.is_transport());
    }
}
