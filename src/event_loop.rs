use crate::data_type::*;
use libc::{fd_set, select, FD_ZERO, FD_SET, FD_ISSET};
use std::collections::HashMap;
use std::ptr::null_mut;
use std::{net::SocketAddr, os::fd::AsRawFd};
use socket2::{Socket, Domain, Type};

struct Client {
    player_id: PlayerId,
    buf: String,       // accumulates partial input until we get a newline
    done: bool,        // true once this client's connection should be closed
}

// Write a string to a raw fd
fn fd_write(fd: i32, msg: &str) {
    let bytes = msg.as_bytes();
    let mut written = 0;
    while written < bytes.len() {
        let n = unsafe {
            libc::write(fd, bytes[written..].as_ptr() as *const libc::c_void, bytes.len() - written)
        };
        if n <= 0 { break; }
        written += n as usize;
    }
}

fn fd_writeln(fd: i32, msg: &str) {
    fd_write(fd, msg);
    fd_write(fd, "\n");
}

// Try to parse a complete line from the client's buffer.
// Returns Some(line_content) if a full line is available, None otherwise.
fn try_read_line(client: &mut Client) -> Option<String> {
    if let Some(pos) = client.buf.find('\n') {
        let line = client.buf[..pos].to_string();
        client.buf = client.buf[pos + 1..].to_string();
        Some(line)
    } else {
        None
    }
}

// Try to parse a valid guess from a line. Returns Ok(n) or Err with message.
fn parse_guess(line: &str) -> Result<u32, String> {
    match line.trim().parse::<u32>() {
        Ok(n) if n < MAX_NUM_TO_GUESS => Ok(n),
        _ => Err(String::from("Parsing failed")),
    }
}

// loops forever, asking the OS:
// "Which of my connections have something happening right now?" and then
// handles each one briefly before looping again.
fn event_loop(server_fd: i32, mut game_state: GameState) {
    let mut clients: HashMap<i32, Client> = HashMap::new();
    let mut next_player_id: PlayerId = 0;

    loop {
        // If all NUM_PLAYERS clients have finished, exit
        if next_player_id >= NUM_PLAYERS && clients.is_empty() {
            break;
        }

        // --- PHASE 1: Build the watch set ---
        let mut read_set: fd_set = unsafe { std::mem::zeroed() };

        unsafe {
            FD_ZERO(&mut read_set);
        }

        // Only watch for new connections if we haven't accepted all players yet
        let mut highest_fd = -1;
        if next_player_id < NUM_PLAYERS {
            unsafe { FD_SET(server_fd, &mut read_set); }
            highest_fd = server_fd;
        }

        for (&fd, client) in &clients {
            if !client.done {
                unsafe { FD_SET(fd, &mut read_set); }
                if fd > highest_fd {
                    highest_fd = fd;
                }
            }
        }

        if highest_fd < 0 {
            break;
        }

        // --- PHASE 2: Hand off to the OS ---

        // Handing the OS a set of file descriptors and saying
        // "Watch all of these. Come back to me when at least one of them has data ready to read."
        // When it returns, it has erased every fd from `read_set` that is not ready.
        let ready = unsafe {
            select(
                highest_fd + 1,
                &mut read_set,
                null_mut(),
                null_mut(),
                null_mut()
            )
        };

        if ready < 0 {
            eprintln!("select error");
            break;
        }

        // --- PHASE 3: Inspect results ---

        // New client connecting?
        if next_player_id < NUM_PLAYERS && unsafe { FD_ISSET(server_fd, &read_set) } {
            let client_fd = unsafe { libc::accept(server_fd, null_mut(), null_mut()) };
            if client_fd >= 0 {
                let player_id = next_player_id;
                next_player_id += 1;

                // Send greeting and first prompt
                fd_writeln(client_fd, &format!("You are player {}", player_id));
                fd_writeln(client_fd, &state_view(&game_state, &player_id));

                clients.insert(client_fd, Client {
                    player_id,
                    buf: String::new(),
                    done: false,
                });
            }
        }

        // Existing client sent data?
        let fds: Vec<i32> = clients.keys().cloned().collect();
        for fd in fds {
            if !unsafe { FD_ISSET(fd, &read_set) } {
                continue;
            }

            let mut raw_buf = [0u8; 1024];
            let bytes_read = unsafe {
                libc::read(fd, raw_buf.as_mut_ptr() as *mut libc::c_void, raw_buf.len())
            };

            if bytes_read <= 0 {
                // Client disconnected
                unsafe { libc::close(fd); }
                clients.remove(&fd);
                continue;
            }

            // Append raw bytes to the client's line buffer
            let data = String::from_utf8_lossy(&raw_buf[..bytes_read as usize]).to_string();
            if let Some(client) = clients.get_mut(&fd) {
                client.buf.push_str(&data);
            }

            // Process all complete lines in the buffer
            loop {
                let line = {
                    match clients.get_mut(&fd) {
                        Some(client) => try_read_line(client),
                        None => break,
                    }
                };

                let line = match line {
                    Some(l) => l,
                    None => break,
                };

                let player_id = clients[&fd].player_id;

                // Try to parse the guess
                match parse_guess(&line) {
                    Ok(guess) => {
                        let action = Action::new(player_id, guess);

                        if game_over(&game_state) {
                            // Game ended while we were reading input
                            fd_writeln(fd, "Sorry, another player won in the meantime!");
                        } else {
                            game_state = do_action(&game_state, &action);
                        }

                        // Send updated state view to this player
                        fd_writeln(fd, &state_view(&game_state, &player_id));

                        if game_over(&game_state) {
                            // Mark this client as done
                            if let Some(client) = clients.get_mut(&fd) {
                                client.done = true;
                            }

                            // Notify all other clients that the game is over
                            for (&other_fd, other_client) in clients.iter_mut() {
                                if other_fd != fd && !other_client.done {
                                    fd_writeln(other_fd, &state_view(&game_state, &other_client.player_id));
                                    other_client.done = true;
                                }
                            }
                        }
                    }
                    Err(msg) => {
                        fd_writeln(fd, &format!("{}. Try again.", msg));
                    }
                }
            }
        }

        // Clean up done clients — shutdown write side so pending data
        // is delivered (FIN) rather than discarded (RST from unread data)
        let done_fds: Vec<i32> = clients.iter()
            .filter(|(_, c)| c.done)
            .map(|(&fd, _)| fd)
            .collect();
        for fd in done_fds {
            unsafe {
                libc::shutdown(fd, libc::SHUT_WR);
                libc::close(fd);
            }
            clients.remove(&fd);
        }
    }
}

pub fn server() {
    server_with_config("127.0.0.1:7878", start_game());
}

pub fn server_with_config(addr: &str, initial_state: GameState) {
    // syscall
    // address scheme: IPV4
    // what kind of socket: TCP
    let socket = Socket::new(Domain::IPV4, Type::STREAM, None).unwrap();
    socket.set_reuse_address(true).unwrap();

    let address: SocketAddr = addr.parse().unwrap();
    socket.bind(&address.into()).unwrap();

    // Listen for incoming connections with a backlog of 128
    socket.listen(128).unwrap();

    let raw_fd = socket.as_raw_fd();

    event_loop(raw_fd, initial_state);
}
