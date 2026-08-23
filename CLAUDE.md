# Claude's Notes for waveform-mcp

## Project Overview

An MCP (Model Context Protocol) server for reading and analyzing VCD/FST waveform files using the wellen library.

## Architecture

- **main.rs**: MCP server handler with tool routing, supports stdio and HTTP modes
- **lib.rs**: Library exports from submodules
- **condition.rs/condition.lalrpop**: Parser for conditional expressions (LALRPOP grammar)
- **formatting.rs**: Time and signal value formatting utilities
- **hierarchy.rs**: Signal/scope lookup by hierarchical path
- **signal.rs**: Signal reading, event finding, and metadata
- **capture.rs**: Logic-analyzer capture as a data model, serialized to VCD
- **ad3.rs**: WaveForms SDK FFI for live capture (behind the `ad3` feature)

## MCP Tools (8 total, plus 2 behind the `ad3` feature)

1. `open_waveform` - Open VCD/FST files, assign optional alias
2. `close_waveform` - Close waveform and free memory
3. `list_signals` - List signals with name_pattern, hierarchy_prefix filters
4. `read_hierarchy` - Read hierarchy tree from an optional scope
5. `read_signal` - Read values at time_index or time_indices
6. `get_signal_info` - Get signal type, width, index range
7. `find_signal_events` - Find all changes in time range
8. `find_conditional_events` - Complex condition search with expression parser

With `--features ad3`:

9. `list_devices` - Enumerate connected Digilent devices
10. `capture_logic` - Capture from the logic analyzer and open it as a waveform

## Key Dependencies

- `wellen` - Wavefile parsing
- `rmcp` - MCP server framework
- `tokio` - Async runtime
- `lalrpop` - Condition parser grammar
- `axum` - HTTP server

## Transport Modes

- **Stdio** (default): `cargo run --release`
- **HTTP**: `cargo run --release -- --http --bind-address 127.0.0.1:8000`

## Condition Expression Syntax

Supported in `find_conditional_events`:
- Signal paths: `TOP.signal`
- Bitwise: `~`, `&`, `|`, `^`
- Boolean: `&&`, `||`, `!`
- Comparison: `==`, `!=`
- Special: `$past(signal)` for previous value
- Literals: `4'b0101`, `3'd2`, `5'h1A`
- Bit extract: `signal[bit]` or `signal[msb:lsb]`

Operator precedence (high to low): `~`/`!`, `==`/`!=`, `&`, `^`, `|`, `&&`, `||`

## CLI Tool

A standalone CLI binary `waveform-cli` is available for direct command-line access:

```bash
# Usage: waveform-cli <command> [args...] [-- <command2> ...]
waveform-cli open_waveform test.vcd
waveform-cli open_waveform test.vcd -- list_signals test.vcd --pattern clk

# Chain multiple commands with -- separator
waveform-cli open_waveform test.vcd -- read_signal test.vcd top.clk --time-index 0
```

**CLI Commands:**
- `open_waveform <path> [--alias <name>]` - Open waveform file
- `close_waveform <id>` - Close waveform
- `list_signals <id> [--pattern <p>] [--hierarchy <h>] [--recursive <bool>] [--limit <n>]`
- `read_hierarchy <id> [--scope <scope>] [--recursive <bool>] [--limit <n>]`
- `read_signal <id> <signal> [--time-index <idx> | --time-indices <list>]`
- `get_signal_info <id> <signal>`
- `find_signal_events <id> <signal> [--start <idx>] [--end <idx>] [--limit <n>]`
- `find_conditional_events <id> <condition> [--start <idx>] [--end <idx>] [--limit <n>]`

## Build & Test

```bash
cargo build --release
cargo test

# Live capture from a Digilent device; needs the WaveForms SDK installed
cargo build --release --features ad3
```

The `ad3` feature links `libdwf`. Capture tools live in their own
`#[tool_router]` block because the macro registers every `#[tool]` in its block
regardless of `#[cfg]`.

**Binaries:**
- `target/release/waveform-mcp` - MCP server (stdio/HTTP modes)
- `target/release/waveform-cli` - Standalone CLI tool

## Notes for Development

- Waveform store uses `Arc<RwLock<HashMap<>>>` for shared state
- Signal data must be loaded with `waveform.load_signals()` before reading
- Time values formatted with actual timescale (ns, ps, etc.)
- Condition parser grammar in `src/condition/condition.lalrpop`
