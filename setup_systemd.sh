#!/bin/bash
set -e

# Resolves the absolute path of the directory containing this script
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$SCRIPT_DIR"
BINARY_PATH="$PROJECT_DIR/target/release/gtaurus_server"
SERVICE_PATH="$HOME/.config/systemd/user/gtaurus_server.service"

echo "Building gtaurus_server in release mode..."
cargo build --release

echo "Ensuring systemd user directory exists..."
mkdir -p "$HOME/.config/systemd/user/"

echo "Creating systemd user service file..."
cat <<EOF > "$SERVICE_PATH"
[Unit]
Description=Gtaurus Standalone Server
After=network.target

[Service]
Type=simple
WorkingDirectory=$PROJECT_DIR
ExecStart=$BINARY_PATH
Restart=always
RestartSec=5

[Install]
WantedBy=default.target
EOF

echo "Reloading systemd user daemon..."
systemctl --user daemon-reload

echo ""
echo "--------------------------------------------------------"
echo "Service 'gtaurus_server.service' has been created."
echo "--------------------------------------------------------"
echo "To ENABLE the service to start automatically on login:"
echo "  systemctl --user enable gtaurus_server.service"
echo ""
echo "To START the service now:"
echo "  systemctl --user start gtaurus_server.service"
echo ""
echo "To check the STATUS of the service:"
echo "  systemctl --user status gtaurus_server.service"
echo ""
echo "To check the LOGS of the service:"
echo "  journalctl --user -u gtaurus_server.service -f"
echo ""
echo "Optional: If you want the service to run even when you're NOT logged in:"
echo "  loginctl enable-linger $USER"
echo "--------------------------------------------------------"
