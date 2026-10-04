#!/usr/bin/env bash
# Деплой signaling-сервера через systemd (сервер без Docker).
#
#   HOST=root@92.242.60.161 KEY=~/.ssh/deploy_ci_ed25519 ./deploy-systemd.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
HOST="${HOST:-root@92.242.60.161}"
KEY="${KEY:-$HOME/.ssh/deploy_ci_ed25519}"
PORT="${PORT:-8080}"

SSH=(ssh -i "$KEY" -o BatchMode=yes -o ConnectTimeout=20)
SCP=(scp -i "$KEY" -o BatchMode=yes -o ConnectTimeout=20)

echo "==> Сборка релиза"
(cd "$ROOT" && cargo build --release -p p2pchat-signal-server)

echo "==> Установка на $HOST"
"${SCP[@]}" "$ROOT/target/release/p2pchat-signal-server" "$HOST:/usr/local/bin/p2pchat-signal-server"

"${SSH[@]}" "$HOST" "chmod +x /usr/local/bin/p2pchat-signal-server
cat > /etc/systemd/system/p2pchat-signal.service <<EOF
[Unit]
Description=p2pchat signaling server
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart=/usr/local/bin/p2pchat-signal-server --listen 0.0.0.0:${PORT}
Restart=always
RestartSec=3
NoNewPrivileges=true
PrivateTmp=true
StandardOutput=journal
StandardError=journal
SyslogIdentifier=p2pchat-signal

[Install]
WantedBy=multi-user.target
EOF
systemctl daemon-reload
systemctl enable --now p2pchat-signal
systemctl restart p2pchat-signal
sleep 1
systemctl is-active p2pchat-signal"

echo "==> Готово: ws://${HOST#*@}:${PORT}"