#!/bin/bash
set -e

USER_HOME=$HOME
CONFIG_DIR="$USER_HOME/.config/systemd/user"
SERVICE_FILE="crowsnest.service"
CONF_FILE="crowsnest.conf"
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"

echo "==== Crowsnest User-Service Installer ===="

# 1. Ensure user systemd directory exists
mkdir -p "$CONFIG_DIR"

# 2. Stop standard service if it currently exists globally
if systemctl is-active --quiet crowsnest; then
    echo "Stopping global system crowsnest service..."
    sudo systemctl stop crowsnest || true
    sudo systemctl disable crowsnest || true
fi

# 3. Copy our updated user service config
echo "Installing crowsnest.service to $CONFIG_DIR..."
cp "$SCRIPT_DIR/$SERVICE_FILE" "$CONFIG_DIR/$SERVICE_FILE"

# 4. Ensure target directories exist
mkdir -p "$USER_HOME/printer_data/config"
mkdir -p "$USER_HOME/printer_data/logs"
mkdir -p "$USER_HOME/crowsnest"

# 5. Bootstrap default crowsnest.conf if one doesn't exist
if [ ! -f "$USER_HOME/printer_data/config/$CONF_FILE" ]; then
    echo "Creating default $CONF_FILE in printer_data..."
    # Replace references to eddiem dynamically with the real user
    sed "s|/home/eddiem|$USER_HOME|g" "$SCRIPT_DIR/$CONF_FILE" > "$USER_HOME/printer_data/config/$CONF_FILE"
else
    echo "Found existing $CONF_FILE. Skipping overwrite."
fi

# 6. Make sure the user's services persist after logout
echo "Enabling linger for $USER..."
sudo loginctl enable-linger $USER || true

# 7. Start user service
echo "Reloading systemctl daemon and enabling service..."
systemctl --user daemon-reload
systemctl --user enable crowsnest.service
systemctl --user restart crowsnest.service

echo "Done! Crowsnest is now running safely under the $USER space."
