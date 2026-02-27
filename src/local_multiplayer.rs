use crate::data_type::*;

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

//
// Main Game Loop
//

pub fn local_game() {
  // Inner function that repeatedly processes turns
  fn game_loop (st: GameState) {
    // 1. Ask which player is taking a turn
    println!("which player");
    let player_id = get_valid_input(NUM_PLAYERS);
    // 2. Show this player the current state
    state_view(&st, &player_id);

    if !game_over(&st) {
        let a = Action::new(player_id, get_valid_input(MAX_NUM_TO_GUESS));
        let new_st = do_action(&st, &a);
        game_loop(new_st)
    }
  }

  game_loop(start_game())
}
