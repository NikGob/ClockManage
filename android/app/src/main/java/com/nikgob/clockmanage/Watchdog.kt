package com.nikgob.clockmanage

import android.Manifest
import android.accessibilityservice.AccessibilityServiceInfo
import android.app.AlarmManager
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.provider.Settings
import android.view.accessibility.AccessibilityManager

/**
 * Keeps the blocker alive. Some phones (Xiaomi's HyperOS above all) kill the app's process, and
 * Android then marks the accessibility service as crashed: its switch in Accessibility stays on,
 * but nothing blocks until someone turns it off and on again. So:
 * - [running] asks the system which services are really bound, not only switched on;
 * - whenever the process starts, and on a timer while the phone is awake, [check] looks: with
 *   WRITE_SECURE_SETTINGS (granted once over adb) it turns the service off and on again by
 *   itself, otherwise it shows a notification while the phone should block.
 */
object Watchdog {
    /** How often to look while the phone is awake (a plain RTC alarm: it never wakes the phone). */
    private const val EVERY_MS = 5 * 60_000L
    /** Nag before the lock too: the autolock or a block may start soon. */
    private const val AHEAD_MS = 30 * 60_000L
    private const val CH_DEAD = "blocker"
    private const val ID_DEAD = 5

    @Volatile private var reviving = false

    /** Switched on in Accessibility (it may still be crashed). */
    fun switchedOn(ctx: Context): Boolean {
        val list = Settings.Secure.getString(ctx.contentResolver, Settings.Secure.ENABLED_ACCESSIBILITY_SERVICES) ?: return false
        return list.split(':').any { it.startsWith(ctx.packageName + "/") }
    }

    /** Really working: the system has the service bound right now. */
    fun running(ctx: Context): Boolean {
        if (BlockerService.alive) return true
        val am = ctx.getSystemService(AccessibilityManager::class.java) ?: return false
        return am.getEnabledAccessibilityServiceList(AccessibilityServiceInfo.FEEDBACK_ALL_MASK)
            .any { it.resolveInfo?.serviceInfo?.packageName == ctx.packageName }
    }

    /** Granted over adb: `adb shell pm grant com.nikgob.clockmanage android.permission.WRITE_SECURE_SETTINGS`. */
    fun canRevive(ctx: Context): Boolean =
        ctx.checkSelfPermission(Manifest.permission.WRITE_SECURE_SETTINGS) == PackageManager.PERMISSION_GRANTED

    /** The phone should block now or soon, by the last snapshot of the PC. */
    private fun lockNear(ctx: Context): Boolean {
        val s = Store(ctx)
        val snap = s.snapshot() ?: return false
        val now = s.now()
        if (snap.blockedAt(now, s.syncedAt)) return true
        val next = snap.nextChange(now) ?: return false
        return next - now < AHEAD_MS && snap.blockedAt(next + 1000, s.syncedAt)
    }

    /** Look now and again in a few minutes. Off the main thread: a revive waits between two writes. */
    fun check(ctx: Context, done: (() -> Unit)? = null) {
        val app = ctx.applicationContext
        arm(app)
        Thread {
            try {
                checkNow(app)
            } catch (e: Exception) {
                // Never let the watchdog take the process down with it.
            } finally {
                done?.invoke()
            }
        }.start()
    }

    private fun checkNow(ctx: Context) {
        val s = Store(ctx)
        if (!s.paired || running(ctx)) {
            clear(ctx)
            return
        }
        // The process may have just started for the service itself: give the bind a moment
        // before deciding it is dead (a revive in the middle of it would only delay it).
        Thread.sleep(4000)
        if (running(ctx)) {
            clear(ctx)
            return
        }
        if (canRevive(ctx)) {
            revive(ctx)
            Thread.sleep(3000)
            if (running(ctx)) {
                clear(ctx)
                return
            }
        }
        // The PC shows whether the phone blocks: tell it, it was told "on" before.
        Sync.kick(ctx)
        if (lockNear(ctx)) nag(ctx) else clear(ctx)
    }

    /**
     * Turn the service off and on again: Android forgets that it crashed and binds it anew (the
     * same as the switch in Accessibility). The two writes are apart, or the system sees no change.
     */
    private fun revive(ctx: Context) {
        if (reviving) return
        reviving = true
        try {
            val cr = ctx.contentResolver
            val me = ComponentName(ctx, BlockerService::class.java).flattenToString()
            val others = (Settings.Secure.getString(cr, Settings.Secure.ENABLED_ACCESSIBILITY_SERVICES) ?: "")
                .split(':').filter { it.isNotBlank() && !it.startsWith(ctx.packageName + "/") }
            Settings.Secure.putString(cr, Settings.Secure.ENABLED_ACCESSIBILITY_SERVICES, others.joinToString(":"))
            Thread.sleep(1500)
            Settings.Secure.putString(cr, Settings.Secure.ENABLED_ACCESSIBILITY_SERVICES, (others + me).joinToString(":"))
            Settings.Secure.putInt(cr, Settings.Secure.ACCESSIBILITY_ENABLED, 1)
        } catch (e: SecurityException) {
            // The permission was taken back.
        } finally {
            reviving = false
        }
    }

    private fun arm(ctx: Context) {
        val am = ctx.getSystemService(AlarmManager::class.java) ?: return
        val pi = PendingIntent.getBroadcast(ctx, 1, Intent(ctx, WatchdogReceiver::class.java), PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        // RTC, not RTC_WAKEUP: with the screen off nothing can be opened anyway, and the alarm
        // comes as soon as the phone wakes up.
        am.set(AlarmManager.RTC, System.currentTimeMillis() + EVERY_MS, pi)
    }

    fun channel(ctx: Context) {
        ctx.getSystemService(NotificationManager::class.java).createNotificationChannel(
            NotificationChannel(CH_DEAD, tr("android_watchdog_blocking_stopped"), NotificationManager.IMPORTANCE_HIGH).apply {
                description = tr("android_watchdog_android_stopped_the_blocking")
            },
        )
    }

    private fun nag(ctx: Context) {
        if (Build.VERSION.SDK_INT >= 33 && ctx.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) return
        channel(ctx)
        val open = PendingIntent.getActivity(
            ctx, 2,
            Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
            PendingIntent.FLAG_IMMUTABLE,
        )
        val n = Notification.Builder(ctx, CH_DEAD)
            .setSmallIcon(R.drawable.ic_stat)
            .setContentTitle(tr("android_watchdog_blocking_stopped"))
            .setContentText(tr("android_watchdog_android_stopped_the_blocking"))
            .setStyle(Notification.BigTextStyle().bigText(tr("android_watchdog_android_stopped_the_blocking")))
            .setCategory(Notification.CATEGORY_ERROR)
            .setContentIntent(open)
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .build()
        ctx.getSystemService(NotificationManager::class.java).notify(ID_DEAD, n)
    }

    fun clear(ctx: Context) {
        ctx.getSystemService(NotificationManager::class.java).cancel(ID_DEAD)
    }

    /** Xiaomi, Redmi, POCO: HyperOS / MIUI kill the blocker unless autostart is on. */
    val xiaomi: Boolean get() = Build.MANUFACTURER.lowercase() in setOf("xiaomi", "redmi", "poco")

    /** HyperOS / MIUI: the autostart list; elsewhere (or if it moved) the app info. */
    fun openAutostart(ctx: Context) {
        val miui = Intent("miui.intent.action.OP_AUTO_START").addCategory(Intent.CATEGORY_DEFAULT)
        try {
            ctx.startActivity(miui)
        } catch (e: Exception) {
            openAppInfo(ctx)
        }
    }

    fun openAppInfo(ctx: Context) {
        ctx.startActivity(Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, android.net.Uri.fromParts("package", ctx.packageName, null)))
    }
}

/** The watchdog's timer, while the phone is awake. */
class WatchdogReceiver : BroadcastReceiver() {
    override fun onReceive(ctx: Context, intent: Intent) {
        val done = goAsync()
        Watchdog.check(ctx) { done.finish() }
    }
}
