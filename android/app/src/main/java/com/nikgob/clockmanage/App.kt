package com.nikgob.clockmanage

import android.app.Application

/** Runs first in every process start (the screen, the blocker, alarms, the widget): the texts follow the paired PC's language from the start. */
class App : Application() {
    override fun onCreate() {
        super.onCreate()
        I18n.pc = Store(this).lang
    }
}
