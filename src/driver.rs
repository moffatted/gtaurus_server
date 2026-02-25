use serialport::SerialPort;
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

pub struct SerialWrapper(pub Box<dyn SerialPort>);
unsafe impl Send for SerialWrapper {}

const MAX_BUFFER_SIZE: usize = 127;

#[derive(Clone)]
pub enum ConnectionStatus {
    Disconnected,
    Serial(String),
    Telnet(String),
}
impl std::fmt::Display for ConnectionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectionStatus::Disconnected => write!(f, "Disconnected"),
            ConnectionStatus::Serial(p) => write!(f, "Serial: {p}"),
            ConnectionStatus::Telnet(h) => write!(f, "WiFi: {h}"),
        }
    }
}

pub trait GCodeConnection: Send {
    fn connect_serial(&mut self, port_name: &str, baud_rate: u32) -> Result<(), String>;
    fn connect_telnet(&mut self, host: &str, port: u16) -> Result<(), String>;
    fn send_command(&mut self, cmd: String) -> Result<(), String>;
    fn send_realtime(&mut self, byte: u8) -> Result<(), String>;
    fn disconnect(&mut self);
    fn get_status(&self) -> String;
    fn add_rx_subscriber(&mut self, tx: std::sync::mpsc::Sender<String>);
}

enum ActiveConnection {
    None,
    Serial {
        _port: SerialWrapper,
        rt_port: Arc<Mutex<Box<dyn SerialPort + Send>>>,
        cmd_tx: std::sync::mpsc::Sender<String>,
    },
    Telnet {
        _stream: TcpStream,
        rt_stream: Arc<Mutex<TcpStream>>,
        cmd_tx: std::sync::mpsc::Sender<String>,
    },
}

pub struct FluidNCDriver {
    conn: ActiveConnection,
    status: Arc<Mutex<ConnectionStatus>>,
    subscribers: Arc<Mutex<Vec<std::sync::mpsc::Sender<String>>>>,
}

impl FluidNCDriver {
    pub fn new() -> Self {
        Self {
            conn: ActiveConnection::None,
            status: Arc::new(Mutex::new(ConnectionStatus::Disconnected)),
            subscribers: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn emit(subs: &Arc<Mutex<Vec<std::sync::mpsc::Sender<String>>>>, line: &str) {
        if let Ok(mut subs_guard) = subs.lock() {
            subs_guard.retain(|tx| tx.send(line.to_string()).is_ok());
        }
    }

    fn spawn_serial_reader(
        reader_port: SerialWrapper,
        pending_bytes: Arc<Mutex<usize>>,
        pending_lens: Arc<Mutex<VecDeque<usize>>>,
        subscribers: Arc<Mutex<Vec<std::sync::mpsc::Sender<String>>>>,
        status: Arc<Mutex<ConnectionStatus>>,
    ) {
        thread::spawn(move || {
            let mut reader = BufReader::new(reader_port.0);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) => {
                        Self::emit(&subscribers, "[GTaurus] Serial EOF");
                        if let Ok(mut s) = status.lock() {
                            *s = ConnectionStatus::Disconnected;
                        }
                        break;
                    }
                    Ok(_) => {
                        let trimmed = line.trim().to_string();
                        if !trimmed.is_empty() {
                            if trimmed == "ok" {
                                let mut lenses = pending_lens.lock().unwrap();
                                if let Some(len) = lenses.pop_front() {
                                    let mut bytes = pending_bytes.lock().unwrap();
                                    *bytes = bytes.saturating_sub(len);
                                }
                            }
                            Self::emit(&subscribers, &trimmed);
                        }
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => continue,
                    Err(_) => {
                        Self::emit(&subscribers, "[GTaurus] Serial read error");
                        if let Ok(mut s) = status.lock() {
                            *s = ConnectionStatus::Disconnected;
                        }
                        break;
                    }
                }
            }
        });
    }

    fn spawn_serial_writer(
        mut writer_port: SerialWrapper,
        rx: std::sync::mpsc::Receiver<String>,
        pending_bytes: Arc<Mutex<usize>>,
        pending_lens: Arc<Mutex<VecDeque<usize>>>,
        status: Arc<Mutex<ConnectionStatus>>,
        subscribers: Arc<Mutex<Vec<std::sync::mpsc::Sender<String>>>>,
    ) {
        thread::spawn(move || {
            for cmd in rx {
                let cmd_len = cmd.len() + 1;
                loop {
                    if *pending_bytes.lock().unwrap() + cmd_len < MAX_BUFFER_SIZE {
                        break;
                    }
                    thread::sleep(Duration::from_millis(1));
                }
                let full_cmd = format!("{}\n", cmd);
                if writer_port.0.write_all(full_cmd.as_bytes()).is_err()
                    || writer_port.0.flush().is_err()
                {
                    Self::emit(&subscribers, "[GTaurus] Serial write error");
                    if let Ok(mut s) = status.lock() {
                        *s = ConnectionStatus::Disconnected;
                    }
                    break;
                }
                {
                    let mut bytes = pending_bytes.lock().unwrap();
                    *bytes += cmd_len;
                    pending_lens.lock().unwrap().push_back(cmd_len);
                }
            }
        });
    }

    fn spawn_tcp_reader(
        stream: TcpStream,
        subscribers: Arc<Mutex<Vec<std::sync::mpsc::Sender<String>>>>,
        status: Arc<Mutex<ConnectionStatus>>,
    ) {
        thread::spawn(move || {
            let mut reader = BufReader::new(stream);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) => {
                        Self::emit(&subscribers, "[GTaurus] Telnet connection closed");
                        if let Ok(mut s) = status.lock() {
                            *s = ConnectionStatus::Disconnected;
                        }
                        break;
                    }
                    Ok(_) => {
                        let trimmed = line.trim().to_string();
                        if !trimmed.is_empty() {
                            Self::emit(&subscribers, &trimmed);
                        }
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => continue,
                    Err(e) => {
                        Self::emit(&subscribers, &format!("[GTaurus] Telnet read error: {e}"));
                        if let Ok(mut s) = status.lock() {
                            *s = ConnectionStatus::Disconnected;
                        }
                        break;
                    }
                }
            }
        });
    }

    fn spawn_tcp_writer(
        stream: Arc<Mutex<TcpStream>>,
        rx: std::sync::mpsc::Receiver<String>,
        status: Arc<Mutex<ConnectionStatus>>,
        subscribers: Arc<Mutex<Vec<std::sync::mpsc::Sender<String>>>>,
    ) {
        thread::spawn(move || {
            for cmd in rx {
                let mut s = stream.lock().unwrap();
                let full_cmd = format!("{}\n", cmd);
                if s.write_all(full_cmd.as_bytes()).is_err() || s.flush().is_err() {
                    Self::emit(&subscribers, "[GTaurus] Telnet write error");
                    if let Ok(mut s) = status.lock() {
                        *s = ConnectionStatus::Disconnected;
                    }
                    break;
                }
            }
        });
    }
}

impl GCodeConnection for FluidNCDriver {
    fn connect_serial(&mut self, port_name: &str, baud_rate: u32) -> Result<(), String> {
        self.disconnect();
        let port = serialport::new(port_name, baud_rate)
            .timeout(Duration::from_millis(100))
            .open()
            .map_err(|e| e.to_string())?;

        let reader_clone = SerialWrapper(port.try_clone().map_err(|e| e.to_string())?);
        let writer_clone = SerialWrapper(port.try_clone().map_err(|e| e.to_string())?);
        let rt_inner: Box<dyn SerialPort + Send> = port.try_clone().map_err(|e| e.to_string())?;
        let rt_port = Arc::new(Mutex::new(rt_inner));

        let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<String>();
        let pending_bytes = Arc::new(Mutex::new(0usize));
        let pending_lens = Arc::new(Mutex::new(VecDeque::<usize>::new()));

        Self::emit(
            &self.subscribers,
            &format!("[GTaurus] Connected via Serial: {}", port_name),
        );

        Self::spawn_serial_reader(
            reader_clone,
            pending_bytes.clone(),
            pending_lens.clone(),
            self.subscribers.clone(),
            self.status.clone(),
        );
        Self::spawn_serial_writer(
            writer_clone,
            cmd_rx,
            pending_bytes,
            pending_lens,
            self.status.clone(),
            self.subscribers.clone(),
        );

        if let Ok(mut s) = self.status.lock() {
            *s = ConnectionStatus::Serial(port_name.to_string());
        }
        self.conn = ActiveConnection::Serial {
            _port: SerialWrapper(port),
            rt_port,
            cmd_tx,
        };
        Ok(())
    }

    fn connect_telnet(&mut self, host: &str, port: u16) -> Result<(), String> {
        self.disconnect();
        let addr = format!("{}:{}", host, port);
        let stream =
            TcpStream::connect(&addr).map_err(|e| format!("Cannot connect to {addr}: {e}"))?;
        stream
            .set_read_timeout(Some(Duration::from_millis(100)))
            .map_err(|e| e.to_string())?;

        let reader_clone = stream.try_clone().map_err(|e| e.to_string())?;
        let writer_arc = Arc::new(Mutex::new(stream.try_clone().map_err(|e| e.to_string())?));
        let rt_stream = writer_arc.clone();

        let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<String>();

        Self::emit(
            &self.subscribers,
            &format!("[GTaurus] Connected via Telnet: {addr}"),
        );

        Self::spawn_tcp_reader(reader_clone, self.subscribers.clone(), self.status.clone());
        Self::spawn_tcp_writer(
            writer_arc,
            cmd_rx,
            self.status.clone(),
            self.subscribers.clone(),
        );

        if let Ok(mut s) = self.status.lock() {
            *s = ConnectionStatus::Telnet(addr);
        }
        self.conn = ActiveConnection::Telnet {
            _stream: stream,
            rt_stream,
            cmd_tx,
        };
        Ok(())
    }

    fn send_command(&mut self, cmd: String) -> Result<(), String> {
        if self.get_status() == "Disconnected" {
            return Err("Not connected".to_string());
        }
        match &self.conn {
            ActiveConnection::Serial { cmd_tx, .. } => cmd_tx.send(cmd).map_err(|e| e.to_string()),
            ActiveConnection::Telnet { cmd_tx, .. } => cmd_tx.send(cmd).map_err(|e| e.to_string()),
            ActiveConnection::None => Err("Not connected".to_string()),
        }
    }

    fn send_realtime(&mut self, byte: u8) -> Result<(), String> {
        if self.get_status() == "Disconnected" {
            return Err("Not connected".to_string());
        }
        match &self.conn {
            ActiveConnection::Serial { rt_port, .. } => {
                let mut p = rt_port.lock().map_err(|_| "Poisoned".to_string())?;
                p.write_all(&[byte]).map_err(|e| e.to_string())?;
                p.flush().map_err(|e| e.to_string())
            }
            ActiveConnection::Telnet { rt_stream, .. } => {
                let mut s = rt_stream.lock().map_err(|_| "Poisoned".to_string())?;
                s.write_all(&[byte]).map_err(|e| e.to_string())?;
                s.flush().map_err(|e| e.to_string())
            }
            ActiveConnection::None => Err("Not connected".to_string()),
        }
    }

    fn disconnect(&mut self) {
        self.conn = ActiveConnection::None;
        if let Ok(mut s) = self.status.lock() {
            *s = ConnectionStatus::Disconnected;
        }
    }

    fn get_status(&self) -> String {
        if let Ok(s) = self.status.lock() {
            s.to_string()
        } else {
            "Disconnected".to_string()
        }
    }

    fn add_rx_subscriber(&mut self, tx: std::sync::mpsc::Sender<String>) {
        if let Ok(mut subs) = self.subscribers.lock() {
            subs.push(tx);
        }
    }
}
