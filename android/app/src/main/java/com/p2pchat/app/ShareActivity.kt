package com.p2pchat.app

import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity

/**
 * Промежуточная активность для системного меню «Поделиться» (ACTION_SEND).
 *
 * Существует, чтобы приложение появилось в шите share. Ничего не показывает:
 * вытаскивает текст и сразу передаёт его в [MainActivity], после чего закрывается.
 * Дальше пользователь выбирает группу — и ссылка попадает в поле ввода.
 */
class ShareActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        val sharedText = intent
            ?.takeIf { it.action == Intent.ACTION_SEND }
            ?.getStringExtra(Intent.EXTRA_TEXT)
            ?.trim()
            ?.takeIf { it.isNotEmpty() }

        val target = Intent(this, MainActivity::class.java).apply {
            addFlags(Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_SINGLE_TOP)
            if (sharedText != null) putExtra(MainActivity.EXTRA_SHARED_TEXT, sharedText)
        }
        startActivity(target)
        finish()
    }
}
