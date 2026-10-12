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
  echo "## Инфраструктура (нужны только эти):"
  echo "SIGNAL_URL=wss://90.188.89.63.sslip.io:9443"
  echo "TURN_CREDS_URL=https://90.188.89.63.sslip.io:9443/turn-credentials"
  echo
  echo "## Статические TURN-креды больше НЕ нужны — TURN выдаёт временные"
  echo "## через TURN REST API (TURN_CREDS_URL)."
} | tee "$OUT"

echo
echo "==> Сохранено в $OUT"
echo "==> Пароли keystore показаны выше — не коммить этот файл (он в .gitignore)."
