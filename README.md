# Gtaurus Standalone Server

The `gtaurus_server` is a standalone backend application written in Rust. It provides a WebSocket bridge between the Gtaurus web frontend and a CNC machine running FluidNC.

## Purpose

Web browsers have strict security models that prevent them from directly interacting with raw local hardware such as Serial/USB ports. While modern browsers have some experimental capabilities like WebSerial or WebUSB, they are not universally supported and can be restrictive.

To run Gtaurus as a pure web application (accessing it via browser from another device or machine) while still controlling local hardware attached to a host computer, you need a local broker.

The `gtaurus_server` acts as this broker:

- It runs locally on the machine physically connected to the CNC controller.
- It exposes a WebSocket server on `ws://0.0.0.0:9001` that the Gtaurus web frontend can connect to.
- It serves the frontend static dashboard files over HTTP on `http://0.0.0.0:8080`.
- It translates JSON payloads via WebSocket into raw hardware commands (Serial/USB or Telnet) to drive the FluidNC controller.
- It provides file storage boundaries to allow the web app to safely upload G-code files directly to the host machine for standalone execution.
- It forwards real-time hardware status and responses back to the frontend.
- **Robustness**: Automatically detects and reconnects to the serial port if the connection is lost (e.g., USB unplugged or CNC powered off).

## Technical Details

- **Language**: Rust
- **Transport Protocol**: WebSockets via `tokio-tungstenite`.
- **Port**: `9001` (by default, configurable via `server_config.json`).
- **Driver Architecture**: Implements a `FluidNCDriver` containing the same serial and network logic as the Tauri desktop application, but decoupled from the desktop windowing environment.
- **Logging**: All activity and errors are logged to the console and to `gtaurus_server.log`.

## Prerequisites

- Rust (`rustup`) toolchain.
- Standard build tools (e.g., `build-essential` on Linux).

### 📷 Camera Support (Optional)

To use the built-in Camera Viewer and hardware settings manager within the Gtaurus web interface:

- You must have a camera physically connected to your CNC host machine.
- **Dependency**: [Crowsnest](https://github.com/mainsail-crew/crowsnest) must be installed on your Linux host (specifically v4+ supporting `ustreamer` or `camera-streamer`).
- **Configuration Path**: The server expects your Crowsnest config to be located at `~/printer_data/config/crowsnest.conf` (standard for Moonraker/Mainsail setups).
- **Auto-Bootstrapping**: If Crowsnest is installed globally via root, Gtaurus Server will automatically migrate it to a user-level `systemd` service (`systemctl --user ...`) on startup via the `setup_crowsnest.sh` script. This enables seamless, passwordless camera restarts when adjusting settings (brightness, exposure, etc.) from the UI.

## Building

To build the server for release:

```bash
cargo build --release
```

The compiled binary will be located in the `target/release/` directory.

## Running

You can run the application directly with Cargo during development:

```bash
cargo run
```

Or run the compiled binary:

```bash
./target/release/gtaurus_server
```

## Configuration

On the first run, the server creates a `server_config.json` file. You can customize the following options:

```json
{
  "port": 9001,
  "auto_connect": true,
  "default_serial_port": null,
  "default_baud_rate": 115200,
  "http_port": 8080,
  "web_root": "./public"
}
```

- `port`: The WebSocket port to listen on.
- `auto_connect`: If `true`, the server will automatically search for and connect to a serial port on startup and whenever the connection is lost.
- `default_serial_port`: Set this to a specific port (e.g., `"/dev/ttyUSB0"`) to bypass auto-discovery.
- `default_baud_rate`: The baud rate for the serial connection (default is 115200).
- `http_port`: The port to run the lightweight HTTP server on for serving the frontend application.
- `web_root`: The relative or absolute path of the directory containing the compiled `gtaurus-app` frontend files (e.g., HTML, JS, CSS). Default is `./public`.

## Deploying the Web Dashboard

To run the web dashboard seamlessly without needing `Node.js` or `npm run dev` running on your CNC machine host:

1. On your development machine, go to the `gtaurus-app` directory and build the frontend:

   ```bash
   npm run build
   ```

2. Copy the contents of the newly generated `dist/` directory from `gtaurus-app`.
3. Paste these files into a `public/` directory inside the `gtaurus_server` folder on your remote host.
4. Run `gtaurus_server`. The server will now host the frontend locally at `http://<your-host-ip>:8080` while handling WebSocket connections automatically on port `9001`.

## Firewall & Security

To protect the server from unauthorized access or potential attacks, it is highly recommended to set up a firewall rule. By default, the server listens on port `9001`.

If you are using `ufw` (Uncomplicated Firewall) on Linux, you can allow traffic to the WebSocket port with:

```bash
# Allow access from any IP (use with caution)
sudo ufw allow 9001/tcp

# Or, more securely, only allow access from a specific IP address
sudo ufw allow from 192.168.1.100 to any port 9001 proto tcp
```

## Systemd Service (Linux)

To have the server start automatically as a background service on Linux, you can use the provided setup script:

```bash
chmod +x setup_systemd.sh
./setup_systemd.sh
```

This script creates a `systemd` user service. You can then manage it with:

```bash
# Enable to start on login
systemctl --user enable gtaurus_server.service

# Start the service
systemctl --user start gtaurus_server.service

# Check status
systemctl --user status gtaurus_server.service

# View live logs
journalctl --user -u gtaurus_server.service -f
```

### Running Headless on Boot

By default, user-level systemd services only start when that specific user logs in via the desktop or SSH, and they stop immediately when the user logs out.

If you want the `gtaurus_server` to start automatically the moment the machine receives power (e.g., for headless deployment in a cabinet) **without** requiring you to manually log into an account, you MUST enable "linger" for your user account:

```bash
loginctl enable-linger $USER
```
