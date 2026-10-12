package com.nikgob.clockmanage

import android.app.PendingIntent
import android.appwidget.AppWidgetManager
import android.appwidget.AppWidgetProvider
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.os.SystemClock
import android.view.View
import android.widget.RemoteViews

/**
 * Home-screen widget: what runs and how long is left. The countdown is a system Chronometer,
 * so the app is not woken to draw it; the widget is redrawn only when the phase changes
 * (the same moments the single exact alarm fires) and after a sync.
 */
class TimerWidget : AppWidgetProvider() {
    override fun onUpdate(ctx: Context, mgr: AppWidgetManager, ids: IntArray) {
        update(ctx)
    }

    companion object {
        fun update(ctx: Context) {
            val mgr = AppWidgetManager.getInstance(ctx)
            val ids = mgr.getAppWidgetIds(ComponentName(ctx, TimerWidget::class.java))
            if (ids.isEmpty()) return
            mgr.updateAppWidget(ids, views(ctx))
        }

        private fun views(ctx: Context): RemoteViews {
            val v = RemoteViews(ctx.packageName, R.layout.widget)
            val open = PendingIntent.getActivity(ctx, 1, Intent(ctx, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE)
            v.setOnClickPendingIntent(R.id.w_root, open)
            val s = Store(ctx)
            val snap = s.snapshot()
            val now = s.now()
            val e = snap?.entryAt(now)

            // Chronometer base is on the elapsed-realtime clock: "this many ms from now".
            fun countdown(leftMs: Long, running: Boolean) {
                v.setViewVisibility(R.id.w_chrono, View.VISIBLE)
                v.setViewVisibility(R.id.w_big, View.GONE)
                v.setChronometer(R.id.w_chrono, SystemClock.elapsedRealtime() + leftMs, null, running)
                v.setChronometerCountDown(R.id.w_chrono, true)
            }
            fun countup(elapsedMs: Long) {
                v.setViewVisibility(R.id.w_chrono, View.VISIBLE)
                v.setViewVisibility(R.id.w_big, View.GONE)
                v.setChronometer(R.id.w_chrono, SystemClock.elapsedRealtime() - elapsedMs, null, true)
                v.setChronometerCountDown(R.id.w_chrono, false)
            }
            fun big(text: String) {
                v.setViewVisibility(R.id.w_chrono, View.GONE)
                v.setViewVisibility(R.id.w_big, View.VISIBLE)
                v.setTextViewText(R.id.w_big, text)
                v.setChronometer(R.id.w_chrono, SystemClock.elapsedRealtime(), null, false)
            }

            when {
                !s.paired -> { big("—"); v.setTextViewText(R.id.w_state, ""); v.setTextViewText(R.id.w_title, "ClockManage"); v.setTextViewText(R.id.w_sub, tr("android_widget_pair_with_a_pc")) }
                e == null -> { big("—"); v.setTextViewText(R.id.w_state, ""); v.setTextViewText(R.id.w_title, tr("android_main_waiting_for_data_from_the_pc")); v.setTextViewText(R.id.w_sub, "") }
                else -> {
                    when {
                        e.paused -> countdown(e.remaining(now), false)
                        e.running -> countdown(e.until!! - now, true)
                        e.segment && e.stopwatch -> countup(e.elapsed(now))
                        e.segment -> countdown(e.segmentEnd - now, true) // negative after the planned end
                        e.kind == "await" -> big(tr("android_widget_time"))
                        e.kind == "done" -> big("✓")
                        e.kind == "idle" -> big(Fmt.dur(snap.blocks.filter { !it.isBreak }.sumOf { it.minutes } * Snapshot.MIN))
                        else -> big("")
                    }
                    v.setTextViewText(R.id.w_state, when {
                        e.paused -> tr("today_paused")
                        e.kind == "work" -> tr("log_work")
                        e.kind == "break" || e.kind == "lunch_break" -> tr("log_break")
                        e.segment && e.prep -> tr("android_widget_getting_ready_for_bed")
                        e.segment && e.stopwatch -> tr("mini_untimed")
                        e.segment -> if (e.alarm) tr("android_widget_nap") else tr("today_segment")
                        e.kind == "await" -> tr("android_widget_break_over")
                        e.kind == "done" -> tr("android_widget_day_closed")
                        e.kind == "idle" -> tr("today_planned")
                        else -> ""
                    })
                    v.setTextViewText(R.id.w_title, e.title)
                    v.setTextViewText(R.id.w_sub, e.subtitle.ifEmpty { if (snap.blockedAt(now, s.syncedAt)) tr("app_tray_lock_on") else "" })
                }
            }
            return v
        }
    }
}
