use crate::data_type::*;

use std::rc::Rc;
use std::cell::RefCell;
use tokio::net::TcpListener;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::task::LocalSet;

pub fn server() {
    server_with_config("127.0.0.1:7878", start_game());
}

pub fn server_with_config(addr: &str, initial_state: GameState) {
    // Single-threaded async event loop — same concept as the hand-written
    // select() loop in event_loop.rs, but with async/await ergonomics.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    // LocalSet lets us spawn non-Send tasks on this single thread
    let local = LocalSet::new();
    local.block_on(&rt, async_server(addr, initial_state));
}

async fn async_server(addr: &str, initial_state: GameState) {
    let listener = TcpListener::bind(addr).await.unwrap();

    // Rc<RefCell<>> instead of Arc<Mutex<>> — no threads, no locking needed
    let shared_game_state = Rc::new(RefCell::new(initial_state));

    let mut handles = Vec::new();

    for player_id in 0..NUM_PLAYERS {
        let (stream, _addr) = listener.accept().await.unwrap();

        let shared_game_state = Rc::clone(&shared_game_state);

        // spawn_local runs the task on the current thread's event loop
        let handle = tokio::task::spawn_local(async move {
            handle_client(stream, player_id, shared_game_state).await;
        });
        handles.push(handle);
    }

    // Wait for all client tasks to finish
    for handle in handles {
        handle.await.unwrap();
    }
}

async fn handle_client(
    stream: tokio::net::TcpStream,
    player_id: PlayerId,
    shared_game_state: Rc<RefCell<GameState>>,
) {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);

    writer.write_all(format!("You are player {}\n", player_id).as_bytes()).await.unwrap();

    loop {
        // Read current game state — just borrow, no locking
        let current_st = shared_game_state.borrow().clone();

        // Send the current state view to this player
        writer.write_all(format!("{}\n", state_view(&current_st, &player_id)).as_bytes()).await.unwrap();

        if game_over(&current_st) {
            break;
        }

        // Read this player's next guess
        let guess = async_get_valid_input(MAX_NUM_TO_GUESS, &mut reader, &mut writer).await;
        let action = Action::new(player_id, guess);

        // Try to update shared state
        let committed = {
            let mut state = shared_game_state.borrow_mut();
            if game_over(&state) {
                false
            } else {
                *state = do_action(&state, &action);
                true
            }
        };

        if !committed {
            writer.write_all(b"Sorry, another player won in the meantime!\n").await.unwrap();
        }
    }
}

// Async version of get_valid_input — reads lines without blocking the event loop
async fn async_get_valid_input(
    max: u32,
    reader: &mut BufReader<tokio::net::tcp::OwnedReadHalf>,
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
) -> u32 {
    loop {
        let mut input = String::new();
        reader.read_line(&mut input).await.unwrap();

        match input.trim().parse::<u32>() {
            Ok(n) if n < max => return n,
            _ => {
                writer.write_all(b"Parsing failed. Try again.\n").await.unwrap();
            }
        }
    }
}
