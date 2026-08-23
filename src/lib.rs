//! Waveform MCP Server Library
//!
//! This library provides utilities for working with waveform files.

#[cfg(feature = "ad3")]
pub mod ad3;
pub mod capture;
pub mod cli_parser;
pub mod condition;
pub mod formatting;
pub mod hierarchy;
pub mod signal;

// Re-export public functions
pub use capture::LogicCapture;
pub use cli_parser::{Command, parse_args};
pub use condition::find_conditional_events;
pub use formatting::{format_signal_value, format_time};
pub use hierarchy::find_scope_by_path;
pub use hierarchy::find_signal_by_path;
pub use hierarchy::read_hierarchy;
pub use signal::find_signal_events;
pub use signal::get_signal_metadata;
pub use signal::list_signals;
pub use signal::read_signal_values;
