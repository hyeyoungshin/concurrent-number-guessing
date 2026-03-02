Test Harness for Number Guessing Servers
Context
The project has two concurrent server implementations (box_cas, state_actor) with identical TCP protocols but no tests. We need an integration test that verifies the game works end-to-end: 3 clients connect, make guesses, one wins, others see loss messages.

Step 1: Make server functions configurable
src/data_type.rs — Add a start_game_with_secret(secret: u32) function alongside the existing start_game():


pub fn start_game_with_secret(secret: u32) -> GameState {
    GameState::InProgress(secret, HashMap::new())
}
Keep start_game() unchanged (still uses random).

src/box_cas.rs — Change server() to accept port and initial state:


pub fn server() { server_with_config("127.0.0.1:7878", start_game()); }
pub fn server_with_config(addr: &str, initial_state: GameState) { ... }
src/state_actor.rs — Same pattern:


pub fn server() { server_with_config("127.0.0.1:7878", start_game()); }
pub fn server_with_config(addr: &str, initial_state: GameState) { ... }
Step 2: Create integration test
tests/server_test.rs — One test file that tests both server implementations.

Structure:

A generic run_server_test(server_fn) function that:

Picks a free port (bind to port 0, get assigned port, close, pass to server)
Spawns the server in a background thread
Connects 3 TCP clients
Each client runs a binary search strategy to find the known secret number (e.g., 10)
Asserts:
Each client receives "You are player {0,1,2}"
First prompt is "Guess a number from 0 to 19."
Hint messages match ("Guess higher"/"Guess lower.")
Exactly one client gets "You won! Game over."
Other clients get "Player {winner} won."
Two test functions call run_server_test with box_cas::server_with_config and state_actor::server_with_config respectively.

Since we control the secret number (e.g., 10), we can have player 0 do a binary search that converges on 10, while players 1 and 2 guess values that never hit 10 (e.g., always guess 0). Player 0 should win deterministically.

Test strategy:

Player 0: binary search → will find 10 in ~4 guesses
Players 1 and 2: always guess 0 (never wins since secret is 10)
This makes the outcome deterministic: player 0 always wins
Step 3: Verify
Run cargo test to confirm both server tests pass.

Files to modify
src/data_type.rs — add start_game_with_secret()
src/box_cas.rs — extract server_with_config()
src/state_actor.rs — extract server_with_config()
tests/server_test.rs — new integration test file