use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::thread;
use std::time::Duration;

use number_guessing::data_type::*;

const SECRET: u32 = 10;

/// Connects a client, reads the greeting, then plays with the given strategy.
/// Returns a Vec of all lines received from the server.
fn play_client(addr: &str, strategy: fn(&str) -> Option<u32>) -> Vec<String> {
    let stream = TcpStream::connect(addr).unwrap();
    stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut writer = stream;

    let mut lines = Vec::new();

    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => break,        // connection closed
            Ok(_) => {}
            Err(_) => break,       // timeout or error
        }
        let line = line.trim_end().to_string();
        lines.push(line.clone());

        if line.contains("won") {
            break;
        }

        if let Some(guess) = strategy(&line) {
            if writeln!(writer, "{}", guess).is_err() {
                break; // server closed connection
            }
        }
    }

    lines
}

/// Binary search strategy that converges on the secret number.
/// Tracks state via the hint messages from the server.
fn binary_search_strategy() -> fn(&str) -> Option<u32> {
    // We use a simple approach: parse the hint and do binary search.
    // Since we can't carry mutable state in a fn pointer, we use a
    // thread-local to track bounds.
    thread_local! {
        static LO: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
        static HI: std::cell::Cell<u32> = const { std::cell::Cell::new(MAX_NUM_TO_GUESS) };
    }

    fn strat(line: &str) -> Option<u32> {
        if line.starts_with("Guess higher") {
            // Last guess was too low, raise lower bound
            LO.with(|lo| {
                HI.with(|hi| {
                    let mid = (lo.get() + hi.get()) / 2;
                    // The last guess was mid-ish, so raise lo
                    lo.set(lo.get().max(mid + 1));
                })
            });
        } else if line.starts_with("Guess lower") {
            // Last guess was too high, lower upper bound
            HI.with(|hi| {
                LO.with(|lo| {
                    let mid = (lo.get() + hi.get()) / 2;
                    hi.set(hi.get().min(mid));
                })
            });
        } else if line.starts_with("Guess a number") || line.starts_with("You are player") {
            // Initial prompt or greeting — no guess needed for greeting
            if line.starts_with("You are player") {
                // Reset bounds for this new client
                LO.with(|lo| lo.set(0));
                HI.with(|hi| hi.set(MAX_NUM_TO_GUESS));
                return None;
            }
        } else {
            return None;
        }

        // Make a guess: midpoint of current bounds
        let guess = LO.with(|lo| HI.with(|hi| (lo.get() + hi.get()) / 2));
        Some(guess)
    }

    strat
}

/// Strategy that always guesses 0 (will never win when secret != 0).
fn always_zero(_line: &str) -> Option<u32> {
    if _line.starts_with("Guess") {
        Some(0)
    } else {
        None
    }
}

/// Find a free port by binding to port 0.
fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

/// Generic test runner that works with any server implementation.
fn run_server_test(server_fn: fn(&str, GameState)) {
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let server_addr = addr.clone();

    // Spawn server in background thread
    let server_handle = thread::spawn(move || {
        server_fn(&server_addr, start_game_with_secret(SECRET));
    });

    // Small delay to let the server start listening
    thread::sleep(Duration::from_millis(100));

    // Spawn 3 clients: player 0 does binary search, players 1 and 2 always guess 0
    let addr0 = addr.clone();
    let addr1 = addr.clone();
    let addr2 = addr.clone();

    let c0 = thread::spawn(move || play_client(&addr0, binary_search_strategy()));
    let c1 = thread::spawn(move || play_client(&addr1, always_zero));
    let c2 = thread::spawn(move || play_client(&addr2, always_zero));

    let lines0 = c0.join().unwrap();
    let lines1 = c1.join().unwrap();
    let lines2 = c2.join().unwrap();

    // Player 0 should get greeting
    assert!(lines0[0].starts_with("You are player"), "Player 0 missing greeting");

    // Player 0 should win (binary search finds 10)
    let winner_line = lines0.iter().find(|l| l.contains("won"));
    assert!(winner_line.is_some(), "Player 0 should have a win/loss line. Got: {:?}", lines0);
    assert_eq!(winner_line.unwrap(), "You won! Game over.", "Player 0 should win. Got: {:?}", lines0);

    // Players 1 and 2 should see the loss message
    for (i, lines) in [(1, &lines1), (2, &lines2)] {
        assert!(lines[0].starts_with("You are player"), "Player {} missing greeting", i);
        let end_line = lines.iter().find(|l| l.contains("won"));
        assert!(end_line.is_some(), "Player {} should have an end-game line. Got: {:?}", i, lines);
        // The loser sees "Player X won." where X is the winner's id
        let end = end_line.unwrap();
        assert!(
            end.contains("won") && !end.contains("You won"),
            "Player {} should see loss message, got: {}", i, end
        );
    }
}

#[test]
fn test_box_cas_server() {
    run_server_test(number_guessing::box_cas::server_with_config);
}

#[test]
fn test_state_actor_server() {
    run_server_test(number_guessing::state_actor::server_with_config);
}

#[test]
fn test_event_loop_server() {
    run_server_test(number_guessing::event_loop::server_with_config);
}

#[test]
fn test_event_loop_spawn_local_server() {
    run_server_test(number_guessing::event_loop_spawn_local::server_with_config);
}

#[test]
fn test_event_loop_futures_unordered_server() {
    run_server_test(number_guessing::event_loop_futures_unordered::server_with_config);
}
