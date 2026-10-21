package com.nikgob.clockmanage

import android.Manifest
import android.app.Activity
import android.content.Intent
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.provider.Settings
import android.text.InputType
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.widget.EditText
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.Switch
import android.widget.TextView
import android.widget.Toast
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

/**
 * The only screen: pairing when the phone is not connected yet, otherwise the timer.
 * It ticks once a second only while visible; the long poll lives exactly as long.
 */
class MainActivity : Activity() {
    private lateinit var p: Palette
    private lateinit var store: Store
    private val ui = Handler(Looper.getMainLooper())
    private var snap: Snapshot? = null
    private var shownVersion = ""
    private var visible = false
    private var shownKind = ""

    // timer screen views
    private lateinit var status: TextView
    private lateinit var hero: LinearLayout
    private lateinit var lockIcon: ImageView
    private lateinit var lock: TextView
    private lateinit var ring: Ring
    private lateinit var state: TextView
    private lateinit var big: TextView
    private lateinit var title: TextView
    private lateinit var sub: TextView
    private lateinit var actions: LinearLayout
    private lateinit var total: TextView
    private lateinit var totalBar: Bar
    private lateinit var rows: LinearLayout
    private lateinit var blocker: TextView
    private lateinit var blockerBtn: View
    private lateinit var restrictedBtn: View
    private lateinit var appsBtn: android.widget.Button
    private lateinit var updateBox: LinearLayout
    private val rowViews = mutableListOf<Pair<TextView, Bar>>()
    /** The timer card's colours: background and content. */
    private var heroBg = 0
    private var heroFg = 0

    private val onSync: (Snapshot?, String?) -> Unit = { s, _ ->
        if (!store.paired) showPair() else {
            snap = s ?: snap
            // The PC sent new colours (or the first ones): draw the screen in them.
            if (Palette(this).key != p.key) {
                p = Palette(this)
                showTimer()
            }
            render()
            // The PC was updated while the app was open (or within the hourly check): ask now.
            if (s != null && s.apkCode > Updater.current(this) && s.apkCode != askedCode) {
                askedCode = s.apkCode
                checkUpdate(true)
            }
        }
    }
    private var askedCode = -1L

    private val tick = object : Runnable {
        override fun run() {
            if (!visible) return
            render()
            ui.postDelayed(this, 1000 - store.now() % 1000)
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        p = Palette(this)
        store = Store(this)
        Alarms.channels(this)
        if (Build.VERSION.SDK_INT >= 33 && checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != android.content.pm.PackageManager.PERMISSION_GRANTED) {
            requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), 1)
        }
        if (store.paired) showTimer() else showPair()
    }

    override fun onStart() {
        super.onStart()
        visible = true
        Sync.listen(onSync)
        if (store.paired) {
            snap = store.snapshot()
            Sync.startLive(this, "app")
            ui.post(tick)
            checkUpdate(false)
        }
    }

    /** An update card when the PC carries a newer APK (checked on open at most once an hour, and at once when a sync reports a newer one). */
    private fun checkUpdate(force: Boolean) {
        Thread {
            val a = Updater.check(this, force)
            ui.post { if (::updateBox.isInitialized) showUpdate(a) }
        }.start()
    }

    private fun showUpdate(a: Updater.Available?) {
        updateBox.removeAllViews()
        val was = updateBox.visibility
        updateBox.visibility = if (a == null) View.GONE else View.VISIBLE
        if (a == null) return
        if (was != View.VISIBLE) updateBox.riseIn()
        updateBox.addWithMargins(label(tr("android_main_version_is_available", "name" to a.name), 16f, p.onContainer, bold = true))
        val info = label(tr("android_main_from_the_pc_over_wi_fi", "size" to "%.1f".format(if (I18n.ru) Locale("ru") else Locale.US, a.size / 1048576.0)), 14f, p.onContainer)
        updateBox.addWithMargins(info, top = dp(2))
        val go = pill(tr("android_main_update"), p) {}
        go.setOnClickListener {
            if (!Updater.canInstall(this)) {
                Toast.makeText(this, tr("android_main_allow_clockmanage_to_install_apps_and"), Toast.LENGTH_LONG).show()
                Updater.openInstallPermission(this)
                return@setOnClickListener
            }
            go.isEnabled = false
            Thread {
                val err = Updater.downloadAndInstall(this) { pct -> ui.post { go.text = tr("android_main_downloading", "pct" to pct) } }
                ui.post {
                    go.isEnabled = true
                    go.text = tr("android_main_update")
                    if (err != null) info.text = err
                }
            }.start()
        }
        updateBox.addWithMargins(go, top = dp(10), width = ViewGroup.LayoutParams.WRAP_CONTENT)
    }

    override fun onStop() {
        visible = false
        Sync.unlisten(onSync)
        Sync.stopLive("app")
        ui.removeCallbacks(tick)
        super.onStop()
    }

    override fun onResume() {
        super.onResume()
        if (store.paired && ::blocker.isInitialized) renderBlocker()
    }

    private fun page(): LinearLayout {
        val col = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(dp(20), dp(24), dp(20), dp(32))
        }
        val scroll = ScrollView(this).apply {
            setBackgroundColor(p.bg)
            isFillViewport = true
            addView(col)
        }
        setContentView(scroll)
        systemBars()
        // Edge to edge on Android 15: keep content clear of the bars.
        scroll.setOnApplyWindowInsetsListener { v, insets ->
            @Suppress("DEPRECATION")
            v.setPadding(0, insets.systemWindowInsetTop, 0, insets.systemWindowInsetBottom)
            insets
        }
        return col
    }

    /** Status and navigation bars in the screen's colour, with icons that read on it. */
    @Suppress("DEPRECATION")
    private fun systemBars() {
        window.statusBarColor = p.bg
        window.navigationBarColor = p.bg
        if (Build.VERSION.SDK_INT >= 30) {
            val light = android.view.WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS or android.view.WindowInsetsController.APPEARANCE_LIGHT_NAVIGATION_BARS
            window.insetsController?.setSystemBarsAppearance(if (p.dark) 0 else light, light)
        } else {
            window.decorView.systemUiVisibility = if (p.dark) 0 else View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR or View.SYSTEM_UI_FLAG_LIGHT_NAVIGATION_BAR
        }
    }

    // ---------------- pairing ----------------

    private fun showPair() {
        ui.removeCallbacks(tick)
        val col = page()
        col.addWithMargins(label("ClockManage", 32f, p.text, bold = true))
        col.addWithMargins(label(tr("android_main_pair_the_phone_with_a_pc"), 16f, p.muted), top = dp(8))
        col.addWithMargins(
            label(tr("android_main_on_the_pc_settings_phone_turn"), 15f, p.muted),
            top = dp(16),
        )

        var host = store.host
        var port = store.port
        val chosen = label(if (host.isNotEmpty()) tr("android_main_pc", "store" to store.pcName.ifEmpty { host }) else tr("android_main_no_pc_selected"), 16f, p.text, bold = true)
        val found = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
        val find = pill(tr("android_main_find_pc"), p, "tonal") {}
        col.addWithMargins(find, top = dp(24), width = ViewGroup.LayoutParams.WRAP_CONTENT)
        col.addWithMargins(found, top = dp(8))
        find.setOnClickListener {
            find.isEnabled = false
            find.text = tr("android_main_searching")
            Thread {
                val list = Discovery.find()
                ui.post {
                    find.isEnabled = true
                    find.text = tr("android_main_search_again")
                    found.removeAllViews()
                    if (list.isEmpty()) found.addWithMargins(label(tr("android_main_not_found_check_that_the_pc"), 14f, p.error))
                    list.forEach { pc ->
                        found.addWithMargins(pill("${pc.name} · ${pc.host}", p, "text") {
                            host = pc.host
                            port = pc.port
                            store.pcName = pc.name
                            chosen.text = tr("android_main_found_pc", "name" to pc.name)
                        }, width = ViewGroup.LayoutParams.WRAP_CONTENT)
                    }
                }
            }.start()
        }

        val manual = EditText(this).apply {
            hint = tr("android_main_address_hint")
            inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_URI
            setSingleLine()
        }
        col.addWithMargins(manual, top = dp(8))
        col.addWithMargins(chosen, top = dp(20))
        val pin = EditText(this).apply {
            hint = tr("android_main_pin_from_the_pc_screen")
            inputType = InputType.TYPE_CLASS_NUMBER
            textSize = 24f
            letterSpacing = 0.2f
            filters = arrayOf(android.text.InputFilter.LengthFilter(6))
        }
        col.addWithMargins(pin, top = dp(8))
        val err = label("", 14f, p.error)
        val go = pill(tr("android_main_pair"), p) {}
        col.addWithMargins(go, top = dp(16), width = ViewGroup.LayoutParams.WRAP_CONTENT)
        col.addWithMargins(err, top = dp(8))
        go.setOnClickListener {
            val typed = manual.text.toString().trim()
            if (typed.isNotEmpty()) {
                host = typed.substringBefore(':')
                port = typed.substringAfter(':', "47811").toIntOrNull() ?: 47811
                store.pcName = host
            }
            if (host.isEmpty()) { err.text = tr("android_main_find_the_pc_or_enter_its"); return@setOnClickListener }
            val code = pin.text.toString().trim()
            if (code.length != 6) { err.text = tr("android_main_the_pin_is_6_digits"); return@setOnClickListener }
            go.isEnabled = false
            err.text = ""
            val h = host
            val pt = port
            Thread {
                val result = try { Result.success(Api.pair(h, pt, code)) } catch (e: Exception) { Result.failure(e) }
                ui.post {
                    go.isEnabled = true
                    result.onSuccess { (token, name) ->
                        store.host = h
                        store.port = pt
                        store.token = token
                        store.pcName = name
                        Toast.makeText(this, tr("android_main_paired_with", "name" to name), Toast.LENGTH_SHORT).show()
                        showTimer()
                        Sync.startLive(this, "app")
                        ui.post(tick)
                    }.onFailure { err.text = it.message ?: tr("android_main_didnt_work") }
                }
            }.start()
        }
    }

    // ---------------- timer ----------------

    private fun sectionTitle(text: String) = label(text, 14f, p.primary, bold = true)

    private fun showTimer() {
        val col = page()
        status = label("", 13f, p.muted)
        col.addWithMargins(status)
        updateBox = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            background = rounded(p.container, dp(24).toFloat())
            setPadding(dp(20), dp(14), dp(20), dp(14))
            visibility = View.GONE
        }
        col.addWithMargins(updateBox, top = dp(12))
        col.smoothChanges()

        // The timer card, like the PC's: the lock on top, the ring with the time, the buttons.
        hero = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            gravity = Gravity.CENTER_HORIZONTAL
            setPadding(dp(20), dp(18), dp(20), dp(24))
        }
        val lockRow = row()
        lockIcon = iconView(R.drawable.ic_lock, p.text, 20)
        lock = label("", 14f, p.text, bold = true)
        lockRow.addView(lockIcon)
        lockRow.addView(lock, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f).apply { marginStart = dp(10) })
        hero.addWithMargins(lockRow)

        val size = minOf(resources.displayMetrics.widthPixels - dp(80), dp(260))
        val dial = FrameLayout(this)
        ring = Ring(this)
        dial.addView(ring, FrameLayout.LayoutParams(size, size))
        val center = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            gravity = Gravity.CENTER
        }
        state = label("", 12f, p.text, bold = true).apply { isAllCaps = true; letterSpacing = 0.12f }
        big = label("", 64f, p.text).apply {
            typeface = Typeface.create("sans-serif-light", Typeface.NORMAL)
            fontFeatureSettings = "tnum"
            includeFontPadding = false
        }
        center.addView(state)
        center.addView(big)
        dial.addView(center, FrameLayout.LayoutParams(size, size))
        hero.addWithMargins(dial, top = dp(20), width = size)

        title = label("", 24f, p.text).apply { gravity = Gravity.CENTER }
        sub = label("", 14f, p.text).apply { gravity = Gravity.CENTER; alpha = 0.8f }
        actions = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            gravity = Gravity.CENTER_HORIZONTAL
        }
        hero.addWithMargins(title, top = dp(16))
        hero.addWithMargins(sub, top = dp(4))
        hero.addWithMargins(actions, top = dp(16))
        col.addWithMargins(hero, top = dp(12))

        val plan = card(p)
        plan.addWithMargins(label(tr("main_day_plan"), 22f, p.text))
        total = label("", 14f, p.muted, bold = true)
        plan.addWithMargins(total, top = dp(4))
        totalBar = Bar(this).apply { colors(p.primary, p.outlineVariant) }
        plan.addWithMargins(totalBar, top = dp(8))
        rows = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
        plan.addWithMargins(rows, top = dp(8))
        col.addWithMargins(plan, top = dp(16))

        val bl = card(p)
        val blHead = row()
        blHead.addView(iconView(R.drawable.ic_shield, p.primary, 22))
        blHead.addView(label(tr("android_main_blocking_on_the_phone"), 18f, p.text), LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f).apply { marginStart = dp(12) })
        bl.addWithMargins(blHead)
        blocker = label("", 14f, p.muted)
        bl.addWithMargins(blocker, top = dp(8))
        blockerBtn = pill(tr("android_main_turn_on_in_accessibility"), p, "filled") {
            startActivity(Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS))
            Toast.makeText(this, tr("android_main_find_clockmanage_blocking_and_turn_it"), Toast.LENGTH_LONG).show()
        }
        bl.addWithMargins(blockerBtn, top = dp(12), width = ViewGroup.LayoutParams.WRAP_CONTENT)
        // Android 13+ greys out accessibility for sideloaded apps until "restricted settings" are
        // allowed on the app's info page.
        restrictedBtn = pill(tr("android_main_app_info"), p, "text") {
            startActivity(Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, android.net.Uri.fromParts("package", packageName, null)))
        }
        bl.addWithMargins(restrictedBtn, top = dp(4), width = ViewGroup.LayoutParams.WRAP_CONTENT)
        appsBtn = pill(tr("blocking_apps"), p, "tonal", R.drawable.ic_phone) { startActivity(Intent(this, AppsActivity::class.java)) }
        bl.addWithMargins(appsBtn, top = dp(12), width = ViewGroup.LayoutParams.WRAP_CONTENT)
        val notify = Switch(this).apply {
            text = tr("android_main_notifications_the_countdown_in_the_shade")
            setTextColor(p.text)
            val states = arrayOf(intArrayOf(android.R.attr.state_checked), intArrayOf())
            thumbTintList = android.content.res.ColorStateList(states, intArrayOf(p.onPrimary, p.outline))
            trackTintList = android.content.res.ColorStateList(states, intArrayOf(p.primary, p.cardHighest))
            isChecked = store.notify
            setOnCheckedChangeListener { _, on ->
                store.notify = on
                store.snapshot()?.let { Alarms.schedule(this@MainActivity, it) }
            }
        }
        bl.addWithMargins(notify, top = dp(12))
        col.addWithMargins(bl, top = dp(16))

        col.addWithMargins(pill(tr("android_main_unpair_from_the_pc"), p, "text") {
            store.forget()
            store.host = ""
            Sync.stopLive("app")
            showPair()
        }, top = dp(16), width = ViewGroup.LayoutParams.WRAP_CONTENT)
        col.addWithMargins(label(tr("android_main_version", "v" to Updater.currentName(this)), 12f, p.muted).apply {
            setOnClickListener { checkUpdate(true); Toast.makeText(this@MainActivity, tr("android_main_checking_the_pc_for_an_update"), Toast.LENGTH_SHORT).show() }
        }, top = dp(8))

        snap = store.snapshot()
        shownVersion = ""
        shownKind = ""
        render()
        renderBlocker()
        // The screen opens in a cascade.
        (0 until col.childCount).forEach { col.getChildAt(it).riseIn(it * 45L) }
    }

    private fun renderBlocker() {
        val on = Sync.blockerEnabled(this)
        val s = snap
        blocker.text = when {
            !on && Build.VERSION.SDK_INT >= 33 -> tr("android_main_blocker_off_restricted") +
                tr("android_main_if_the_switch_is_grey_and") +
                tr("android_main_allow_restricted_settings_then_turn_it")
            !on -> tr("android_main_blocker_off")
            s == null -> tr("android_main_on_waiting_for_data_from_the")
            s.blockedAt(store.now(), store.syncedAt) -> tr("android_main_on_and_working_sites_apps", "sites" to s.sites.size, "apps" to s.apps.size)
            else -> tr("android_main_on_no_lock_right_now")
        }
        blocker.setTextColor(if (on) p.muted else p.error)
        blockerBtn.visibility = if (on) View.GONE else View.VISIBLE
        restrictedBtn.visibility = if (!on && Build.VERSION.SDK_INT >= 33) View.VISIBLE else View.GONE
        appsBtn.text = tr("android_main_apps", "count" to (s?.apps?.size ?: 0))
    }

    private fun act(action: String, expect: String?) {
        Sync.action(this, action, expect) { err -> if (err != null) Toast.makeText(this, err, Toast.LENGTH_SHORT).show() }
    }

    /** The timer card's colours for a phase, as on the PC: waiting — primary, a break — tertiary, done — secondary. */
    private fun paintHero(e: Entry?) {
        val (bg, fg) = when (e?.kind) {
            "await" -> p.container to p.onContainer
            "break", "lunch_break" -> p.tertiary to p.onTertiary
            "done" -> p.secondary to p.onSecondary
            else -> p.card to p.text
        }
        heroBg = bg
        heroFg = fg
        hero.background = rounded(bg, dp(32).toFloat())
        for (t in listOf(lock, state, big, title, sub)) t.setTextColor(fg)
        lockIcon.imageTintList = android.content.res.ColorStateList.valueOf(fg)
        // On the plain card the progress is the primary colour, on a coloured one the content colour.
        val arc = when {
            e?.paused == true -> p.outline
            bg == p.card -> p.primary
            else -> fg
        }
        ring.colors(arc, withAlpha(fg, 0.14f))
        actions.tag = null
    }

    private fun render() {
        if (!::status.isInitialized) return
        val s = snap
        val now = store.now()
        val hm = SimpleDateFormat("HH:mm", Locale.forLanguageTag("ru"))
        status.text = when {
            Sync.online -> tr("android_main_synced", "pc_name" to store.pcName)
            store.syncedAt > 0 -> "○ ${Sync.lastError ?: tr("android_main_no_connection_to_the_pc")} — ${tr("android_main_counting_on_my_own_last_synced")} ${hm.format(Date(store.syncedAt - store.offset))}"
            else -> "○ ${Sync.lastError ?: tr("android_main_connecting")}"
        }
        status.setTextColor(if (Sync.online) p.primary else p.muted)
        if (s == null) {
            title.text = tr("android_main_waiting_for_data_from_the_pc")
            return
        }
        val e = s.entryAt(now)
        // Phase changed (work -> break -> waiting…): new colours, the timer drops in anew.
        val kind = (e?.kind ?: "") + (e?.paused ?: false)
        if (kind != shownKind) {
            paintHero(e)
            if (shownKind.isNotEmpty()) {
                big.scaleX = 0.8f; big.scaleY = 0.8f; big.alpha = 0f
                big.animate().scaleX(1f).scaleY(1f).alpha(1f).setDuration(420).setInterpolator(android.view.animation.OvershootInterpolator(1.6f)).start()
                state.alpha = 0f
                state.animate().alpha(1f).setDuration(300).start()
            }
            shownKind = kind
        }

        val blocked = s.blockedAt(now, store.syncedAt)
        val focusUntil = s.focusUntil?.takeIf { now < it }
        lock.text = if (blocked) {
            if (focusUntil != null && ((s.lockReason != "study" && s.lockReason != "autolock") || focusUntil >= minOf(s.dayEndAt, s.doneAt ?: s.dayEndAt))) tr("android_main_focus_lock_until", "focus_until" to hm.format(Date(focusUntil - store.offset)))
            else if (s.lockReason == "autolock" || !s.started) tr("android_main_autolock_until_start_day")
            else tr("android_main_lock_on_until_or_until_the", "day_end" to s.dayEnd)
        } else when (s.lockReason) {
            "break_access" -> tr("blocking_access_in_the_break")
            "emergency" -> tr("blocking_emergency_access")
            "segment_access" -> "${e?.title ?: ""} — ${tr("today_access_open")}"
            "completed" -> tr("android_main_all_blocks_done_no_lock")
            "day_end" -> tr("android_main_no_lock_after", "day_end" to s.dayEnd)
            else -> tr("android_main_no_lock")
        }
        lockIcon.setImageResource(if (blocked) R.drawable.ic_lock else R.drawable.ic_lock_open)

        var fraction = -1f
        when (e?.kind) {
            "work", "break", "lunch_break" -> {
                state.text = if (e.paused) tr("today_paused") else if (e.kind == "work") tr("log_work") else tr("log_break")
                big.text = Fmt.mmss(e.remaining(now))
                big.textSize = 64f
                fraction = e.elapsed(now).toFloat() / e.durMs.coerceAtLeast(1)
            }
            "await" -> { state.text = tr("android_main_waiting_for_you"); big.text = Fmt.mmss(e.elapsed(now)); big.textSize = 52f }
            "segment" -> {
                val left = e.remaining(now)
                if (e.stopwatch) {
                    // "Без времени": counts up.
                    state.text = if (left < 0) tr("today_untimed_longer_than_usual") else tr("mini_untimed")
                    big.text = Fmt.mmss(e.elapsed(now))
                } else {
                    state.text = when {
                        e.prep -> if (left < 0) tr("mini_time_to_lie_down") else tr("mini_getting_ready")
                        left < 0 -> tr("mini_over")
                        e.alarm -> tr("today_nap_alarm")
                        else -> tr("today_segment")
                    }
                    big.text = if (left < 0) "+" + Fmt.mmss(-left) else Fmt.mmss(left)
                    if (!e.prep) fraction = (now - e.from).toFloat() / e.durMs.coerceAtLeast(1)
                }
                big.setTextColor(if (left < 0 && !e.stopwatch) p.error else heroFg)
                big.textSize = 64f
            }
            "lunch" -> { state.text = tr("log_lunch"); big.text = Fmt.mmss(e.elapsed(now)); big.textSize = 52f }
            "done" -> { state.text = tr("today_done"); big.text = "✓"; big.textSize = 56f; fraction = 1f }
            else -> {
                state.text = tr("today_planned")
                big.text = Fmt.dur(s.blocks.filter { !it.isBreak }.sumOf { it.minutes } * Snapshot.MIN)
                big.textSize = 40f
            }
        }
        ring.fraction = fraction
        title.text = e?.title ?: ""
        sub.text = e?.subtitle ?: ""
        sub.visibility = if (sub.text.isEmpty()) View.GONE else View.VISIBLE

        // Buttons: rebuilt only when what they do changes. The first is the main one.
        val wanted = mutableListOf<Triple<String, String, String?>>()
        val icons = mutableListOf<Int>()
        fun want(text: String, action: String, expect: String?, icon: Int) { wanted += Triple(text, action, expect); icons += icon }
        when {
            e == null || e.kind == "idle" -> if (s.canStartDay) want(tr("app_start_day"), "start_day", null, R.drawable.ic_play)
            e.paused -> want(tr("app_resume"), "resume", null, R.drawable.ic_play)
            e.kind == "work" || e.kind == "break" -> want(tr("day_pause"), "pause", null, R.drawable.ic_pause)
            e.kind == "await" || e.kind == "lunch" -> want(tr("overlay_start"), "start_next", e.kind, R.drawable.ic_skip)
            e.segment && e.prep -> {
                want(tr("app_in_bed"), "lay_down", "segment", R.drawable.ic_bed)
                want(tr("today_skip_the_nap"), "end_segment", "segment", R.drawable.ic_close)
            }
            e.segment -> {
                want(if (e.alarm) tr("app_im_up") else tr("overlay_done"), "end_segment", "segment", if (e.alarm) R.drawable.ic_alarm else R.drawable.ic_check)
                // "Открыть доступ" during a lunch: everything opens until it ends, as on the PC.
                if (s.canSegmentAccess && e.segmentTimed) {
                    want(tr("today_open_access_until", "time" to hm.format(Date(e.segmentEnd - store.offset))), "segment_access", "segment", R.drawable.ic_lock_open)
                }
            }
        }
        val sig = wanted.joinToString { it.second } + Sync.online + heroBg
        if (actions.tag != sig) {
            actions.tag = sig
            actions.removeAllViews()
            wanted.forEachIndexed { i, (text, a, expect) ->
                // On the card: the main button filled in the content colour, the others tonal.
                val b = if (i == 0) {
                    if (heroBg == p.card) pillIn(text, p.primary, p.onPrimary, icon = icons[i]) { act(a, expect) } else pillIn(text, heroFg, heroBg, icon = icons[i]) { act(a, expect) }
                } else {
                    pillIn(text, withAlpha(heroFg, 0.12f), heroFg, icon = icons[i]) { act(a, expect) }
                }
                b.isEnabled = Sync.online
                actions.addWithMargins(b, top = if (i == 0) 0 else dp(8), width = ViewGroup.LayoutParams.WRAP_CONTENT)
                b.scaleX = 0.6f; b.scaleY = 0.6f; b.alpha = 0f
                b.animate().scaleX(1f).scaleY(1f).alpha(if (Sync.online) 1f else 0.5f).setStartDelay(i * 60L).setDuration(380)
                    .setInterpolator(android.view.animation.OvershootInterpolator(2f)).start()
            }
        }

        val work = s.blocks.indices.sumOf { e?.work(it, now) ?: 0L }
        val studyMin = s.blocks.filter { !it.isBreak }.sumOf { it.minutes }
        // "0 мин из 0 ч учёбы" says nothing: an empty plan shows only its hint below.
        total.visibility = if (studyMin > 0) View.VISIBLE else View.GONE
        totalBar.visibility = total.visibility
        total.text = tr("android_main_of_of_study", "work" to Fmt.dur(work), "study_min" to Fmt.hours(studyMin))
        totalBar.progress = ((work * 1000) / (studyMin * Snapshot.MIN).coerceAtLeast(1)).toInt()
        if (shownVersion != s.version + Sync.online + (e?.block ?: -1)) {
            shownVersion = s.version + Sync.online + (e?.block ?: -1)
            buildRows(s, e?.block)
        }
        s.blocks.forEachIndexed { i, b ->
            val (meta, bar) = rowViews.getOrNull(i) ?: return@forEachIndexed
            if (b.isBreak) {
                meta.text = tr("android_main_min", "minutes" to b.minutes) + when { b.done -> tr("plan_done"); b.started -> tr("stats_running"); else -> tr("android_main_segment_not_study") }
                return@forEachIndexed
            }
            val w = e?.work(i, now) ?: 0L
            meta.text = tr("android_main_of_min", "w" to w / Snapshot.MIN, "minutes" to b.minutes) + if (b.done) tr("plan_suffix_done") else ""
            val to = ((w * 1000) / (b.minutes * Snapshot.MIN).coerceAtLeast(1)).toInt().coerceIn(0, 1000)
            // Big jumps (a fresh list, a block closed) glide; the per-second creep is set directly.
            if (kotlin.math.abs(to - bar.progress) > 20) {
                android.animation.ObjectAnimator.ofInt(bar, "progress", bar.progress, to).setDuration(700).apply {
                    interpolator = android.view.animation.DecelerateInterpolator(2f)
                }.start()
            } else {
                bar.progress = to
            }
        }
    }

    /** A plan row's mark, as on the PC: done ✓, current ●, a segment ☕, still to do ○. */
    private fun mark(b: Block, current: Boolean): View = when {
        b.done -> iconView(R.drawable.ic_check, p.primary, 20)
        b.isBreak -> iconView(R.drawable.ic_coffee, p.muted, 20)
        else -> View(this).apply {
            val dot = dp(if (current) 10 else 12)
            background = GradientDrawable().apply {
                shape = GradientDrawable.OVAL
                if (current) setColor(p.text) else setStroke(dp(2), p.muted)
            }
            layoutParams = LinearLayout.LayoutParams(dot, dot).apply { marginStart = dp(4); marginEnd = dp(4) }
        }
    }

    private fun buildRows(s: Snapshot, current: Int?) {
        rows.removeAllViews()
        rowViews.clear()
        if (s.blocks.isEmpty()) rows.addWithMargins(label(tr("android_main_todays_plan_is_empty_make_it"), 14f, p.muted))
        val editable = s.canEditPlan && Sync.online
        s.blocks.forEachIndexed { i, b ->
            val now = i == current && !b.done
            val item = LinearLayout(this).apply {
                orientation = LinearLayout.VERTICAL
                setPadding(dp(12), dp(10), dp(12), dp(10))
                // The block running now stands out, like the PC's highlighted row.
                if (now) background = rounded(p.secondary, dp(20).toFloat())
            }
            val r = row()
            r.addView(mark(b, now))
            val text = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
            text.addView(label(b.name, 16f, if (now) p.onSecondary else if (b.isBreak) p.muted else p.text, bold = !b.isBreak))
            val meta = label("", 13f, if (now) p.onSecondary else p.muted)
            text.addView(meta)
            r.addView(text, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f).apply { marginStart = dp(14) })
            if (!b.isBreak && editable) {
                // − / + like the PC's steppers. During the lock a started block can't go below the time already worked.
                val lower = (b.minutes - STEP).coerceAtLeast(b.minMinutes.coerceAtLeast(1))
                val higher = (b.minutes + STEP).coerceAtMost(480)
                val toast: (String?) -> Unit = { err -> err?.let { Toast.makeText(this, it, Toast.LENGTH_SHORT).show() } }
                val step = row().apply { background = rounded(p.cardHighest, dp(20).toFloat()) }
                val minus = pillIn("−", 0, p.text) { Sync.setMinutes(this, i, lower, toast) }
                val plus = pillIn("+", 0, p.text) { Sync.setMinutes(this, i, higher, toast) }
                minus.isEnabled = !b.done && lower < b.minutes
                plus.isEnabled = higher > b.minutes
                minus.alpha = if (minus.isEnabled) 1f else 0.35f
                plus.alpha = if (plus.isEnabled) 1f else 0.35f
                for (btn in listOf(minus, plus)) {
                    btn.setPadding(0, 0, 0, 0)
                    btn.textSize = 20f
                    step.addView(btn, LinearLayout.LayoutParams(dp(44), dp(40)))
                }
                r.addView(step)
            }
            item.addView(r)
            val bar = Bar(this).apply { colors(if (now) p.onSecondary else p.primary, if (now) withAlpha(p.onSecondary, 0.2f) else p.outlineVariant) }
            if (!b.isBreak) item.addWithMargins(bar, top = dp(8))
            rows.addWithMargins(item, top = dp(4))
            rowViews += meta to bar
        }
    }

    companion object {
        private const val STEP = 15
    }
}
