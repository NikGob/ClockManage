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
 * Blocks apps and sites on the phone during the study lock. Purely event driven: Android calls it
 * when a window changes, nothing runs in between. The lock decision comes from the last snapshot
 * of the PC (works offline until the day end).
 *
 * The same service turns the screen-on event into one sync, so the phone catches up with the
 * PC whenever you pick it up — no background polling.
 */
class BlockerService : AccessibilityService() {

    private var snap: Snapshot? = null
    private var syncedAt = 0L
    private lateinit var store: Store
    private var lastCheck = 0L
    private var lastBlock = 0L
    private var lastToast = 0L
    private var lastUrl = ""

    private val screenOn = object : BroadcastReceiver() {
        override fun onReceive(ctx: Context, intent: Intent) {
            Sync.kick(ctx)
        }
    }

    override fun onServiceConnected() {
        instance = this
        store = Store(this)
        reload()
        val filter = IntentFilter().apply {
            addAction(Intent.ACTION_SCREEN_ON)
            addAction(Intent.ACTION_USER_PRESENT)
        }
        // System broadcasts still arrive with NOT_EXPORTED; other apps can't fake them.
        if (android.os.Build.VERSION.SDK_INT >= 33) registerReceiver(screenOn, filter, RECEIVER_NOT_EXPORTED) else registerReceiver(screenOn, filter)
        Sync.kick(this, force = true)
    }

    override fun onDestroy() {
        instance = null
        try { unregisterReceiver(screenOn) } catch (e: IllegalArgumentException) { }
        super.onDestroy()
    }

    override fun onInterrupt() {}

    private fun reload() {
        snap = store.snapshot()
        syncedAt = store.syncedAt
    }

    override fun onAccessibilityEvent(ev: AccessibilityEvent) {
        val pkg = ev.packageName?.toString() ?: return
        if (pkg == packageName) return
        val s = snap ?: return
        if (!s.blockedAt(store.now(), syncedAt)) return

        if (pkg in s.apps) {
            if (ev.eventType == AccessibilityEvent.TYPE_WINDOW_STATE_CHANGED) block(appLabel(pkg), home = true)
            return
        }
        val now = System.currentTimeMillis()
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
            Toast.makeText(this, "Не-не-не: $what — после учёбы", Toast.LENGTH_SHORT).show()
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
