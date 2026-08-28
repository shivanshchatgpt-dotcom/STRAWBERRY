//! 🍓 STRAWBERRY core — deterministic, offline chat compression.
//!
//! Shared by the Tauri app and the clipboard capture daemon so both produce
//! byte-identical output. Every function here is a pure function of its
//! inputs: no AI, no network, no clocks, no randomness.
//!
//! - [`extractor`] pulls typed artifacts out of raw chat text, including
//!   rejected approaches (negative knowledge) and verbatim identifiers.
//! - [`brief`] renders the human-facing markdown brief.
//! - [`handoff`] renders the AI-to-AI packet: eight priority slots filled
//!   greedily against a token budget.

pub mod ast;
pub mod brief;
pub mod extractor;
pub mod handoff;
pub mod ocr;
pub mod rules;

pub use ast::{analyze_source, SymbolKind, SymbolItem, SymbolicAnalysis};
pub use ocr::{is_diagram_format, ocr_image_rgba, preserve_diagram, OcrResult};
pub use brief::{generate, GeneratedBrief, Stats};
pub use extractor::{extract, CodeBlock, Extraction, Identifier, RejectedOption};
pub use handoff::{
    build as build_handoff, build_from_raw as build_handoff_from_raw, render as render_handoff,
    to_json as handoff_to_json, HandoffPacket, Slot, DEFAULT_TOKEN_BUDGET,
};
