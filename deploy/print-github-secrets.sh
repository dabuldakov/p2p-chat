#!/usr/bin/env bash
# Печатает значения, которые нужно добавить в
# GitHub → Settings → Secrets and variables → Actions.
# Результат также сохраняется в android/keystore/GITHUB_SECRETS.txt (gitignore).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
KS="$ROOT/android/keystore/p2pchat-release.jks"
CRED="$ROOT/android/keystore/CREDENTIALS.txt"
OUT="$ROOT/android/keystore/GITHUB_SECRETS.txt"

[ -f "$KS" ] || { echo "Нет keystore: $KS (создайте или сгенерируйте заново)"; exit 1; }
[ -f "$CRED" ] || { echo "Нет $CRED"; exit 1; }
# shellcheck disable=SC1090
source "$CRED"

{
  echo "# Секреты для GitHub Actions (Settings → Secrets and variables → Actions)"
  echo
  echo "## KEYSTORE_BASE64 (одной строкой):"
  base64 -w0 "$KS"
  echo
  echo
  echo "## Простые значения:"
  echo "KEY_ALIAS=$KEY_ALIAS"
  echo "KEYSTORE_PASSWORD=$KEYSTORE_PASSWORD"
  echo "KEY_PASSWORD=$KEY_PASSWORD"
  echo
  echo "## Инфраструктура (сервер 92.242.60.161):"
  echo "SIGNAL_URL=ws://92.242.60.161:8080"
  echo "TURN_URL=turn:92.242.60.161:3478?transport=udp"
  echo "TURN_USER=p2pchat"
  echo "TURN_PASS=LacNywVkdr2vxem3dybr"
} | tee "$OUT"

echo
echo "==> Сохранено в $OUT"
echo "==> Пароли TURN/keystore были показаны выше — не коммить этот файл (он в .gitignore)."
