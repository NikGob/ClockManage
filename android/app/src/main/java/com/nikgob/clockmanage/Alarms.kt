package com.nikgob.clockmanage

import android.app.AlarmManager
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.media.AudioAttributes
import android.media.RingtoneManager
import android.os.Build

/**
 * Phase transitions without polling: one exact alarm at the next transition of the projected
 * timeline. The countdown in the shade is the system chronometer — it costs nothing to run.
 */
object Alarms {
    private const val CH_STATUS = "status"
    private const val CH_ALARM = "alarm"
    private const val CH_WAKE = "wake"
    private const val ID_STATUS = 1
    private const val ID_ALARM = 2
    private const val ID_WAKE = 3
    /** While the nap alarm rings, check with the PC this often (it may have been ended there). */
    private const val WAKE_RECHECK_MS = 30_000L
    /** Over-time reminder for other segments. */
    private const val OVERRUN_MS = 5 * 60_000L

    fun channels(ctx: Context) {
        val nm = ctx.getSystemService(NotificationManager::class.java)
        nm.createNotificationChannel(
            NotificationChannel(CH_STATUS, "Таймер", NotificationManager.IMPORTANCE_LOW).apply {
                description = "Что идёт сейчас и сколько осталось"
                setShowBadge(false)
            },
        )
        nm.createNotificationChannel(
            NotificationChannel(CH_ALARM, "Конец перерыва", NotificationManager.IMPORTANCE_HIGH).apply {
                description = "Перерыв окончен — пора начинать следующую часть"
                setSound(
                    RingtoneManager.getDefaultUri(RingtoneManager.TYPE_ALARM),
                    AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_ALARM).build(),
                )
                enableVibration(true)
            },
        )
        nm.createNotificationChannel(
            NotificationChannel(CH_WAKE, "Будильник сна", NotificationManager.IMPORTANCE_HIGH).apply {
                description = "Конец отрезка «Сон»: звонит, пока не нажмёшь «Встал»"
                setSound(
                    RingtoneManager.getDefaultUri(RingtoneManager.TYPE_ALARM),
                    AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_ALARM).build(),
                )
                enableVibration(true)
                vibrationPattern = longArrayOf(0, 800, 400, 800)
                setBypassDnd(true)
            },
        )
    }

    private fun pending(ctx: Context): PendingIntent =
        PendingIntent.getBroadcast(ctx, 0, Intent(ctx, AlarmReceiver::class.java), PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)

    /** Re-arm the single alarm at the next transition and refresh the status notification. */
    fun schedule(ctx: Context, snap: Snapshot) {
        val s = Store(ctx)
        val now = s.now()
        val am = ctx.getSystemService(AlarmManager::class.java)
        val pi = pending(ctx)
        am.cancel(pi)
        val cur = snap.entryAt(now)
        val next = when {
            // A segment: its planned end, then over-time reminders (every 30 s while a nap rings,
            // to hear quickly that "Встал" was pressed on the PC).
            // Steps are counted from the planned end, not from "now": re-arming on every sync
            // must not push the reminder further away.
            cur != null && cur.segment -> if (cur.segmentEnd > now) cur.segmentEnd else {
                val step = if (cur.alarm) WAKE_RECHECK_MS else OVERRUN_MS
                cur.segmentEnd + ((now - cur.segmentEnd) / step + 1) * step
            }
            else -> snap.timeline.firstOrNull { it.running && it.until!! > now }?.until
        }
        if (next != null) {
            val at = next - s.offset + 500
            val exact = Build.VERSION.SDK_INT < 31 || am.canScheduleExactAlarms()
            try {
                if (exact && cur != null && cur.segment && cur.alarm && cur.segmentEnd > now) {
                    // The nap: a real alarm clock (shown in the status bar, survives Doze).
                    am.setAlarmClock(AlarmManager.AlarmClockInfo(at, open(ctx)), pi)
                } else if (exact) {
                    am.setExactAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, at, pi)
                } else {
                    am.setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, at, pi)
                }
            } catch (e: SecurityException) {
                // Exact alarms revoked in the settings (Android 12): late is better than a crash.
                am.setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, at, pi)
            }
        }
        if (cur == null || !cur.segment) ctx.getSystemService(NotificationManager::class.java).cancel(ID_WAKE)
        status(ctx, snap)
        TimerWidget.update(ctx)
    }

    private fun open(ctx: Context): PendingIntent =
        PendingIntent.getActivity(ctx, 0, Intent(ctx, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE)

    private fun actionIntent(ctx: Context, action: String, expect: String?, code: Int): PendingIntent =
        PendingIntent.getBroadcast(
            ctx, code,
            Intent(ctx, ActionReceiver::class.java).putExtra("action", action).putExtra("expect", expect),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )

    /** Ongoing notification with the live countdown while the day runs. */
    fun status(ctx: Context, snap: Snapshot) {
        val s = Store(ctx)
        val nm = ctx.getSystemService(NotificationManager::class.java)
        val e = snap.entryAt(s.now())
        if (!s.notify || e == null || e.kind in setOf("idle", "done")) {
            nm.cancel(ID_STATUS)
            return
        }
        val b = Notification.Builder(ctx, CH_STATUS)
            .setSmallIcon(R.drawable.ic_stat)
            .setContentTitle(if (e.paused) "Пауза · ${e.title}" else e.title)
            .setContentText(e.subtitle.ifEmpty { if (snap.lockBase) "Блокировка включена" else "" })
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .setContentIntent(open(ctx))
            .setCategory(Notification.CATEGORY_STATUS)
        if (e.running) {
            b.setUsesChronometer(true).setChronometerCountDown(true).setShowWhen(true).setWhen(e.until!! - s.offset)
        } else if (e.segment) {
            // Counts down to the planned end, then shows the over time as negative.
            b.setUsesChronometer(true).setChronometerCountDown(true).setShowWhen(true).setWhen(e.segmentEnd - s.offset)
        } else {
            b.setShowWhen(false)
        }
        when {
            e.paused -> b.addAction(Notification.Action.Builder(null, "Продолжить", actionIntent(ctx, "resume", null, 11)).build())
            e.kind == "work" -> b.addAction(Notification.Action.Builder(null, "Пауза", actionIntent(ctx, "pause", null, 12)).build())
            e.kind == "await" || e.kind == "lunch" -> b.addAction(Notification.Action.Builder(null, "Начать", actionIntent(ctx, "start_next", e.kind, 13)).build())
            e.segment -> b.addAction(Notification.Action.Builder(null, if (e.alarm) "Встал" else "Закончил", actionIntent(ctx, "end_segment", "segment", 15)).build())
        }
        nm.notify(ID_STATUS, b.build())
    }

    /** A loud "the break is over" when the projected timeline reaches the waiting state. */
    fun breakOver(ctx: Context, e: Entry) {
        if (!Store(ctx).notify) return
        val nm = ctx.getSystemService(NotificationManager::class.java)
        val n = Notification.Builder(ctx, CH_ALARM)
            .setSmallIcon(R.drawable.ic_stat)
            .setContentTitle("Перерыв окончен")
            .setContentText(e.subtitle.ifEmpty { "Пора начинать" })
            .setAutoCancel(true)
            .setCategory(Notification.CATEGORY_ALARM)
            .setContentIntent(open(ctx))
            .addAction(Notification.Action.Builder(null, "Начать", actionIntent(ctx, "start_next", "await", 14)).build())
            .build()
        nm.notify(ID_ALARM, n)
    }

    /**
     * End of a segment. A nap rings like an alarm clock: insistent (the sound repeats until you
     * react) on the alarm stream, which media volume and "do not disturb" don't silence.
     */
    fun segmentOver(ctx: Context, e: Entry, overMs: Long) {
        val nm = ctx.getSystemService(NotificationManager::class.java)
        val over = if (overMs >= 60_000) " · +${Fmt.dur(overMs)}" else ""
        val n = Notification.Builder(ctx, if (e.alarm) CH_WAKE else CH_ALARM)
            .setSmallIcon(R.drawable.ic_stat)
            .setContentTitle(if (e.alarm) "Вставай! ${e.title} окончен$over" else "${e.title} окончен$over")
            .setContentText(e.subtitle)
            .setCategory(Notification.CATEGORY_ALARM)
            .setContentIntent(open(ctx))
            .setFullScreenIntent(open(ctx), e.alarm)
            .setOngoing(e.alarm)
            .addAction(Notification.Action.Builder(null, if (e.alarm) "Встал" else "Закончил", actionIntent(ctx, "end_segment", "segment", 16)).build())
            .build()
        if (e.alarm) n.flags = n.flags or Notification.FLAG_INSISTENT
        // A nap keeps one ringing notification; other segments remind again every 5 minutes.
        if (!e.alarm || overMs < WAKE_RECHECK_MS) nm.notify(if (e.alarm) ID_WAKE else ID_ALARM, n)
    }

    fun clearAlarm(ctx: Context) {
        val nm = ctx.getSystemService(NotificationManager::class.java)
        nm.cancel(ID_ALARM)
        nm.cancel(ID_WAKE)
    }
}

/** Fires at a projected transition: notify, re-arm, and sync once to correct the projection. */
class AlarmReceiver : BroadcastReceiver() {
    override fun onReceive(ctx: Context, intent: Intent) {
        val s = Store(ctx)
        val snap = s.snapshot() ?: return
        val now = s.now()
        val e = snap.entryAt(now)
        if (e != null && e.kind == "await" && now - e.from < 60_000) Alarms.breakOver(ctx, e)
        if (e != null && e.segment && now >= e.segmentEnd && Store(ctx).notify) Alarms.segmentOver(ctx, e, now - e.segmentEnd)
        Alarms.schedule(ctx, snap)
        val done = goAsync()
        Sync.kick(ctx, force = true) { done.finish() }
    }
}

/** Buttons in the notifications: pause / resume / start the next part. */
class ActionReceiver : BroadcastReceiver() {
    override fun onReceive(ctx: Context, intent: Intent) {
        val action = intent.getStringExtra("action") ?: return
        val done = goAsync()
        Alarms.clearAlarm(ctx)
        Sync.action(ctx, action, intent.getStringExtra("expect")) { err ->
            if (err != null) android.widget.Toast.makeText(ctx, err, android.widget.Toast.LENGTH_SHORT).show()
            done.finish()
        }
    }
}

class BootReceiver : BroadcastReceiver() {
    override fun onReceive(ctx: Context, intent: Intent) {
        Alarms.channels(ctx)
        Store(ctx).snapshot()?.let { Alarms.schedule(ctx, it) }
        val done = goAsync()
        Sync.kick(ctx, force = true) { done.finish() }
    }
}
