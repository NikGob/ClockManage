package com.nikgob.clockmanage

import android.app.Activity
import android.content.Intent
import android.content.res.ColorStateList
import android.graphics.Color
import android.graphics.drawable.ColorDrawable
import android.graphics.drawable.RippleDrawable
import android.os.Build
import android.os.Bundle
import android.view.View
import android.view.ViewGroup
import android.widget.CheckBox
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.Toast

/**
 * Pick the apps blocked on the phone. The list lives on the PC (so the lock rules apply to it:
 * during the study day apps can only be added), this screen just edits it.
 */
class AppsActivity : Activity() {
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

        val col = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(dp(16), 0, dp(16), dp(24))
        }
        page.addView(ScrollView(this).apply { addView(col) }, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 0, 1f))
        col.addWithMargins(label(tr("android_apps_during_study_the_checked_apps_close"), 14f, p.muted).apply { setPadding(dp(4), 0, dp(4), 0) }, top = dp(4))

        val pm = packageManager
        val apps = pm.queryIntentActivities(Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_LAUNCHER), 0)
            .distinctBy { it.activityInfo.packageName }
            .filter { it.activityInfo.packageName != packageName }
            .map { Triple(it.activityInfo.packageName, it.loadLabel(pm).toString(), it) }
            .sortedWith(compareBy({ it.first !in chosen }, { it.second.lowercase() }))
        val list = card(p).apply { setPadding(dp(8), dp(8), dp(8), dp(8)) }
        col.addWithMargins(list, top = dp(16))
        val states = arrayOf(intArrayOf(android.R.attr.state_checked), intArrayOf())
        apps.forEach { (pkg, name, info) ->
            // One row per app: its icon, its name and the check; the whole row toggles.
            val item = row().apply {
                setPadding(dp(12), dp(8), dp(4), dp(8))
                background = RippleDrawable(ColorStateList.valueOf(withAlpha(p.text, 0.12f)), null, rounded(Color.WHITE, dp(16).toFloat()))
            }
            val icon = ImageView(this).apply { setImageDrawable(info.loadIcon(pm)) }
            item.addView(icon, LinearLayout.LayoutParams(dp(36), dp(36)))
            item.addFill(label(name, 16f, p.text), start = dp(16))
            val check = CheckBox(this).apply {
                buttonTintList = ColorStateList(states, intArrayOf(p.primary, p.outline))
                isChecked = pkg in chosen
                setOnCheckedChangeListener { _, on -> if (on) chosen += pkg else chosen -= pkg }
            }
            item.addView(check)
            item.setOnClickListener { check.toggle() }
            list.addWithMargins(item)
        }
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
}
