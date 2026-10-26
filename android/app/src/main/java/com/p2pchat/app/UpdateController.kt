package com.p2pchat.app

import android.app.Application
import android.content.Intent
import android.net.Uri
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

internal fun shouldCheckUpdate(now: Long, lastCheck: Long, checkFailed: Boolean, force: Boolean): Boolean {
    if (force) return true
    val interval = if (checkFailed) 5 * 60_000L else 30 * 60_000L
    return lastCheck == 0L || now - lastCheck >= interval
}

internal class UpdateController(
    private val app: Application,
    private val scope: CoroutineScope,
    private val installedVersion: () -> String,
) {
    var update by mutableStateOf<AppUpdate?>(null)
        private set
    var checkFailed by mutableStateOf(false)
        private set
    private var lastCheck = 0L

    fun check(force: Boolean = false) {
        if (BuildConfig.PLAY_BUILD) return
        val now = System.currentTimeMillis()
        if (!shouldCheckUpdate(now, lastCheck, checkFailed, force)) return
        lastCheck = now
        Logs.i("update: checking installed=${installedVersion()}")
        scope.launch {
            val result = withContext(Dispatchers.IO) { UpdateChecker.check(installedVersion()) }
            apply(result)
            Logs.i("update: installed=${installedVersion()} ok=${result.ok} latest=${result.update?.version}")
        }
    }

    fun checkNow(onDone: (Boolean) -> Unit) {
        if (BuildConfig.PLAY_BUILD) {
            openPlay()
            onDone(true)
            return
        }
        lastCheck = System.currentTimeMillis()
        scope.launch {
            val result = withContext(Dispatchers.IO) { UpdateChecker.check(installedVersion()) }
            apply(result)
            Logs.i("update(now): installed=${installedVersion()} ok=${result.ok} latest=${result.update?.version}")
            onDone(result.update != null)
        }
    }

    private fun apply(result: UpdateCheck) {
        if (result.ok) {
            update = result.update
            checkFailed = false
        } else {
            checkFailed = true
        }
    }

    private fun openPlay() {
        val id = app.packageName
        val market = Intent(Intent.ACTION_VIEW, Uri.parse("market://details?id=$id"))
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        val web = Intent(Intent.ACTION_VIEW, Uri.parse("https://play.google.com/store/apps/details?id=$id"))
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        runCatching { app.startActivity(market) }.onFailure { runCatching { app.startActivity(web) } }
    }
}
