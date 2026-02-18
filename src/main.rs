use rand::prelude::*;
use im::HashMap;

// Data Definitions

#[derive(PartialEq)]
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
fn do_action(st: GameState, a: Action) -> GameState {
    match st {
        GameState::Over(_) => st,
        GameState::InProgress(secret_num, hash) => {
            if secret_num == a.guess {
                GameState::Over(a.player_id)
            } else {
                let new_hash = hash.update(a.player_id, a.guess);
                println!("{:?}", new_hash);
                GameState::InProgress(secret_num, new_hash)
            }
        }
    }
}

// Prints the message that should be shown to a specific player
fn state_view(st: &GameState, player_id: &PlayerId) {
    match st {
        GameState::Over(winner) => {
            if winner == player_id {
                println!("You won! Game over.");
            } else {
                println!("Player {player_id} won.");
            }
        }, 
        GameState::InProgress(secret_num, hash) => {
            let last_guess = hash.get(player_id);
            match last_guess {
                Some(guess) => {
                    if secret_num < guess {
                        println!("Guess lower.");
                    } else {
                        println!("Guess higher");
                    }
                }, 
                None => println!("Guess a number.")
            }
        }
    }
}

//
// Input Parsing and Validation
// 

// Keeps asking for input until a valid number is entered
// It's used to get 1) player id and 2) guesses
fn get_valid_input(max: u32) -> u32 {
  use std::io;

  let mut input = String::new();
  match io::stdin().read_line(&mut input) {
    Ok(_) => {
        let number = input.trim().parse::<u32>();
        match number {
            Ok(num) if num < max => num,
            Ok(_) => {
                println!("Invalid input. Try again.");
                get_valid_input(max)
            },
            Err(msg) => {
                println!("{msg}. Try again.");
                get_valid_input(max)
            }
        }
    },
    Err(msg) => {
        println!("{msg}. Try again.");
        get_valid_input(max)
    }
  }
}

fn is_game_over(st: &GameState) -> bool {
    match st {
        GameState::Over(_) => true,
        _ => false,
    }
}

//
// Main Game Loop
//

fn local_game() {
  // Inner function that repeatedly processes turns
  fn game_loop (st: GameState) {
    // 1. Ask which player is taking a turn
    println!("which player");
    let player_id = get_valid_input(NUM_PLAYERS);
    // 2. Show this player the current state
    state_view(&st, &player_id);

    if !is_game_over(&st) {
        let a = Action { player_id, guess: get_valid_input(MAX_NUM_TO_GUESS) };
        let new_st = do_action(st, a);
        game_loop(new_st)
    }
  }

  game_loop(start_game(NUM_PLAYERS))
}

fn main() {
  local_game()
    
}
