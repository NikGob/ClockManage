package com.nikgob.clockmanage

import android.content.Context
import android.os.Handler
import android.os.Looper
import android.provider.Settings
import org.json.JSONObject
import java.util.concurrent.Executors

/**
 * Talking to the PC. No polling in the background:
 * - while the timer screen is open, one long-poll request is kept open (the PC answers when the
 *   day changes, at most every 25 s);
 * - otherwise a single sync runs when the screen turns on, at phase transitions (alarms) and
 *   after an action from a notification.
 */
object Sync {
    private val io = Executors.newSingleThreadExecutor()
    private val main = Handler(Looper.getMainLooper())
    private val listeners = mutableSetOf<(Snapshot?, String?) -> Unit>()

    @Volatile private var live: Thread? = null
    @Volatile var online = false
        private set
    @Volatile var lastError: String? = null
        private set

    fun listen(l: (Snapshot?, String?) -> Unit) { listeners += l }
    fun unlisten(l: (Snapshot?, String?) -> Unit) { listeners -= l }

    private fun publish(ctx: Context, snap: Snapshot?, err: String?) {
        online = err == null
        lastError = err
        main.post { listeners.toList().forEach { it(snap, err) } }
        if (snap != null) {
            Alarms.schedule(ctx, snap)
            BlockerService.refresh()
        }
    }

    fun blockerEnabled(ctx: Context): Boolean {
        val list = Settings.Secure.getString(ctx.contentResolver, Settings.Secure.ENABLED_ACCESSIBILITY_SERVICES) ?: return false
        return list.split(':').any { it.startsWith(ctx.packageName + "/") }
    }

    /** One request; [wait] > 0 makes it a long poll that returns when [have] is outdated. */
    private fun fetch(ctx: Context, have: String, wait: Int): Snapshot {
        val s = Store(ctx)
        s.triedAt = System.currentTimeMillis()
        val blocker = if (blockerEnabled(ctx)) 1 else 0
        val sent = System.currentTimeMillis()
        val raw = Api.of(s).get("/api/state?v=$have&wait=$wait&blocker=$blocker", readTimeoutMs = (wait + 10) * 1000)
        val got = System.currentTimeMillis()
        val snap = Snapshot.parse(raw)
        s.save(raw, snap, sent, got)
        return snap
    }

    private fun handle(ctx: Context, e: Exception) {
        if (e is ApiError && e.unauthorized) Store(ctx).forget()
        publish(ctx, Store(ctx).snapshot(), e.message ?: "Ошибка")
    }

    /** A single sync in the background (screen on, alarm, boot). Rate-limited to once a minute. */
    fun kick(ctx: Context, force: Boolean = false, done: (() -> Unit)? = null) {
        val app = ctx.applicationContext
        val s = Store(app)
        if (!s.paired || live != null || (!force && System.currentTimeMillis() - s.triedAt < 60_000)) {
            done?.invoke()
            return
        }
        io.execute {
            try {
                publish(app, fetch(app, "", 0), null)
            } catch (e: Exception) {
                handle(app, e)
            } finally {
                done?.invoke()
            }
        }
    }

    /** Keep a long poll open while the timer is on screen. */
    fun startLive(ctx: Context) {
        if (live != null) return
        val app = ctx.applicationContext
        val t = Thread {
            var backoff = 3_000L
            var have = ""
            while (live === Thread.currentThread()) {
                if (!Store(app).paired) break
                try {
                    val snap = fetch(app, have, 25)
                    have = snap.version
                    backoff = 3_000L
                    publish(app, snap, null)
                } catch (e: Exception) {
                    handle(app, e)
                    have = ""
                    try { Thread.sleep(backoff) } catch (i: InterruptedException) { break }
                    backoff = (backoff * 2).coerceAtMost(30_000L)
                }
            }
        }
        live = t
        t.isDaemon = true
        t.start()
    }

    fun stopLive() {
        val t = live
        live = null
        t?.interrupt()
    }

    /** Pause / resume / start next / start day; [expect] guards against a stale button. */
    fun action(ctx: Context, action: String, expect: String?, done: (String?) -> Unit) =
        post(ctx, "/api/action", JSONObject().put("action", action).apply { if (expect != null) put("expect", expect) }, done)

    fun setMinutes(ctx: Context, index: Int, minutes: Int, done: (String?) -> Unit) =
        post(ctx, "/api/plan", JSONObject().put("index", index).put("minutes", minutes), done)

    fun setApps(ctx: Context, apps: Collection<String>, done: (String?) -> Unit) =
        post(ctx, "/api/apps", JSONObject().put("apps", org.json.JSONArray(apps)), done)

    private fun post(ctx: Context, path: String, body: JSONObject, done: (String?) -> Unit) {
        val app = ctx.applicationContext
        io.execute {
            val s = Store(app)
            val err = try {
                val sent = System.currentTimeMillis()
                val raw = Api.of(s).post(path, body)
                val snap = Snapshot.parse(raw)
                s.save(raw, snap, sent, System.currentTimeMillis())
                publish(app, snap, null)
                null
            } catch (e: Exception) {
                if (e is ApiError && e.unauthorized) s.forget()
                e.message ?: "Ошибка"
            }
            main.post { done(err) }
        }
    }
}
