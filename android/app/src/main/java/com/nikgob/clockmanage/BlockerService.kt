package com.nikgob.clockmanage

import android.accessibilityservice.AccessibilityService
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.view.accessibility.AccessibilityEvent
import android.view.accessibility.AccessibilityNodeInfo
import android.widget.Toast

/**
 * Blocks apps and sites on the phone during the study lock. Event driven: Android calls it when
 * a window changes. The lock decision comes from the last snapshot of the PC (works offline
 * until the day end), and is looked at again:
 * - at once when a new snapshot arrives (the app on screen is closed, not only the next one);
 * - when the lock is due to change by itself (an access window ends, the autolock comes).
 *
 * While the screen is on the service keeps a long poll to the PC open, so a lock started there
 * reaches the phone within a second; with the screen off nothing runs.
 */
class BlockerService : AccessibilityService() {

    private var snap: Snapshot? = null
    private var syncedAt = 0L
    private lateinit var store: Store
    private var lastCheck = 0L
    private var lastBlock = 0L
    private var lastToast = 0L
    private var lastUrl = ""
    private val main = android.os.Handler(android.os.Looper.getMainLooper())
    private val timed = Runnable { checkNow(); schedule() }

    private val screen = object : BroadcastReceiver() {
        override fun onReceive(ctx: Context, intent: Intent) {
            if (intent.action == Intent.ACTION_SCREEN_OFF) Sync.stopLive("screen") else Sync.startLive(ctx, "screen")
        }
    }

    override fun onServiceConnected() {
        instance = this
        store = Store(this)
        reload()
        val filter = IntentFilter().apply {
            addAction(Intent.ACTION_SCREEN_ON)
            addAction(Intent.ACTION_SCREEN_OFF)
            addAction(Intent.ACTION_USER_PRESENT)
        }
        // System broadcasts still arrive with NOT_EXPORTED; other apps can't fake them.
        if (android.os.Build.VERSION.SDK_INT >= 33) registerReceiver(screen, filter, RECEIVER_NOT_EXPORTED) else registerReceiver(screen, filter)
        val power = getSystemService(Context.POWER_SERVICE) as android.os.PowerManager
        if (power.isInteractive) Sync.startLive(this, "screen") else Sync.kick(this, force = true)
    }

    override fun onDestroy() {
        instance = null
        main.removeCallbacks(timed)
        Sync.stopLive("screen")
        try { unregisterReceiver(screen) } catch (e: IllegalArgumentException) { }
        super.onDestroy()
    }

    override fun onInterrupt() {}

    private fun reload() {
        snap = store.snapshot()
        syncedAt = store.syncedAt
        checkNow()
        schedule()
    }

    /** Look at the app on screen now: a lock that just began closes it without waiting for a tap. */
    private fun checkNow() {
        val s = snap ?: return
        if (!s.blockedAt(store.now(), syncedAt)) return
        val pkg = rootInActiveWindow?.packageName?.toString() ?: return
        if (pkg == packageName) return
        if (pkg in s.apps) {
            block(appLabel(pkg), home = true)
        } else if (pkg in BROWSERS) {
            val url = urlOf(pkg) ?: return
            Sites.match(url, s.sites)?.let { block(it, home = false) }
        }
    }

    /** Wake up when the lock changes by itself (an access window ends, the autolock comes). */
    private fun schedule() {
        main.removeCallbacks(timed)
        val s = snap ?: return
        val now = store.now()
        val at = s.nextChange(now) ?: return
        main.postDelayed(timed, (at - now).coerceIn(0, 6 * Snapshot.HOUR) + 500)
    }

    override fun onAccessibilityEvent(ev: AccessibilityEvent) {
        val pkg = ev.packageName?.toString() ?: return
        if (pkg == packageName) return
        val s = snap ?: return
        if (!s.blockedAt(store.now(), syncedAt)) return
        val now = System.currentTimeMillis()

        if (pkg in s.apps) {
            // A new window at once; inside an app that was already open, a couple of times a second.
            if (ev.eventType == AccessibilityEvent.TYPE_WINDOW_STATE_CHANGED || now - lastBlock > 1500) block(appLabel(pkg), home = true)
            return
        }
        // Content changes come in bursts (scrolling, typing): look at most a few times a second.
        if (ev.eventType == AccessibilityEvent.TYPE_WINDOW_CONTENT_CHANGED && now - lastCheck < 700) return
        lastCheck = now
        if (pkg in BROWSERS) {
            val url = urlOf(pkg) ?: return
            val hit = Sites.match(url, s.sites) ?: return
            // Back closes the page; if the same page shows again right away (new tab), go home.
            block(hit, home = url == lastUrl && now - lastBlock < 3000)
            lastUrl = url
        } else if (pkg == YOUTUBE && s.sites.any { it.startsWith("youtube.com/shorts") } && shortsOpen()) {
            block("YouTube Shorts", home = false)
        }
    }

    private fun block(what: String, home: Boolean) {
        val now = System.currentTimeMillis()
        lastBlock = now
        performGlobalAction(if (home) GLOBAL_ACTION_HOME else GLOBAL_ACTION_BACK)
        if (now - lastToast > 2500) {
            lastToast = now
            Toast.makeText(this, tr("android_blocker_nope_after_studying", "what" to what), Toast.LENGTH_SHORT).show()
        }
    }

    private fun appLabel(pkg: String): String = try {
        packageManager.getApplicationLabel(packageManager.getApplicationInfo(pkg, 0)).toString()
    } catch (e: Exception) {
        pkg
    }

    /** The address shown in the browser toolbar (skipped while the user is typing in it). */
    private fun urlOf(pkg: String): String? {
        val root = rootInActiveWindow ?: return null
        for (id in BROWSERS[pkg].orEmpty()) {
            val n = root.findAccessibilityNodeInfosByViewId("$pkg:id/$id").firstOrNull() ?: continue
            if (n.isFocused) return null
            return n.text?.toString()?.takeIf { it.isNotBlank() }
        }
        return null
    }

    private fun shortsOpen(): Boolean {
        val root = rootInActiveWindow ?: return false
        return SHORTS_IDS.any { root.findAccessibilityNodeInfosByViewId("$YOUTUBE:id/$it").any(AccessibilityNodeInfo::isVisibleToUser) }
    }

    companion object {
        @Volatile private var instance: BlockerService? = null

        /** Android runs the service in this process. Off while it is switched on in the settings
         *  means Android stopped it (the process was killed, say after "not responding") and won't
         *  start it again until it is switched off and on. */
        val running: Boolean get() = instance != null

        /** A new snapshot arrived. */
        fun refresh() {
            instance?.let { s -> android.os.Handler(android.os.Looper.getMainLooper()).post { s.reload() } }
        }

        private const val YOUTUBE = "com.google.android.youtube"
        private val SHORTS_IDS = listOf("reel_recycler", "reel_player_page_container")

        /** Browser package -> view ids of its address bar. */
        private val BROWSERS = mapOf(
            "com.android.chrome" to listOf("url_bar"),
            "com.chrome.beta" to listOf("url_bar"),
            "com.chrome.dev" to listOf("url_bar"),
            "com.microsoft.emmx" to listOf("url_bar"),
            "com.brave.browser" to listOf("url_bar"),
            "com.vivaldi.browser" to listOf("url_bar"),
            "com.kiwibrowser.browser" to listOf("url_bar"),
            "org.mozilla.firefox" to listOf("mozac_browser_toolbar_url_view", "url_bar_title"),
            "org.mozilla.firefox_beta" to listOf("mozac_browser_toolbar_url_view"),
            "org.mozilla.fenix" to listOf("mozac_browser_toolbar_url_view"),
            "com.sec.android.app.sbrowser" to listOf("location_bar_edit_text", "custom_location_bar_edit_text"),
            "com.opera.browser" to listOf("url_field"),
            "com.opera.mini.native" to listOf("url_field"),
            "com.yandex.browser" to listOf("bro_omnibar_address_title_text", "bro_omnibox_collapsed_title"),
            "ru.yandex.searchplugin" to listOf("bro_omnibar_address_title_text"),
            "com.duckduckgo.mobile.android" to listOf("omnibarTextInput"),
        )
    }
}

/** The PC's site rules on the phone: `x.com` covers its subdomains, `youtube.com/shorts` a path. */
object Sites {
    fun match(url: String, sites: List<String>): String? {
        var u = url.trim().lowercase()
        for (p in listOf("https://", "http://")) if (u.startsWith(p)) u = u.removePrefix(p)
        u = u.removePrefix("www.").removePrefix("m.")
        val host = u.substringBefore('/').substringBefore('?').substringBefore(':')
        val path = u.substringAfter('/', "").let { "/$it" }
        return sites.firstOrNull { rule ->
            val rHost = rule.substringBefore('/')
            val rPath = rule.substringAfter('/', "").let { if (it.isEmpty()) "" else "/$it" }
            val hostOk = host == rHost || host.endsWith(".$rHost")
            hostOk && (rPath.isEmpty() || path == rPath || path.startsWith("$rPath/") || path.startsWith("$rPath?"))
        }
    }
}
