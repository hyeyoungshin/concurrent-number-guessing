use im::HashMap;
use std::io::{BufRead, Write};
use rand::prelude::*;


//
// Data Definitions
//

pub type SecretNumber = u32;
pub type Guess = u32;
pub type PlayerId = u32;

#[derive(PartialEq, Clone)]
pub enum GameState {
    InProgress(SecretNumber, HashMap<PlayerId, Guess>), // game is on-going
    Over(PlayerId), // winner                           // game is over
}

#[derive(PartialEq)]
pub struct Action {
    player_id: PlayerId,
    guess: u32
}

impl Action {
    pub fn new(player_id: PlayerId, guess: Guess) -> Self {
        Action { player_id, guess }
    }

    pub fn get_guess(&self) -> u32 {
        self.guess
    }

    pub fn get_player_id(&self) -> PlayerId {
        self.player_id
    }
}

pub const MAX_NUM_TO_GUESS: u32 = 20;
pub const NUM_PLAYERS: u32 = 3;

//
// Game Logic
//

// Creates a new game with a random secret number
// add parameter num_players: u32
pub fn start_game() -> GameState {
    let mut rng = rand::rng();
    let answer = rng.random_range(0..MAX_NUM_TO_GUESS);
    
    GameState::InProgress(answer, HashMap::new())
}

pub fn game_over(st: &GameState) -> bool {
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
pub fn do_action(st: &GameState, a: &Action) -> GameState {
    match st {
        GameState::Over(_) => st.clone(),
        GameState::InProgress(secret_num, hash) => {
            if *secret_num == a.get_guess() {
                GameState::Over(a.get_player_id())
            } else {
                let new_hash = hash.update(a.get_player_id(), a.get_guess());
                println!("{:?}", new_hash);
                GameState::InProgress(*secret_num, new_hash)
            }
        }
    }
}

// Prints the message that should be shown to a specific player
pub fn state_view(st: &GameState, player_id: &PlayerId) -> String {
    match st {
        GameState::Over(winner) => {
            if winner == player_id {
                format!("You won! Game over.")
            } else {
                format!("Player {winner} won.")
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
                None => format!("Guess a number from 0 to {}.", MAX_NUM_TO_GUESS - 1)
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
pub fn get_valid_input(max: u32, in_port: &mut impl BufRead, out_port: &mut impl Write) -> u32 {
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
