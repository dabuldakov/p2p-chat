#!/usr/bin/env bash
# Деплой coturn (STUN/TURN) на VPS через Docker (host network).
# Использует TURN REST API: общий секрет вместо статического пользователя.
#
#   HOST=user@host SSH_PORT=2222 TURN_SECRET=... PUBLIC_IP=1.2.3.4 LAN_IP=10.0.0.5 ./deploy-turn.sh
set -euo pipefail

HOST="${HOST:-}"
SSH_PORT="${SSH_PORT:-22}"
KEY="${KEY:-$HOME/.ssh/deploy_ci_ed25519}"
PUBLIC_IP="${PUBLIC_IP:?Задайте PUBLIC_IP}"
LAN_IP="${LAN_IP:-$PUBLIC_IP}"
TURN_SECRET="${TURN_SECRET:?Задайте TURN_SECRET (общий с signaling)}"
[ -n "$HOST" ] || { echo "Задайте HOST=user@host"; exit 1; }

SSH=(ssh -i "$KEY" -p "$SSH_PORT" -o BatchMode=yes -o ConnectTimeout=20)

CONF="listening-ip=${LAN_IP}
relay-ip=${LAN_IP}
external-ip=${PUBLIC_IP}/${LAN_IP}
listening-port=3478
fingerprint
use-auth-secret
static-auth-secret=${TURN_SECRET}
realm=p2pchat
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

echo "==> Готово. TURN: turn:${PUBLIC_IP}:3478 (временные креды через TURN REST API)"
