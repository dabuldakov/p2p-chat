#!/usr/bin/env bash
# Docker-деплой signaling-сервера (без sudo — нужен доступ к docker).
#
#   HOST=dmitry_buldakov@90.188.89.63 SSH_PORT=2222 SERVICE_PORT=8090 \
#     ./deploy-docker.sh
#
# Что делает:
#   1. собирает релизный бинарь локально (архитектура совпадает: x86_64 Ubuntu 24.04)
#   2. копирует его в ~/p2pchat на сервере
#   3. запускает контейнер ubuntu:24.04 с авто-рестартом и маппингом порта
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
HOST="${HOST:-dmitry_buldakov@90.188.89.63}"
SSH_PORT="${SSH_PORT:-2222}"
SERVICE_PORT="${SERVICE_PORT:-8090}"
KEY="${KEY:-$HOME/.ssh/deploy_ci_ed25519}"

SSH=(ssh -i "$KEY" -p "$SSH_PORT" -o BatchMode=yes -o ConnectTimeout=20)
SCP=(scp -i "$KEY" -P "$SSH_PORT" -o BatchMode=yes -o ConnectTimeout=20)
REMOTE_DIR="p2pchat"

echo "==> Сборка релиза"
(cd "$ROOT" && cargo build --release -p p2pchat-signal-server)

BIN="$ROOT/target/release/p2pchat-signal-server"

echo "==> Копирование на $HOST"
"${SSH[@]}" "$HOST" "mkdir -p ~/$REMOTE_DIR"
"${SCP[@]}" "$BIN" "$HOST:~/$REMOTE_DIR/p2pchat-signal-server"

echo "==> Запуск контейнера (порт $SERVICE_PORT)"
"${SSH[@]}" "$HOST" "docker rm -f p2pchat-signal 2>/dev/null || true
docker pull -q ubuntu:24.04
docker run -d --name p2pchat-signal --restart unless-stopped \
  -p ${SERVICE_PORT}:8080 \
  -v /home/\${USER}/${REMOTE_DIR}/p2pchat-signal-server:/usr/local/bin/p2pchat-signal-server:ro \
  ubuntu:24.04 /usr/local/bin/p2pchat-signal-server --listen 0.0.0.0:8080
sleep 2
docker logs p2pchat-signal 2>&1 | tail -3
docker ps --filter name=p2pchat-signal --format '{{.Names}} {{.Status}} {{.Ports}}'"

echo "==> Готово. Проверка: cargo run -p p2pchat-signal-server --example remote_check -- ws://${HOST#*@}:${SERVICE_PORT}"