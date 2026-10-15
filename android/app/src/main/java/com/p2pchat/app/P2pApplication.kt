package com.p2pchat.app

import android.app.Application

/** Точка входа приложения: инициализирует общепроцессный рантайм чата. */
class P2pApplication : Application() {
    override fun onCreate() {
        super.onCreate()
        ChatRuntime.init(this)
    }
}