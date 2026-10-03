package com.nikgob.clockmanage

import android.Manifest
import android.app.Activity
import android.content.Intent
import android.graphics.Typeface
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
import android.widget.LinearLayout
import android.widget.ProgressBar
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
    private lateinit var lock: TextView
    private lateinit var state: TextView
    private lateinit var big: TextView
    private lateinit var title: TextView
    private lateinit var sub: TextView
    private lateinit var actions: LinearLayout
    private lateinit var total: TextView
    private lateinit var rows: LinearLayout
    private lateinit var blocker: TextView
    private lateinit var blockerBtn: View
    private lateinit var restrictedBtn: View
    private lateinit var appsBtn: android.widget.Button
    private lateinit var updateBox: LinearLayout
    private val rowViews = mutableListOf<Pair<TextView, ProgressBar>>()

    private val onSync: (Snapshot?, String?) -> Unit = { s, _ ->
        if (!store.paired) showPair() else { snap = s ?: snap; render() }
    }

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
            Sync.startLive(this)
            ui.post(tick)
            checkUpdate(false)
        }
    }

    /** An update card when the PC carries a newer APK (checked at most once an hour). */
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
        updateBox.addWithMargins(label("Доступна версия ${a.name}", 16f, p.onContainer, bold = true))
        val info = label("С ПК по Wi-Fi, ${"%.1f".format(a.size / 1048576.0)} МБ. Настройки и подключение сохранятся.", 14f, p.onContainer)
        updateBox.addWithMargins(info, top = dp(2))
        val go = pill("Обновить", p) {}
        go.setOnClickListener {
            if (!Updater.canInstall(this)) {
                Toast.makeText(this, "Разреши ClockManage устанавливать приложения и вернись", Toast.LENGTH_LONG).show()
                Updater.openInstallPermission(this)
                return@setOnClickListener
            }
            go.isEnabled = false
            Thread {
                val err = Updater.downloadAndInstall(this) { pct -> ui.post { go.text = "Скачиваю… $pct%" } }
                ui.post {
                    go.isEnabled = true
                    go.text = "Обновить"
                    if (err != null) info.text = err
                }
            }.start()
        }
        updateBox.addWithMargins(go, top = dp(10), width = ViewGroup.LayoutParams.WRAP_CONTENT)
    }

    override fun onStop() {
        visible = false
        Sync.unlisten(onSync)
        Sync.stopLive()
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
        // Edge to edge on Android 15: keep content clear of the bars.
        scroll.setOnApplyWindowInsetsListener { v, insets ->
            @Suppress("DEPRECATION")
            v.setPadding(0, insets.systemWindowInsetTop, 0, insets.systemWindowInsetBottom)
            insets
        }
        return col
    }

    // ---------------- pairing ----------------

    private fun showPair() {
        ui.removeCallbacks(tick)
        val col = page()
        col.addWithMargins(label("ClockManage", 32f, p.text, bold = true))
        col.addWithMargins(label("Подключи телефон к ПК в той же сети Wi-Fi.", 16f, p.muted), top = dp(8))
        col.addWithMargins(
            label("На ПК: Настройки → Телефон → включи синхронизацию и нажми «Подключить телефон». Потом выбери ПК здесь и введи PIN.", 15f, p.muted),
            top = dp(16),
        )

        var host = store.host
        var port = store.port
        val chosen = label(if (host.isNotEmpty()) "ПК: ${store.pcName.ifEmpty { host }}" else "ПК не выбран", 16f, p.text, bold = true)
        val found = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
        val find = pill("Найти ПК", p, "tonal") {}
        col.addWithMargins(find, top = dp(24), width = ViewGroup.LayoutParams.WRAP_CONTENT)
        col.addWithMargins(found, top = dp(8))
        find.setOnClickListener {
            find.isEnabled = false
            find.text = "Ищу…"
            Thread {
                val list = Discovery.find()
                ui.post {
                    find.isEnabled = true
                    find.text = "Найти ещё раз"
                    found.removeAllViews()
                    if (list.isEmpty()) found.addWithMargins(label("Не нашёл. Проверь, что ПК в той же сети и синхронизация включена, — или введи адрес вручную.", 14f, p.error))
                    list.forEach { pc ->
                        found.addWithMargins(pill("${pc.name} · ${pc.host}", p, "text") {
                            host = pc.host
                            port = pc.port
                            store.pcName = pc.name
                            chosen.text = "ПК: ${pc.name}"
                        }, width = ViewGroup.LayoutParams.WRAP_CONTENT)
                    }
                }
            }.start()
        }

        val manual = EditText(this).apply {
            hint = "или адрес вручную: 192.168.1.5:47811"
            inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_URI
            setSingleLine()
        }
        col.addWithMargins(manual, top = dp(8))
        col.addWithMargins(chosen, top = dp(20))
        val pin = EditText(this).apply {
            hint = "PIN с экрана ПК"
            inputType = InputType.TYPE_CLASS_NUMBER
            textSize = 24f
            letterSpacing = 0.2f
            filters = arrayOf(android.text.InputFilter.LengthFilter(6))
        }
        col.addWithMargins(pin, top = dp(8))
        val err = label("", 14f, p.error)
        val go = pill("Подключить", p) {}
        col.addWithMargins(go, top = dp(16), width = ViewGroup.LayoutParams.WRAP_CONTENT)
        col.addWithMargins(err, top = dp(8))
        go.setOnClickListener {
            val typed = manual.text.toString().trim()
            if (typed.isNotEmpty()) {
                host = typed.substringBefore(':')
                port = typed.substringAfter(':', "47811").toIntOrNull() ?: 47811
                store.pcName = host
            }
            if (host.isEmpty()) { err.text = "Сначала найди ПК или введи адрес"; return@setOnClickListener }
            val code = pin.text.toString().trim()
            if (code.length != 6) { err.text = "PIN — 6 цифр"; return@setOnClickListener }
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
                        Toast.makeText(this, "Подключено к $name", Toast.LENGTH_SHORT).show()
                        showTimer()
                        Sync.startLive(this)
                        ui.post(tick)
                    }.onFailure { err.text = it.message ?: "Не получилось" }
                }
            }.start()
        }
    }

    // ---------------- timer ----------------

    private fun showTimer() {
        val col = page()
        status = label("", 13f, p.muted)
        col.addWithMargins(status)
        lock = label("", 14f, p.muted)
        col.addWithMargins(lock, top = dp(4))
        updateBox = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            background = rounded(p.container, dp(20).toFloat())
            setPadding(dp(20), dp(14), dp(20), dp(14))
            visibility = View.GONE
        }
        col.addWithMargins(updateBox, top = dp(12))
        col.smoothChanges()

        val hero = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            gravity = Gravity.CENTER_HORIZONTAL
        }
        state = label("", 13f, p.primary, bold = true).apply { isAllCaps = true; letterSpacing = 0.1f }
        big = label("", 72f, p.text).apply {
            typeface = Typeface.create("sans-serif-light", Typeface.NORMAL)
            fontFeatureSettings = "tnum"
        }
        title = label("", 22f, p.text, bold = true).apply { gravity = Gravity.CENTER }
        sub = label("", 15f, p.muted).apply { gravity = Gravity.CENTER }
        actions = row().apply { gravity = Gravity.CENTER }
        hero.addWithMargins(state, width = ViewGroup.LayoutParams.WRAP_CONTENT)
        hero.addWithMargins(big, width = ViewGroup.LayoutParams.WRAP_CONTENT)
        hero.addWithMargins(title, top = dp(4))
        hero.addWithMargins(sub, top = dp(4))
        hero.addWithMargins(actions, top = dp(20))
        col.addWithMargins(hero, top = dp(28), bottom = dp(28))

        val plan = card(p)
        plan.addWithMargins(label("План дня", 18f, p.text, bold = true))
        total = label("", 14f, p.muted)
        plan.addWithMargins(total, top = dp(2))
        rows = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
        plan.addWithMargins(rows, top = dp(8))
        col.addWithMargins(plan)

        val bl = card(p)
        bl.addWithMargins(label("Блокировка на телефоне", 18f, p.text, bold = true))
        blocker = label("", 14f, p.muted)
        bl.addWithMargins(blocker, top = dp(4))
        blockerBtn = pill("Включить в спецвозможностях", p, "tonal") {
            startActivity(Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS))
            Toast.makeText(this, "Найди «ClockManage — блокировка» и включи", Toast.LENGTH_LONG).show()
        }
        bl.addWithMargins(blockerBtn, top = dp(12), width = ViewGroup.LayoutParams.WRAP_CONTENT)
        // Android 13+ greys out accessibility for sideloaded apps until "restricted settings" are
        // allowed on the app's info page.
        restrictedBtn = pill("О приложении", p, "text") {
            startActivity(Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, android.net.Uri.fromParts("package", packageName, null)))
        }
        bl.addWithMargins(restrictedBtn, top = dp(4), width = ViewGroup.LayoutParams.WRAP_CONTENT)
        appsBtn = pill("Приложения", p, "text") { startActivity(Intent(this, AppsActivity::class.java)) }
        bl.addWithMargins(appsBtn, top = dp(4), width = ViewGroup.LayoutParams.WRAP_CONTENT)
        val notify = Switch(this).apply {
            text = "Уведомления: отсчёт в шторке и конец перерыва"
            setTextColor(p.text)
            isChecked = store.notify
            setOnCheckedChangeListener { _, on ->
                store.notify = on
                store.snapshot()?.let { Alarms.schedule(this@MainActivity, it) }
            }
        }
        bl.addWithMargins(notify, top = dp(8))
        col.addWithMargins(bl, top = dp(16))

        col.addWithMargins(pill("Отключиться от ПК", p, "text") {
            store.forget()
            store.host = ""
            Sync.stopLive()
            showPair()
        }, top = dp(16), width = ViewGroup.LayoutParams.WRAP_CONTENT)
        col.addWithMargins(label("Версия ${Updater.currentName(this)}", 12f, p.muted).apply {
            setOnClickListener { checkUpdate(true); Toast.makeText(this@MainActivity, "Проверяю обновление на ПК…", Toast.LENGTH_SHORT).show() }
        }, top = dp(8))

        snap = store.snapshot()
        shownVersion = ""
        render()
        renderBlocker()
        // The screen opens in a cascade.
        (0 until col.childCount).forEach { col.getChildAt(it).riseIn(it * 45L) }
    }

    private fun renderBlocker() {
        val on = Sync.blockerEnabled(this)
        val s = snap
        blocker.text = when {
            !on && Build.VERSION.SDK_INT >= 33 -> "Выключена. Без неё телефон только показывает таймер.\n\n" +
                "Если переключатель серый и пишет «Ограниченная настройка»: «О приложении» → ⋮ справа сверху → " +
                "«Разрешить ограниченные настройки», потом включи снова."
            !on -> "Выключена. Без неё телефон только показывает таймер."
            s == null -> "Включена. Ждёт данных с ПК."
            s.blockedAt(store.now(), store.syncedAt) -> "Включена и работает: ${s.sites.size} сайтов, ${s.apps.size} приложений."
            else -> "Включена. Сейчас блокировки нет."
        }
        blockerBtn.visibility = if (on) View.GONE else View.VISIBLE
        restrictedBtn.visibility = if (!on && Build.VERSION.SDK_INT >= 33) View.VISIBLE else View.GONE
        appsBtn.text = "Приложения (${s?.apps?.size ?: 0})"
    }

    private fun act(action: String, expect: String?) {
        Sync.action(this, action, expect) { err -> if (err != null) Toast.makeText(this, err, Toast.LENGTH_SHORT).show() }
    }

    private fun render() {
        if (!::status.isInitialized) return
        val s = snap
        val now = store.now()
        val hm = SimpleDateFormat("HH:mm", Locale.forLanguageTag("ru"))
        status.text = when {
            Sync.online -> "● ${store.pcName} · синхронизировано"
            store.syncedAt > 0 -> "○ ${Sync.lastError ?: "Нет связи с ПК"} — считаю сам · связь была в ${hm.format(Date(store.syncedAt - store.offset))}"
            else -> "○ ${Sync.lastError ?: "Подключаюсь…"}"
        }
        status.setTextColor(if (Sync.online) p.primary else p.muted)
        if (s == null) {
            title.text = "Жду данных с ПК"
            return
        }
        lock.text = if (s.blockedAt(now, store.syncedAt)) "🔒 Блокировка включена · до ${s.dayEnd} или до конца плана" else when (s.lockReason) {
            "pause_access" -> "Доступ на паузе открыт"
            "emergency" -> "Аварийный доступ"
            "completed" -> "Все блоки отсижены — блокировки нет"
            "day_end" -> "После ${s.dayEnd} блокировки нет"
            else -> "Без блокировки"
        }

        val e = s.entryAt(now)
        when (e?.kind) {
            "work", "break", "lunch_break" -> {
                state.text = if (e.paused) "пауза" else if (e.kind == "work") "работа" else "перерыв"
                big.text = Fmt.mmss(e.remaining(now))
                big.textSize = 72f
            }
            "await" -> { state.text = "ждём тебя"; big.text = Fmt.mmss(e.elapsed(now)); big.textSize = 56f }
            "segment" -> {
                val left = e.remaining(now)
                state.text = if (left < 0) "превышено" else if (e.alarm) "сон · будильник" else "отрезок"
                big.text = if (left < 0) "+" + Fmt.mmss(-left) else Fmt.mmss(left)
                big.setTextColor(if (left < 0) p.error else p.text)
                big.textSize = 72f
            }
            "lunch" -> { state.text = "обед"; big.text = Fmt.mmss(e.elapsed(now)); big.textSize = 56f }
            "done" -> { state.text = "готово"; big.text = "✓"; big.textSize = 56f }
            else -> {
                state.text = "в плане"
                big.text = Fmt.dur(s.blocks.filter { !it.isBreak }.sumOf { it.minutes } * Snapshot.MIN)
                big.textSize = 44f
            }
        }
        if (e?.kind != "segment") big.setTextColor(p.text)
        title.text = e?.title ?: ""
        sub.text = e?.subtitle ?: ""
        // Phase changed (work -> break -> waiting…): the timer drops in anew.
        val kind = (e?.kind ?: "") + (e?.paused ?: false)
        if (kind != shownKind) {
            if (shownKind.isNotEmpty()) {
                big.scaleX = 0.8f; big.scaleY = 0.8f; big.alpha = 0f
                big.animate().scaleX(1f).scaleY(1f).alpha(1f).setDuration(420).setInterpolator(android.view.animation.OvershootInterpolator(1.6f)).start()
                state.alpha = 0f
                state.animate().alpha(1f).setDuration(300).start()
            }
            shownKind = kind
        }

        // Buttons: rebuilt only when what they do changes.
        val wanted = mutableListOf<Triple<String, String, String?>>()
        when {
            e == null || e.kind == "idle" -> if (s.canStartDay) wanted += Triple("Начать день", "start_day", null)
            e.paused -> wanted += Triple("Продолжить", "resume", null)
            e.kind == "work" || e.kind == "break" -> wanted += Triple("Пауза", "pause", null)
            e.kind == "await" || e.kind == "lunch" -> wanted += Triple("Начать", "start_next", e.kind)
            e.segment -> wanted += Triple(if (e.alarm) "Встал" else "Закончил", "end_segment", "segment")
        }
        val sig = wanted.joinToString { it.second } + Sync.online
        if (actions.tag != sig) {
            actions.tag = sig
            actions.removeAllViews()
            wanted.forEachIndexed { i, (text, a, expect) ->
                val b = pill(text, p) { act(a, expect) }.apply { isEnabled = Sync.online }
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
        total.text = "${Fmt.dur(work)} из ${Fmt.hours(studyMin)} учёбы"
        if (shownVersion != s.version + Sync.online) {
            shownVersion = s.version + Sync.online
            buildRows(s)
        }
        s.blocks.forEachIndexed { i, b ->
            val (meta, bar) = rowViews.getOrNull(i) ?: return@forEachIndexed
            if (b.isBreak) {
                meta.text = "${b.minutes} мин · " + when { b.done -> "прошёл"; b.started -> "идёт"; else -> "отрезок, не учёба" }
                return@forEachIndexed
            }
            val w = e?.work(i, now) ?: 0L
            meta.text = "${w / Snapshot.MIN} из ${b.minutes} мин" + if (b.done) " · готово" else ""
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

    private fun buildRows(s: Snapshot) {
        rows.removeAllViews()
        rowViews.clear()
        if (s.blocks.isEmpty()) rows.addWithMargins(label("План на сегодня пуст — составь его на ПК.", 14f, p.muted))
        val editable = s.canEditPlan && Sync.online
        s.blocks.forEachIndexed { i, b ->
            val r = row()
            val text = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
            text.addView(label((if (b.done) "✓ " else if (b.isBreak) "☕ " else "") + b.name, 16f, if (b.isBreak) p.muted else p.text, bold = !b.isBreak))
            val meta = label("", 13f, p.muted)
            text.addView(meta)
            r.addView(text, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f))
            // During the lock a started block can't go below the time already worked.
            val lower = (b.minutes - STEP).coerceAtLeast(b.minMinutes.coerceAtLeast(1))
            val higher = (b.minutes + STEP).coerceAtMost(480)
            val toast: (String?) -> Unit = { err -> err?.let { Toast.makeText(this, it, Toast.LENGTH_SHORT).show() } }
            val minus = pill("−", p, "text") { Sync.setMinutes(this, i, lower, toast) }
            val plus = pill("+", p, "text") { Sync.setMinutes(this, i, higher, toast) }
            minus.isEnabled = editable && !b.done && !b.isBreak && lower < b.minutes
            plus.isEnabled = editable && !b.isBreak && higher > b.minutes
            if (b.isBreak) { minus.visibility = View.INVISIBLE; plus.visibility = View.INVISIBLE }
            minus.alpha = if (minus.isEnabled) 1f else 0.35f
            plus.alpha = if (plus.isEnabled) 1f else 0.35f
            r.addView(minus, LinearLayout.LayoutParams(dp(52), dp(48)))
            r.addView(plus, LinearLayout.LayoutParams(dp(52), dp(48)))
            rows.addWithMargins(r, top = dp(8))
            val bar = ProgressBar(this, null, android.R.attr.progressBarStyleHorizontal).apply {
                max = 1000
                progressTintList = android.content.res.ColorStateList.valueOf(p.primary)
                progressBackgroundTintList = android.content.res.ColorStateList.valueOf(p.outline)
            }
            rows.addWithMargins(bar, top = dp(2))
            if (b.isBreak) bar.visibility = View.GONE
            rowViews += meta to bar
        }
    }

    companion object {
        private const val STEP = 15
    }
}
