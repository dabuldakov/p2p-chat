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
 * Точка/цифра на иконке рисуется launcher'ом по активным уведомлениям канала
 * с `setShowBadge(true)`. Число задаём через `setNumber(N)` — сейчас это число
 * групп с непрочитанными. Постоянное уведомление foreground-службы живёт на
 * канале без бейджа, чтобы не раздувать счётчик.
 */
object Notifier {
    const val CH_MESSAGES = "p2pchat_messages"
    const val CH_SERVICE = "p2pchat_connection"
    private const val ID_MESSAGES = 1001

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
    }

    /** Показать/обновить сводное уведомление. `badge` — число на иконке. */
    fun showMessages(
        ctx: Context,
        title: String,
        text: String,
        badge: Int,
        group: String?,
        silent: Boolean,
    ) {
        val nm = NotificationManagerCompat.from(ctx)
        if (!nm.areNotificationsEnabled()) return
        val intent = Intent(ctx, MainActivity::class.java)
            .addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP)
        if (group != null) intent.putExtra(MainActivity.EXTRA_GROUP, group)
        val pi = PendingIntent.getActivity(
            ctx,
            0,
            intent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        val n = NotificationCompat.Builder(ctx, CH_MESSAGES)
            .setSmallIcon(R.drawable.ic_notify)
            .setContentTitle(title)
            .setContentText(text)
            .setNumber(badge)
            .setBadgeIconType(NotificationCompat.BADGE_ICON_SMALL)
            .setVisibility(NotificationCompat.VISIBILITY_PRIVATE)
            .setSilent(silent)
            .setAutoCancel(true)
            .setContentIntent(pi)
            .build()
        runCatching { nm.notify(ID_MESSAGES, n) }
    }

    fun clear(ctx: Context) {
        runCatching { NotificationManagerCompat.from(ctx).cancel(ID_MESSAGES) }
    }
}
