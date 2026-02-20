mod local_multiplayer;
mod box_cas;

// use crate::local_multiplayer::local_game;

fn main() {
    use std::net::TcpListener;
  let listener = TcpListener::bind("127.0.0.1:7878").unwrap();

    // a single stream represents an open connection between the client and the server
    // connection is the name for the full request and response process in which 
    // 1. a client connects to the server
    // 2. the server generates a response, and 
    // 3. the server closes the connection
    for stream in listener.incoming() {
        // unwrap to panic in case stream has errors
        // when a client connects to the server we are not actually iterating over connections
        // instead, we're iterating over "connection attempts"
        // connection might not be successful for a number of reasons, many of them operating system specific
        // e.g. many OS have a limit to the number of simultaneous open connections they can support
        let stream = stream.unwrap();

        println!("Connection established!");
    } 
}
