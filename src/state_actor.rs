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

fn game_over(st: &GameState) -> bool {
    match st {
        GameState::Over(_) => true,
        _ => false,
    }
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
        Ok(n) if n < max => Ok(n),
        _ => Err(String::from("Parsing failed")),
    }
}

// Keeps asking for input until a valid number is entered
// It's used to get 1) player id and 2) guesses
fn get_valid_input(max: u32, mut in_port: impl BufRead, mut out_port: impl Write) -> u32 {
  let mut input = String::new();
  
  let len =  in_port.read_line(&mut input);

  match len {
    Ok(_) => {
        match parse_number_input(input, max) {
            Ok(num) => num,
            Err(msg) => {
                writeln!(out_port, "{msg}. Try again.").unwrap();
                get_valid_input(max, in_port, out_port)
            }
        }
    },
    Err(msg) => {
        writeln!(out_port, "{msg}. Try again.").unwrap();
        get_valid_input(max, in_port, out_port)
    }
  }
}

// Synchronous actors in terms of channels
use std::sync::mpsc::{Receiver, Sender};
use std::sync::mpsc;


struct Request {
    msg: Msg,
    reply_to: Sender<Response>,
}
enum Msg {
    DisplayState(PlayerId),
    ProcessAction(PlayerId, Action)
}

enum Response {
    DisplayState(GameState),
    OtherPlayerWon(GameState),
    ActionCommitted,
}

fn sync_message(state_update_channel: &Sender<Request>, msg: Msg) -> Response {
    // Create a temporary reply channel
    let (resp_tx, resp_rx) = mpsc::channel();
    // Wrap request with reply sender
    let request = Request {msg, reply_to: resp_tx};
    // Send message to actor
    state_update_channel.send(request).unwrap();
    // Wait for response and return it
    resp_rx.recv().unwrap()
}

fn handle_request(request: &Request, state_rx: Receiver<Request>) -> Response {
    let mut state = start_game(NUM_PLAYERS);
    let mut last_displayed_state_for_players = HashMap::new();

    match request.msg {
        Msg::DisplayState(player_id) => {
            last_displayed_state_for_players.insert(player_id, state);
            Response::DisplayState(state)
         },
         Msg::ProcessAction(player_id, a) => {
            if game_over(state) {
                Response::OtherPlayerWon(state)
            } else {
                state = do_action(state, &a);
                Response::ActionCommitted
            }
        }
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
use std::u32::MAX;

pub fn server() {
    let listener = TcpListener::bind("127.0.0.1:7878").unwrap();

    let (state_tx, state_rx) = mpsc::channel::<Request>();

    // state actor
    thread::spawn(move || {
        // Loop receiving messages
        for request in state_rx {
            let response = handle_request(&request, state_rx);
            request.reply_to.send(response).unwrap();
        }
    });

    for player_id in 0..NUM_PLAYERS {
        
        let (stream, _addr) = listener.accept().unwrap();

        let reader = BufReader::new(stream.try_clone().unwrap());
        let writer = LineWriter::new(stream);

        let state_tx = state_tx.clone();

        // Each client gets a cloned of state_tx
        thread::spawn(move || { 
            handle_client(reader, writer, player_id, &state_tx);
        });
    }
}

// Client-handling actor
fn handle_client(mut reader: BufReader<TcpStream>, mut writer: LineWriter<TcpStream>, player_id: u32, state_update_channel: &Sender<Request>) {
    writeln!(writer, "You are player {}", player_id).unwrap();
    
    loop {
        match sync_message(state_update_channel, Msg::DisplayState(player_id)) {
            Response::DisplayState(state) => {
                writeln!(writer, "{}", state_view(&state, &player_id)).unwrap();
                if !game_over(&state) {
                    let a = Action { player_id, guess: get_valid_input(MAX_NUM_TO_GUESS, reader, writer)};
                    match sync_message(state_update_channel, Msg::ProcessAction(player_id, a)) {
                        Response::OtherPlayerWon(end_state) => {
                            writeln!(writer, "Sorry, another player won in the meantime!").unwrap();
                            writeln!(writer, "{}", state_view(&end_state, &player_id)).unwrap();
                        },
                        Response::ActionCommitted => { break; }
                        _ => { panic!("response mismatch"); }
                    }
                }
            }
            _ => { panic!("response mismatch"); }
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
