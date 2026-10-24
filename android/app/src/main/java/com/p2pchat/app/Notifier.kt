package com.p2pchat.app

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat

/**
 * Уведомления и бейдж на иконке.
 *
 * Постим по одному уведомлению на каждую непрочитанную **группу** и одно
 * сводное (group summary). Так число на иконке = числу непрочитанных групп
 * на всех launcher'ах: те, что считают уведомления (Realme/ColorOS и др.),
 * покажут количество уведомлений; Samsung, который уважает `setNumber`,
 * возьмёт число из сводного уведомления.
 *
 * Постоянное уведомление foreground-службы живёт на канале без бейджа.
 */
object Notifier {
    const val CH_MESSAGES = "p2pchat_messages"
    const val CH_SERVICE = "p2pchat_connection"
    const val CH_OWNER = "p2pchat_owner"
    private const val GROUP_KEY = "p2pchat"
    private const val ID_SUMMARY = 1000

    private val posted = mutableSetOf<Int>()
    private val ownerPosted = mutableSetOf<Int>()

    fun ensureChannels(ctx: Context) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        val mgr = ctx.getSystemService(NotificationManager::class.java) ?: return
        if (mgr.getNotificationChannel(CH_MESSAGES) == null) {
            mgr.createNotificationChannel(
                NotificationChannel(CH_MESSAGES, ctx.getString(R.string.messages_channel), NotificationManager.IMPORTANCE_DEFAULT).apply {
                    description = ctx.getString(R.string.messages_channel_desc)
                    setShowBadge(true)
                },
            )
        }
        if (mgr.getNotificationChannel(CH_SERVICE) == null) {
            mgr.createNotificationChannel(
                NotificationChannel(CH_SERVICE, ctx.getString(R.string.service_channel), NotificationManager.IMPORTANCE_LOW).apply {
                    setShowBadge(false)
                },
            )
        }
        if (mgr.getNotificationChannel(CH_OWNER) == null) {
            mgr.createNotificationChannel(
                NotificationChannel(CH_OWNER, ctx.getString(R.string.owner_channel), NotificationManager.IMPORTANCE_DEFAULT).apply {
                    description = ctx.getString(R.string.owner_channel_desc)
                    setShowBadge(false)
                },
            )
        }
    }

    /** Уведомление о выборах владельца (не входит в счётчик непрочитанных). */
    fun showOwnerElection(ctx: Context, groupHex: String, groupName: String, text: String) {
        val nm = NotificationManagerCompat.from(ctx)
        if (!nm.areNotificationsEnabled()) return
        val id = ("$GROUP_KEY-owner-$groupHex").hashCode()
        val n = NotificationCompat.Builder(ctx, CH_OWNER)
            .setSmallIcon(R.drawable.ic_notify)
            .setContentTitle(ctx.getString(R.string.owner_notification_title))
            .setContentText("$groupName: $text")
            .setSubText(groupName)
            .setVisibility(NotificationCompat.VISIBILITY_PRIVATE)
            .setAutoCancel(true)
            .setContentIntent(openIntent(ctx, groupHex, id))
            .build()
        runCatching { nm.notify(id, n) }
        ownerPosted.add(id)
    }

    fun clearOwnerElection(ctx: Context, groupHex: String) {
        val id = ("$GROUP_KEY-owner-$groupHex").hashCode()
        ownerPosted.remove(id)
        runCatching { NotificationManagerCompat.from(ctx).cancel(id) }
    }

    private fun openIntent(ctx: Context, group: String?, id: Int): PendingIntent {
        val intent = Intent(ctx, MainActivity::class.java)
            .addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP)
        if (group != null) intent.putExtra(MainActivity.EXTRA_GROUP, group)
        return PendingIntent.getActivity(
            ctx, id, intent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
    }

    /** Показать/обновить уведомления по группам. `items` — (группа, текст). */
    fun showGroups(ctx: Context, items: List<Pair<GroupInfo, String>>, silent: Boolean) {
        val nm = NotificationManagerCompat.from(ctx)
        if (!nm.areNotificationsEnabled()) return

        val newIds = mutableSetOf<Int>()
        for ((group, text) in items) {
            val groupHex = group.id.toHex()
            val id = (GROUP_KEY + groupHex).hashCode()
            newIds.add(id)
            val n = NotificationCompat.Builder(ctx, CH_MESSAGES)
                .setSmallIcon(R.drawable.ic_notify)
                .setContentTitle(group.name)
                .setContentText(text)
                .setGroup(GROUP_KEY)
                .setVisibility(NotificationCompat.VISIBILITY_PRIVATE)
                .setSilent(silent)
                .setAutoCancel(true)
                .setContentIntent(openIntent(ctx, groupHex, id))
                .build()
            runCatching { nm.notify(id, n) }
        }

        // сводное — задаёт число на иконке для launcher'ов, уважающих setNumber (Samsung)
        val summary = NotificationCompat.Builder(ctx, CH_MESSAGES)
            .setSmallIcon(R.drawable.ic_notify)
            .setContentTitle(ctx.getString(R.string.messages_channel))
            .setGroup(GROUP_KEY)
            .setGroupSummary(true)
            .setNumber(items.size)
            .setVisibility(NotificationCompat.VISIBILITY_PRIVATE)
            .setSilent(true)
            .setContentIntent(openIntent(ctx, null, ID_SUMMARY))
            .build()
        runCatching { nm.notify(ID_SUMMARY, summary) }

        // убираем уведомления групп, которые уже не непрочитаны
        for (old in posted - newIds) runCatching { nm.cancel(old) }
        posted.clear()
        posted.addAll(newIds)
    }

    fun clearAll(ctx: Context) {
        val nm = NotificationManagerCompat.from(ctx)
        for (id in posted) runCatching { nm.cancel(id) }
        posted.clear()
        runCatching { nm.cancel(ID_SUMMARY) }
    }
}
