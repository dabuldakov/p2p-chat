package com.p2pchat.app

import android.app.Notification
import android.app.Service
import android.content.Context
import android.content.Intent
import android.os.IBinder
import androidx.core.app.NotificationCompat
import androidx.core.content.ContextCompat

/**
 * Foreground-служба: держит процесс приложения живым в фоне, чтобы транспорт
 * (WebRTC/signaling) не рвался и сообщения приходили сразу. Показывает
 * постоянное уведомление. Включается по желанию пользователя.
 */
class ChatService : Service() {

    companion object {
        private const val NOTIF_ID = 1

        fun start(ctx: Context) {
            ContextCompat.startForegroundService(ctx, Intent(ctx, ChatService::class.java))
        }

        fun stop(ctx: Context) {
            ctx.stopService(Intent(ctx, ChatService::class.java))
        }
    }

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()

        // Каналы (сервис — без бейджа, чтобы не влиять на точку/цифру).
        Notifier.ensureChannels(this)

        val notification: Notification = NotificationCompat.Builder(this, Notifier.CH_SERVICE)
            .setContentTitle("p2pchat")
            .setContentText(getString(R.string.service_text))
            .setSmallIcon(android.R.drawable.stat_notify_sync)
            .setOngoing(true)
            .setPriority(NotificationCompat.PRIORITY_LOW)
            .build()

        startForeground(NOTIF_ID, notification)
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int = START_STICKY

    // Android 15+: у dataSync FGS есть лимит времени в фоне — корректно останавливаемся.
    override fun onTimeout(startId: Int) {
        Logs.w("ChatService: timeout (startId=$startId) — останавливаемся")
        stopSelf()
    }

    override fun onTimeout(startId: Int, fgsType: Int) {
        Logs.w("ChatService: timeout (startId=$startId, type=$fgsType) — останавливаемся")
        stopSelf()
    }
}