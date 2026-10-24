package com.nikgob.clockmanage

import android.animation.ValueAnimator
import android.content.Context
import android.content.res.ColorStateList
import android.content.res.Configuration
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.Path
import android.graphics.Rect
import android.graphics.RectF
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.graphics.drawable.RippleDrawable
import android.graphics.drawable.StateListDrawable
import android.os.Build
import android.os.SystemClock
import android.text.InputType
import android.util.TypedValue
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.widget.Button
import android.widget.EditText
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.TextView
import org.json.JSONObject
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.ceil
import kotlin.math.cos
import kotlin.math.max
import kotlin.math.min
import kotlin.math.pow
import kotlin.math.sin

/**
 * Colours: the PC's own scheme when it sent one (the app looks like the PC, light or dark as set
 * there), otherwise Material You on Android 12+ and the app's green; light and dark.
 */
class Palette(private val ctx: Context) {
    private val pc: JSONObject? = Store(ctx).theme?.let { runCatching { JSONObject(it) }.getOrNull() }
    private val systemDark = (ctx.resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK) == Configuration.UI_MODE_NIGHT_YES
    val dark = when (pc?.optString("mode")) {
        "dark" -> true
        "light" -> false
        else -> systemDark
    }
    private val roles = pc?.optJSONObject(if (dark) "dark" else "light")

    /** What the colours come from: the screen is built anew when it changes. */
    val key = "${pc?.toString()}|$dark"

    private fun sys(id: Int, fallback: String) = if (Build.VERSION.SDK_INT >= 31) ctx.getColor(id) else Color.parseColor(fallback)
    private fun role(name: String, fallback: Int): Int =
        roles?.optString(name)?.takeIf { it.startsWith("#") }?.let { runCatching { Color.parseColor(it) }.getOrNull() } ?: fallback

    val bg = role("surface", if (dark) Color.parseColor("#111410") else Color.parseColor("#FCFDF7"))
    val card = role("surface-container", if (dark) sys(android.R.color.system_neutral1_900, "#1D211B") else sys(android.R.color.system_neutral1_50, "#EEF2E9"))
    val cardHigh = role("surface-container-high", if (dark) sys(android.R.color.system_neutral1_800, "#272B25") else sys(android.R.color.system_neutral1_100, "#E6E9E0"))
    val cardHighest = role("surface-container-highest", if (dark) sys(android.R.color.system_neutral2_800, "#32362F") else sys(android.R.color.system_neutral2_100, "#E0E4DB"))
    val text = role("on-surface", if (dark) Color.parseColor("#E1E4DC") else Color.parseColor("#1A1C19"))
    val muted = role("on-surface-variant", if (dark) Color.parseColor("#C3C8BB") else Color.parseColor("#43483F"))
    val primary = role("primary", if (dark) sys(android.R.color.system_accent1_200, "#8BD88A") else sys(android.R.color.system_accent1_600, "#2E7D32"))
    val onPrimary = role("on-primary", if (dark) sys(android.R.color.system_accent1_800, "#00390A") else Color.WHITE)
    val container = role("primary-container", if (dark) sys(android.R.color.system_accent1_700, "#0F5223") else sys(android.R.color.system_accent1_100, "#B6F2AF"))
    val onContainer = role("on-primary-container", if (dark) sys(android.R.color.system_accent1_100, "#B6F2AF") else sys(android.R.color.system_accent1_900, "#002204"))
    val secondary = role("secondary-container", if (dark) sys(android.R.color.system_accent2_700, "#3A4B39") else sys(android.R.color.system_accent2_100, "#D3E8CF"))
    val onSecondary = role("on-secondary-container", if (dark) sys(android.R.color.system_accent2_100, "#D3E8CF") else sys(android.R.color.system_accent2_900, "#0F1F11"))
    val tertiary = role("tertiary-container", if (dark) sys(android.R.color.system_accent3_700, "#1F4D53") else sys(android.R.color.system_accent3_100, "#BCEBF1"))
    val onTertiary = role("on-tertiary-container", if (dark) sys(android.R.color.system_accent3_100, "#BCEBF1") else sys(android.R.color.system_accent3_900, "#001F23"))
    val outline = role("outline", if (dark) Color.parseColor("#8D9387") else Color.parseColor("#73796E"))
    val outlineVariant = role("outline-variant", if (dark) Color.parseColor("#43483F") else Color.parseColor("#C3C8BB"))
    val error = role("error", if (dark) Color.parseColor("#FFB4AB") else Color.parseColor("#BA1A1A"))
}

/** [c] at [alpha] (0..1). */
fun withAlpha(c: Int, alpha: Float): Int = (c and 0x00FFFFFF) or ((alpha * 255).toInt().coerceIn(0, 255) shl 24)

fun Context.dp(v: Float): Int = TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_DIP, v, resources.displayMetrics).toInt()
fun Context.dp(v: Int): Int = dp(v.toFloat())
fun Context.sp(v: Float): Float = TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_SP, v, resources.displayMetrics)

fun rounded(color: Int, radius: Float, stroke: Int = 0, strokeColor: Int = 0) = GradientDrawable().apply {
    setColor(color)
    cornerRadius = radius
    if (stroke > 0) setStroke(stroke, strokeColor)
}

fun Context.label(text: String, sp: Float, color: Int, bold: Boolean = false): TextView = TextView(this).apply {
    this.text = text
    setTextSize(TypedValue.COMPLEX_UNIT_SP, sp)
    setTextColor(color)
    if (bold) typeface = Typeface.create("sans-serif-medium", Typeface.NORMAL)
}

/** A Material icon (`R.drawable.ic_*`, the PC's set) in [color]. */
fun Context.iconView(res: Int, color: Int, size: Int = 24): ImageView = ImageView(this).apply {
    setImageResource(res)
    imageTintList = ColorStateList.valueOf(color)
    layoutParams = LinearLayout.LayoutParams(dp(size), dp(size))
}

/** A small filled circle: the connection light, a plan row's mark. */
fun Context.dot(color: Int, size: Int = 8): View = View(this).apply {
    background = GradientDrawable().apply { shape = GradientDrawable.OVAL; setColor(color) }
    layoutParams = LinearLayout.LayoutParams(dp(size), dp(size))
}

/** M3 pill button: filled, tonal, outlined or text, with an optional leading icon. */
fun Context.pill(text: String, p: Palette, style: String = "filled", icon: Int = 0, onClick: (View) -> Unit): Button {
    val (bgc, fg) = when (style) {
        "filled" -> p.primary to p.onPrimary
        "tonal" -> p.secondary to p.onSecondary
        else -> Color.TRANSPARENT to p.primary
    }
    return pillIn(text, bgc, fg, if (style == "outlined") p.outline else 0, icon, onClick)
}

/** A pill in given colours (buttons on the coloured timer card), [stroke] for an outlined one. */
fun Context.pillIn(text: String, bgc: Int, fg: Int, stroke: Int = 0, icon: Int = 0, onClick: (View) -> Unit): Button = Button(this).apply {
    this.text = text
    isAllCaps = false
    stateListAnimator = null
    setTextSize(TypedValue.COMPLEX_UNIT_SP, 15f)
    typeface = Typeface.create("sans-serif-medium", Typeface.NORMAL)
    setTextColor(fg)
    minHeight = dp(48)
    minimumHeight = dp(48)
    minWidth = 0
    minimumWidth = 0
    setPadding(dp(if (icon != 0) 16 else 24), 0, dp(24), 0)
    if (icon != 0) {
        setCompoundDrawablesRelativeWithIntrinsicBounds(icon, 0, 0, 0)
        compoundDrawablePadding = dp(8)
        compoundDrawableTintList = ColorStateList.valueOf(fg)
    }
    val r = dp(24).toFloat()
    background = RippleDrawable(ColorStateList.valueOf(withAlpha(fg, 0.2f)), rounded(bgc, r, if (stroke != 0) dp(1) else 0, stroke), rounded(Color.WHITE, r))
    setOnClickListener(onClick)
    pressBounce()
}

/** A round icon button (the plan's − and +). */
fun Context.iconButton(text: String, bgc: Int, fg: Int, onClick: (View) -> Unit): Button = pillIn(text, bgc, fg, onClick = onClick).apply {
    setPadding(0, 0, 0, 0)
    textSize = 20f
    minHeight = dp(40)
    minimumHeight = dp(40)
}

/** An M3 outlined text field: the outline turns primary and thicker while it has focus. */
fun Context.field(hint: String, p: Palette, type: Int = InputType.TYPE_CLASS_TEXT): EditText = EditText(this).apply {
    this.hint = hint
    inputType = type
    setSingleLine()
    setTextColor(p.text)
    setHintTextColor(p.muted)
    textSize = 16f
    minHeight = dp(56)
    setPadding(dp(16), 0, dp(16), 0)
    val r = dp(12).toFloat()
    background = StateListDrawable().apply {
        addState(intArrayOf(android.R.attr.state_focused), rounded(Color.TRANSPARENT, r, dp(2), p.primary))
        addState(intArrayOf(), rounded(Color.TRANSPARENT, r, dp(1), p.outline))
    }
    if (Build.VERSION.SDK_INT >= 29) textCursorDrawable = rounded(p.primary, 0f).apply { setSize(dp(2), dp(20)) }
}

/** Press: sink a little; release: spring back with a small overshoot. Never eats the touch. */
@android.annotation.SuppressLint("ClickableViewAccessibility")
fun View.pressBounce() {
    setOnTouchListener { v, e ->
        when (e.actionMasked) {
            android.view.MotionEvent.ACTION_DOWN -> v.animate().scaleX(0.94f).scaleY(0.94f).setDuration(90)
                .setInterpolator(android.view.animation.DecelerateInterpolator()).start()
            android.view.MotionEvent.ACTION_UP, android.view.MotionEvent.ACTION_CANCEL -> v.animate().scaleX(1f).scaleY(1f).setDuration(380)
                .setInterpolator(android.view.animation.OvershootInterpolator(3f)).start()
        }
        false
    }
}

/** Children appear, disappear and resize smoothly (the system animates layout changes). */
fun ViewGroup.smoothChanges() {
    layoutTransition = android.animation.LayoutTransition().apply {
        enableTransitionType(android.animation.LayoutTransition.CHANGING)
        setDuration(260)
    }
}

/** A view arrives: rise and fade in, `delay` ms later (cascades on screen open). */
fun View.riseIn(delay: Long = 0) {
    alpha = 0f
    translationY = 24f * resources.displayMetrics.density
    animate().alpha(1f).translationY(0f).setStartDelay(delay).setDuration(420)
        .setInterpolator(android.view.animation.PathInterpolator(0.05f, 0.7f, 0.1f, 1f)).start()
}

/** A card like the PC's surfaces: rounded, on the surface container colour. */
fun Context.card(p: Palette): LinearLayout = LinearLayout(this).apply {
    orientation = LinearLayout.VERTICAL
    background = rounded(p.card, dp(28).toFloat())
    setPadding(dp(20), dp(20), dp(20), dp(20))
    smoothChanges()
}

fun ViewGroup.addWithMargins(v: View, top: Int = 0, bottom: Int = 0, width: Int = ViewGroup.LayoutParams.MATCH_PARENT): View {
    addView(v, LinearLayout.LayoutParams(width, ViewGroup.LayoutParams.WRAP_CONTENT).apply {
        topMargin = top
        bottomMargin = bottom
    })
    return v
}

fun Context.row(): LinearLayout = LinearLayout(this).apply {
    orientation = LinearLayout.HORIZONTAL
    gravity = Gravity.CENTER_VERTICAL
}

/** [v] in a row with the rest of the width: `row.addFill(text)`. */
fun LinearLayout.addFill(v: View, start: Int = 0) {
    addView(v, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f).apply { marginStart = start })
}

/** Children side by side, wrapping to a new line when they don't fit: centred (the timer's buttons) or from the start (choices). */
class Flow(ctx: Context) : ViewGroup(ctx) {
    private val gap = ctx.dp(8)
    var centred = true

    private fun lines(max: Int): List<List<View>> {
        val out = mutableListOf<MutableList<View>>()
        var used = 0
        for (i in 0 until childCount) {
            val v = getChildAt(i)
            if (v.visibility == GONE) continue
            if (out.isEmpty() || used + gap + v.measuredWidth > max) {
                out += mutableListOf(v)
                used = v.measuredWidth
            } else {
                out.last() += v
                used += gap + v.measuredWidth
            }
        }
        return out
    }

    override fun onMeasure(w: Int, h: Int) {
        val max = MeasureSpec.getSize(w)
        for (i in 0 until childCount) {
            getChildAt(i).measure(MeasureSpec.makeMeasureSpec(max, MeasureSpec.AT_MOST), MeasureSpec.makeMeasureSpec(0, MeasureSpec.UNSPECIFIED))
        }
        val ls = lines(max)
        val height = ls.sumOf { line -> line.maxOf { it.measuredHeight } } + gap * (ls.size - 1).coerceAtLeast(0)
        setMeasuredDimension(max, height)
    }

    override fun onLayout(changed: Boolean, l: Int, t: Int, r: Int, b: Int) {
        var y = 0
        for (line in lines(r - l)) {
            val lineH = line.maxOf { it.measuredHeight }
            var x = if (centred) (r - l - (line.sumOf { it.measuredWidth } + gap * (line.size - 1))) / 2 else 0
            for (v in line) {
                v.layout(x, y, x + v.measuredWidth, y + v.measuredHeight)
                x += v.measuredWidth + gap
            }
            y += lineH + gap
        }
    }
}

/**
 * The timer dial, as on the PC: a wavy ring (M3 Expressive — the part done is a travelling wave
 * while the timer runs and flat when it stands, the rest a flat track after a small gap) with the
 * state and the time in its centre. Everything is drawn here, centred on the view, so the time
 * sits exactly in the middle of the ring.
 */
class Dial(ctx: Context) : View(ctx) {
    private val maxSize = ctx.dp(300)
    private val minState = ctx.sp(11f)
    private val track = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.STROKE; strokeCap = Paint.Cap.ROUND }
    private val wave = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.STROKE; strokeCap = Paint.Cap.ROUND; strokeJoin = Paint.Join.ROUND }
    private val stateText = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        textAlign = Paint.Align.CENTER
        typeface = Typeface.create("sans-serif-medium", Typeface.NORMAL)
        letterSpacing = 0.12f
    }
    private val timeText = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        textAlign = Paint.Align.CENTER
        typeface = Typeface.create("sans-serif-light", Typeface.NORMAL)
        fontFeatureSettings = "tnum"
    }
    private val path = Path()
    private val box = RectF()
    private val bounds = Rect()

    /** Small caps above the time: "WORK", "BREAK"… */
    var state = ""
        set(v) { if (field != v) { field = v; invalidate() } }
    var time = ""
        set(v) { if (field != v) { field = v; invalidate() } }
    /** The time's size against the ring: below 1 for longer texts ("1:05:00", the plan's hours). */
    var timeScale = 1f
        set(v) { if (field != v) { field = v; invalidate() } }
    /** The time in another colour (a segment over its time); 0: the text colour. */
    var timeColor = 0
        set(v) { if (field != v) { field = v; invalidate() } }

    private var target = 0f
    private var shown = 0f
    private var amp = 0f
    private var targetAmp = 0f
    private var phase = 0f
    private var last = 0L

    /** The part done (0..1, below 0: no arc), and whether it moves now (the wave runs). */
    fun progress(fraction: Float, running: Boolean) {
        target = fraction
        targetAmp = if (running && fraction >= 0f) 1f else 0f
        invalidate()
    }

    fun colors(arc: Int, trackColor: Int, text: Int) {
        wave.color = arc
        track.color = trackColor
        stateText.color = text
        invalidate()
    }

    override fun onMeasure(w: Int, h: Int) {
        val width = MeasureSpec.getSize(w)
        setMeasuredDimension(width, min(width, maxSize))
    }

    override fun onDraw(c: Canvas) {
        val now = SystemClock.uptimeMillis()
        val dt = if (last == 0L) 16f else min(64L, now - last).toFloat()
        last = now
        val motion = ValueAnimator.areAnimatorsEnabled()
        val k = if (motion) 1f - 0.001f.pow(dt / 600f) else 1f
        val goal = target.coerceIn(0f, 1f)
        shown += (goal - shown) * (if (abs(goal - shown) > 0.5f) 1f else k)
        amp += ((if (motion) targetAmp else 0f) - amp) * k
        if (motion) phase += dt * 0.0022f

        val size = height.toFloat()
        val cx = width / 2f
        val cy = height / 2f
        val stroke = size * 0.035f
        val r = size / 2f - stroke
        track.strokeWidth = stroke
        wave.strokeWidth = stroke
        box.set(cx - r, cy - r, cx + r, cy + r)

        // The track: flat, after the part done, with a gap on both sides.
        val p = shown
        val gap = if (p > 0.001f && p < 0.999f) 0.09f else 0f
        val a0 = p * 2 * PI.toFloat() + gap
        val a1 = 2 * PI.toFloat() - gap
        if (a1 - a0 > 0.01f) c.drawArc(box, -90f + Math.toDegrees(a0.toDouble()).toFloat(), Math.toDegrees((a1 - a0).toDouble()).toFloat(), false, track)

        // The part done: a wave tapered at both ends, so the round caps stay on the track.
        if (p > 0.001f) {
            val end = p * 2 * PI.toFloat()
            val steps = max(8, ceil(end / (PI.toFloat() / 90f)).toInt())
            val waveH = r * 1.9f / 88f * amp
            path.reset()
            for (i in 0..steps) {
                val a = i.toFloat() / steps * end
                val taper = min(1f, min(a / 0.35f, (end - a) / 0.35f))
                val rr = r + waveH * taper * sin(a * 22f - phase * 6f)
                val x = cx + rr * sin(a)
                val y = cy - rr * cos(a)
                if (i == 0) path.moveTo(x, y) else path.lineTo(x, y)
            }
            c.drawPath(path, wave)
        }

        // State and time, centred as one block on the ring's centre (by the height of capitals and digits).
        timeText.textSize = size * 0.22f * timeScale
        timeText.color = if (timeColor != 0) timeColor else stateText.color
        stateText.textSize = max(size * 0.042f, minState)
        timeText.getTextBounds("0", 0, 1, bounds)
        val digits = bounds.height().toFloat()
        if (state.isEmpty()) {
            c.drawText(time, cx, cy + digits / 2f, timeText)
        } else {
            stateText.getTextBounds("A", 0, 1, bounds)
            val caps = bounds.height().toFloat()
            val between = size * 0.05f
            val top = cy - (caps + between + digits) / 2f
            c.drawText(state, cx, top + caps, stateText)
            c.drawText(time, cx, top + caps + between + digits, timeText)
        }

        val moving = abs(goal - shown) > 0.0005f || abs(targetAmp - amp) > 0.01f || (motion && targetAmp > 0f)
        if (moving) postInvalidateOnAnimation() else last = 0L
    }
}

/** A slim rounded progress bar (0..1000), like the PC's plan rows. */
class Bar(ctx: Context) : View(ctx) {
    private val back = Paint(Paint.ANTI_ALIAS_FLAG)
    private val fill = Paint(Paint.ANTI_ALIAS_FLAG)
    private val box = RectF()

    /** 0..1000; animated with ObjectAnimator.ofInt(bar, "progress", …). */
    var progress = 0
        set(v) { if (field != v) { field = v.coerceIn(0, 1000); invalidate() } }

    fun colors(fillColor: Int, backColor: Int) {
        fill.color = fillColor
        back.color = backColor
        invalidate()
    }

    override fun onMeasure(w: Int, h: Int) = setMeasuredDimension(MeasureSpec.getSize(w), context.dp(4))

    override fun onDraw(c: Canvas) {
        val r = height / 2f
        box.set(0f, 0f, width.toFloat(), height.toFloat())
        c.drawRoundRect(box, r, r, back)
        if (progress > 0) {
            box.right = (width * progress / 1000f).coerceAtLeast(height.toFloat())
            c.drawRoundRect(box, r, r, fill)
        }
    }
}
