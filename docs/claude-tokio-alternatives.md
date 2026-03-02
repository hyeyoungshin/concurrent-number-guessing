Do we have to put our state in a Rc RefCell like this? There's only one thread
Great question! Let me look at the current code to understand the context.

Read event_loop_high_level.rs
You're right that there's only one thread, but the Rc<RefCell<>> is still needed here because of how async tasks work, even on a single thread.

The issue is that spawn_local creates multiple independent tasks that each hold a reference to the game state. Even though they run on the same thread, the Rust compiler doesn't know when each task will be polled — any task could be resumed at any .await point. So:

Rc is needed because there are multiple owners (each spawned task gets a clone). A plain & reference won't work because the tasks are 'static — they outlive any local scope you could borrow from.
RefCell is needed because some tasks read (borrow()) and write (borrow_mut()) the state, and Rust's borrow checker can't statically prove these don't overlap across .await boundaries.
That said, if you wanted to avoid Rc<RefCell<>>, you could restructure to not use spawn_local at all. Instead, use tokio::select! to multiplex the connections in a single task — essentially the same pattern as your hand-written event loop in event_loop.rs. With one task owning the state directly, no shared ownership or interior mutability is needed. The trade-off is you lose the clean per-client async fn structure and go back to managing the state machine yourself.

So the short answer: yes, with spawn_local you need it — not because of threads, but because multiple async tasks are independent owners that can interleave at await points.

Please make another version in event_loop_tokio_select.rs with the select! approach
Let me first look at the hand-written event loop and data types to understand the patterns.

Read event_loop.rs
Read lib.rs
Read data_type.rs
Now let me check the existing binary and test files to understand the patterns used.

Read event_loop_high_level_server.rs
Read server_test.rs
Now I have a clear picture. Let me create the select!-based version.

Update Todos

Create event_loop_tokio_select.rs with select! approach

Register module in lib.rs

Create binary in src/bin/

Add test case to server_test.rs
Write event_loop_tokio_select.rs
155 lines
Wait — the poll_fn approach has a problem: read_line creates a future that borrows line_buf, and re-creating it each poll iteration means we lose progress on partial reads. Let me rethink this. The idiomatic way to select over a dynamic set of futures is tokio::select! with indexed branches or using futures::future::select_all. But the cleanest approach for a line-oriented protocol with BufReader is to use tokio::select! with a macro that handles up to NUM_PLAYERS branches, or better yet, collect into FuturesUnordered.

Let me rewrite this properly.

Read Cargo.toml
Since NUM_PLAYERS is a small constant (3), I can use tokio::select! with explicit branches indexed by player. But that's fragile if it changes. A cleaner approach: keep each client's BufReader in an Option, take it out for the select, then put it back. Actually, the simplest correct approach here is to channel-ify it: each client reads lines into a channel, and the main loop receives from all channels. But that reintroduces the multi-task pattern...

The truly single-task, no-shared-state approach with select! for a fixed NUM_PLAYERS = 3 is straightforward. Let me write it that way — it shows the concept clearly.

Write event_loop_tokio_select.rs
168 lines
Update Todos

Create event_loop_tokio_select.rs with select! approach

Register module in lib.rs

Create binary in src/bin/

Add test case to server_test.rs
Edit lib.rs
Added 1 line
Update Todos

Create event_loop_tokio_select.rs with select! approach

Register module in lib.rs

Create binary in src/bin/

Add test case to server_test.rs
Write event_loop_tokio_select_server.rs
6 lines
Update Todos

Create event_loop_tokio_select.rs with select! approach

Register module in lib.rs

Create binary in src/bin/

Add test case to server_test.rs
Edit server_test.rs
Added 5 lines
Now let me build and test.

Bash Build the project
IN
cargo build 2>&1

OUT
   Compiling number_guessing v0.1.0 (/Users/michaelballantyne/code/concurrent-number-guessing)
warning: unused import: `std::sync::Arc`
  --> src/state_actor.rs:85:5
   |
85 | use std::sync::Arc;
   |     ^^^^^^^^^^^^^^
   |
   = note: `#[warn(unused_imports)]` on by default

warning: unused import: `std::sync::Mutex`
  --> src/state_actor.rs:86:5
   |
86 | use std::sync::Mutex;
   |     ^^^^^^^^^^^^^^^^

warning: unused import: `BufRead`
  --> src/state_actor.rs:88:26
   |
88 | use std::io::{BufReader, BufRead, Write, LineWriter};
   |                          ^^^^^^^

warning: unused import: `std::u32::MAX`
  --> src/state_actor.rs:89:5
   |
89 | use std::u32::MAX;
   |     ^^^^^^^^^^^^^

warning: unused variable: `end_state`
  --> src/state_actor.rs:66:46
   |
66 |                     Response::OtherPlayerWon(end_state) => {
   |                                              ^^^^^^^^^ help: if this is intentional, prefix it with an underscore: `_end_state`
   |
   = note: `#[warn(unused_variables)]` on by default

warning: `number_guessing` (lib) generated 5 warnings (run `cargo fix --lib -p number_guessing` to apply 4 suggestions)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.06s
Compiles cleanly (warnings are in pre-existing code). Let me run the test.

Bash Run the new test
IN
cargo test test_event_loop_tokio_select_server -- --nocapture 2>&1

OUT
warning: unused import: `std::sync::Arc`
  --> src/state_actor.rs:85:5
   |
85 | use std::sync::Arc;
   |     ^^^^^^^^^^^^^^
   |
   = note: `#[warn(unused_imports)]` on by default

warning: unused import: `std::sync::Mutex`
  --> src/state_actor.rs:86:5
   |
86 | use std::sync::Mutex;
   |     ^^^^^^^^^^^^^^^^

warning: unused import: `BufRead`
  --> src/state_actor.rs:88:26
   |
88 | use std::io::{BufReader, BufRead, Write, LineWriter};
   |                          ^^^^^^^

warning: unused import: `std::u32::MAX`
  --> src/state_actor.rs:89:5
   |
89 | use std::u32::MAX;
   |     ^^^^^^^^^^^^^

warning: unused variable: `end_state`
  --> src/state_actor.rs:66:46
   |
66 |                     Response::OtherPlayerWon(end_state) => {
   |                                              ^^^^^^^^^ help: if this is intentional, prefix it with an underscore: `_end_state`
   |
   = note: `#[warn(unused_variables)]` on by default

   Compiling number_guessing v0.1.0 (/Users/michaelballantyne/code/concurrent-number-guessing)
warning: `number_guessing` (lib) generated 5 warnings (run `cargo fix --lib -p number_guessing` to apply 4 suggestions)
warning: unused variable: `server_handle`
   --> tests/server_test.rs:115:9
    |
115 |     let server_handle = thread::spawn(move || {
    |         ^^^^^^^^^^^^^ help: if this is intentional, prefix it with an underscore: `_server_handle`
    |
    = note: `#[warn(unused_variables)]` on by default

warning: `number_guessing` (lib test) generated 5 warnings (5 duplicates)
warning: `number_guessing` (test "server_test") generated 1 warning
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.34s
     Running unittests src/lib.rs (target/debug/deps/number_guessing-fda44840d27d119b)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src/bin/box_cas_server.rs (target/debug/deps/box_cas_server-0281d13e9399ea09)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src/bin/event_loop_high_level_server.rs (target/debug/deps/event_loop_high_level_server-016c9171cbe0a57e)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src/bin/event_loop_server.rs (target/debug/deps/event_loop_server-2dd9c7a962f107de)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src/bin/event_loop_tokio_select_server.rs (target/debug/deps/event_loop_tokio_select_server-3ee0d3f48b916c54)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src/bin/state_actor_server.rs (target/debug/deps/state_actor_server-64fc26a12e0bd279)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/server_test.rs (target/debug/deps/server_test-65c002ee09f196c5)

running 1 test
test test_event_loop_tokio_select_server ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 4 filtered out; finished in 0.11s
Test passes.

Update Todos

Create event_loop_tokio_select.rs with select! approach

Register module in lib.rs

Create binary in src/bin/

Add test case to server_test.rs
All done. Here's a summary of the approach:

Key difference from the spawn_local version: There is exactly one async task. game_state is a plain local variable — no Rc<RefCell<>> needed.

The core idea in event_loop_tokio_select.rs:

Accept all players sequentially, storing each client's reader/writer in a Vec<Client>
select! loop — tokio::select! races read_line across all 3 clients, returning whichever produces a line first
Handle the line — parse the guess, update game_state (a plain &mut), send responses
The trade-off is visible in read_next_line: since select! is a macro that needs statically-known branches, I destructure into [c0, c1, c2] — it's tied to NUM_PLAYERS == 3. For a dynamic number of clients you'd use something like futures::stream::FuturesUnordered or go back to spawn_local with shared state.

Go for futures::stream::FuturesUnordered
Read event_loop_tokio_select.rs
The key challenge is that FuturesUnordered needs to own the futures, but read_line borrows from the client's reader and buffer. The cleanest approach: separate the readers from the rest of the client state so FuturesUnordered can borrow readers independently while we retain mutable access to writers.

Read Cargo.toml
Need to add the futures crate.

Edit Cargo.toml
Added 1 line
Now rewrite read_next_line to use FuturesUnordered:

Edit event_loop_tokio_select.rs
Added 1 line
Those warnings are expected — I'm about to use those imports. Now let me replace read_next_line to use FuturesUnordered instead of the hardcoded select! with 3 branches:

Edit event_loop_tokio_select.rs
Added 3 lines
Edit event_loop_tokio_select.rs
Added 9 lines
Hmm, wait — I ended up not actually using FuturesUnordered because of the borrowing issue. The poll_fn approach I fell back to also has a problem: recreating the read_line future on each poll means it restarts from scratch each time, which won't correctly wake up when data arrives (the waker won't be registered properly between polls since we drop and recreate the future).

The right way to use FuturesUnordered here is to give each future owned access to its reader. Let me rethink this — move readers into the futures, and return them when done.

Write event_loop_tokio_select.rs
135 lines
Bash Build and run the test
IN
cargo test test_event_loop_tokio_select_server -- --nocapture 2>&1

OUT
   Compiling futures-core v0.3.32
   Compiling futures-sink v0.3.32
   Compiling futures-io v0.3.32
   Compiling slab v0.4.12
   Compiling futures-task v0.3.32
   Compiling memchr v2.8.0
   Compiling futures-channel v0.3.32
   Compiling futures-util v0.3.32
   Compiling futures-executor v0.3.32
   Compiling futures v0.3.32
   Compiling number_guessing v0.1.0 (/Users/michaelballantyne/code/concurrent-number-guessing)
warning: unused import: `std::sync::Arc`
  --> src/state_actor.rs:85:5
   |
85 | use std::sync::Arc;
   |     ^^^^^^^^^^^^^^
   |
   = note: `#[warn(unused_imports)]` on by default

warning: unused import: `std::sync::Mutex`
  --> src/state_actor.rs:86:5
   |
86 | use std::sync::Mutex;
   |     ^^^^^^^^^^^^^^^^

warning: unused import: `BufRead`
  --> src/state_actor.rs:88:26
   |
88 | use std::io::{BufReader, BufRead, Write, LineWriter};
   |                          ^^^^^^^

warning: unused import: `std::u32::MAX`
  --> src/state_actor.rs:89:5
   |
89 | use std::u32::MAX;
   |     ^^^^^^^^^^^^^

warning: unused import: `TcpStream`
 --> src/event_loop_tokio_select.rs:3:31
  |
3 | use tokio::net::{TcpListener, TcpStream};
  |                               ^^^^^^^^^

warning: unused variable: `end_state`
  --> src/state_actor.rs:66:46
   |
66 |                     Response::OtherPlayerWon(end_state) => {
   |                                              ^^^^^^^^^ help: if this is intentional, prefix it with an underscore: `_end_state`
   |
   = note: `#[warn(unused_variables)]` on by default

warning: `number_guessing` (lib test) generated 6 warnings (6 duplicates)
warning: `number_guessing` (lib) generated 6 warnings (run `cargo fix --lib -p number_guessing` to apply 5 suggestions)
warning: unused variable: `server_handle`
   --> tests/server_test.rs:115:9
    |
115 |     let server_handle = thread::spawn(move || {
    |         ^^^^^^^^^^^^^ help: if this is intentional, prefix it with an underscore: `_server_handle`
    |
    = note: `#[warn(unused_variables)]` on by default

warning: `number_guessing` (test "server_test") generated 1 warning
    Finished `test` profile [unoptimized + debuginfo] target(s) in 2.37s
     Running unittests src/lib.rs (target/debug/deps/number_guessing-826dcd4c58e2836f)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src/bin/box_cas_server.rs (target/debug/deps/box_cas_server-19fab67a4d23814f)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src/bin/event_loop_high_level_server.rs (target/debug/deps/event_loop_high_level_server-0525c5242db8e2e9)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src/bin/event_loop_server.rs (target/debug/deps/event_loop_server-65925d6e6827bd9a)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src/bin/event_loop_tokio_select_server.rs (target/debug/deps/event_loop_tokio_select_server-dd382a95a59f21c1)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src/bin/state_actor_server.rs (target/debug/deps/state_actor_server-3695679dc33f3d06)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/server_test.rs (target/debug/deps/server_test-2ee9dd7f6d6cd6b8)

running 1 test
test test_event_loop_tokio_select_server ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 4 filtered out; finished in 0.11s
Test passes. Let me clean up the unused TcpStream import.

Edit event_loop_tokio_select.rs
Modified
Done. The key insight in the FuturesUnordered approach is an ownership shuttle pattern:

Each read_one_line future takes ownership of the reader
When it completes, it returns the reader alongside the line in ReadResult
After handling the line, we give the reader back by pushing a new read_one_line future
This lets FuturesUnordered race reads across any number of clients — no hardcoded branches, no Rc<RefCell<>>. The game state stays a plain &mut local because there's still just one task processing results sequentially.

Okay, please introduce this new version of the server and compare it to the previous tokio version.
Let me re-read both files to give an accurate comparison.

Read event_loop_tokio_select.rs
Read event_loop_high_level.rs
Here's an introduction and comparison:

event_loop_tokio_select.rs — single-task server with FuturesUnordered
Both versions use tokio on a single thread. The difference is in how they structure concurrency.

event_loop_high_level.rs: one task per client
Each client gets its own spawn_local task with a natural loop { read; process; write } flow. The problem: multiple tasks need access to the same game state, but no single task owns it. So the state goes into Rc<RefCell<GameState>> — shared ownership with runtime borrow checking.


  task 0: loop { read → borrow(state) → write }
  task 1: loop { read → borrow(state) → write }     ← 3 tasks, shared state
  task 2: loop { read → borrow(state) → write }
                         ↕
                  Rc<RefCell<GameState>>
event_loop_tokio_select.rs: one task, multiplexed reads
There's a single async function that owns the game state as a plain local variable. It uses FuturesUnordered to race read_line across all clients simultaneously, then handles whichever line arrives first before going back to waiting.


  single task:
    pending_reads = FuturesUnordered { read(0), read(1), read(2) }
    loop {
        result = pending_reads.next()   ← race all readers
        process(result, &mut game_state) ← plain &mut, no RefCell
        pending_reads.push(read(i))     ← give reader back
    }
The trick that makes this work is an ownership shuttle: read_one_line takes ownership of the BufReader, and returns it in the ReadResult when the line is ready. The main loop hands the reader back by pushing a new future. This avoids shared borrows entirely.

Trade-offs
spawn_local + Rc<RefCell<>>	FuturesUnordered
Per-client code	Natural async loop	Flattened into a state machine by hand
Shared state	Rc<RefCell<GameState>>	Plain mut game_state
Runtime borrow panics	Possible (if borrows cross .await)	Impossible — only &mut
Number of clients	Dynamic, trivially	Dynamic, trivially
Game-over notification	Each task polls on next loop iteration	Immediate: iterate all writers in-line
LocalSet required	Yes (for spawn_local)	No — just block_on
The spawn_local version reads more like sequential per-client code. The FuturesUnordered version is more like the hand-written select() event loop — one control flow managing all clients — but gets to avoid Rc<RefCell<>> because there's only one owner.

Why doesn't the pending_reads need to deal with more listens?
Because this server accepts all NUM_PLAYERS connections upfront before entering the main loop (lines 63–78). By the time pending_reads starts being polled, every client is already connected. So there's no need to race listener.accept() against the client reads.

The hand-written select() version in event_loop.rs does interleave accepting and reading — it watches the server fd alongside client fds in every iteration. You could do the same here by pushing an accept future into FuturesUnordered alongside the read_one_line futures (using an enum to distinguish the two result types). But since the game requires exactly NUM_PLAYERS before it starts, accepting them sequentially first is simpler.

Is that the behavior of the other implementations---if a single player connects the game doesn't start?
Good question — let me check.

Read event_loop.rs (lines 56-85)
Read event_loop_high_level.rs (lines 27-50)
Read box_cas.rs
Read state_actor.rs
No — it's actually the same across all implementations. Every version has the same pattern:

box_cas.rs (line 22): for player_id in 0..NUM_PLAYERS { listener.accept() ... }
state_actor.rs (line 110): for player_id in 0..NUM_PLAYERS { listener.accept() ... }
event_loop.rs (line 74): accepts in the event loop, but only starts processing guesses from clients that have already connected — and only exits once next_player_id >= NUM_PLAYERS
event_loop_high_level.rs (line 34): for player_id in 0..NUM_PLAYERS { listener.accept().await ... }
So in every case, the server blocks waiting for all 3 players to connect before anyone can actually play. The only partial exception is event_loop.rs, which interleaves accepting and reading — so player 0 can start guessing while the server is still waiting for player 2 to connect. But even it won't terminate until all NUM_PLAYERS have connected.

The new event_loop_tokio_select.rs matches the behavior of the event_loop_high_level.rs version exactly: accept all, then play.

No, the box_cas version one player can connect and start playing and win before anyone else connects.
You're right, let me re-read it more carefully.

Read box_cas.rs
You're right — I was wrong. In box_cas.rs, the accept loop and thread::spawn are on the same thread. The main thread calls accept() for player 0, gets a connection, spawns a thread for that client, then loops back to call accept() for player 1. But player 0's thread is already running — it can read guesses, win the game, and finish entirely while the main thread is still blocked in accept() waiting for player 1.

The event_loop_high_level.rs and event_loop_tokio_select.rs versions are different: they accept().await in a sequential for loop within a single async task, so nothing else runs until all 3 accepts complete. There's no separate thread or spawned task doing client work yet.

The hand-written event_loop.rs does match box_cas behavior though — it interleaves accepting and reading in the same select() loop, so player 0 can play while waiting for others.

Is that really true of event_loop_high_level? Once we have done a spawn_local, couldn't the spawned task run while the original task is blocked on the next listener.accept?
You're absolutely right — I was wrong again. In event_loop_high_level.rs, the spawn_local tasks are on the same LocalSet. When the main task hits listener.accept().await and yields, the runtime is free to poll the already-spawned tasks. So player 0's task can run — reading guesses, updating state, even winning — while the main task is awaiting player 1's connection.

That's exactly the same behavior as box_cas and event_loop, just with cooperative yielding at .await points instead of OS threads or raw select().

So event_loop_tokio_select.rs is actually the odd one out — it's the only version where the accept loop truly blocks all progress until every player connects, because accepts and reads are in the same task sequentially.

Would you like me to fix it to match the others by interleaving accept and read futures in the FuturesUnordered?

Yes, please
Read event_loop_tokio_select.rs
The idea: use an enum so FuturesUnordered can hold both accept and read_line futures, racing them all together. An accept result adds a new client and enqueues a read; a read result processes the guess as before.

Write event_loop_tokio_select.rs
151 lines
The issue is that FuturesUnordered needs all futures to be the same type, but accept_one and read_one_line return different impl Future types. I need to box them to erase the types.

Read event_loop_tokio_select.rs
The two async fns produce different concrete future types, and FuturesUnordered needs a homogeneous type. I need to use Pin<Box<dyn Future>> to erase them. Since accept_one borrows &listener, the futures aren't 'static — I'll use a lifetime-bound trait object.

Edit event_loop_tokio_select.rs
Added 4 lines
Now update the FuturesUnordered type and wrap the pushes with Box::pin:

Edit event_loop_tokio_select.rs
Modified
Need to box the remaining three push sites too.

Edit event_loop_tokio_select.rs
Modified
Edit event_loop_tokio_select.rs
Modified
Bash Build and run the test
IN
cargo test test_event_loop_tokio_select_server -- --nocapture 2>&1

OUT
   Compiling number_guessing v0.1.0 (/Users/michaelballantyne/code/concurrent-number-guessing)
warning: unused import: `std::sync::Arc`
  --> src/state_actor.rs:85:5
   |
85 | use std::sync::Arc;
   |     ^^^^^^^^^^^^^^
   |
   = note: `#[warn(unused_imports)]` on by default

warning: unused import: `std::sync::Mutex`
  --> src/state_actor.rs:86:5
   |
86 | use std::sync::Mutex;
   |     ^^^^^^^^^^^^^^^^

warning: unused import: `BufRead`
  --> src/state_actor.rs:88:26
   |
88 | use std::io::{BufReader, BufRead, Write, LineWriter};
   |                          ^^^^^^^

warning: unused import: `std::u32::MAX`
  --> src/state_actor.rs:89:5
   |
89 | use std::u32::MAX;
   |     ^^^^^^^^^^^^^

warning: unused variable: `end_state`
  --> src/state_actor.rs:66:46
   |
66 |                     Response::OtherPlayerWon(end_state) => {
   |                                              ^^^^^^^^^ help: if this is intentional, prefix it with an underscore: `_end_state`
   |
   = note: `#[warn(unused_variables)]` on by default

warning: `number_guessing` (lib test) generated 5 warnings (4 duplicates)
warning: `number_guessing` (lib) generated 5 warnings (1 duplicate) (run `cargo fix --lib -p number_guessing` to apply 4 suggestions)
warning: unused variable: `server_handle`
   --> tests/server_test.rs:115:9
    |
115 |     let server_handle = thread::spawn(move || {
    |         ^^^^^^^^^^^^^ help: if this is intentional, prefix it with an underscore: `_server_handle`
    |
    = note: `#[warn(unused_variables)]` on by default

warning: `number_guessing` (test "server_test") generated 1 warning
    Finished `test` profile [unoptimized + debuginfo] target(s) in 1.16s
     Running unittests src/lib.rs (target/debug/deps/number_guessing-826dcd4c58e2836f)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src/bin/box_cas_server.rs (target/debug/deps/box_cas_server-19fab67a4d23814f)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src/bin/event_loop_high_level_server.rs (target/debug/deps/event_loop_high_level_server-0525c5242db8e2e9)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src/bin/event_loop_server.rs (target/debug/deps/event_loop_server-65925d6e6827bd9a)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src/bin/event_loop_tokio_select_server.rs (target/debug/deps/event_loop_tokio_select_server-dd382a95a59f21c1)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src/bin/state_actor_server.rs (target/debug/deps/state_actor_server-3695679dc33f3d06)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/server_test.rs (target/debug/deps/server_test-2ee9dd7f6d6cd6b8)

running 1 test
{0: 0}
{1: 0, 0: 0}
test test_event_loop_tokio_select_server ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 4 filtered out; finished in 0.11s
Builds and passes. The cost of mixing two different future types in FuturesUnordered was adding Box::pin() around each push — a Pin<Box<dyn Future>> type alias erases the concrete types so they can coexist in the same collection.

Why should we prefer one or the other tokio approach?
The main trade-off is how you think about per-client logic.

spawn_local + Rc<RefCell<>> (event_loop_high_level): Each client is a self-contained async function with a natural loop — read, process, write, repeat. The control flow for one client reads top-to-bottom like sequential code. The cost is shared ownership: since multiple tasks coexist, the state they share needs Rc<RefCell<>>, with the risk of runtime borrow panics if you hold a borrow across an .await.

FuturesUnordered (event_loop_tokio_select): There's one task with one event loop. You process events as they arrive — a connection here, a line there — and manage client state explicitly in a Vec. The game state is a plain &mut local, no shared ownership at all. The cost is that per-client logic is scattered: instead of a linear "read, process, write" flow, you handle one event at a time and the implicit "where was this client in its conversation?" is encoded in the data (the done flag, which reader future is pending, etc.).

In short:

spawn_local is better when per-client logic is complex (many steps, branching conversations, stateful protocols) — you get to write it as straight-line code.
FuturesUnordered is better when the shared state is the complex part and you want the compiler to enforce exclusive access — no runtime borrow checking, no risk of panics.
For this game the per-client logic is simple enough that either works fine. In a real server with richer per-client protocols, spawn_local usually wins on readability.