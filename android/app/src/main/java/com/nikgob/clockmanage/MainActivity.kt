package com.nikgob.clockmanage

import android.Manifest
import android.app.Activity
import android.content.Intent
import android.content.res.ColorStateList
import android.graphics.Color
import android.graphics.drawable.ColorDrawable
import android.graphics.drawable.GradientDrawable
import android.graphics.drawable.RippleDrawable
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.provider.Settings
import android.text.InputType
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.widget.Button
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
 * The only screen: pairing when the phone is not connected yet, otherwise the day — the timer
 * card, the plan and the phone's blocking, drawn like the PC's Today screen in the PC's colours.
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
    /** The language the screen was built in: the PC switched languages — build it anew. */
    private var shownRu = I18n.ru
    /** Whether the blocker was on when its card was last built (null: not built yet). */
    private var shownBlocker: Boolean? = null

    // timer screen views
    private lateinit var statusDot: View
    private lateinit var status: TextView
    private lateinit var updateBox: LinearLayout
    private lateinit var hero: LinearLayout
    private lateinit var lockIcon: ImageView
    private lateinit var lockHead: TextView
    private lateinit var lockTail: TextView
    private lateinit var dial: Dial
    private lateinit var title: TextView
    private lateinit var sub: TextView
    private lateinit var actions: Flow
    private lateinit var total: TextView
    private lateinit var totalBar: Bar
    private lateinit var rows: LinearLayout
    private lateinit var blockerChip: TextView
    private lateinit var blocker: TextView
    private lateinit var steps: LinearLayout
    private lateinit var appsBtn: Button
    private val rowViews = mutableListOf<Pair<TextView, Bar>>()
    /** The timer card's colours: background and content. */
    private var heroBg = 0
    private var heroFg = 0

    private val onSync: (Snapshot?, String?) -> Unit = { s, _ ->
        if (!store.paired) showPair() else {
            snap = s ?: snap
            // The PC sent new colours or another language (or the first ones): build the screen in them.
            if (Palette(this).key != p.key || I18n.ru != shownRu) {
                p = Palette(this)
                Alarms.channels(this)
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
        val head = row()
        head.addView(iconView(R.drawable.ic_download, p.onContainer))
        head.addFill(label(tr("android_main_version_is_available", "name" to a.name), 16f, p.onContainer, bold = true), start = dp(12))
        updateBox.addWithMargins(head)
        val info = label(tr("android_main_from_the_pc_over_wi_fi", "size" to "%.1f".format(if (I18n.ru) Locale("ru") else Locale.US, a.size / 1048576.0)), 14f, p.onContainer)
        updateBox.addWithMargins(info, top = dp(6))
        val go = pillIn(tr("android_main_update"), p.onContainer, p.container) {}
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
        updateBox.addWithMargins(go, top = dp(12), width = ViewGroup.LayoutParams.WRAP_CONTENT)
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
        // Back from the Accessibility settings: show at once whether the blocker is on now.
        if (store.paired && ::blocker.isInitialized) renderBlocker()
    }

    private fun page(): LinearLayout {
        val col = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(dp(16), dp(8), dp(16), dp(24))
        }
        val scroll = ScrollView(this).apply {
            setBackgroundColor(p.bg)
            isFillViewport = true
            addView(col)
        }
        setContentView(scroll)
        // No flash of the launch theme's colour behind the screen.
        window.setBackgroundDrawable(ColorDrawable(p.bg))
        systemBars()
        // Edge to edge on Android 15+: keep content clear of the bars and the camera cutout.
        scroll.setOnApplyWindowInsetsListener { v, insets ->
            @Suppress("DEPRECATION")
            v.setPadding(insets.systemWindowInsetLeft, insets.systemWindowInsetTop, insets.systemWindowInsetRight, insets.systemWindowInsetBottom)
            insets
        }
        return col
    }

    /** Status and navigation bars in the screen's colours ([nav] under the gesture bar), with icons that read on them. */
    @Suppress("DEPRECATION")
    private fun systemBars(nav: Int = p.bg) {
        window.statusBarColor = p.bg
        window.navigationBarColor = nav
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
        pages.clear()
        shownRu = I18n.ru
        val col = page()
        col.gravity = Gravity.CENTER_HORIZONTAL

        // The app's clock in a primary-container circle, the name and what to do.
        val logo = ImageView(this).apply {
            setImageResource(R.drawable.ic_launcher_foreground)
            imageTintList = ColorStateList.valueOf(p.onContainer)
            background = GradientDrawable().apply { shape = GradientDrawable.OVAL; setColor(p.container) }
        }
        col.addView(logo, LinearLayout.LayoutParams(dp(88), dp(88)).apply { topMargin = dp(40) })
        col.addWithMargins(label("ClockManage", 32f, p.text).apply { gravity = Gravity.CENTER }, top = dp(20))
        col.addWithMargins(label(tr("android_main_pair_the_phone_with_a_pc"), 16f, p.muted).apply { gravity = Gravity.CENTER }, top = dp(8))

        val box = card(p)
        col.addWithMargins(box, top = dp(28))
        box.addWithMargins(label(tr("android_main_on_the_pc_settings_phone_turn"), 14f, p.muted))

        var host = store.host
        var port = store.port
        val found = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
        val pcRows = mutableListOf<View>()
        val find = pill(tr("android_main_find_pc"), p, "tonal", R.drawable.ic_computer) {}
        box.addWithMargins(find, top = dp(16), width = ViewGroup.LayoutParams.WRAP_CONTENT)
        box.addWithMargins(found, top = dp(8))
        find.setOnClickListener {
            find.isEnabled = false
            find.text = tr("android_main_searching")
            Thread {
                val list = Discovery.find()
                ui.post {
                    find.isEnabled = true
                    find.text = tr("android_main_search_again")
                    found.removeAllViews()
                    pcRows.clear()
                    if (list.isEmpty()) found.addWithMargins(label(tr("android_main_not_found_check_that_the_pc"), 14f, p.error), top = dp(4))
                    list.forEach { pc ->
                        val r = pcRow(pc.name, "${pc.host}:${pc.port}") { v ->
                            host = pc.host
                            port = pc.port
                            store.pcName = pc.name
                            // The chosen PC stands out, like a selected list item.
                            pcRows.forEach { it.background = selectable(Color.TRANSPARENT) }
                            v.background = selectable(p.secondary)
                        }
                        pcRows += r
                        found.addWithMargins(r, top = dp(4))
                    }
                    if (list.size == 1) pcRows.first().performClick()
                }
            }.start()
        }

        val manual = field(tr("android_main_address_hint"), p, InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_URI)
        box.addWithMargins(manual, top = dp(12))
        val pin = field(tr("android_main_pin_from_the_pc_screen"), p, InputType.TYPE_CLASS_NUMBER).apply {
            textSize = 22f
            letterSpacing = 0.3f
            gravity = Gravity.CENTER
            filters = arrayOf(android.text.InputFilter.LengthFilter(6))
        }
        box.addWithMargins(pin, top = dp(12))
        val err = label("", 14f, p.error)
        box.addWithMargins(err, top = dp(8))
        val go = pill(tr("android_main_pair"), p) {}
        box.addWithMargins(go, top = dp(8))
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
        (0 until col.childCount).forEach { col.getChildAt(it).riseIn(it * 45L) }
    }

    /** A list item's background: [color] with a ripple on touch. */
    private fun selectable(color: Int): RippleDrawable {
        val r = dp(16).toFloat()
        return RippleDrawable(ColorStateList.valueOf(withAlpha(p.text, 0.12f)), rounded(color, r), rounded(Color.WHITE, r))
    }

    /** A found PC: the computer icon, its name and address. */
    private fun pcRow(name: String, address: String, onPick: (View) -> Unit): View = row().apply {
        setPadding(dp(12), dp(10), dp(12), dp(10))
        background = selectable(Color.TRANSPARENT)
        addView(iconView(R.drawable.ic_computer, p.primary))
        val t = LinearLayout(this@MainActivity).apply { orientation = LinearLayout.VERTICAL }
        t.addView(label(name, 16f, p.text, bold = true))
        t.addView(label(address, 13f, p.muted))
        addFill(t, start = dp(16))
        setOnClickListener(onPick)
    }

    // ---------------- the day: four pages under a navigation bar ----------------

    /** The page on screen: 0 today, 1 the plan, 2 blocking, 3 settings. Kept when the screen is built anew. */
    private var tab = 0
    private val pages = mutableListOf<View>()
    /** Per navigation item: its pill indicator, icon and label. */
    private val navItems = mutableListOf<Triple<View, ImageView, TextView>>()

    /** A horizontal fling flips to the next or previous page. */
    private val swipe by lazy {
        android.view.GestureDetector(this, object : android.view.GestureDetector.SimpleOnGestureListener() {
            override fun onFling(e1: android.view.MotionEvent?, e2: android.view.MotionEvent, vx: Float, vy: Float): Boolean {
                if (e1 == null || pages.isEmpty()) return false
                val dx = e2.x - e1.x
                if (kotlin.math.abs(dx) < dp(80) || kotlin.math.abs(dx) < kotlin.math.abs(e2.y - e1.y) * 2) return false
                val to = (tab + if (dx < 0) 1 else -1).coerceIn(0, pages.size - 1)
                if (to != tab) select(to)
                return to != tab
            }
        })
    }

    override fun dispatchTouchEvent(ev: android.view.MotionEvent): Boolean {
        if (store.paired && pages.isNotEmpty()) swipe.onTouchEvent(ev)
        return super.dispatchTouchEvent(ev)
    }

    @Deprecated("Back on another page goes to Today first")
    override fun onBackPressed() {
        if (store.paired && pages.isNotEmpty() && tab != 0) select(0) else @Suppress("DEPRECATION") super.onBackPressed()
    }

    private fun showTimer() {
        shownRu = I18n.ru
        val root = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setBackgroundColor(p.bg)
        }
        setContentView(root)
        window.setBackgroundDrawable(ColorDrawable(p.bg))
        systemBars(nav = p.card)
        val frame = FrameLayout(this)
        root.addView(frame, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 0, 1f))
        val nav = row().apply { setBackgroundColor(p.card) }
        root.addWithMargins(nav)
        // Edge to edge: the pages clear the status bar, the navigation bar reaches under the gesture bar.
        root.setOnApplyWindowInsetsListener { _, insets ->
            @Suppress("DEPRECATION")
            frame.setPadding(insets.systemWindowInsetLeft, insets.systemWindowInsetTop, insets.systemWindowInsetRight, 0)
            @Suppress("DEPRECATION")
            nav.setPadding(insets.systemWindowInsetLeft, 0, insets.systemWindowInsetRight, insets.systemWindowInsetBottom)
            insets
        }

        pages.clear()
        pages += todayPage()
        pages += planPage()
        pages += blockingPage()
        pages += settingsPage()
        pages.forEach { frame.addView(it, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT)) }
        navItems.clear()
        listOf(
            R.drawable.ic_timer to tr("main_today"),
            R.drawable.ic_list to tr("main_plan"),
            R.drawable.ic_lock to tr("main_blocking"),
            R.drawable.ic_settings to tr("main_settings"),
        ).forEachIndexed { i, (icon, text) -> nav.addFill(navItem(i, icon, text)) }

        snap = store.snapshot()
        shownVersion = ""
        shownKind = ""
        shownBlocker = null
        render()
        renderBlocker()
        select(tab, first = true)
    }

    /** One navigation item, M3 style: the icon in a pill that lights up, the label under it. */
    private fun navItem(i: Int, icon: Int, text: String): View {
        val item = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            gravity = Gravity.CENTER_HORIZONTAL
            setPadding(0, dp(12), 0, dp(14))
            setOnClickListener { select(i) }
        }
        val pill = FrameLayout(this)
        val img = iconView(icon, p.muted)
        pill.addView(img, FrameLayout.LayoutParams(dp(24), dp(24), Gravity.CENTER))
        item.addView(pill, LinearLayout.LayoutParams(dp(64), dp(32)))
        val name = label(text, 12f, p.muted, bold = true).apply { gravity = Gravity.CENTER; maxLines = 1 }
        item.addWithMargins(name, top = dp(4))
        navItems += Triple(pill, img, name)
        return item
    }

    private fun select(i: Int, first: Boolean = false) {
        val changed = i != tab
        tab = i
        navItems.forEachIndexed { k, (pill, img, name) ->
            val on = k == i
            pill.background = if (on) rounded(p.secondary, dp(16).toFloat()) else null
            img.imageTintList = ColorStateList.valueOf(if (on) p.onSecondary else p.muted)
            name.setTextColor(if (on) p.text else p.muted)
            if (on && changed) {
                pill.scaleX = 0.5f
                pill.animate().scaleX(1f).setDuration(300).setInterpolator(android.view.animation.OvershootInterpolator(1.5f)).start()
            }
        }
        pages.forEachIndexed { k, v -> v.visibility = if (k == i) View.VISIBLE else View.GONE }
        val page = pages[i]
        if (first) {
            // The screen opens in a cascade.
            val col = (page as ViewGroup).getChildAt(0) as ViewGroup
            (0 until col.childCount).forEach { col.getChildAt(it).riseIn(it * 45L) }
        } else if (changed) {
            page.alpha = 0f
            page.translationY = dp(12).toFloat()
            page.animate().alpha(1f).translationY(0f).setDuration(220).setInterpolator(android.view.animation.DecelerateInterpolator()).start()
        }
        if (i == 2) renderBlocker()
    }

    /** A page: a column that scrolls when it is taller than the screen, its title on top. */
    private fun scrollPage(title: String?): Pair<ScrollView, LinearLayout> {
        val col = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(dp(16), dp(8), dp(16), dp(16))
        }
        if (title != null) col.addWithMargins(label(title, 28f, p.text), top = dp(12))
        val scroll = ScrollView(this).apply {
            isFillViewport = true
            isVerticalScrollBarEnabled = false
            addView(col)
        }
        return scroll to col
    }

    /** Today: the link to the PC, an update when there is one, and the timer card filling the page. */
    private fun todayPage(): View {
        val (page, col) = scrollPage(tr("main_today"))
        val link = row()
        statusDot = dot(p.primary)
        status = label("", 13f, p.muted)
        link.addView(statusDot)
        link.addFill(status, start = dp(8))
        col.addWithMargins(link, top = dp(4))

        updateBox = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            background = rounded(p.container, dp(28).toFloat())
            setPadding(dp(20), dp(16), dp(20), dp(16))
            visibility = View.GONE
        }
        col.addWithMargins(updateBox, top = dp(16))
        col.smoothChanges()

        // The timer card, like the PC's: the lock on top, the dial, the title and the buttons in
        // the middle of what is left of the page.
        hero = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(dp(20), dp(20), dp(20), dp(24))
        }
        val lockRow = row().apply { gravity = Gravity.TOP }
        lockIcon = iconView(R.drawable.ic_lock, p.text, 20)
        val lockText = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
        lockHead = label("", 14f, p.text, bold = true)
        lockTail = label("", 13f, p.text).apply { alpha = 0.8f }
        lockText.addView(lockHead)
        lockText.addView(lockTail)
        lockRow.addView(lockIcon)
        lockRow.addFill(lockText, start = dp(10))
        hero.addWithMargins(lockRow)

        hero.addView(View(this), LinearLayout.LayoutParams(1, 0, 1f))
        dial = Dial(this)
        hero.addWithMargins(dial, top = dp(16))
        title = label("", 24f, p.text).apply { gravity = Gravity.CENTER }
        sub = label("", 14f, p.text).apply { gravity = Gravity.CENTER; alpha = 0.8f }
        hero.addWithMargins(title, top = dp(16))
        hero.addWithMargins(sub, top = dp(4))
        actions = Flow(this)
        hero.addWithMargins(actions, top = dp(20))
        hero.addView(View(this), LinearLayout.LayoutParams(1, 0, 1f))
        col.addView(hero, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 0, 1f).apply { topMargin = dp(16) })
        return page
    }

    /** The plan: how much of the day's study is done, and the blocks. */
    private fun planPage(): View {
        val (page, col) = scrollPage(tr("main_day_plan"))
        total = label("", 14f, p.muted, bold = true)
        col.addWithMargins(total, top = dp(4))
        totalBar = Bar(this).apply { colors(p.primary, p.outlineVariant) }
        col.addWithMargins(totalBar, top = dp(10))
        // Little padding on the card: the current block's highlight reaches almost to its edges.
        rows = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            background = rounded(p.card, dp(28).toFloat())
            setPadding(dp(8), dp(8), dp(8), dp(8))
        }
        col.addWithMargins(rows, top = dp(16))
        return page
    }

    /** Blocking on the phone: whether it works (or how to turn it on) and the blocked apps. */
    private fun blockingPage(): View {
        val (page, col) = scrollPage(tr("main_blocking"))
        val bl = card(p)
        val blHead = row()
        blHead.addView(iconView(R.drawable.ic_shield, p.primary))
        blHead.addFill(label(tr("android_main_blocking_on_the_phone"), 18f, p.text), start = dp(12))
        blockerChip = label("", 12f, p.text, bold = true).apply { setPadding(dp(12), dp(4), dp(12), dp(4)) }
        blHead.addView(blockerChip)
        bl.addWithMargins(blHead)
        blocker = label("", 14f, p.muted)
        bl.addWithMargins(blocker, top = dp(12))
        steps = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
        bl.addWithMargins(steps)
        col.addWithMargins(bl, top = dp(16))

        val apps = card(p)
        val appsHead = row()
        appsHead.addView(iconView(R.drawable.ic_phone, p.primary))
        appsHead.addFill(label(tr("blocking_phone_apps"), 18f, p.text), start = dp(12))
        apps.addWithMargins(appsHead)
        apps.addWithMargins(label(tr("android_apps_during_study_the_checked_apps_close"), 14f, p.muted), top = dp(12))
        appsBtn = pill(tr("blocking_apps"), p, "tonal") { startActivity(Intent(this, AppsActivity::class.java)) }
        apps.addWithMargins(appsBtn, top = dp(16), width = ViewGroup.LayoutParams.WRAP_CONTENT)
        col.addWithMargins(apps, top = dp(12))
        return page
    }

    /** Settings: the PC, notifications, the language and the version. */
    private fun settingsPage(): View {
        val (page, col) = scrollPage(tr("main_settings"))

        val pc = card(p)
        val pcHead = row()
        pcHead.addView(iconView(R.drawable.ic_computer, p.primary))
        val pcText = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
        pcText.addView(label(store.pcName.ifEmpty { store.host }, 18f, p.text))
        pcText.addView(label("${store.host}:${store.port}", 13f, p.muted))
        pcHead.addFill(pcText, start = dp(12))
        pc.addWithMargins(pcHead)
        pc.addWithMargins(pill(tr("android_main_unpair_from_the_pc"), p, "outlined", R.drawable.ic_logout) {
            store.forget()
            store.host = ""
            Sync.stopLive("app")
            pages.clear()
            showPair()
        }, top = dp(16), width = ViewGroup.LayoutParams.WRAP_CONTENT)
        col.addWithMargins(pc, top = dp(16))

        val notifyCard = card(p)
        val notify = Switch(this).apply {
            text = tr("android_main_notifications_the_countdown_in_the_shade")
            textSize = 16f
            setTextColor(p.text)
            val states = arrayOf(intArrayOf(android.R.attr.state_checked), intArrayOf())
            thumbTintList = ColorStateList(states, intArrayOf(p.onPrimary, p.outline))
            trackTintList = ColorStateList(states, intArrayOf(p.primary, p.cardHighest))
            isChecked = store.notify
            setOnCheckedChangeListener { _, on ->
                store.notify = on
                store.snapshot()?.let { Alarms.schedule(this@MainActivity, it) }
            }
        }
        notifyCard.addWithMargins(notify)
        col.addWithMargins(notifyCard, top = dp(12))

        // The language: like the PC (its interface language; the phone's own before pairing or
        // with an older PC), or a fixed one. The language names are written in their own language.
        val langCard = card(p)
        langCard.addWithMargins(label(tr("settings_language"), 18f, p.text))
        val choices = Flow(this).apply { centred = false }
        listOf("" to tr("android_main_like_the_pc"), "ru" to "Русский", "en" to "English").forEach { (code, name) ->
            val on = store.langPref == code
            val chip = pillIn(name, if (on) p.secondary else Color.TRANSPARENT, if (on) p.onSecondary else p.text, if (on) 0 else p.outline, if (on) R.drawable.ic_check else 0) {
                if (store.langPref == code) return@pillIn
                store.langPref = code
                I18n.fixed = code.ifEmpty { null }
                Alarms.channels(this)
                store.snapshot()?.let { Alarms.schedule(this, it) }
                showTimer()
            }
            chip.minHeight = dp(40)
            chip.minimumHeight = dp(40)
            choices.addView(chip)
        }
        langCard.addWithMargins(choices, top = dp(12))
        col.addWithMargins(langCard, top = dp(12))

        val about = card(p)
        val aboutHead = row()
        aboutHead.addFill(label(tr("android_main_version", "v" to Updater.currentName(this)), 16f, p.text))
        aboutHead.addView(pill(tr("settings_check"), p, "tonal") {
            checkUpdate(true)
            Toast.makeText(this, tr("android_main_checking_the_pc_for_an_update"), Toast.LENGTH_SHORT).show()
            select(0)
        })
        about.addWithMargins(aboutHead)
        col.addWithMargins(about, top = dp(12))
        return page
    }

    private fun renderBlocker() {
        val on = Sync.blockerEnabled(this)
        val s = snap
        blockerChip.text = if (on) tr("android_main_state_on") else tr("settings_off")
        blockerChip.setTextColor(if (on) p.onSecondary else p.error)
        blockerChip.background = if (on) rounded(p.secondary, dp(8).toFloat()) else rounded(Color.TRANSPARENT, dp(8).toFloat(), dp(1), p.error)
        blocker.text = when {
            !on -> tr("android_main_the_phone_only_shows_the_timer")
            s == null -> tr("android_main_waiting_for_data_from_the_pc")
            s.blockedAt(store.now(), store.syncedAt) -> tr("android_main_blocking_now_sites_apps", "sites" to s.sites.size, "apps" to s.apps.size)
            else -> tr("blocking_no_lock_right_now")
        }
        appsBtn.text = tr("android_main_apps", "count" to (s?.apps?.size ?: 0))
        if (shownBlocker == on) return
        shownBlocker = on
        steps.removeAllViews()
        if (on) return
        // Off: how to turn it on, step by step. A sideloaded app on Android 13+ first needs
        // "Allow restricted settings", or its switch in Accessibility stays grey.
        val accessibility = pill(tr("android_main_accessibility"), p, "filled", R.drawable.ic_accessibility) {
            startActivity(Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS))
            Toast.makeText(this, tr("android_main_find_clockmanage_blocking_and_turn_it"), Toast.LENGTH_LONG).show()
        }
        steps.addWithMargins(step(1, tr("android_main_open_accessibility_downloaded_apps"), accessibility), top = dp(16))
        if (Build.VERSION.SDK_INT >= 33) {
            val info = pill(tr("android_main_app_info"), p, "tonal", R.drawable.ic_info) {
                startActivity(Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, android.net.Uri.fromParts("package", packageName, null)))
            }
            steps.addWithMargins(step(2, tr("android_main_if_the_switch_is_grey_restricted"), info), top = dp(16))
            steps.addWithMargins(step(3, tr("android_main_come_back_to_accessibility"), null), top = dp(16))
        }
    }

    /** A numbered step: the number in a primary circle, the text and its button under it. */
    private fun step(n: Int, text: String, button: Button?): View {
        val r = row().apply { gravity = Gravity.TOP }
        val num = label(n.toString(), 13f, p.onPrimary, bold = true).apply {
            gravity = Gravity.CENTER
            background = GradientDrawable().apply { shape = GradientDrawable.OVAL; setColor(p.primary) }
        }
        r.addView(num, LinearLayout.LayoutParams(dp(24), dp(24)))
        val body = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
        body.addView(label(text, 14f, p.text))
        if (button != null) body.addWithMargins(button, top = dp(8), width = ViewGroup.LayoutParams.WRAP_CONTENT)
        r.addFill(body, start = dp(12))
        return r
    }

    private fun act(action: String, expect: String?) {
        Sync.action(this, action, expect) { err -> if (err != null) Toast.makeText(this, err, Toast.LENGTH_SHORT).show() }
    }

    /** The timer card's colours for a phase, as on the PC: waiting — primary, a break or a segment — tertiary, done — secondary. */
    private fun paintHero(e: Entry?) {
        val (bg, fg) = when (e?.kind) {
            "await" -> p.container to p.onContainer
            "break", "lunch_break", "segment" -> p.tertiary to p.onTertiary
            "done" -> p.secondary to p.onSecondary
            else -> p.card to p.text
        }
        heroBg = bg
        heroFg = fg
        hero.background = rounded(bg, dp(32).toFloat())
        for (t in listOf(lockHead, lockTail, title, sub)) t.setTextColor(fg)
        lockIcon.imageTintList = ColorStateList.valueOf(fg)
        // On the plain card the progress is the primary colour, on a coloured one the content colour.
        val arc = when {
            e?.paused == true -> p.outline
            bg == p.card -> p.primary
            else -> fg
        }
        dial.colors(arc, withAlpha(fg, 0.14f), fg)
        actions.tag = null
    }

    private fun render() {
        if (!::status.isInitialized) return
        val s = snap
        val now = store.now()
        val hm = SimpleDateFormat("HH:mm", Locale.forLanguageTag("ru"))
        status.text = when {
            Sync.online -> tr("android_main_synced", "pc_name" to store.pcName)
            store.syncedAt > 0 -> tr("android_main_counting_on_my_own_since", "error" to (Sync.lastError ?: tr("android_main_no_connection_to_the_pc")), "time" to hm.format(Date(store.syncedAt - store.offset)))
            else -> Sync.lastError ?: tr("android_main_connecting")
        }
        statusDot.background = GradientDrawable().apply { shape = GradientDrawable.OVAL; setColor(if (Sync.online) p.primary else p.outline) }
        if (s == null) {
            if (shownKind != "none") { paintHero(null); shownKind = "none" }
            lockHead.text = ""
            lockTail.visibility = View.GONE
            dial.state = ""
            dial.time = ""
            dial.progress(-1f, false)
            title.text = tr("android_main_waiting_for_data_from_the_pc")
            sub.visibility = View.GONE
            return
        }
        val e = s.entryAt(now)
        // Phase changed (work -> break -> waiting…): new colours, the dial springs in anew.
        val kind = (e?.kind ?: "") + (e?.paused ?: false)
        if (kind != shownKind) {
            paintHero(e)
            if (shownKind.isNotEmpty()) {
                dial.scaleX = 0.92f; dial.scaleY = 0.92f
                dial.animate().scaleX(1f).scaleY(1f).setDuration(420).setInterpolator(android.view.animation.OvershootInterpolator(1.6f)).start()
            }
            shownKind = kind
        }

        val blocked = s.blockedAt(now, store.syncedAt)
        val focusUntil = s.focusUntil?.takeIf { now < it }
        val (head, tail) = if (blocked) when {
            focusUntil != null && ((s.lockReason != "study" && s.lockReason != "autolock") || focusUntil >= minOf(s.dayEndAt, s.doneAt ?: s.dayEndAt)) ->
                tr("app_focus_lock") to tr("android_main_lock_until_time", "time" to hm.format(Date(focusUntil - store.offset)))
            s.lockReason == "autolock" || !s.started -> tr("android_main_lock_autolock") to tr("android_main_lock_until_start_day_on_the_pc")
            else -> tr("app_tray_lock_on") to tr("android_main_lock_until_or_until_the_plan", "day_end" to s.dayEnd)
        } else when (s.lockReason) {
            "break_access" -> tr("blocking_access_in_the_break") to ""
            "emergency" -> tr("blocking_emergency_access") to ""
            "segment_access" -> tr("blocking_access_open") to (e?.title ?: "")
            "completed" -> tr("android_main_all_blocks_done_no_lock") to ""
            "day_end" -> tr("android_main_no_lock_after", "day_end" to s.dayEnd) to ""
            else -> tr("android_main_no_lock") to ""
        }
        lockHead.text = head
        lockTail.text = tail
        lockTail.visibility = if (tail.isEmpty()) View.GONE else View.VISIBLE
        lockIcon.setImageResource(if (blocked) R.drawable.ic_lock else R.drawable.ic_lock_open)

        var fraction = -1f
        var running = false
        var scale = 1f
        var over = false
        val state: String
        val time: String
        when (e?.kind) {
            "work", "break", "lunch_break" -> {
                state = if (e.paused) tr("today_paused") else if (e.kind == "work") tr("log_work") else tr("log_break")
                time = Fmt.mmss(e.remaining(now))
                fraction = e.elapsed(now).toFloat() / e.durMs.coerceAtLeast(1)
                running = !e.paused
            }
            "await" -> { state = tr("android_main_waiting_for_you"); time = Fmt.mmss(e.elapsed(now)); scale = 0.8f }
            "segment" -> {
                val left = e.remaining(now)
                if (e.stopwatch) {
                    // "Без времени": counts up.
                    state = if (left < 0) tr("today_untimed_longer_than_usual") else tr("mini_untimed")
                    time = Fmt.mmss(e.elapsed(now))
                } else {
                    state = when {
                        e.prep -> if (left < 0) tr("mini_time_to_lie_down") else tr("mini_getting_ready")
                        left < 0 -> tr("mini_over")
                        e.alarm -> tr("today_nap_alarm")
                        else -> tr("today_segment")
                    }
                    time = if (left < 0) "+" + Fmt.mmss(-left) else Fmt.mmss(left)
                    over = left < 0
                    if (!e.prep) {
                        fraction = (now - e.from).toFloat() / e.durMs.coerceAtLeast(1)
                        running = !over
                    }
                }
            }
            "lunch" -> { state = tr("log_lunch"); time = Fmt.mmss(e.elapsed(now)); scale = 0.8f }
            "done" -> { state = tr("today_done"); time = "✓"; fraction = 1f; scale = 0.9f }
            else -> {
                state = tr("today_planned")
                time = Fmt.dur(s.blocks.filter { !it.isBreak }.sumOf { it.minutes } * Snapshot.MIN)
                scale = 0.5f
            }
        }
        dial.state = state.uppercase(if (I18n.ru) Locale("ru") else Locale.US)
        dial.time = time
        // An hour and more ("1:05:00") takes a smaller size so it stays inside the ring.
        dial.timeScale = if (time.length > 6) scale * 0.78f else scale
        dial.timeColor = if (over) p.error else 0
        dial.progress(fraction, running)
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
                val b = when {
                    i > 0 -> pillIn(text, withAlpha(heroFg, 0.14f), heroFg, icon = icons[i]) { act(a, expect) }
                    heroBg == p.card -> pillIn(text, p.primary, p.onPrimary, icon = icons[i]) { act(a, expect) }
                    else -> pillIn(text, heroFg, heroBg, icon = icons[i]) { act(a, expect) }
                }
                b.minHeight = dp(52)
                b.minimumHeight = dp(52)
                b.isEnabled = Sync.online
                actions.addView(b)
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
            background = GradientDrawable().apply {
                shape = GradientDrawable.OVAL
                if (current) setColor(p.onSecondary) else setStroke(dp(2), p.muted)
            }
            layoutParams = LinearLayout.LayoutParams(dp(12), dp(12)).apply { marginStart = dp(4); marginEnd = dp(4) }
        }
    }

    private fun buildRows(s: Snapshot, current: Int?) {
        rows.removeAllViews()
        rowViews.clear()
        if (s.blocks.isEmpty()) rows.addWithMargins(label(tr("android_main_todays_plan_is_empty_make_it"), 14f, p.muted).apply { setPadding(dp(12), dp(12), dp(12), dp(12)) })
        val editable = s.canEditPlan && Sync.online
        s.blocks.forEachIndexed { i, b ->
            val now = i == current && !b.done
            // The block running now stands out, like the PC's highlighted row.
            val item = row().apply {
                setPadding(dp(12), dp(10), dp(12), dp(10))
                if (now) background = rounded(p.secondary, dp(20).toFloat())
            }
            item.addView(mark(b, now))
            val text = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
            text.addView(label(b.name, 16f, if (now) p.onSecondary else if (b.isBreak) p.muted else p.text, bold = !b.isBreak))
            val meta = label("", 13f, if (now) p.onSecondary else p.muted)
            text.addWithMargins(meta, top = dp(2))
            val bar = Bar(this).apply { colors(if (now) p.onSecondary else p.primary, if (now) withAlpha(p.onSecondary, 0.2f) else p.outlineVariant) }
            if (!b.isBreak) text.addWithMargins(bar, top = dp(8))
            item.addFill(text, start = dp(14))
            if (!b.isBreak && editable) {
                // − / + like the PC's steppers. During the lock a started block can't go below the time already worked.
                val lower = (b.minutes - STEP).coerceAtLeast(b.minMinutes.coerceAtLeast(1))
                val higher = (b.minutes + STEP).coerceAtMost(480)
                val toast: (String?) -> Unit = { err -> err?.let { Toast.makeText(this, it, Toast.LENGTH_SHORT).show() } }
                val fg = if (now) p.onSecondary else p.text
                val step = row().apply { background = rounded(if (now) withAlpha(p.onSecondary, 0.12f) else p.cardHighest, dp(20).toFloat()) }
                val minus = iconButton("−", Color.TRANSPARENT, fg) { Sync.setMinutes(this, i, lower, toast) }
                val plus = iconButton("+", Color.TRANSPARENT, fg) { Sync.setMinutes(this, i, higher, toast) }
                minus.isEnabled = !b.done && lower < b.minutes
                plus.isEnabled = higher > b.minutes
                for (btn in listOf(minus, plus)) {
                    btn.alpha = if (btn.isEnabled) 1f else 0.35f
                    step.addView(btn, LinearLayout.LayoutParams(dp(44), dp(40)))
                }
                item.addView(step, LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply { marginStart = dp(12) })
            }
            rows.addWithMargins(item, top = if (i == 0) 0 else dp(4))
            rowViews += meta to bar
        }
    }

    companion object {
        private const val STEP = 15
    }
}
