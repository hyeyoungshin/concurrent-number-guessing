use crate::data_type::*;

use std::future::Future;
use std::pin::Pin;
use tokio::net::TcpListener;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use futures::stream::{FuturesUnordered, StreamExt};

type EventFuture<'a> = Pin<Box<dyn Future<Output = Event> + 'a>>;

pub fn server() {
    server_with_config("127.0.0.1:7878", start_game());
}

pub fn server_with_config(addr: &str, initial_state: GameState) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async_server(addr, initial_state));
}

struct Client {
    player_id: PlayerId,
    writer: tokio::net::tcp::OwnedWriteHalf,
    done: bool,
}

async fn write_to(writer: &mut tokio::net::tcp::OwnedWriteHalf, msg: &str) {
    writer.write_all(msg.as_bytes()).await.unwrap();
}

async fn writeln_to(writer: &mut tokio::net::tcp::OwnedWriteHalf, msg: &str) {
    write_to(writer, msg).await;
    write_to(writer, "\n").await;
}

/// Events produced by futures in the FuturesUnordered pool.
enum Event {
    /// A new client connected.
    NewClient(tokio::net::TcpStream),
    /// An existing client sent a line.
    Line(ReadResult),
}

struct ReadResult {
    index: usize,
    reader: BufReader<tokio::net::tcp::OwnedReadHalf>,
    line: String,
}

/// Read one line from a client, returning the reader back via ReadResult.
async fn read_one_line(
    index: usize,
    mut reader: BufReader<tokio::net::tcp::OwnedReadHalf>,
) -> Event {
    let mut line = String::new();
    let _ = reader.read_line(&mut line).await;
    Event::Line(ReadResult { index, reader, line })
}

/// Accept one connection from the listener.
async fn accept_one(listener: &TcpListener) -> Event {
    let (stream, _addr) = listener.accept().await.unwrap();
    Event::NewClient(stream)
}

async fn async_server(addr: &str, initial_state: GameState) {
    let listener = TcpListener::bind(addr).await.unwrap();

    let mut game_state = initial_state;
    let mut clients: Vec<Client> = Vec::new();
    let mut next_player_id: PlayerId = 0;

    // FuturesUnordered holds both accept and read_line futures.
    // They race against each other: a player can start guessing
    // while the server is still waiting for others to connect.
    let mut pending: FuturesUnordered<EventFuture<'_>> = FuturesUnordered::new();

    // Seed with the first accept
    pending.push(Box::pin(accept_one(&listener)));

    while let Some(event) = pending.next().await {
        match event {
            Event::NewClient(stream) => {
                let player_id = next_player_id;
                next_player_id += 1;

                let (read_half, write_half) = stream.into_split();
                let reader = BufReader::new(read_half);
                let mut client = Client { player_id, writer: write_half, done: false };

                writeln_to(&mut client.writer, &format!("You are player {}", player_id)).await;
                writeln_to(&mut client.writer, &state_view(&game_state, &player_id)).await;

                let index = clients.len();
                clients.push(client);

                // Start reading from this client
                pending.push(Box::pin(read_one_line(index, reader)));

                // Accept more players if we haven't reached NUM_PLAYERS
                if next_player_id < NUM_PLAYERS {
                    pending.push(Box::pin(accept_one(&listener)));
                }
            }

            Event::Line(ReadResult { index, reader, line }) => {
                let client = &mut clients[index];

                if client.done {
                    continue;
                }

                match line.trim().parse::<u32>() {
                    Ok(n) if n < MAX_NUM_TO_GUESS => {
                        let action = Action::new(client.player_id, n);

                        if game_over(&game_state) {
                            writeln_to(&mut client.writer,
                                "Sorry, another player won in the meantime!").await;
                        } else {
                            game_state = do_action(&game_state, &action);
                        }

                        writeln_to(&mut client.writer,
                            &state_view(&game_state, &client.player_id)).await;

                        if game_over(&game_state) {
                            client.done = true;

                            for other in clients.iter_mut() {
                                if !other.done {
                                    writeln_to(&mut other.writer,
                                        &state_view(&game_state, &other.player_id)).await;
                                    other.done = true;
                                }
                            }
                            continue;
                        }
                    }
                    _ => {
                        writeln_to(&mut client.writer, "Parsing failed. Try again.").await;
                    }
                }

                if !client.done {
                    pending.push(Box::pin(read_one_line(index, reader)));
                }
            }
        }
    }
}
