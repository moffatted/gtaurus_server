mod driver;
mod ws_server;

use driver::{FluidNCDriver, GCodeConnection};
use std::sync::{Arc, Mutex};

pub struct AppState {
    pub driver: Arc<Mutex<Box<dyn GCodeConnection>>>,
}

#[tokio::main]
async fn main() {
    println!("Starting Gtaurus Standalone Server...");

    let state = Arc::new(AppState {
        driver: Arc::new(Mutex::new(Box::new(FluidNCDriver::new()))),
    });

    ws_server::start_server(state).await;
}
