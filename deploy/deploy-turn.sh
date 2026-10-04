#!/usr/bin/env bash
# Деплой coturn (STUN/TURN) на VPS через Docker (host network).
#
#   HOST=dmitry_buldakov@90.188.89.63 SSH_PORT=2222 TURN_PASS=... \
#     PUBLIC_IP=90.188.89.63 LAN_IP=192.168.1.8 ./deploy-turn.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
HOST="${HOST:-dmitry_buldakov@90.188.89.63}"
SSH_PORT="${SSH_PORT:-2222}"
KEY="${KEY:-$HOME/.ssh/deploy_ci_ed25519}"
PUBLIC_IP="${PUBLIC_IP:-90.188.89.63}"
LAN_IP="${LAN_IP:-192.168.1.8}"
TURN_USER="${TURN_USER:-p2pchat}"
TURN_PASS="${TURN_PASS:?Задайте TURN_PASS}"

SSH=(ssh -i "$KEY" -p "$SSH_PORT" -o BatchMode=yes -o ConnectTimeout=20)

CONF="listening-ip=${LAN_IP}
relay-ip=${LAN_IP}
external-ip=${PUBLIC_IP}/${LAN_IP}
listening-port=3478
fingerprint
lt-cred-mech
realm=p2pchat
user=${TURN_USER}:${TURN_PASS}
min-port=49152
max-port=65535
no-multicast-peers
no-cli"

echo "==> Запись конфига и запуск coturn"
"${SSH[@]}" "$HOST" "mkdir -p ~/p2pchat
cat > ~/p2pchat/turnserver.conf <<'EOF'
${CONF}
EOF
docker rm -f p2pchat-turn 2>/dev/null || true
docker pull -q coturn/coturn:latest
docker run -d --name p2pchat-turn --restart unless-stopped --network host \
  -v /home/\${USER}/p2pchat/turnserver.conf:/etc/coturn/turnserver.conf:ro \
  coturn/coturn:latest -c /etc/coturn/turnserver.conf
sleep 2
docker ps --filter name=p2pchat-turn --format '{{.Names}} {{.Status}}'"

echo "==> Готово. TURN: turn:${PUBLIC_IP}:3478 (user=${TURN_USER})"