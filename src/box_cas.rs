use rand::prelude::*;
use im::HashMap;

// Data Definitions

#[derive(PartialEq, Clone)]
enum GameState {
    InProgress(SecretNumber, HashMap<PlayerId, Guess>), // game is on-going
    Over(PlayerId), // winner                           // game is over
}

type SecretNumber = u32;
type Guess = u32;
type PlayerId = u32;

struct Action {
    player_id: PlayerId,
    guess: u32
}

const MAX_NUM_TO_GUESS: u32 = 20;
const NUM_PLAYERS: u32 = 3;

//
// Game Logic
//

// Creates a new game with a random secret number
// add parameter num_players: u32
fn start_game(num_players: u32) -> GameState {
    let mut rng = rand::rng();
    let answer = rng.random_range(0..MAX_NUM_TO_GUESS);

    println!("Guess a number from 0 to {MAX_NUM_TO_GUESS}");
    
    GameState::InProgress(answer, HashMap::new())
}

// Process a player's action and returns the new game state
// Original signature: GameState Action -> GameState
// New signature: &GameState &Action -> GameState
// Changed the signature after try_and_commit_action because 
// I can't move GameState out of a Mutex in try_and_commit_action
// The mutex owns the data. I can only
// - Borrow it 
// - Replace it 
// Plus, the new signature is the functional update pattern in Rust 
// Take &T and return T (new owned value)
// This makes some cloning necessary
fn do_action(st: &GameState, a: &Action) -> GameState {
    match st {
        GameState::Over(_) => st.clone(),
        GameState::InProgress(secret_num, hash) => {
            if *secret_num == a.guess {
                GameState::Over(a.player_id)
            } else {
                let new_hash = hash.update(a.player_id, a.guess);
                println!("{:?}", new_hash);
                GameState::InProgress(*secret_num, new_hash)
            }
        }
    }
}

// Prints the message that should be shown to a specific player
fn state_view(st: &GameState, player_id: &PlayerId) -> String {
    match st {
        GameState::Over(winner) => {
            if winner == player_id {
                String::from("You won! Game over.")
            } else {
                String::from("Player {player_id} won.")
            }
        }, 
        GameState::InProgress(secret_num, hash) => {
            let last_guess = hash.get(player_id);
            match last_guess {
                Some(guess) => {
                    if secret_num < guess {
                        String::from("Guess lower.")
                    } else {
                        String::from("Guess higher")
                    }
                }, 
                None => String::from("Guess a number.")
            }
        }
    }
}

//
// Input Parsing and Validation
// 


fn parse_number_input(str: String, max: u32) -> Result<u32, String> {
    let num = str.trim().parse::<u32>();
    match num {
        Ok(n) if n < max && n >= 0 => Ok(n),
        _ => Err(String::from("Parsing failed")),
    }
}

// Keeps asking for input until a valid number is entered
// It's used to get 1) player id and 2) guesses
fn get_valid_input(max: u32, mut in_port: impl BufRead, out_port: impl Write) -> u32 {
  let mut input = String::new();
  
  let len =  in_port.read_line(&mut input);

  match len {
    Ok(_) => {
        match parse_number_input(input, max) {
            Ok(num) => num,
            Err(msg) => {
                println!("{msg}. Try again.");
                get_valid_input(max, in_port, out_port)
            }
        }
    },
    Err(msg) => {
        println!("{msg}. Try again.");
        get_valid_input(max, in_port, out_port)
    }
  }
}

fn game_over(st: &GameState) -> bool {
    match st {
        GameState::Over(_) => true,
        _ => false,
    }
}

//
// Server and Multi-threading
//
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::io::{BufReader, BufRead, Write, LineWriter};

pub fn server() {
    let listener = TcpListener::bind("127.0.0.1:7878").unwrap();

    // This is the SHARED STATE that all player threads will access
    let shared_game_state = Arc::new(Mutex::new(start_game(NUM_PLAYERS)));

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
fn handle_client(mut reader: BufReader<TcpStream>, mut writer: LineWriter<TcpStream>, player_id: u32, shared_game_state: Arc<Mutex<GameState>>) {
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
        let action = Action { player_id: player_id, guess: get_valid_input(MAX_NUM_TO_GUESS, &mut reader, &mut writer) };
        
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