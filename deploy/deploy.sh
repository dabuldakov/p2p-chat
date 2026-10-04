#!/usr/bin/env bash
# Деплой signaling-сервера на VPS.
#
#   HOST=root@89.104.66.226 KEY=~/.ssh/deploy_ci_ed25519 ./deploy.sh
#
# Что делает:
#   1. собирает релизный бинарь p2pchat-signal-server
#   2. копирует его в /usr/local/bin и unit в /etc/systemd/system
#   3. включает и запускает systemd-сервис
#   4. по возможности открывает порт 8080 в фаерволе
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
HOST="${HOST:-root@89.104.66.226}"
KEY="${KEY:-$HOME/.ssh/deploy_ci_ed25519}"
SSH_OPTS=(-i "$KEY" -o BatchMode=yes -o ConnectTimeout=15)

echo "==> Сборка релиза"
(cd "$ROOT" && cargo build --release -p p2pchat-signal-server)

BIN="$ROOT/target/release/p2pchat-signal-server"
UNIT="$ROOT/deploy/p2pchat-signal-server.service"

echo "==> Копирование на $HOST"
scp "${SSH_OPTS[@]}" "$BIN" "$HOST:/usr/local/bin/p2pchat-signal-server"
scp "${SSH_OPTS[@]}" "$UNIT" "$HOST:/etc/systemd/system/p2pchat-signal-server.service"

echo "==> Запуск сервиса"
ssh "${SSH_OPTS[@]}" "$HOST" bash -s <<'EOF'
set -e
systemctl daemon-reload
systemctl enable --now p2pchat-signal-server
# фаервол (если есть)
if command -v ufw >/dev/null 2>&1; then ufw allow 8080/tcp || true; fi
if command -v firewall-cmd >/dev/null 2>&1; then firewall-cmd --add-port=8080/tcp --permanent || true; firewall-cmd --reload || true; fi
systemctl --no-pager --lines=5 status p2pchat-signal-server || true
EOF

echo "==> Готово. Проверка:"
echo "    curl http://${HOST#*@}:8080   (WebSocket-эндпоинт отвечает на upgrade)"
