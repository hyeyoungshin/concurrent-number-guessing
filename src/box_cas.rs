use crate::data_type::*;

//
// Server and Multi-threading
//
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::io::{BufReader, Write, LineWriter};

pub fn server() {
    server_with_config("127.0.0.1:7878", start_game());
}

pub fn server_with_config(addr: &str, initial_state: GameState) {
    let listener = TcpListener::bind(addr).unwrap();

    // This is the SHARED STATE that all player threads will access
    let shared_game_state = Arc::new(Mutex::new(initial_state));

    for player_id in 0..NUM_PLAYERS {
        let (stream, _addr) = listener.accept().unwrap();

        let reader = BufReader::new(stream.try_clone().unwrap());
        let writer = LineWriter::new(stream);

        let shared_game_state = Arc::clone(&shared_game_state);

        thread::spawn(move || {
          handle_client(reader, writer, player_id, shared_game_state)
        });
    }
}

// fn handle_client(mut reader: impl BufRead, mut writer: impl Writ, player_id: u32, shared_game_state: Arc<Mutex<GameState>>) {
fn handle_client(mut reader: BufReader<TcpStream>, mut writer: LineWriter<TcpStream>, player_id: PlayerId, shared_game_state: Arc<Mutex<GameState>>) {
    writeln!(writer, "You are player {}", player_id).unwrap();

    loop {
        // THREAD-SAFE READ: Get current game state from Mutex (shared box)
        // Multiple threads can read simultaneously
        let current_st = {
            let state = shared_game_state.lock().unwrap();
            state.clone() // copy the data so I can use it after the lock is released
        }; // lock released here

        // Send the current state view to THIS player
        // Each player sees their own personalized view
        writeln!(writer, "{}", state_view(&current_st, &player_id)).unwrap();

        if game_over(&current_st) {
            break;
        }

        // If game is not over, get this player's next guess
        let action = Action::new(player_id, get_valid_input(MAX_NUM_TO_GUESS, &mut reader, &mut writer));

        // CRITICAL: Try to atomically update shared state
        // This might fail if another player wins while we're processing
        if !try_and_commit_action(&shared_game_state, &action) {
            writeln!(writer, "Sorry, another player won in the meantime!").unwrap();
        }
    }
}

// Atomically updates the game state with an action
// Uses Compare-And_Swap (CAS) for lock-free concurrency in the corresponding Racket code
fn try_and_commit_action(game_state: &Arc<Mutex<GameState>>, action: &Action) -> bool {
    // Read the current state from the box
    // Note: Another thread might change this before we commit!
    let mut current_state = game_state.lock().unwrap();

    // if game is already over, can't apply action
    if game_over(&current_state) {
        false // action rejected
    } else {
        // try to automatically update the box using CAS
        // box-cas! does: "if box still contains currentstate, replace with new state"
        // Returns true if successful, false if another thread changed it first
        *current_state = do_action(&*current_state, action);
        true
    }
}
