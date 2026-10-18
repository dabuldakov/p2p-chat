#!/usr/bin/env python3
# Генератор многоязычных страниц сайта.
# Источник структуры — docs/_ru.html (русский). Для каждого языка строки
# заменяются переводами из TRANS. Пишет docs/index.html (английский, корень)
# и docs/<lang>/index.html.
import os, re, sys

DOCS = os.path.dirname(os.path.abspath(__file__))
SRC = os.path.join(DOCS, "_ru.html")

ALL = [
    ("en", "🇬🇧", "English"), ("ru", "🇷🇺", "Русский"), ("es", "🇪🇸", "Español"),
    ("pt", "🇧🇷", "Português"), ("fr", "🇫🇷", "Français"), ("de", "🇩🇪", "Deutsch"),
    ("it", "🇮🇹", "Italiano"), ("zh", "🇨🇳", "中文"), ("ja", "🇯🇵", "日本語"),
    ("ko", "🇰🇷", "한국어"), ("ar", "🇸🇦", "العربية"), ("kk", "🇰🇿", "Қазақша"),
    ("mn", "🇲🇳", "Монгол"), ("uk", "🇺🇦", "Українська"), ("be", "🇧🇾", "Беларуская"),
]

# Языки, для которых уже есть переводы (панель ссылается только на них).
IMPLEMENTED = ["en", "ru"]

# RU → English
EN = {
 "p2pchat — чат без сервера": "p2pchat — chat without a server",
 "Как работает": "How it works", "Как начать": "Getting started", "Группы": "Groups",
 "Шифрование": "Encryption", "Подробности": "Details", "Приватность": "Privacy", "Вопросы": "FAQ",
 "Чат между телефонами — напрямую": "Chat between phones — directly",
 "p2pchat соединяет устройства через интернет без центрального сервера для сообщений. Создавайте несколько групп и приглашайте участников по QR или ссылке. Сервер нужен только на несколько секунд — помочь телефонам найти друг друга.":
   "p2pchat connects devices over the internet without a central server for messages. Create several groups and invite members via QR or a link. The server is needed only for a few seconds — to help the phones find each other.",
 "Скачать APK (Android)": "Download APK (Android)", "Все сборки": "All builds",
 "Последняя сборка публикуется автоматически · Android 8.0+ · открытый исходный код":
   "The latest build is published automatically · Android 8.0+ · open source",
 "версия: загрузка…": "version: loading…", "что нового": "what's new",
 "Два телефона соединяются друг с другом напрямую. Посредник участвует только в «знакомстве».":
   "Two phones connect to each other directly. The middleman takes part only in the “introduction”.",
 "Схема: два телефона и сервер знакомства": "Diagram: two phones and the introduction server",
 "Телефон A": "Phone A", "Телефон B": "Phone B", "сообщения идут напрямую": "messages go directly",
 "Сервер знакомства": "Introduction server", "обмен адресами, пара секунд": "address exchange, a couple of seconds",
 "только установка связи": "connection setup only", "Ретранслятор (TURN)": "Relay (TURN)",
 "запасной путь, если сеть мешает": "fallback path if the network interferes",
 "прямое соединение (основной путь)": "direct connection (main path)",
 "сервер знакомства (не видит сообщений)": "introduction server (cannot see messages)",
 "релей TURN (только при сложной сети)": "TURN relay (only for tricky networks)",
 "Нужен Android-телефон и интернет. Регистрация не нужна — можно с любого числа участников.":
   "You need an Android phone and internet. No registration — any number of participants.",
 "Скачайте и установите приложение": "Download and install the app",
 "Нажмите «Скачать APK» и установите. Если система предупредит — разрешите установку из этого источника.":
   "Tap “Download APK” and install. If the system warns you — allow installation from this source.",
 "Создайте группу": "Create a group",
 "Кнопка «+» внизу → введите название → «Создать». Приложение покажет приглашение (QR и ссылку).":
   "Tap “+” at the bottom → enter a name → “Create”. The app will show an invite (QR and link).",
 "Пригласите участников": "Invite members",
 "Покажите QR или отправьте ссылку-инвайт. Собеседник нажимает «+» → «Войти по инвайту» и сканирует/вставляет её.":
   "Show the QR or send the invite link. The other person taps “+” → “Join by invite” and scans/pastes it.",
 "Общайтесь": "Chat",
 "Пишите сообщения — они идут напрямую между устройствами и зашифрованы. Можно вести несколько групп; свой идентификатор и QR — в «Настройках».":
   "Write messages — they go directly between devices and are encrypted. You can run several groups; your identifier and QR are in Settings.",
 "Путь сообщения": "Message path",
 "Обычно напряжение уходит после нескольких шагов — дальше сервер не участвует.":
   "Usually the tension goes away after a few steps — after that the server is not involved.",
 "Поток установки соединения": "Connection setup flow",
 "Оба онлайн": "Both online", "на сервере знакомства": "on the introduction server",
 "Обмен ключами": "Key exchange", "и адресами": "and addresses",
 "Прямое соединение": "Direct connection", "через STUN": "via STUN",
 "Если не вышло —": "If it fails —", "через TURN": "via TURN",
 "Чат напрямую": "Direct chat", "сервер не участвует": "server not involved",
 "Как ходят сообщения в группе на 30 человек": "How messages flow in a 30-person group",
 "Никто не рассылает всем сразу и не подключается ко всем. Каждый связан с несколькими соседями, а сообщение расходится по сети как слух — и останавливается, как только его увидели все.":
   "Nobody broadcasts to everyone at once or connects to all. Each is linked to a few neighbours, and a message spreads across the network like a rumour — stopping as soon as everyone has seen it.",
 "Распространение сообщения по сети из 30 участников": "Message spreading across a network of 30 participants",
 "хранит и отдаст позже": "stores and delivers later", "вы": "you",
 "1-й прыжок": "hop 1", "2-й прыжок": "hop 2", "3-й прыжок": "hop 3",
 "30 участников · ~4–5 связей у каждого · ~70 соединений вместо 435":
   "30 participants · ~4–5 links each · ~70 connections instead of 435",
 "отправитель": "sender", "1-й прыжок (прямые соседи)": "hop 1 (direct neighbours)",
 "офлайн — получит позже": "offline — will get it later",
 "Никакого «всем сразу»": "No “everyone at once”",
 "30 человек не держат 435 соединений. Каждый связан с 4–5 соседями — всего около 70 каналов. Сообщение доходит за 3–4 «прыжка» за доли секунды.":
   "30 people don’t hold 435 connections. Each is linked to 4–5 neighbours — about 70 links total. A message arrives in 3–4 “hops” in a fraction of a second.",
 "Дубли отсекаются": "Duplicates are dropped",
 "У каждого сообщения есть уникальный id. Кто увидел его повторно — дальше не пересылает. Поэтому лавина не зацикливается и не затапливает сеть.":
   "Every message has a unique id. Whoever sees it again does not forward it. So the flood never loops and never swamps the network.",
 "Офлайн не теряется": "Offline is not lost",
 "Онлайн-соседи хранят сообщения для тех, кто ушёл. Вернувшись, участник сверяется по «дереву контрольных сумм» (Merkle) и добирает только пропущенное.":
   "Online neighbours store messages for those who left. On return, a participant reconciles against a Merkle tree and fetches only what was missed.",
 "Сложные сети": "Tricky networks",
 "Если оператор связи мешает прямому соединению с соседом, на помощь приходит ретранслятор TURN — путь становится длиннее, но сообщение доходит.":
   "If the carrier blocks a direct connection to a neighbour, a TURN relay helps — the path gets longer, but the message gets through.",
 "Шифрование и рукопожатия": "Encryption and handshakes",
 "Групповое шифрование — по протоколу": "Group encryption uses the protocol",
 "MLS (RFC 9420)": "MLS (RFC 9420)",
 ": forward secrecy и post-compromise security. Ниже — что у кого за ключи и все «рукопожатия» по шагам, с указанием задействованных сторон.":
   ": forward secrecy and post-compromise security. Below — who holds which keys and all the “handshakes” step by step, with the parties involved.",
 "Ключи и роли": "Keys and roles",
 "Ключи: аккаунт, устройство, MLS-группа, сервер": "Keys: account, device, MLS group, server",
 "Аккаунт — ключ личности (Ed25519)": "Account — identity key (Ed25519)",
 "Устройство — ключ подписи": "Device — signing key",
 "Секреты эпохи MLS": "MLS epoch secrets",
 "Локальный лог сообщений": "Local message log",
 "общий секрет группы (MLS)": "shared group secret (MLS)",
 "меняется при входе/выходе": "changes on join/leave",
 "публичные ключи, адреса, SDP/ICE": "public keys, addresses, SDP/ICE",
 "контента не видит": "cannot see the content",
 "У каждого — своя личность (подпись Ed25519). Секрет группы — общий, это": "Each has its own identity (Ed25519 signature). The group secret is shared — it is",
 "состояние MLS": "the MLS state",
 ", которое обновляется при каждой смене состава. Сервер видит только публичную часть.":
   ", which updates on every membership change. The server sees only the public part.",
 "1. Создание группы": "1. Creating a group",
 "Задействована:": "Involved:", "только сторона-создатель": "only the creator", ". Сервер не участвует.": ". The server is not involved.",
 "Создание группы локально": "Creating a group locally",
 "Создатель (A)": "Creator (A)", "генерирует id и эпоху 0": "generates an id and epoch 0",
 "MLS: создать группу": "MLS: create group", "Локальное состояние": "Local state",
 "ключи эпохи 0 · мы — владелец": "epoch-0 keys · we are the owner",
 "сервер не задействован": "server not involved",
 "2. Впуск нового участника (по одноразовому билету)": "2. Admitting a new member (via a one-time ticket)",
 "Задействованы:": "Involved:", "хозяин": "host", "и": "and", "новичок": "newcomer",
 ". Ключ группы в ссылке": ". The group key in the link", "не передаётся": "is not transmitted",
 "— только подписанный билет.": "— only a signed ticket.",
 "Впуск: заявка и Welcome попарно": "Admission: request and Welcome, pairwise",
 "Хозяин (A)": "Host (A)", "Новичок (B)": "Newcomer (B)", "создаёт билет": "creates a ticket",
 "nonce · срок · подпись": "nonce · expiry · signature",
 "ссылка/QR с билетом (вне канала)": "link/QR with the ticket (out of band)",
 "заявка: KeyPackage + билет — попарно (X25519)": "request: KeyPackage + ticket — pairwise (X25519)",
 "проверяет билет": "validates the ticket", "подпись · срок · не использован": "signature · expiry · unused",
 "новая эпоха MLS": "new MLS epoch", "Welcome — попарно": "Welcome — pairwise",
 "входит в группу": "joins the group", "та же эпоха MLS": "same MLS epoch",
 "дальше — сообщения по группе": "then — group messages",
 "сообщения/MLS-трафик": "messages/MLS traffic", "попарно (X25519)": "pairwise (X25519)",
 "вне канала (ссылка/QR)": "out of band (link/QR)",
 "3. Отправка сообщения": "3. Sending a message", "получатели": "recipients", "и (опционально)": "and (optionally)", "релей": "relay",
 ", который видит только шифртекст.": ", which sees only ciphertext.",
 "Путь сообщения: шифрование, релей, расшифровка": "Message path: encryption, relay, decryption",
 "Отправитель": "Sender", "Релей (если нужен)": "Relay (if needed)", "Получатель": "Recipient",
 "шифрует в MLS-пакет": "encrypts into an MLS packet", "ключом эпохи": "with the epoch key",
 "шифртекст": "ciphertext", "пересылает как есть": "forwards as is", "расшифровывает": "decrypts",
 "релей не может прочитать": "the relay cannot read it",
 "4. Удаление участника": "4. Removing a member",
 "владелец": "owner", "оставшиеся": "the rest", ". Удалённый ключей новой эпохи не получает.": ". The removed member gets no new-epoch keys.",
 "Удаление участника: Remove и Commit, forward secrecy": "Removing a member: Remove and Commit, forward secrecy",
 "Владелец (A)": "Owner (A)", "Оставшиеся": "The rest", "эпоха +1": "epoch +1",
 "Commit — новой эпохи": "Commit — of the new epoch", "применяют Commit": "apply the Commit",
 "читают новые сообщения": "read new messages",
 "Удалённый (C) не получает ключей новой эпохи → не читает новые сообщения (forward secrecy)":
   "The removed member (C) gets no new-epoch keys → cannot read new messages (forward secrecy)",
 "Что даёт MLS.": "What MLS gives.",
 "Каждая смена состава и смена ключей сдвигает эпоху: старые секреты удаляются (forward secrecy), после ротации система «заживает» даже при утечке ключа (post-compromise security). Кто в группе — не подделать: handshake-сообщения подписаны участниками. Сервер и релей видят только шифртекст.":
   "Every membership or key change advances the epoch: old secrets are deleted (forward secrecy), and after rotation the system “heals” even if a key leaks (post-compromise security). Membership cannot be forged: handshake messages are signed by the participants. The server and relay see only ciphertext.",
 "Приватность и безопасность": "Privacy and security",
 "Сервер не читает переписку": "The server does not read the conversation",
 "Через него проходит только техническая информация для соединения. Сообщения идут между телефонами.":
   "Only technical information for connection passes through it. Messages go between the phones.",
 "Сообщения зашифрованы групповым ключом — он есть только у участников. При выходе или удалении участника ключ меняется (rekey), ключ доставляется оставшимся лично, а вышедший новые сообщения прочитать уже не может.":
   "Messages are encrypted with the group key — only members hold it. When a member leaves or is removed, the key changes (rekey), it is delivered to the remaining members personally, and the leaver can no longer read new messages.",
 "Управление группами": "Group management",
 "Несколько чатов, приглашение по QR/ссылке, выход из группы. Владелец может удалить участника — и оставшиеся сразу меняют ключ, так что удалённый больше не читает новые сообщения.":
   "Several chats, invites via QR/link, leaving a group. The owner can remove a member — and the rest immediately change the key, so the removed member can no longer read new messages.",
 "Без регистрации": "No registration",
 "Ваш «аккаунт» — это ключ устройства. Ни телефона, ни почты, ни пароля.":
   "Your “account” is the device key. No phone, no email, no password.",
 "Открытый код": "Open source",
 "Исходники доступны — можно проверить, как всё работает, и собрать самому.":
   "The source is available — you can check how it works and build it yourself.",
 "Связь устанавливается по защищённому": "The connection is established over a secure",
 "wss://": "wss://",
 "(TLS, сертификат Let's Encrypt). TURN-креды выдаются приложению во время работы (TURN REST API) и живут всего час — постоянных паролей в приложении нет.":
   "(TLS, Let's Encrypt certificate). TURN credentials are issued to the app at runtime (TURN REST API) and live for just an hour — there are no permanent passwords in the app.",
 "Подробности и примеры": "Details and examples",
 "Как это устроено под капотом и как этим пользоваться в жизни.":
   "How it works under the hood and how to use it in real life.",
 "Что происходит при отправке сообщения": "What happens when a message is sent",
 "Сообщение шифруется": "The message is encrypted",
 "Текст зашифровывается групповым ключом (ChaCha20 + Poly1305). Ключ есть только у участников чата.":
   "The text is encrypted with the group key (ChaCha20 + Poly1305). Only chat members hold the key.",
 "Подписывается": "It is signed",
 "У каждого устройства свой ключ подписи (Ed25519) — получатель точно знает, кто автор, и в сети никто не подделает сообщение.":
   "Each device has its own signing key (Ed25519) — the recipient knows exactly who the author is, and nobody on the network can forge the message.",
 "Уходит напрямую": "It goes directly",
 "Через открытый WebRTC-канал данные идут прямо собеседнику. В группе — расходится по сети «как слух» (gossip), каждый пересылает соседям.":
   "Over an open WebRTC channel data goes straight to the peer. In a group it spreads across the network “like a rumour” (gossip), each forwarding to neighbours.",
 "Складывается в общий лог": "It is merged into a shared log",
 "У всех участников получается одинаковый порядок сообщений (CRDT), даже если кто-то был офлайн и догнал позже.":
   "All participants end up with the same message order (CRDT), even if someone was offline and caught up later.",
 "Пример: подключить собеседника": "Example: connecting a peer",
 "Через QR-код": "Via QR code",
 "Откройте приложение — на экране ваш ключ и QR. Собеседник нажимает «Подключиться» и сканирует код. Оба оказываются в одном чате.":
   "Open the app — your key and QR are on screen. The peer taps “Connect” and scans the code. Both end up in the same chat.",
 "Вручную": "Manually",
 "Нажмите «Скопировать ключ» и передайте его любым способом (мессенджер, вслух). Второй человек вставляет ключ и жмёт Connect.":
   "Tap “Copy key” and pass it on any way (messenger, out loud). The second person pastes the key and taps Connect.",
 "Группа": "Group",
 "Один создаёт группу и получает групповой ключ для раздачи. Остальные вступают по нему — дальше сообщения расходятся по всей группе.":
   "One person creates a group and gets a group key to share. Others join with it — then messages spread across the whole group.",
 "Куда идут данные": "Where the data goes",
 "Сообщения": "Messages",
 "Напрямую между телефонами. Ни на одном сервере не хранятся.":
   "Directly between phones. Not stored on any server.",
 "Видит только «кто с кем соединяется» секунду при установке связи. Переписку не читает.":
   "Sees only “who connects to whom” for a second during connection setup. It does not read the conversation.",
 "TURN": "TURN",
 "Ретранслятор только если оператор мешает прямому соединению. Через него идёт шифртекст.":
   "A relay only if the carrier blocks a direct connection. Only ciphertext passes through it.",
 "Ключи": "Keys",
 "Приватные ключи устройства не покидают телефон. Наружу уходят только открытые.":
   "The device’s private keys never leave the phone. Only public keys go out.",
 "Обновления": "Updates",
 "Приложение само проверяет новые версии: на шестерёнке настроек появится красная точка — внутри можно посмотреть, что нового, и скачать обновление.":
   "The app checks for new versions itself: a red dot appears on the settings gear — inside you can see what's new and download the update.",
 "Технологии": "Tech",
 "Частые вопросы": "FAQ",
 "Нужен ли сервер?": "Is a server needed?",
 "Для самих сообщений — нет. Каждый участник сам является узлом: хранит свою копию переписки и пересылает её соседям. Централизован только «сервер знакомства» (пара секунд при подключении) и ретранслятор — если оператор связи мешает прямому соединению.":
   "For the messages themselves — no. Each participant is a node: it keeps its own copy of the conversation and forwards it to neighbours. Only the “introduction server” (a couple of seconds at connect) and the relay are centralised — if the carrier blocks a direct connection.",
 "Кто может прочитать мои сообщения?": "Who can read my messages?",
 "Только участники чата, у которых есть групповой ключ.": "Only chat members who hold the group key.",
 "Работает ли в мобильном интернете (LTE)?": "Does it work on mobile internet (LTE)?",
 "Да. Операторы часто «прячут» устройства за общим адресом; тогда соединение идёт через ретранслятор TURN.":
   "Yes. Carriers often “hide” devices behind a shared address; then the connection goes through a TURN relay.",
 "Можно ли в группы больше двух человек?": "Can groups have more than two people?",
 "Ядро поддерживает группы; связь между участниками строится по принципу «каждый с соседями» и сообщения распространяются по сети.":
   "The core supports groups; peers link “each with neighbours” and messages spread across the network.",
 "Есть ли версия для iOS?": "Is there an iOS version?",
 "Пока только Android. Ядро кроссплатформенное, iOS — в планах.":
   "Android only for now. The core is cross-platform; iOS is planned.",
 "p2pchat · децентрализованный чат": "p2pchat · decentralized chat",
 "Исходный код": "Source code", "Последняя сборка": "Latest build", "Сборки CI": "CI builds",
 "p2pchat — децентрализованный чат между телефонами напрямую, без центрального сервера для сообщений.":
   "p2pchat — a decentralized chat between phones directly, without a central server for messages.",
 "Версия последней сборки": "Version of the latest build",
 "Как это работает": "How it works",
 "Rust-ядро": "Rust core", "CRDT-лог": "CRDT log", "Merkle-синк": "Merkle sync",
 "версия:": "version:", "версия: н/д": "version: n/a", "н/д": "n/a", "МБ": "MB",
 "Сборка от": "Build of", "открытый исходный код": "open source",
 "децентрализованный чат": "decentralized chat",
}

def apply(html, table):
    # длинные строки раньше, чтобы не резать короткими
    for ru in sorted(table, key=len, reverse=True):
        en = table[ru]
        parts = re.split(r"\s+", ru.strip())
        body = r"\s+".join(re.escape(p) for p in parts)
        pat = re.compile(r"(?<![А-Яа-яЁё])" + body + r"(?![А-Яа-яЁё])")
        html = pat.sub(lambda m: en, html)
    return html

def switcher(lang):
    root = "" if lang == "en" else "../"
    items = []
    for code, flag, name in ALL:
        if code not in IMPLEMENTED:
            continue
        href = root + ("" if code == "en" else code + "/")
        cls = ' class="cur"' if code == lang else ""
        items.append(f'<a href="{href}" title="{name}"{cls}>{flag}</a>')
    return '<div class="langs">' + "".join(items) + "</div>"

def render(html, lang):
    html = html.replace("&nbsp;", " ")
    if lang != "ru":
        html = apply(html, EN)
    html = html.replace('<html lang="ru">', f'<html lang="{lang}">')
    # языковая панель + стиль
    if ".langs{" not in html:
        html = html.replace("</style>",
            ".langs{display:flex; gap:6px; flex-wrap:wrap; align-items:center}\n"
            ".langs a{font-size:16px; line-height:1; text-decoration:none; opacity:.55}\n"
            ".langs a:hover{opacity:1}\n"
            ".langs a.cur{opacity:1}\n</style>")
    html = html.replace("</nav>", "</nav>\n      " + switcher(lang))
    return html

def main():
    src = open(SRC, encoding="utf-8").read()
    # корень — английский
    open(os.path.join(DOCS, "index.html"), "w", encoding="utf-8").write(render(src, "en"))
    for code in IMPLEMENTED:
        if code == "en":
            continue
        d = os.path.join(DOCS, code)
        os.makedirs(d, exist_ok=True)
        open(os.path.join(d, "index.html"), "w", encoding="utf-8").write(render(src, code))
    print("generated:", ", ".join(IMPLEMENTED))
if __name__ == "__main__":
    main()
