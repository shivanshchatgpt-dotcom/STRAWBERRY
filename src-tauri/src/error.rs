use std::fmt::Display;

/// Convert any displayable error into a frontend-friendly `String`.
pub fn to_string_err<E: Display>(context: &str) -> impl Fn(E) -> String + '_ {
    move |e| format!("{context}: {e}")
}

/// User-facing error for invalid names.
pub const ERR_INVALID_NAME: &str = "Name cannot be empty and must be at most 200 characters.";

pub const ERR_EMPTY_TEXT: &str = "Chat text cannot be empty.";

pub const ERR_MISSING_ROOT: &str = "The selected index (root) no longer exists.";
pub const ERR_MISSING_FOLDER: &str = "The target folder no longer exists.";
pub const ERR_MISSING_CHAT: &str = "This chat no longer exists.";

pub const ERR_MOVE_INTO_OWN_DESCENDANT: &str =
    "Cannot move an item into itself or into one of its own subfolders.";

pub const ERR_DB_LOCK: &str = "The database is busy. Please try again.";

pub const ERR_JOIN: &str = "A background task failed unexpectedly.";

pub fn duplicate_name(kind: &str, name: &str) -> String {
    format!("A {kind} named \"{name}\" already exists here. Choose a different name.")
}
