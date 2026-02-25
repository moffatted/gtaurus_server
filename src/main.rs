mod driver;
mod ws_server;

use driver::{FluidNCDriver, GCodeConnection};
use std::sync::{Arc, Mutex};

pub struct AppState {
    pub driver: Arc<Mutex<Box<dyn GCodeConnection>>>,
}

#[tokio::main]
async fn main() {
    println!(r#"
    
     ____ _                                    ____                             
    / ___| |_ __ _ _   _ _ __ _   _ ___       / ___|  ___ _ ____   _____ _ __ 
   | |  _| __/ _` | | | | '__| | | / __| _____\___ \ / _ \ '__\ \ / / _ \ '__|
   | |_| | || (_| | |_| | |  | |_| \__ \_____|___) |  __/ |   \ V /  __/ |   
    \____|\__\__,_|\__,_|_|   \__,_|___/     |____/ \___|_|    \_/ \___|_|   

    SECURE YOUR MACHINE: Ensure a firewall rule is in place so your CNC 
    can only be accessed from your trusted local network.
                                                                             
    "#);
    println!("Starting Gtaurus Standalone Server...");
    println!(">> Ready to bridge the gap between web and machine.");
    println!("");
    println!("Web Dashboard: http://0.0.0.0:8080");
    println!("WebSocket API: ws://0.0.0.0:9001");
    println!("");

    let state = Arc::new(AppState {
        driver: Arc::new(Mutex::new(Box::new(FluidNCDriver::new()))),
    });

    ws_server::start_server(state).await;
}
