package com.p2pchat.app

import android.app.Application

/**
 * Централизованные логи. Важные события (открытие/обрыв соединения, ошибки)
 * пишутся всегда; подробная диагностика кадров/сигналинга — только когда
 * включён режим отладки.
 *
 * Режим отладки: всегда включён в debug-сборках ([BuildConfig.DEBUG]) и может
 * быть включён вручную в настройках (сохраняется в SharedPreferences).
 */
object Logs {
    private const val TAG = "p2pchat"
    private const val PREFS = "p2pchat"
    private const val KEY_DEBUG = "debug_logs"

    @Volatile
    var enabled: Boolean = false
        private set

    fun init(app: Application) {
        enabled = BuildConfig.DEBUG ||
            app.getSharedPreferences(PREFS, 0).getBoolean(KEY_DEBUG, false)
    }

    fun setEnabled(app: Application, value: Boolean) {
        enabled = value
        app.getSharedPreferences(PREFS, 0).edit().putBoolean(KEY_DEBUG, value).apply()
    }

    /** Подробная диагностика (только в режиме отладки). */
    fun d(msg: String) {
        if (enabled) android.util.Log.d(TAG, msg)
    }

    /** Значимые события (всегда). */
    fun i(msg: String) = android.util.Log.i(TAG, msg)

    /** Проблемы (всегда). */
    fun w(msg: String) = android.util.Log.w(TAG, msg)
}
