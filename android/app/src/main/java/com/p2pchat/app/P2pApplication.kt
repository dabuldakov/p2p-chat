package com.p2pchat.app

import android.app.Application
import com.p2pchat.app.gif.CoilSetup
import com.p2pchat.app.gif.Media3Cache

/** Точка входа приложения: инициализирует общепроцессный рантайм чата. */
class P2pApplication : Application() {
    override fun onCreate() {
        super.onCreate()
        ChatRuntime.init(this)
        CoilSetup.install(this)
        Media3Cache.install(this)
    }
}
