use crate::data_type::*;
use libc::{fd_set, select, FD_ZERO, FD_SET, FD_ISSET};
use std::collections::HashMap;
use std::ptr::null_mut;

#[derive(PartialEq)]
struct Client {
    last_seen: Option<GameState>,
    last_done: Option<Action>,
    fd: i32,
}

fn process_guess(client: &mut Client, data: &[u8]) {
    // 1. parse data into a guess
    // 2. compare against secret number
    // 3. update client state
    // 4. write response back to client.fd
}

// loops forever, asking the OS: 
// "Which of my connections have something happening right now?" and then 
// handles each one briefly before looping again.
fn event_loop (server_fd: i32) {
    let mut clients: HashMap<i32, Client> = HashMap::new();
    
    loop {
        // --- PHASE 1: Build the watch set ---
        let mut read_set: fd_set = unsafe { std::mem::zeroed() };

        unsafe {
            FD_ZERO(&mut read_set);
            FD_SET(server_fd, &mut read_set);
        }

        let mut highest_fd = server_fd;

        for (&fd, _) in &clients {
            unsafe{ FD_SET(fd, &mut read_set); }
            if fd > highest_fd {
                highest_fd = fd;
            }
        }

       // --- PHASE 2: Hand off to the OS ---

        // Handing the OS a set of file descriptiors and saying
        // "Watch all of these. Come back to me when at least one of them has data ready to read."
        // When it returns, it has erased every fd from `read_set` that is not ready.
        // For example, if I am watching `server_fd` and three client fds, and only one client sent data,
        // `read_set` now contains just that once client's fd.
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
        if unsafe { FD_ISSET(server_fd, &read_set) } {
          let client_fd = unsafe { libc::accept(server_fd, null_mut(), null_mut()) };
          if client_fd >= 0 {
            clients.insert(client_fd, Client {
                last_seen: None,
                last_done: None,
                fd: client_fd,
            });
            println!("New client connected: fd = {client_fd}");
          }
        }

        // Existing client sent data?
        let fds: Vec<i32> = clients.keys().cloned().collect();
        for fd in fds {
            if unsafe { FD_ISSET(fd, &read_set) } {
                let mut buf = [0u8; 1024];
                let bytes_read = unsafe {
                    libc::read(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len())
                };

                if bytes_read <= 0 {
                    // Client disconnected
                    println!("Client disconnected: fd={fd}");
                    unsafe { libc::close(fd); }
                    clients.remove(&fd);
                } else {
                    //Process the guess
                    let data = &buf[..bytes_read as usize];
                    // if let handles the None case gracefully by doing nothing
                    if let Some(client) = clients.get_mut(&fd) {
                        process_guess(client, data);
                    }
                    // match clients.get_mut(&fd) {
                    //     Some(client) => process_guess(client, data),
                    //     None => {} 
                    // }
                }
            }
        }
    }
}


use std::{net::SocketAddr, os::fd::AsRawFd};
use socket2::{Socket, Domain, Type};

fn server() -> std::io::Result<()> {
    // syscall
    // address scheme: IPV4
    // what kind of socket: TCP 
    let socket = Socket::new(Domain::IPV4, Type::STREAM, None)?;
    
    let address: SocketAddr = "127.0.0.1:7878".parse().unwrap();
    socket.bind(&address.into())?;

    // Listen for incoming connections with a backlog of 128
    socket.listen(128)?;
    
    let raw_fd = socket.as_raw_fd();

    event_loop(raw_fd);

    Ok(())
}