package com.p2pchat.app

/** Группы, по которым есть непрочитанные (в порядке списка). */
internal fun unreadGroups(groups: List<GroupInfo>, unread: Map<String, ULong>): List<GroupInfo> =
    groups.filter { (unread[it.id.toHex()] ?: 0uL) > 0uL }

/** Текст уведомления: последнее сообщение или запасной текст. */
internal fun notificationText(lastMessage: String?, fallback: String): String =
    lastMessage?.takeIf { it.isNotBlank() } ?: fallback

/** Помечать прочитанным только когда экран группы открыт и приложение видно. */
internal fun shouldMarkRead(appVisible: Boolean, groupOpen: Boolean): Boolean =
    appVisible && groupOpen
