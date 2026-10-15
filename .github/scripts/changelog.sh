#!/usr/bin/env bash
# Группирует темы коммитов (conventional commits) в markdown для релиза.
# Читает темы из stdin, печатает разделы по префиксу.
set -euo pipefail

subjects="$(cat)"

emit() {
  local title="$1" pat="$2"
  local lines
  lines="$(printf '%s\n' "$subjects" | grep -E "^${pat}(:|\(| |!)" || true)"
  if [ -n "$lines" ]; then
    echo "### $title"
    printf '%s\n' "$lines" | sed 's/^/- /'
    echo
  fi
}

emit "Новые возможности" 'feat'
emit "Исправления" 'fix'
emit "Документация" 'docs'
emit "Рефакторинг" 'refactor'
emit "Производительность" 'perf'
emit "Тесты" 'test'

others="$(printf '%s\n' "$subjects" | grep -vE '^(feat|fix|docs|refactor|perf|test)(:|\(| |!)' | grep -v '^$' || true)"
if [ -n "$others" ]; then
  echo "### Прочее"
  printf '%s\n' "$others" | sed 's/^/- /'
  echo
fi
