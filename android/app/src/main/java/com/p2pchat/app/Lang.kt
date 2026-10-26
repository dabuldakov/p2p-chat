package com.p2pchat.app

import androidx.core.content.edit
import android.content.Context
import android.content.res.Configuration
import java.util.Locale

/**
 * Выбор языка приложения. Хранится в SharedPreferences; применяется обёрткой
 * контекста ([wrap]) в `attachBaseContext` приложения и активити, поэтому
 * строки из ресурсов (`stringResource`/`getString`) сразу на нужном языке.
 */
object Lang {
    private const val PREFS = "p2pchat"
    private const val KEY = "lang_code"
    const val DEFAULT = "en"

    /** Код языка → флаг (эмодзи) и самоназвание. */
    val LANGUAGES: List<Triple<String, String, String>> = listOf(
        Triple("en", "🇬🇧", "English"),
        Triple("ru", "🇷🇺", "Русский"),
        Triple("es", "🇪🇸", "Español"),
        Triple("pt", "🇧🇷", "Português"),
        Triple("fr", "🇫🇷", "Français"),
        Triple("de", "🇩🇪", "Deutsch"),
        Triple("it", "🇮🇹", "Italiano"),
        Triple("zh", "🇨🇳", "中文"),
        Triple("ja", "🇯🇵", "日本語"),
        Triple("ko", "🇰🇷", "한국어"),
        Triple("ar", "🇸🇦", "العربية"),
        Triple("kk", "🇰🇿", "Қазақша"),
        Triple("mn", "🇲🇳", "Монгол"),
        Triple("uk", "🇺🇦", "Українська"),
        Triple("be", "🇧🇾", "Беларуская"),
    )

    fun get(ctx: Context): String =
        ctx.getSharedPreferences(PREFS, 0).getString(KEY, DEFAULT) ?: DEFAULT

    /** Контекст с применённой локалью (для attachBaseContext). */
    fun wrap(base: Context): Context {
        val loc = Locale.forLanguageTag(get(base))
        Locale.setDefault(loc)
        val cfg = Configuration(base.resources.configuration)
        cfg.setLocale(loc)
        return base.createConfigurationContext(cfg)
    }

    /** Сохранить и применить выбор (в текущем процессе). */
    fun set(ctx: Context, code: String) {
        ctx.getSharedPreferences(PREFS, 0).edit { putString(KEY, code) }
        val loc = Locale.forLanguageTag(code)
        Locale.setDefault(loc)
        val res = ctx.resources
        val cfg = Configuration(res.configuration)
        cfg.setLocale(loc)
        @Suppress("DEPRECATION")
        res.updateConfiguration(cfg, res.displayMetrics)
    }
}
