#!/bin/bash
# 🍓 STRAWBERRY Daemon launcher — copy this anywhere, works on any Linux setup.
# Usage: ./run-daemon.sh        (foreground)
#        nohup ./run-daemon.sh & (background)

# Auto-detect desktop environment (Wayland/X11 both supported)
export WAYLAND_DISPLAY="${WAYLAND_DISPLAY:-wayland-0}"
export DISPLAY="${DISPLAY:-:0}"
export DBUS_SESSION_BUS_ADDRESS="${DBUS_SESSION_BUS_ADDRESS:-unix:path=$RUNTIME_DIR/bus}"
export XDG_CURRENT_DESKTOP="${XDG_CURRENT_DESKTOP:-}"

DIR="$(cd "$(dirname "$0")" && pwd)"
BIN="$DIR/target/release/strawberry-daemon"

if [ ! -x "$BIN" ]; then
    echo "🍓 Building first time…"
    cargo build --release --manifest-path "$DIR/Cargo.toml"
fi

exec "$BIN"
