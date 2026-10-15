#!/usr/bin/env bash
#
# Группирует темы коммитов (conventional commits) в Markdown для релиза.
# Читает темы из stdin, пишет разделы формата:
#   ## <заголовок группы>
#   - пункт
# Этот же формат разбирает клиент (ChangelogParser) для списка «что нового».
set -euo pipefail

# locale-safe: ${line^} и tr не зависят от C-locale
export LC_ALL=C.UTF-8

order=(feat fix perf refactor docs test build ci style chore revert other)
declare -A heading=(
  [feat]="✨ Новое"
  [fix]="🐛 Исправления"
  [perf]="⚡ Быстродействие"
  [refactor]="♻️ Рефакторинг"
  [docs]="📝 Документация"
  [test]="✅ Тесты"
  [build]="🔧 Сборка"
  [ci]="🔧 CI"
  [style]="💅 Стиль"
  [chore]="🧹 Прочее"
  [revert]="⏪ Откаты"
  [other]="⏳ Разное"
)
declare -A items

while IFS= read -r subject; do
  [ -n "$subject" ] || continue

  # "feat(chat): текст" -> "feat"
  type="${subject%%:*}"
  type="${type%%(*}"
  type="$(printf '%s' "$type" | tr '[:upper:]' '[:lower:]' | tr -d '[:space:]')"

  # Версионные коммиты в список изменений не попадают.
  [ "$type" = "release" ] && continue
  [ -n "${heading[$type]:-}" ] || type="other"

  text="$(printf '%s' "${subject#*:}" | sed -E 's/^[[:space:]]+//')"
  [ "$text" = "$subject" ] && text="$subject"

  scope=""
  [[ "$subject" == *"("* ]] && scope="$(printf '%s' "${subject#*(}" | sed -E 's/\).*//')"

  if [ -n "$scope" ] && [ "$text" != "$subject" ]; then
    line="${scope}: ${text}"
  else
    line="$text"
  fi
  line="${line^}"

  items[$type]="${items[$type]:-}- ${line}"$'\n'
done

for key in "${order[@]}"; do
  if [ -n "${items[$key]:-}" ]; then
    printf '## %s\n%s\n' "${heading[$key]}" "${items[$key]}"
  fi
done
