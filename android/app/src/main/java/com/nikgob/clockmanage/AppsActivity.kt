package com.nikgob.clockmanage

import android.app.Activity
import android.content.Intent
import android.content.pm.ResolveInfo
import android.content.res.ColorStateList
import android.graphics.Color
import android.graphics.drawable.Drawable
import android.graphics.drawable.ColorDrawable
import android.graphics.drawable.RippleDrawable
import android.os.Build
import android.os.Bundle
import android.util.LruCache
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.widget.BaseAdapter
import android.widget.CheckBox
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.ListView
import android.widget.ProgressBar
import android.widget.Toast

/**
 * Pick the apps blocked on the phone. The list lives on the PC (so the lock rules apply to it:
 * during the study day apps can only be added), this screen just edits it.
 */
class AppsActivity : Activity() {
    private val iconLoader = java.util.concurrent.Executors.newSingleThreadExecutor()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val p = Palette(this)
        val store = Store(this)
        val chosen = store.snapshot()?.apps?.toMutableSet() ?: mutableSetOf()

        val page = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
        page.setBackgroundColor(p.bg)
        setContentView(page)
        window.setBackgroundDrawable(ColorDrawable(p.bg))
        @Suppress("DEPRECATION")
        run {
            window.statusBarColor = p.bg
            window.navigationBarColor = p.bg
            if (Build.VERSION.SDK_INT >= 30) {
                val light = android.view.WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS or android.view.WindowInsetsController.APPEARANCE_LIGHT_NAVIGATION_BARS
                window.insetsController?.setSystemBarsAppearance(if (p.dark) 0 else light, light)
            } else {
                window.decorView.systemUiVisibility = if (p.dark) 0 else View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR or View.SYSTEM_UI_FLAG_LIGHT_NAVIGATION_BAR
            }
        }
        page.setOnApplyWindowInsetsListener { v, insets ->
            @Suppress("DEPRECATION")
            v.setPadding(insets.systemWindowInsetLeft, insets.systemWindowInsetTop, insets.systemWindowInsetRight, insets.systemWindowInsetBottom)
            insets
        }

        // Top bar: back, the title, Save.
        val bar = row().apply { setPadding(dp(4), dp(8), dp(12), dp(8)) }
        val back = ImageView(this).apply {
            setImageResource(R.drawable.ic_arrow_back)
            imageTintList = ColorStateList.valueOf(p.text)
            scaleType = ImageView.ScaleType.CENTER
            background = RippleDrawable(ColorStateList.valueOf(withAlpha(p.text, 0.12f)), null, null)
            setOnClickListener { finish() }
        }
        bar.addView(back, LinearLayout.LayoutParams(dp(48), dp(48)))
        bar.addFill(label(tr("blocking_apps"), 22f, p.text), start = dp(8))
        val save = pill(tr("overlay_save"), p) {}
        bar.addView(save)
        page.addWithMargins(bar)

        page.addWithMargins(label(tr("android_apps_during_study_the_checked_apps_close"), 14f, p.muted).apply { setPadding(dp(20), dp(4), dp(20), 0) })

        // The list: one card that scrolls, rows made only for what is on screen. Some phones carry
        // hundreds of apps: building them all at once, icons too, froze the screen into "not responding".
        val list = ListView(this).apply {
            background = rounded(p.card, dp(28).toFloat())
            clipToOutline = true
            setPadding(dp(8), dp(8), dp(8), dp(8))
            clipToPadding = false
            divider = null
            selector = ColorDrawable(Color.TRANSPARENT)
            visibility = View.GONE
        }
        val loading = ProgressBar(this).apply { indeterminateTintList = ColorStateList.valueOf(p.primary) }
        val body = android.widget.FrameLayout(this)
        body.addView(loading, android.widget.FrameLayout.LayoutParams(dp(40), dp(40), Gravity.CENTER))
        body.addView(list)
        page.addView(body, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 0, 1f).apply {
            setMargins(dp(16), dp(16), dp(16), dp(24))
        })

        // Names off the main thread; icons one by one as their rows show.
        val pm = packageManager
        Thread {
            val apps = pm.queryIntentActivities(Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_LAUNCHER), 0)
                .distinctBy { it.activityInfo.packageName }
                .filter { it.activityInfo.packageName != packageName }
                .map { App(it.activityInfo.packageName, it.loadLabel(pm).toString(), it) }
                .sortedWith(compareBy({ it.pkg !in chosen }, { it.name.lowercase() }))
            runOnUiThread {
                if (isDestroyed) return@runOnUiThread
                list.adapter = Rows(apps, chosen, p)
                loading.visibility = View.GONE
                list.visibility = View.VISIBLE
            }
        }.start()

        // Listed on the PC but not installed here: keep them.
        save.setOnClickListener {
            save.isEnabled = false
            Sync.setApps(this, chosen) { err ->
                save.isEnabled = true
                if (err == null) {
                    Toast.makeText(this, tr("log_saved"), Toast.LENGTH_SHORT).show()
                    finish()
                } else {
                    Toast.makeText(this, err, Toast.LENGTH_LONG).show()
                }
            }
        }
    }

    override fun onDestroy() {
        iconLoader.shutdownNow()
        super.onDestroy()
    }

    private class App(val pkg: String, val name: String, val info: ResolveInfo)

    /** One row per app: its icon, its name and the check; the whole row toggles. */
    private inner class Rows(private val apps: List<App>, private val chosen: MutableSet<String>, private val p: Palette) : BaseAdapter() {
        private val states = arrayOf(intArrayOf(android.R.attr.state_checked), intArrayOf())
        private val icons = LruCache<String, Drawable>(64)

        override fun getCount() = apps.size
        override fun getItem(i: Int) = apps[i]
        override fun getItemId(i: Int) = i.toLong()

        override fun getView(i: Int, old: View?, parent: ViewGroup): View {
            val item = old as? LinearLayout ?: row().apply {
                setPadding(dp(12), dp(8), dp(4), dp(8))
                background = RippleDrawable(ColorStateList.valueOf(withAlpha(p.text, 0.12f)), null, rounded(Color.WHITE, dp(16).toFloat()))
                addView(ImageView(context), LinearLayout.LayoutParams(dp(36), dp(36)))
                addFill(label("", 16f, p.text), start = dp(16))
                addView(CheckBox(context).apply { buttonTintList = ColorStateList(states, intArrayOf(p.primary, p.outline)) })
                layoutParams = android.widget.AbsListView.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT)
            }
            val app = apps[i]
            val icon = item.getChildAt(0) as ImageView
            val check = item.getChildAt(2) as CheckBox
            (item.getChildAt(1) as android.widget.TextView).text = app.name
            check.setOnCheckedChangeListener(null)
            check.isChecked = app.pkg in chosen
            check.setOnCheckedChangeListener { _, on -> if (on) chosen += app.pkg else chosen -= app.pkg }
            item.setOnClickListener { check.toggle() }
            icon.tag = app.pkg
            val cached = icons.get(app.pkg)
            icon.setImageDrawable(cached)
            if (cached == null && !iconLoader.isShutdown) iconLoader.execute {
                val d = app.info.loadIcon(packageManager)
                runOnUiThread {
                    icons.put(app.pkg, d)
                    if (icon.tag == app.pkg) icon.setImageDrawable(d)
                }
            }
            return item
        }
    }
}
