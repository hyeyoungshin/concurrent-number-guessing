# Concurrent Number Guessing

A multi-player TCP number-guessing game used as a vehicle for exploring different concurrency models in Rust. Three players connect and take turns guessing a secret number (0–19). The first to guess correctly wins; the others are notified immediately.

The same game logic ([src/data_type.rs](src/data_type.rs)) is reused across every implementation. What changes is only *how concurrent access to that shared state is managed*.

## Running a server

Each approach is its own binary:

```
cargo run --bin box_cas_server
cargo run --bin state_actor_server
cargo run --bin event_loop_server           # raw select()
cargo run --bin event_loop_futures_unordered
cargo run --bin event_loop_spawn_local
```

Connect three clients with `nc 127.0.0.1 7878`.

---

## Approach 1 — Shared State with `Arc<Mutex<T>>`

**Source:** [src/box_cas.rs](src/box_cas.rs)

### Background

The classic multi-threaded model: spawn one OS thread per client and protect the shared value behind a mutex. `Arc` (atomically reference-counted pointer) lets multiple threads hold an owner handle to the same allocation. `Mutex` ensures only one thread can read or write the inner value at a time. When a thread is done updating it drops the guard, releasing the lock for the next waiter.

The name `box_cas` comes from the Racket version of this project, where the equivalent pattern is a mutable `box` updated with `box-cas!` (compare-and-swap). In Rust the `Mutex` serves the same role: one canonical location that all threads contend on.

### Code example

```rust
// Shared across all player threads
let shared_game_state = Arc::new(Mutex::new(initial_state));

// Each player gets a clone of the Arc (not a clone of the data)
let shared_game_state = Arc::clone(&shared_game_state);
thread::spawn(move || {
    handle_client(reader, writer, player_id, shared_game_state)
});
```

Inside `handle_client`, reading and writing always goes through the lock:

```rust
// Read — lock, clone the data, then immediately release
let current_st = {
    let state = shared_game_state.lock().unwrap();
    state.clone()
}; // lock released here

// Write — lock, check, update, release
fn try_and_commit_action(game_state: &Arc<Mutex<GameState>>, action: &Action) -> bool {
    let mut current_state = game_state.lock().unwrap();
    if game_over(&current_state) {
        false
    } else {
        *current_state = do_action(&*current_state, action);
        true
    }
}
```

**Trade-offs:** Simple mental model. The risk is lock contention — every thread blocks while any other thread holds the mutex — and potential deadlocks if locking order is not consistent.

---

## Approach 2 — Message Passing (State Actor)

**Source:** [src/state_actor.rs](src/state_actor.rs)

### Background

Instead of letting threads touch shared memory directly, *only one thread* ever owns the game state: the **state actor**. All other threads (one per player) communicate with it by sending messages over `mpsc` channels and waiting for a reply.

This is the actor model: a concurrent entity that processes one message at a time, with no locks on the state itself because no one else can touch it. The pattern of attaching a reply channel (`reply_to: Sender<Response>`) to each request makes the async-style request/response cycle synchronous from the caller's perspective.

### Code example

The state actor loop — it owns `state` exclusively, no mutex needed:

```rust
thread::spawn(move || {
    let mut state = initial_state;
    let mut last_displayed = HashMap::new();
    for request in state_rx {                          // blocks until a message arrives
        let response = handle_request(&request, &mut state, &mut last_displayed);
        request.reply_to.send(response).unwrap();      // send reply back to caller
    }
});
```

A player thread sending a synchronous request-response:

```rust
fn sync_message(state_update_channel: &Sender<Request>, msg: Msg) -> Response {
    let (resp_tx, resp_rx) = mpsc::channel();          // one-shot reply channel
    state_update_channel.send(Request { msg, reply_to: resp_tx }).unwrap();
    resp_rx.recv().unwrap()                            // block until actor replies
}
```

**Trade-offs:** State logic is entirely isolated and easy to reason about — no data races are even possible. The cost is latency: every state interaction requires a round-trip through the channel, and the actor becomes a sequential bottleneck under high load.

---

## Approach 3 — Single-Threaded Event Loop

Rather than using multiple OS threads, a single thread handles all clients by interleaving their I/O. This is the foundation of how frameworks like Node.js and many network servers work. This project implements it at three levels of abstraction.

### 3a — Raw `select()` syscall

**Source:** [src/event_loop.rs](src/event_loop.rs)

#### Background

The lowest-level form: ask the OS directly which file descriptors are ready to read, handle them one by one, then ask again. `select()` is a POSIX syscall that takes a set of fds and blocks until at least one has data available. Because only one fd is processed at a time and the thread never blocks waiting on a single client, all clients make progress concurrently — with zero threads.

#### Code example

```rust
loop {
    // Phase 1: tell the OS which fds to watch
    FD_ZERO(&mut read_set);
    FD_SET(server_fd, &mut read_set);
    for (&fd, client) in &clients {
        if !client.done { FD_SET(fd, &mut read_set); }
    }

    // Phase 2: block until something is ready
    select(highest_fd + 1, &mut read_set, null_mut(), null_mut(), null_mut());

    // Phase 3: handle whichever fds are now marked ready
    if FD_ISSET(server_fd, &read_set) { /* accept new client */ }
    for fd in client_fds {
        if FD_ISSET(fd, &read_set) { /* read and process one chunk */ }
    }
}
```

**Trade-offs:** Maximum control and portability. Painful to write: you manage buffers, partial reads, and fd sets by hand. `select()` also has a hard limit on the number of fds it can watch (typically 1024).

---

### 3b — Async event loop with `FuturesUnordered`

**Source:** [src/event_loop_futures_unordered.rs](src/event_loop_futures_unordered.rs)

#### Background

`FuturesUnordered` is the async equivalent of the raw `select()` loop. Instead of registering file descriptors with the OS manually, you push `Future`s into the collection and `await` whichever one completes next. The Tokio single-threaded runtime handles the polling. The server loop stays in one place — there is one central `while let Some(event) = pending.next().await` — making the control flow explicit.

#### Code example

```rust
// Each pending I/O operation is a future in the pool
let mut pending: FuturesUnordered<EventFuture<'_>> = FuturesUnordered::new();
pending.push(Box::pin(accept_one(&listener)));   // start waiting for first client

while let Some(event) = pending.next().await {   // run whichever future finishes first
    match event {
        Event::NewClient(stream) => {
            // spawn a read future for this client
            pending.push(Box::pin(read_one_line(index, reader)));
            // keep accepting more clients
            if next_player_id < NUM_PLAYERS {
                pending.push(Box::pin(accept_one(&listener)));
            }
        }
        Event::Line(result) => {
            // process the guess, push another read future if the game continues
            pending.push(Box::pin(read_one_line(result.index, result.reader)));
        }
    }
}
```

**Trade-offs:** All state lives in local variables — no `Arc`, no `Mutex`, no locking. The explicit event loop structure keeps control flow easy to trace. Ownership of each reader is passed *into* the future and returned through the event, which is why `ReadResult` carries the `reader` back out.

---

### 3c — Async event loop with `spawn_local`

**Source:** [src/event_loop_spawn_local.rs](src/event_loop_spawn_local.rs)

#### Background

`spawn_local` lets you write each client handler as an independent `async` task — structurally identical to the multi-threaded version — while still running everything on a single thread via Tokio's `LocalSet`. Because there is only one thread, you can use `Rc<RefCell<T>>` instead of `Arc<Mutex<T>>`: no atomic operations, no locking, the borrow checker enforces the single-owner invariant at compile time.

#### Code example

```rust
// Rc<RefCell<>> — safe because there is only one thread
let shared_game_state = Rc::new(RefCell::new(initial_state));

for player_id in 0..NUM_PLAYERS {
    let (stream, _) = listener.accept().await.unwrap();
    let shared_game_state = Rc::clone(&shared_game_state);

    // spawn_local: runs on this thread's event loop, not a new thread
    tokio::task::spawn_local(async move {
        handle_client(stream, player_id, shared_game_state).await;
    });
}
```

Inside the handler, borrowing is just a `borrow()` / `borrow_mut()` call — no `.lock().unwrap()`:

```rust
// Read — just borrow, no blocking
let current_st = shared_game_state.borrow().clone();

// Write — mutable borrow, released at end of block
let committed = {
    let mut state = shared_game_state.borrow_mut();
    if game_over(&state) { false } else { *state = do_action(&state, &action); true }
};
```

**Trade-offs:** Closest in structure to the multi-threaded `Arc<Mutex<>>` version, making the two easy to compare directly. Tasks *look* concurrent but never actually run simultaneously — they interleave at `await` points — so `borrow_mut()` never panics in practice. The code is simpler and faster than the mutex version for single-machine workloads.

---

## Summary

| Approach | Threads | Shared state | Blocking? |
|---|---|---|---|
| `Arc<Mutex<T>>` | one per client | mutex-guarded | yes — threads block on lock |
| State actor | one per client + one actor | channel messages | yes — threads block on reply |
| Raw `select()` | 1 | local variables | no — OS multiplexes fds |
| `FuturesUnordered` | 1 | local variables | no — runtime multiplexes futures |
| `spawn_local` | 1 | `Rc<RefCell<T>>` | no — tasks interleave at `.await` |
