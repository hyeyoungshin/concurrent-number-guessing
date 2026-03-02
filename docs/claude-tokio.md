# Conversation: Building a Tokio-based Number Guessing Server

## Context

This project implements a concurrent number guessing game server in multiple ways,
each demonstrating a different concurrency model. A Claude Code session helped
build out the project infrastructure and the tokio-based implementation.

## Session Summary

### 1. Runnable binary entry points

**Problem:** The server modules (`box_cas.rs`, `state_actor.rs`) were only accessible
through `main.rs`. Wanted clickable "Run" buttons in VS Code for each server.

**Solution:**
- Created `src/lib.rs` to export all modules as `pub mod`
- Created `src/bin/box_cas_server.rs`, `src/bin/state_actor_server.rs` — each with
  a `fn main()` that calls the respective `server()` function
- Updated `main.rs` to use `use number_guessing::state_actor` instead of `mod` declarations
- Cargo auto-discovers files in `src/bin/` as binary targets; rust-analyzer shows
  a clickable Run button above each `fn main()`

### 2. Refactored `box_cas.rs` to use shared `data_type.rs`

`box_cas.rs` had duplicated copies of `start_game`, `do_action`, `state_view`,
`get_valid_input`, `game_over`, and `parse_number_input`. Removed all duplicates
so it only contains the server/threading code and gets everything else from
`use crate::data_type::*`.

### 3. Integration test harness

**Problem:** No tests existed. Needed an automated test that works for any server
implementation.

**Approach:**
- Made servers configurable: added `start_game_with_secret(secret)` to `data_type.rs`,
  extracted `server_with_config(addr, initial_state)` in each server module
- Test picks a free port (bind to port 0), spawns the server in a background thread,
  connects 3 TCP clients
- Player 0 uses binary search strategy (always wins), players 1 and 2 always guess 0
- Asserts: correct greetings, player 0 wins, others see loss message

**File:** `tests/server_test.rs`

### 4. Completed `event_loop.rs` (raw `select()` implementation)

Finished the hand-written single-threaded event loop server using `libc::select()`.

Key design decisions:
- Per-client state machine with `buf: String` for accumulating partial reads into lines
- Game state is a local variable (no mutex needed — single thread)
- `fd_write`/`fd_writeln` helpers for raw `libc::write`
- Custom `parse_guess` instead of blocking `get_valid_input`
- Uses `shutdown(SHUT_WR)` before `close()` to ensure game-over messages are
  delivered to losing clients (avoids RST from unread data in receive buffer)

### 5. Tokio-based high-level event loop (`event_loop_high_level.rs`)

**Question:** What is the standard Rust approach for async I/O servers?

**Answer:** **tokio** — an async runtime that provides the event loop (`epoll`/`kqueue`
under the hood) with high-level `async`/`await` syntax.

**Initial implementation** used multi-threaded runtime (`Runtime::new()`), then
switched to single-threaded to match the event loop concept:

```rust
let rt = tokio::runtime::Builder::new_current_thread()
    .enable_all()
    .build()
    .unwrap();
let local = LocalSet::new();
local.block_on(&rt, async_server(addr, initial_state));
```

**Key design choices for single-threaded:**
- `new_current_thread()` — one OS thread, one event loop
- `LocalSet` + `spawn_local` — tasks stay on the current thread (no `Send` requirement)
- `Rc<RefCell<>>` instead of `Arc<Mutex<>>` — no thread-safety overhead

**Comparison of all implementations:**

| Concept | `box_cas.rs` (threads) | `event_loop.rs` (manual) | `event_loop_high_level.rs` (tokio) |
|---|---|---|---|
| Concurrency | `thread::spawn` | `select()` loop | `tokio::task::spawn_local` |
| I/O | `BufReader::read_line` (blocking) | `libc::read` + manual line buffer | `AsyncBufReadExt::read_line` (non-blocking) |
| Shared state | `Arc<Mutex<>>` | local variable | `Rc<RefCell<>>` |
| Event loop | OS thread scheduler | hand-written `select()` | tokio runtime (hidden) |
| Lines of logic | ~50 | ~150 | ~40 |

The tokio version reads almost identically to the threaded `box_cas.rs` version,
but runs on a single-threaded async event loop under the hood.

### 6. Does tokio use multiple threads?

By default with `Runtime::new()` (multi-thread feature), yes — it runs a thread pool.
With `new_current_thread()`, everything runs on a single OS thread with `kqueue`/`epoll`
multiplexing the I/O — conceptually the same as the hand-written `select()` loop.

## Files created/modified

- `src/lib.rs` — module exports
- `src/bin/box_cas_server.rs` — binary entry point
- `src/bin/state_actor_server.rs` — binary entry point
- `src/bin/event_loop_server.rs` — binary entry point
- `src/bin/event_loop_high_level_server.rs` — binary entry point
- `src/data_type.rs` — added `start_game_with_secret()`
- `src/box_cas.rs` — removed duplicated functions, added `server_with_config()`
- `src/state_actor.rs` — added `server_with_config()`
- `src/event_loop.rs` — completed implementation
- `src/event_loop_high_level.rs` — new tokio-based implementation
- `tests/server_test.rs` — integration tests for all 4 server implementations
- `Cargo.toml` — added tokio dependency
- `.gitignore` — added `/target`
