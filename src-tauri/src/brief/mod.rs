//! Brief generation for the Tauri app.
//!
//! The engine lives in the `strawberry-core` crate so the app and the
//! clipboard capture daemon produce byte-identical output from the same rules.
//! This module is the app-facing shim: it re-exports the engine and adds the
//! conversions to the app's own DB row types.

pub use strawberry_core::brief::{generate, Stats};

use crate::db::models::ChatStats;

impl From<Stats> for ChatStats {
    fn from(s: Stats) -> Self {
        Self {
            char_count: s.char_count,
            word_count: s.word_count,
            code_block_count: s.code_block_count,
            error_count: s.error_count,
            command_count: s.command_count,
            url_count: s.url_count,
        }
    }
}

impl From<ChatStats> for Stats {
    fn from(s: ChatStats) -> Self {
        Self {
            char_count: s.char_count,
            word_count: s.word_count,
            code_block_count: s.code_block_count,
            error_count: s.error_count,
            command_count: s.command_count,
            url_count: s.url_count,
        }
    }
}
