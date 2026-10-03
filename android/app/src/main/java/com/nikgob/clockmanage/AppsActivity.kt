package com.nikgob.clockmanage

import android.app.Activity
import android.content.Intent
import android.os.Bundle
import android.view.ViewGroup
import android.widget.CheckBox
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

        val col = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(dp(20), dp(24), dp(20), dp(32))
        }
        val scroll = ScrollView(this).apply {
            setBackgroundColor(p.bg)
            addView(col)
        }
        setContentView(scroll)
        scroll.setOnApplyWindowInsetsListener { v, insets ->
            @Suppress("DEPRECATION")
            v.setPadding(0, insets.systemWindowInsetTop, 0, insets.systemWindowInsetBottom)
            insets
        }
        col.addWithMargins(label("Приложения", 28f, p.text, bold = true))
        col.addWithMargins(label("Во время учёбы отмеченные закрываются сразу при открытии. Пока идёт учебный день, список можно только расширять. Сайты берутся с ПК.", 14f, p.muted), top = dp(8))

        val save = pill("Сохранить", p) {}
        col.addWithMargins(save, top = dp(16), width = ViewGroup.LayoutParams.WRAP_CONTENT)

        val pm = packageManager
        val apps = pm.queryIntentActivities(Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_LAUNCHER), 0)
            .map { it.activityInfo.packageName to it.loadLabel(pm).toString() }
            .distinctBy { it.first }
            .filter { it.first != packageName }
            .sortedWith(compareBy({ it.first !in chosen }, { it.second.lowercase() }))
        apps.forEach { (pkg, name) ->
            col.addWithMargins(CheckBox(this).apply {
                text = name
                setTextColor(p.text)
                textSize = 16f
                isChecked = pkg in chosen
                setOnCheckedChangeListener { _, on -> if (on) chosen += pkg else chosen -= pkg }
            }, top = dp(4))
        }
        // Listed on the PC but not installed here: keep them.
        save.setOnClickListener {
            save.isEnabled = false
            Sync.setApps(this, chosen) { err ->
                save.isEnabled = true
                if (err == null) {
                    Toast.makeText(this, "Сохранено", Toast.LENGTH_SHORT).show()
                    finish()
                } else {
                    Toast.makeText(this, err, Toast.LENGTH_LONG).show()
                }
            }
        }
    }
}
