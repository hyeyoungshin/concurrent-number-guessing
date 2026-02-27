use im::HashMap;
use crate::data_type::*;

// Synchronous actors in terms of channels
use std::sync::mpsc::{Sender};
use std::sync::mpsc;

struct Request {
    msg: Msg,
    reply_to: Sender<Response>,
}
enum Msg {
    DisplayState(PlayerId),
    ProcessAction(Action)
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

// State Actor (Business logic)
fn handle_request(request: &Request, state: &mut GameState, last_displayed: &mut HashMap<PlayerId, GameState>) -> Response {
    match &request.msg {
        Msg::DisplayState(player_id) => {
            last_displayed.insert(*player_id, state.clone());
            Response::DisplayState(state.clone())
        },
        Msg::ProcessAction(a) => {
            if game_over(&state) {
                Response::OtherPlayerWon(state.clone())
            } else {
                *state = do_action(&state, &a);
                Response::ActionCommitted
            }
        }
    }
}

// Client Actor
fn handle_client(reader: &mut BufReader<TcpStream>, writer: &mut LineWriter<TcpStream>, player_id: u32, state_update_channel: &Sender<Request>) {
    writeln!(writer, "You are player {}", player_id).unwrap();
    writeln!(writer, "Guess a number from 0 to {MAX_NUM_TO_GUESS}").unwrap();
    
    loop {
        match sync_message(state_update_channel, Msg::DisplayState(player_id)) {
            Response::DisplayState(state) => {
                writeln!(writer, "{}", state_view(&state, &player_id)).unwrap();
                if game_over(&state) {
                    break;
                }
                
                let a = Action::new(player_id, get_valid_input(MAX_NUM_TO_GUESS, reader, writer));
                match sync_message(state_update_channel, Msg::ProcessAction(a)) {
                    Response::OtherPlayerWon(end_state) => {
                        writeln!(writer, "Sorry, another player won in the meantime!").unwrap();
                    },
                    Response::ActionCommitted => { 
                        // Guess recorded, game continues — loop again
                    },
                    _ => { panic!("response mismatch"); }
                }
            }
            _ => { panic!("response mismatch"); }
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
        let mut state = start_game();
        let mut last_displayed = HashMap::new();
        // Loop receiving messages
        for request in state_rx {
            let response = handle_request(&request, &mut state, &mut last_displayed);
            request.reply_to.send(response).unwrap();
        }
    });

    for player_id in 0..NUM_PLAYERS {
        let (stream, _addr) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut writer = LineWriter::new(stream);
        let state_tx = state_tx.clone();
        // Each client gets a cloned of state_tx
        thread::spawn(move || { 
            handle_client(&mut reader, &mut writer, player_id, &state_tx);
        });
    }
}
