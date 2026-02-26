use gtaurus_common::{
    DriverEventObserver, FluidNCDriver as LibDriver, GCodeConnection as LibGCodeConnection,
};
use std::sync::{Arc, Mutex};

pub trait GCodeConnection: LibGCodeConnection + Send {}
impl GCodeConnection for FluidNCDriver {}

pub struct FluidNCDriver {
    inner: LibDriver,
}

struct ServerObserver {
    subscribers: Arc<Mutex<Vec<std::sync::mpsc::Sender<String>>>>,
}

impl DriverEventObserver for ServerObserver {
    fn emit(&self, line: &str) {
        println!("{}", line);
        if let Ok(mut subs_guard) = self.subscribers.lock() {
            subs_guard.retain(|tx| tx.send(line.to_string()).is_ok());
        }
    }
}

impl FluidNCDriver {
    pub fn new() -> Self {
        let dummy_subs = Arc::new(Mutex::new(Vec::new()));
        let mut inner = LibDriver::new(Arc::new(ServerObserver {
            subscribers: dummy_subs.clone(),
        }));

        // We want the inner driver's subscribers to be the ones the observer uses.
        // So we swap them out or just point the observer to the driver's one.
        // Actually, LibDriver.subscribers is pub.
        let observer = Arc::new(ServerObserver {
            subscribers: inner.subscribers.clone(),
        });
        inner.observer = observer;

        Self { inner }
    }
}

// Forward all GCodeConnection methods to the inner driver
impl LibGCodeConnection for FluidNCDriver {
    fn connect_serial(&mut self, port_name: &str, baud_rate: u32) -> Result<(), String> {
        self.inner.connect_serial(port_name, baud_rate)
    }

    fn connect_telnet(&mut self, host: &str, port: u16) -> Result<(), String> {
        self.inner.connect_telnet(host, port)
    }

    fn send_command(&mut self, cmd: String) -> Result<(), String> {
        self.inner.send_command(cmd)
    }

    fn send_realtime(&mut self, byte: u8) -> Result<(), String> {
        self.inner.send_realtime(byte)
    }

    fn disconnect(&mut self) {
        self.inner.disconnect()
    }

    fn get_status(&self) -> String {
        self.inner.get_status()
    }

    fn add_rx_subscriber(&mut self, tx: std::sync::mpsc::Sender<String>) {
        self.inner.add_rx_subscriber(tx)
    }

    fn set_auto_connect_suspended(&mut self, suspended: bool) {
        self.inner.set_auto_connect_suspended(suspended)
    }

    fn is_auto_connect_suspended(&self) -> bool {
        self.inner.is_auto_connect_suspended()
    }
}
