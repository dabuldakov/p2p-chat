# Google Play — материалы и чек-лист (p2pchat)

Пакет: `com.p2pchat.app` (навсегда).  
Контакт: `dabuldakov@gmail.com` (замените, если нужно).  
Privacy policy: https://dabuldakov.github.io/p2p-chat/privacy.html  
RU: https://dabuldakov.github.io/p2p-chat/ru/privacy.html

## Store listing

**Название (≤30):** `p2pchat — P2P messenger`

**Короткое описание (≤80):**
`Encrypted P2P chat without servers. Groups, invites and GIFs.`

**Полное описание (EN):**
```
p2pchat is a decentralized messenger: messages go directly between phones, are end-to-end encrypted, and are not stored on any central server.

• No accounts, no phone numbers — your identity is a key generated on your device.
• Groups: create a group, invite by QR code or link, chat.
• End-to-end encryption (MLS). The server only helps devices find each other for a few seconds.
• GIF search, link previews, avatars, message editing and deletion.
• Send photos, videos and audio; record voice and video messages.
• Emoji reactions: up to 100 per participant, grouped with counters.
• Storage screen: media/cache sizes, cleanup by category and media re-download.
• Group owner transfer: if the owner disappears or members challenge them, the group can vote a new owner.
• Seed-phrase backup to restore your identity on a new device.
• Open source.

How it works: two devices connect directly (or through a TURN relay when the network requires it). The signaling server exchanges only connection metadata and never sees your messages.

Privacy: no ads, no analytics, no central storage of messages. See the privacy policy.

Note: to start chatting you need at least two devices with the app.
```

**Полное описание (RU):**
```
p2pchat — децентрализованный мессенджер: сообщения идут напрямую между телефонами, защищены сквозным шифрованием и не хранятся на центральном сервере.

• Без аккаунтов и номеров телефона — идентичность создаётся ключом на устройстве.
• Группы: создайте группу, пригласите по QR или ссылке, общайтесь.
• Сквозное шифрование (MLS). Сервер лишь помогает устройствам найти друг друга на несколько секунд.
• Поиск гифок, превью ссылок, аватары, редактирование и удаление сообщений.
• Отправка фото, видео и аудио; запись голосовых и видеосообщений.
• Реакции на сообщения: до 100 на участника, с группировкой и счётчиками.
• Экран хранилища: размеры медиа/кэша, очистка по категориям и повторная догрузка файлов.
• Передача владения группой: если владелец пропал или участники инициировали вызов, группа может выбрать нового владельца.
• Резервная копия seed-фразой для восстановления на новом устройстве.
• Открытый исходный код.

Как это работает: два устройства соединяются напрямую (или через TURN-релей, если сеть требует). Signaling-сервер передаёт только метаданные соединения и не видит сообщения.

Приватность: без рекламы, аналитики и центрального хранения сообщений. Подробнее — в политике конфиденциальности.

Важно: для общения нужно минимум два устройства с приложением.
```

**Категория:** Communication.  
**Теги:** messaging, privacy, peer-to-peer, chat.  
**Иконка:** `store/play_icon_512.png` (512×512).  
**Feature graphic:** `store/play_feature_1024x500.png` (1024×500).  
**Скриншоты:** 2–8 телефонов, 16:9/9:16, без персональных данных.

## Data safety (форма Play)

| Тип данных | Собираем мы | Передаём | Назначение |
|---|---|---|---|
| Сообщения | Нет (только на устройстве) | Другим участникам группы | Функциональность, E2E |
| Имя (ник) | Нет | Участникам групп | Профиль |
| Фото (аватар) | Нет | Участникам групп | Профиль |
| Идентификатор устройства/ключа | Нет | Участникам групп | Функциональность |
| IP-адрес | Обрабатывается signaling/TURN | Нет | Соединение, безопасность |
| Поисковые запросы GIF | Нет | KLIPY (api.klipy.com) | Функциональность |
| Жалобы (ключи, ID, причина, комментарий, скриншот) | Да (на наш сервер) | Нет | Модерация UGC |
| События управления группой (владелец, heartbeat, заявки, голоса) | Нет (на устройствах) | Участникам группы | Функциональность |
| Фото, видео и аудио из сообщений | Нет (на устройствах) | Участникам группы | Функциональность |

Дополнительно:
- Шифрование в пути: да (TLS + MLS).
- Пользователь может удалить данные: да (удаление сообщений/выход из групп/очистка данных).
- Реклама/аналитика/трекинг: нет.
- Местоположение, контакты, финансы, здоровье: нет.
- Жалобы хранятся до 90 дней, только для разбора и блокировки ключа нарушителя.

**Terms:** https://dabuldakov.github.io/p2p-chat/terms.html (RU: /ru/terms.html)

**Report server:** `POST https://90.188.89.63.sslip.io:9443/report` — подпись Ed25519, лимиты, хранение 90 дней.

## App access (доступ для ревьюера)

Входа нет. Приложение P2P, нужны два устройства:
1. Установите приложение на два устройства (или два эмулятора).
2. На устройстве A нажмите «+» → введите название → «Создать».
3. Скопируйте инвайт (кнопка/QR) и передайте на устройство B.
4. На B: «+» → вставьте инвайт → «Войти по инвайту».
5. Отправьте сообщение — оно появится на обоих.

Если у ревьюера одно устройство, тест может быть невозможен — по возможности добавим демо-режим.

## Content rating

- Пользовательский контент (текст, изображения, GIF): да.
- Общение между пользователями: да.
- Модерация/жалобы/блокировка: **добавить** (см. шаг 3).
- Ожидаемая категория: Teen / 12+ (по опроснику).

## FGS declaration (Play Console)

Тип: `dataSync`. Обоснование: приложение поддерживает P2P-соединения и доставку сообщений, когда пользователь включил «Держать связь в фоне» (опционально, по умолчанию выключено). На Android 15 действует лимит 6 часов; при необходимости заменим на `specialUse` с пояснением.

## Чек-лист релиза

- [x] targetSdk/compileSdk 36
- [x] AAB собирается в CI (`p2pchat-aab`)
- [x] Upload key (release keystore) + Play App Signing
- [x] Privacy policy (EN/RU)
- [x] Иконка 512 и feature graphic
- [ ] Скриншоты
- [ ] Data safety заполнена
- [ ] Content rating
- [x] Жалобы (сервер /report + скриншот) + Terms
- [x] Блокировка пользователей (локальный blocklist)
- [ ] App access / review notes
- [ ] Закрытое тестирование: **12 тестеров × 14 дней** (личный аккаунт)
- [ ] Production

## Загрузка AAB

1. GitHub Actions → последний `android` run → artifact **`p2pchat-aab`** → скачать.
2. Play Console → Create app → загрузить `.aab` в Internal/Closed testing.
3. При первом AAB включить **Play App Signing** (upload key = наш release keystore).
