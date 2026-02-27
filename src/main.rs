mod local_multiplayer;
mod box_cas;
mod state_actor;
mod event_loop;
mod data_type;


use crate::state_actor::server;
// use crate::local_multiplayer::local_game;

fn main() {
    server();
}
