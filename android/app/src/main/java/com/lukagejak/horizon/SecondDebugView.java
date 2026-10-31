package com.lukagejak.horizon;

import android.content.Context;
import android.graphics.Canvas;
import android.graphics.Color;
import android.graphics.Paint;
import android.graphics.Path;
import android.graphics.RectF;
import android.graphics.Typeface;
import android.text.TextPaint;
import android.text.TextUtils;
import android.view.View;

import java.util.HashMap;

/** The bottom screen's Debug tab: performance numbers and the debug details, drawn as cards and charts. */
final class SecondDebugView extends View {
    private static final int BG = 0xFF0B0B10;
    private static final int CARD = 0xFF141419;
    private static final int INK = 0xFFF2F2F6;
    private static final int MUTED = 0xFF8A8A96;
    private static final int ACCENT = 0xFF8B7DE8;
    private static final int GOOD = 0xFF6ED6A0;
    private static final int WARN = 0xFFF0A45A;
    private static final int BAD = 0xFFF06A6A;

    private final float density;
    private final float scaled;
    private final Paint paint = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final Paint fill = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final Paint card = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final Paint grid = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final Path area = new Path();
    private final Path line = new Path();
    private final RectF box = new RectF();
    private final HashMap<String, String> stats = new HashMap<>();
    private final float[] frames = new float[120];
    private int framesLength;
    private boolean hasFrames;

    SecondDebugView(Context context) {
        super(context);
        density = getResources().getDisplayMetrics().density;
        scaled = getResources().getDisplayMetrics().scaledDensity;
        paint.setTypeface(Typeface.create("sans-serif", Typeface.NORMAL));
        fill.setStyle(Paint.Style.FILL);
        card.setColor(CARD);
        grid.setColor(0x22FFFFFF);
        grid.setStrokeWidth(dp(1));
    }

    private float dp(float v) {
        return v * density;
    }

    private float sp(float v) {
        return v * scaled;
    }

    /** The game's `platform("second_debug", text)` output, key=value lines and a frames list. */
    void setStats(String text) {
        stats.clear();
        for (String line : text.split("\n")) {
            int at = line.indexOf('=');
            if (at > 0) {
                stats.put(line.substring(0, at), line.substring(at + 1));
            }
        }
        String list = stats.get("frames");
        if (list != null && !list.isEmpty()) {
            String[] parts = list.split(",");
            framesLength = Math.min(parts.length, frames.length);
            int start = Math.max(0, parts.length - framesLength);
            for (int i = 0; i < framesLength; i++) {
                try {
                    frames[i] = Float.parseFloat(parts[start + i]);
                } catch (NumberFormatException e) {
                    frames[i] = 0f;
                }
            }
            hasFrames = framesLength > 1;
        }
        invalidate();
    }

    private String get(String key, String fallback) {
        String value = stats.get(key);
        return value == null || value.isEmpty() ? fallback : value;
    }

    private int number(String key, int fallback) {
        try {
            return Integer.parseInt(get(key, Integer.toString(fallback)));
        } catch (NumberFormatException e) {
            return fallback;
        }
    }

    private float decimal(String key, float fallback) {
        try {
            return Float.parseFloat(get(key, Float.toString(fallback)));
        } catch (NumberFormatException e) {
            return fallback;
        }
    }

    private void text(Canvas c, String s, float x, float y, float size, int color, Typeface face) {
        paint.setStyle(Paint.Style.FILL);
        paint.setTypeface(face);
        paint.setTextSize(sp(size));
        paint.setColor(color);
        c.drawText(s, x, y, paint);
    }

    @Override
    protected void onDraw(Canvas c) {
        super.onDraw(c);
        float w = getWidth();
        float h = getHeight();
        if (w <= 0 || h <= 0) {
            return;
        }
        c.drawColor(BG);
        float left = dp(70);
        float right = w - dp(36);
        float content = right - left;
        float top = dp(24);
        Typeface regular = Typeface.create("sans-serif", Typeface.NORMAL);
        Typeface medium = Typeface.create("sans-serif-medium", Typeface.NORMAL);

        text(c, "PERFORMANCE", left, top + sp(12), 13, MUTED, medium);
        int fps = number("fps", 0);
        String fpsText = Integer.toString(fps);
        float fpsY = top + sp(72);
        text(c, fpsText, left, fpsY, 56, fps >= 60 ? GOOD : fps >= 30 ? WARN : BAD, medium);
        float fpsWidth = paint.measureText(fpsText);
        text(c, "FPS", left + fpsWidth + dp(10), fpsY, 18, MUTED, medium);
        text(
                c,
                "min " + get("min", "-") + "   ·   1% low " + get("low", "-"),
                left, fpsY + sp(18), 15, INK, regular);

        drawFrames(c, left + content * 0.42f, top + sp(4), right, top + dp(150));
        drawBudget(c, left, fpsY + sp(38), content * 0.36f);

        float cardsTop = top + dp(168);
        float gap = dp(16);
        float cardW = (content - gap) / 2;
        float cardH = dp(78);
        int time = number("time", 0);
        int hours = ((time / 1000) + 6) % 24;
        int minutes = (time % 1000) * 60 / 1000;
        String[][] entries = {
                {"POSITION",
                        get("x", "-") + ", " + get("y", "-") + ", " + get("z", "-"),
                        "chunk " + get("cx", "-") + ", " + get("cz", "-")},
                {"CHUNKS",
                        get("generating", "-") + " generating",
                        get("chunks", "-") + " loaded · " + get("queued", "-") + " queued"},
                {"CPU", percent("cpu"), "process"},
                {"GPU", percent("gpu"), "device"},
                {"MEMORY", get("mem", "-") + " MiB", "resident set"},
                {"WORLD",
                        dimension(get("dim", "-")),
                        "Day " + get("day", "-") + " · "
                                + String.format(java.util.Locale.US, "%02d:%02d", hours, minutes)},
        };
        for (int i = 0; i < entries.length; i++) {
            float x = left + (i % 2) * (cardW + gap);
            boolean last = i / 2 == 2;
            float y = cardsTop + (i / 2) * (cardH + gap) + (last ? dp(15) : 0);
            float rowHeight = last ? dp(72) : cardH;
            drawCard(c, x, y, cardW, rowHeight, entries[i][0], entries[i][1], entries[i][2],
                    regular, medium);
        }
    }

    private void drawFrames(Canvas c, float left, float top, float right, float bottom) {
        box.set(left, top, right, bottom);
        c.drawRoundRect(box, dp(10), dp(10), card);
        float axis = dp(44);
        float graphLeft = left + dp(14) + axis;
        float graphRight = right - dp(14);
        float graphTop = top + dp(18);
        float graphBottom = bottom - dp(18);
        float maxMs = 33.0f;
        for (int i = 0; i <= 3; i++) {
            float y = graphBottom - (graphBottom - graphTop) * i / 3f;
            c.drawLine(graphLeft, y, graphRight, y, grid);
        }
        text(c, "FRAME TIME", left + dp(14), top + dp(13), 10, MUTED, Typeface.create("sans-serif-medium", Typeface.NORMAL));
        text(c, "33 ms", left + dp(14), graphTop + sp(9), 9, MUTED, Typeface.create("sans-serif", Typeface.NORMAL));
        text(c, "16.7", left + dp(14), graphTop + (graphBottom - graphTop) * 0.5f, 9, MUTED, Typeface.create("sans-serif", Typeface.NORMAL));
        text(c, "0", left + dp(14), graphBottom - sp(2), 9, MUTED, Typeface.create("sans-serif", Typeface.NORMAL));
        if (!hasFrames) {
            return;
        }
        float budgetY = graphBottom - (graphBottom - graphTop) * (16.7f / maxMs);
        grid.setColor(0x55FFB35A);
        c.drawLine(graphLeft, budgetY, graphRight, budgetY, grid);
        grid.setColor(0x22FFFFFF);
        area.reset();
        line.reset();
        for (int i = 0; i < framesLength; i++) {
            float x = graphLeft + (graphRight - graphLeft) * i / (framesLength - 1f);
            float value = Math.min(frames[i], maxMs);
            float y = graphBottom - (graphBottom - graphTop) * (value / maxMs);
            if (i == 0) {
                area.moveTo(x, graphBottom);
                area.lineTo(x, y);
                line.moveTo(x, y);
            } else {
                area.lineTo(x, y);
                line.lineTo(x, y);
            }
        }
        area.lineTo(graphRight, graphBottom);
        area.close();
        fill.setColor(0x338B7DE8);
        c.drawPath(area, fill);
        linePaint();
        c.drawPath(line, paint);
    }

    private void linePaint() {
        paint.setStyle(Paint.Style.STROKE);
        paint.setStrokeWidth(dp(2));
        paint.setColor(ACCENT);
        paint.setTypeface(Typeface.create("sans-serif", Typeface.NORMAL));
    }

    private void drawBudget(Canvas c, float left, float top, float width) {
        double frame = decimal("frame", 0f);
        float fraction = (float) Math.min(1.0, frame / 16.7);
        int color = frame <= 16.7 ? GOOD : frame <= 33.4 ? WARN : BAD;
        box.set(left, top, left + width, top + dp(10));
        c.drawRoundRect(box, dp(5), dp(5), card);
        box.set(left, top, left + width * fraction, top + dp(10));
        fill.setColor(color);
        c.drawRoundRect(box, dp(5), dp(5), fill);
        text(c, "FRAME BUDGET", left, top + dp(24), 10, MUTED, Typeface.create("sans-serif-medium", Typeface.NORMAL));
        paint.setStyle(Paint.Style.FILL);
        paint.setTypeface(Typeface.create("sans-serif-medium", Typeface.NORMAL));
        paint.setTextSize(sp(10));
        float labelWidth = paint.measureText("FRAME BUDGET");
        text(c, String.format(java.util.Locale.US, "%.1f / 16.7 ms", frame),
                left + labelWidth + dp(12), top + dp(24), 10, INK,
                Typeface.create("sans-serif", Typeface.NORMAL));
    }

    private void drawCard(Canvas c, float x, float y, float w, float h, String label, String value,
            String secondary, Typeface regular, Typeface medium) {
        box.set(x, y, x + w, y + h);
        c.drawRoundRect(box, dp(10), dp(10), card);
        text(c, label, x + dp(16), y + dp(24), 11, MUTED, medium);
        paint.setStyle(Paint.Style.FILL);
        paint.setTypeface(medium);
        paint.setTextSize(sp(20));
        paint.setColor(INK);
        TextPaint measured = new TextPaint(paint);
        String shown = TextUtils.ellipsize(value, measured, w - dp(32),
                TextUtils.TruncateAt.END).toString();
        c.drawText(shown, x + dp(16), y + dp(46), paint);
        paint.setTypeface(regular);
        paint.setTextSize(sp(14));
        paint.setColor(MUTED);
        measured = new TextPaint(paint);
        String sub = TextUtils.ellipsize(secondary, measured, w - dp(32),
                TextUtils.TruncateAt.END).toString();
        c.drawText(sub, x + dp(16), y + h - dp(12), paint);
    }

    private void drawBar(Canvas c, String label, float x, float y, float w, float fraction,
            String value, int color, Typeface regular, Typeface medium) {
        fraction = Math.max(0f, Math.min(1f, fraction));
        text(c, label, x, y + sp(10), 10, MUTED, medium);
        paint.setStyle(Paint.Style.FILL);
        paint.setTypeface(regular);
        paint.setTextSize(sp(10));
        paint.setColor(INK);
        c.drawText(value, x + w - paint.measureText(value), y + sp(10), paint);
        box.set(x, y + dp(16), x + w, y + dp(25));
        c.drawRoundRect(box, dp(4.5f), dp(4.5f), card);
        box.set(x, y + dp(16), x + w * fraction, y + dp(25));
        fill.setColor(color);
        c.drawRoundRect(box, dp(4.5f), dp(4.5f), fill);
    }

    private String percent(String key) {
        String value = get(key, "-");
        return value.equals("-") ? "-" : value + "%";
    }

    private static String dimension(String id) {
        if (id.endsWith("overworld")) {
            return "Overworld";
        }
        if (id.endsWith("the_nether")) {
            return "Nether";
        }
        if (id.endsWith("the_end")) {
            return "End";
        }
        return id;
    }
}
