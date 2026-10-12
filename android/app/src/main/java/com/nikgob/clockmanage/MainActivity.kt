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
        if (!store.paired) showPair() else {
            snap = s ?: snap
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
            Sync.startLive(this)
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
        updateBox.addWithMargins(label(L("Доступна версия ${a.name}", "Version ${a.name} is available"), 16f, p.onContainer, bold = true))
        val info = label(L("С ПК по Wi-Fi, ${"%.1f".format(a.size / 1048576.0)} МБ. Настройки и подключение сохранятся.", "From the PC over Wi-Fi, ${"%.1f".format(Locale.US, a.size / 1048576.0)} MB. Settings and pairing are kept."), 14f, p.onContainer)
        updateBox.addWithMargins(info, top = dp(2))
        val go = pill(L("Обновить", "Update"), p) {}
        go.setOnClickListener {
            if (!Updater.canInstall(this)) {
                Toast.makeText(this, L("Разреши ClockManage устанавливать приложения и вернись", "Allow ClockManage to install apps and come back"), Toast.LENGTH_LONG).show()
                Updater.openInstallPermission(this)
                return@setOnClickListener
            }
            go.isEnabled = false
            Thread {
                val err = Updater.downloadAndInstall(this) { pct -> ui.post { go.text = L("Скачиваю… $pct%", "Downloading… $pct%") } }
                ui.post {
                    go.isEnabled = true
                    go.text = L("Обновить", "Update")
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
        col.addWithMargins(label(L("Подключи телефон к ПК в той же сети Wi-Fi.", "Pair the phone with a PC on the same Wi-Fi network."), 16f, p.muted), top = dp(8))
        col.addWithMargins(
            label(L("На ПК: Настройки → Телефон → включи синхронизацию и нажми «Подключить телефон». Потом выбери ПК здесь и введи PIN.", "On the PC: Settings → Phone → turn on sync and press “Pair a phone”. Then pick the PC here and enter the PIN."), 15f, p.muted),
            top = dp(16),
        )

        var host = store.host
        var port = store.port
        val chosen = label(if (host.isNotEmpty()) L("ПК: ${store.pcName.ifEmpty { host }}", "PC: ${store.pcName.ifEmpty { host }}") else L("ПК не выбран", "No PC selected"), 16f, p.text, bold = true)
        val found = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
        val find = pill(L("Найти ПК", "Find PC"), p, "tonal") {}
        col.addWithMargins(find, top = dp(24), width = ViewGroup.LayoutParams.WRAP_CONTENT)
        col.addWithMargins(found, top = dp(8))
        find.setOnClickListener {
            find.isEnabled = false
            find.text = L("Ищу…", "Searching…")
            Thread {
                val list = Discovery.find()
                ui.post {
                    find.isEnabled = true
                    find.text = L("Найти ещё раз", "Search again")
                    found.removeAllViews()
                    if (list.isEmpty()) found.addWithMargins(label(L("Не нашёл. Проверь, что ПК в той же сети и синхронизация включена, — или введи адрес вручную.", "Not found. Check that the PC is on the same network and sync is on — or enter the address by hand."), 14f, p.error))
                    list.forEach { pc ->
                        found.addWithMargins(pill("${pc.name} · ${pc.host}", p, "text") {
                            host = pc.host
                            port = pc.port
                            store.pcName = pc.name
                            chosen.text = L("ПК: ${pc.name}", "PC: ${pc.name}")
                        }, width = ViewGroup.LayoutParams.WRAP_CONTENT)
                    }
                }
            }.start()
        }

        val manual = EditText(this).apply {
            hint = L("или адрес вручную: 192.168.1.5:47811", "or the address by hand: 192.168.1.5:47811")
            inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_URI
            setSingleLine()
        }
        col.addWithMargins(manual, top = dp(8))
        col.addWithMargins(chosen, top = dp(20))
        val pin = EditText(this).apply {
            hint = L("PIN с экрана ПК", "PIN from the PC screen")
            inputType = InputType.TYPE_CLASS_NUMBER
            textSize = 24f
            letterSpacing = 0.2f
            filters = arrayOf(android.text.InputFilter.LengthFilter(6))
        }
        col.addWithMargins(pin, top = dp(8))
        val err = label("", 14f, p.error)
        val go = pill(L("Подключить", "Pair"), p) {}
        col.addWithMargins(go, top = dp(16), width = ViewGroup.LayoutParams.WRAP_CONTENT)
        col.addWithMargins(err, top = dp(8))
        go.setOnClickListener {
            val typed = manual.text.toString().trim()
            if (typed.isNotEmpty()) {
                host = typed.substringBefore(':')
                port = typed.substringAfter(':', "47811").toIntOrNull() ?: 47811
                store.pcName = host
            }
            if (host.isEmpty()) { err.text = L("Сначала найди ПК или введи адрес", "Find the PC or enter its address first"); return@setOnClickListener }
            val code = pin.text.toString().trim()
            if (code.length != 6) { err.text = L("PIN — 6 цифр", "The PIN is 6 digits"); return@setOnClickListener }
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
                        Toast.makeText(this, L("Подключено к $name", "Paired with $name"), Toast.LENGTH_SHORT).show()
                        showTimer()
                        Sync.startLive(this)
                        ui.post(tick)
                    }.onFailure { err.text = it.message ?: L("Не получилось", "Didn’t work") }
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
        plan.addWithMargins(label(L("План дня", "Day plan"), 18f, p.text, bold = true))
        total = label("", 14f, p.muted)
        plan.addWithMargins(total, top = dp(2))
        rows = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
        plan.addWithMargins(rows, top = dp(8))
        col.addWithMargins(plan)

        val bl = card(p)
        bl.addWithMargins(label(L("Блокировка на телефоне", "Blocking on the phone"), 18f, p.text, bold = true))
        blocker = label("", 14f, p.muted)
        bl.addWithMargins(blocker, top = dp(4))
        blockerBtn = pill(L("Включить в спецвозможностях", "Turn on in Accessibility"), p, "tonal") {
            startActivity(Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS))
            Toast.makeText(this, L("Найди «ClockManage — блокировка» и включи", "Find “ClockManage — blocking” and turn it on"), Toast.LENGTH_LONG).show()
        }
        bl.addWithMargins(blockerBtn, top = dp(12), width = ViewGroup.LayoutParams.WRAP_CONTENT)
        // Android 13+ greys out accessibility for sideloaded apps until "restricted settings" are
        // allowed on the app's info page.
        restrictedBtn = pill(L("О приложении", "App info"), p, "text") {
            startActivity(Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, android.net.Uri.fromParts("package", packageName, null)))
        }
        bl.addWithMargins(restrictedBtn, top = dp(4), width = ViewGroup.LayoutParams.WRAP_CONTENT)
        appsBtn = pill(L("Приложения", "Apps"), p, "text") { startActivity(Intent(this, AppsActivity::class.java)) }
        bl.addWithMargins(appsBtn, top = dp(4), width = ViewGroup.LayoutParams.WRAP_CONTENT)
        val notify = Switch(this).apply {
            text = L("Уведомления: отсчёт в шторке и конец перерыва", "Notifications: the countdown in the shade and the break end")
            setTextColor(p.text)
            isChecked = store.notify
            setOnCheckedChangeListener { _, on ->
                store.notify = on
                store.snapshot()?.let { Alarms.schedule(this@MainActivity, it) }
            }
        }
        bl.addWithMargins(notify, top = dp(8))
        col.addWithMargins(bl, top = dp(16))

        col.addWithMargins(pill(L("Отключиться от ПК", "Unpair from the PC"), p, "text") {
            store.forget()
            store.host = ""
            Sync.stopLive()
            showPair()
        }, top = dp(16), width = ViewGroup.LayoutParams.WRAP_CONTENT)
        col.addWithMargins(label(L("Версия ${Updater.currentName(this)}", "Version ${Updater.currentName(this)}"), 12f, p.muted).apply {
            setOnClickListener { checkUpdate(true); Toast.makeText(this@MainActivity, L("Проверяю обновление на ПК…", "Checking the PC for an update…"), Toast.LENGTH_SHORT).show() }
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
            !on && Build.VERSION.SDK_INT >= 33 -> L("Выключена. Без неё телефон только показывает таймер.\n\n", "Off. Without it the phone only shows the timer.\n\n") +
                L("Если переключатель серый и пишет «Ограниченная настройка»: «О приложении» → ⋮ справа сверху → ", "If the switch is grey and says “Restricted setting”: “App info” → ⋮ at the top right → ") +
                L("«Разрешить ограниченные настройки», потом включи снова.", "“Allow restricted settings”, then turn it on again.")
            !on -> L("Выключена. Без неё телефон только показывает таймер.", "Off. Without it the phone only shows the timer.")
            s == null -> L("Включена. Ждёт данных с ПК.", "On. Waiting for data from the PC.")
            s.blockedAt(store.now(), store.syncedAt) -> L("Включена и работает: ${s.sites.size} сайтов, ${s.apps.size} приложений.", "On and working: ${s.sites.size} sites, ${s.apps.size} apps.")
            else -> L("Включена. Сейчас блокировки нет.", "On. No lock right now.")
        }
        blockerBtn.visibility = if (on) View.GONE else View.VISIBLE
        restrictedBtn.visibility = if (!on && Build.VERSION.SDK_INT >= 33) View.VISIBLE else View.GONE
        appsBtn.text = L("Приложения (${s?.apps?.size ?: 0})", "Apps (${s?.apps?.size ?: 0})")
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
            Sync.online -> L("● ${store.pcName} · синхронизировано", "● ${store.pcName} · synced")
            store.syncedAt > 0 -> "○ ${Sync.lastError ?: L("Нет связи с ПК", "No connection to the PC")} — ${L("считаю сам · связь была в", "counting on my own · last synced at")} ${hm.format(Date(store.syncedAt - store.offset))}"
            else -> "○ ${Sync.lastError ?: L("Подключаюсь…", "Connecting…")}"
        }
        status.setTextColor(if (Sync.online) p.primary else p.muted)
        if (s == null) {
            title.text = L("Жду данных с ПК", "Waiting for data from the PC")
            return
        }
        val focusUntil = s.focusUntil?.takeIf { now < it }
        lock.text = if (s.blockedAt(now, store.syncedAt)) {
            if (focusUntil != null && (s.lockReason != "study" || focusUntil >= minOf(s.dayEndAt, s.doneAt ?: s.dayEndAt))) L("🔒 Фокус-блокировка · до ${hm.format(Date(focusUntil - store.offset))}", "🔒 Focus lock · until ${hm.format(Date(focusUntil - store.offset))}")
            else L("🔒 Блокировка включена · до ${s.dayEnd} или до конца плана", "🔒 Lock on · until ${s.dayEnd} or until the plan is done")
        } else when (s.lockReason) {
            "pause_access" -> L("Доступ на паузе открыт", "Access on pause is open")
            "emergency" -> L("Аварийный доступ", "Emergency access")
            "completed" -> L("Все блоки отсижены — блокировки нет", "All blocks done — no lock")
            "day_end" -> L("После ${s.dayEnd} блокировки нет", "No lock after ${s.dayEnd}")
            else -> L("Без блокировки", "No lock")
        }

        val e = s.entryAt(now)
        when (e?.kind) {
            "work", "break", "lunch_break" -> {
                state.text = if (e.paused) L("пауза", "paused") else if (e.kind == "work") L("работа", "work") else L("перерыв", "break")
                big.text = Fmt.mmss(e.remaining(now))
                big.textSize = 72f
            }
            "await" -> { state.text = L("ждём тебя", "waiting for you"); big.text = Fmt.mmss(e.elapsed(now)); big.textSize = 56f }
            "segment" -> {
                val left = e.remaining(now)
                if (e.stopwatch) {
                    // "Без времени": counts up.
                    state.text = if (left < 0) L("без времени · дольше обычного", "untimed · longer than usual") else L("без времени", "untimed")
                    big.text = Fmt.mmss(e.elapsed(now))
                    big.setTextColor(p.text)
                } else {
                    state.text = when {
                        e.prep -> if (left < 0) L("пора ложиться", "time to lie down") else L("подготовка", "getting ready")
                        left < 0 -> L("превышено", "over")
                        e.alarm -> L("сон · будильник", "nap · alarm")
                        else -> L("отрезок", "segment")
                    }
                    big.text = if (left < 0) "+" + Fmt.mmss(-left) else Fmt.mmss(left)
                    big.setTextColor(if (left < 0) p.error else p.text)
                }
                big.textSize = 72f
            }
            "lunch" -> { state.text = L("обед", "lunch"); big.text = Fmt.mmss(e.elapsed(now)); big.textSize = 56f }
            "done" -> { state.text = L("готово", "done"); big.text = "✓"; big.textSize = 56f }
            else -> {
                state.text = L("в плане", "planned")
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
            e == null || e.kind == "idle" -> if (s.canStartDay) wanted += Triple(L("Начать день", "Start day"), "start_day", null)
            e.paused -> wanted += Triple(L("Продолжить", "Resume"), "resume", null)
            e.kind == "work" || e.kind == "break" -> wanted += Triple(L("Пауза", "Pause"), "pause", null)
            e.kind == "await" || e.kind == "lunch" -> wanted += Triple(L("Начать", "Start"), "start_next", e.kind)
            e.segment && e.prep -> {
                wanted += Triple(L("Лёг", "In bed"), "lay_down", "segment")
                wanted += Triple(L("Не буду спать", "Skip the nap"), "end_segment", "segment")
            }
            e.segment -> wanted += Triple(if (e.alarm) L("Встал", "I’m up") else L("Закончил", "Done"), "end_segment", "segment")
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
        total.text = L("${Fmt.dur(work)} из ${Fmt.hours(studyMin)} учёбы", "${Fmt.dur(work)} of ${Fmt.hours(studyMin)} of study")
        if (shownVersion != s.version + Sync.online) {
            shownVersion = s.version + Sync.online
            buildRows(s)
        }
        s.blocks.forEachIndexed { i, b ->
            val (meta, bar) = rowViews.getOrNull(i) ?: return@forEachIndexed
            if (b.isBreak) {
                meta.text = L("${b.minutes} мин · ", "${b.minutes} min · ") + when { b.done -> L("прошёл", "done"); b.started -> L("идёт", "running"); else -> L("отрезок, не учёба", "segment, not study") }
                return@forEachIndexed
            }
            val w = e?.work(i, now) ?: 0L
            meta.text = L("${w / Snapshot.MIN} из ${b.minutes} мин", "${w / Snapshot.MIN} of ${b.minutes} min") + if (b.done) L(" · готово", " · done") else ""
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
        if (s.blocks.isEmpty()) rows.addWithMargins(label(L("План на сегодня пуст — составь его на ПК.", "Today’s plan is empty — make it on the PC."), 14f, p.muted))
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
