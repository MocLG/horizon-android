package com.lukagejak.horizon;

import android.content.Context;
import android.graphics.Canvas;
import android.graphics.Paint;
import android.graphics.RectF;
import android.graphics.Typeface;
import android.text.TextPaint;
import android.text.TextUtils;
import android.view.MotionEvent;
import android.view.View;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.Map;

/** The bottom screen's Items tab: the player inventory as tappable slots. */
final class SecondInventoryView extends View {
    private static final int BG = 0xFF0B0B10;
    private static final int CARD = 0xFF141419;
    private static final int SLOT = 0xFF1C1C23;
    private static final int INK = 0xFFF2F2F6;
    private static final int MUTED = 0xFF8A8A96;
    private static final int ACCENT = 0xFF8B7DE8;
    private static final int GOOD = 0xFF6ED6A0;

    private static final class SlotData {
        String id;
        String name;
        int count;
    }

    private static final class Cell {
        final int slot;
        final RectF rect;

        Cell(int slot, float x, float y, float size) {
            this.slot = slot;
            this.rect = new RectF(x, y, x + size, y + size);
        }
    }

    private final float density;
    private final float scaled;
    private final Paint paint = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final Paint fill = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final Paint border = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final RectF box = new RectF();
    private final HashMap<Integer, SlotData> slots = new HashMap<>();
    private final HashMap<Integer, RectF> rects = new HashMap<>();
    private final ArrayList<Cell> cells = new ArrayList<>();
    private boolean overlay = true;
    private boolean splitMode;
    private String lastRects = "";
    private SlotData carried;
    private int selected;
    private int lastWidth = -1;
    private int lastHeight = -1;
    private int downSlot = -1;

    SecondInventoryView(Context context) {
        super(context);
        density = getResources().getDisplayMetrics().density;
        scaled = getResources().getDisplayMetrics().scaledDensity;
        paint.setTypeface(Typeface.create("sans-serif", Typeface.NORMAL));
        fill.setStyle(Paint.Style.FILL);
        border.setStyle(Paint.Style.STROKE);
        border.setStrokeWidth(dp(1.5f));
        border.setColor(ACCENT);
        setClickable(true);
    }

    private float dp(float v) {
        return v * density;
    }

    private float sp(float v) {
        return v * scaled;
    }

    /** `slot|id|count|name` lines, then `carried|...` and `selected|n`. */
    void setData(String text) {
        slots.clear();
        carried = null;
        selected = 0;
        for (String line : text.split("\n")) {
            String[] parts = line.split("\\|", 4);
            if (parts.length < 2) {
                continue;
            }
            if (parts[0].equals("selected")) {
                try {
                    selected = Integer.parseInt(parts[1].trim());
                } catch (NumberFormatException ignored) {
                }
            } else if (parts[0].equals("carried")) {
                carried = slotData(parts);
            } else {
                try {
                    slots.put(Integer.parseInt(parts[0].trim()), slotData(parts));
                } catch (NumberFormatException ignored) {
                }
            }
        }
        invalidate();
    }

    /** Real inventory slot rectangles in bottom-surface pixels: `rect|slot|x|y|w|h`. */
    void setRects(String text) {
        if (text.equals(lastRects)) {
            return;
        }
        lastRects = text;
        rects.clear();
        for (String line : text.split("\n")) {
            String[] parts = line.split("\\|");
            if (parts.length < 6 || !parts[0].equals("rect")) {
                continue;
            }
            try {
                int slot = Integer.parseInt(parts[1].trim());
                float x = Float.parseFloat(parts[2].trim());
                float y = Float.parseFloat(parts[3].trim());
                float w = Float.parseFloat(parts[4].trim());
                float h = Float.parseFloat(parts[5].trim());
                rects.put(slot, new RectF(x, y, x + w, y + h));
            } catch (NumberFormatException ignored) {
            }
        }
        if (!overlay) {
            invalidate();
        }
    }

    /** The main renderer always draws the real inventory behind this view. */
    void setOverlay(boolean value) {
        overlay = true;
        setWillNotDraw(true);
        lastRects = "";
    }

    /** Toggle right-click/split for the next inventory taps. */
    boolean toggleSplit() {
        splitMode = !splitMode;
        return splitMode;
    }

    private static SlotData slotData(String[] parts) {
        SlotData data = new SlotData();
        data.id = parts.length > 1 ? parts[1] : "";
        try {
            data.count = parts.length > 2 ? Integer.parseInt(parts[2].trim()) : 0;
        } catch (NumberFormatException e) {
            data.count = 0;
        }
        data.name = parts.length > 3 ? parts[3] : shortName(data.id);
        return data;
    }

    private static String shortName(String id) {
        int colon = id.indexOf(':');
        String name = colon >= 0 ? id.substring(colon + 1) : id;
        return name.replace('_', ' ');
    }

    private void ensureCells() {
        int w = getWidth();
        int h = getHeight();
        if (w == lastWidth && h == lastHeight && !cells.isEmpty()) {
            return;
        }
        lastWidth = w;
        lastHeight = h;
        cells.clear();
        float left = dp(70);
        float right = w - dp(36);
        float content = right - left;
        float gap = dp(6);
        float slot = Math.min((content - gap * 8) / 9f, dp(96));
        float gridWidth = 9 * slot + 8 * gap;
        float gridLeft = left + (content - gridWidth) / 2;
        float gridTop = dp(24) + dp(78);
        for (int row = 0; row < 3; row++) {
            for (int col = 0; col < 9; col++) {
                cells.add(new Cell(
                        9 + row * 9 + col,
                        gridLeft + col * (slot + gap),
                        gridTop + row * (slot + gap),
                        slot));
            }
        }
        float hotbar = gridTop + 3 * (slot + gap) + dp(12);
        for (int col = 0; col < 9; col++) {
            cells.add(new Cell(
                    col,
                    gridLeft + col * (slot + gap),
                    hotbar,
                    slot));
        }
        float armor = hotbar + slot + dp(20);
        for (int i = 0; i < 5; i++) {
            cells.add(new Cell(
                    500 + i,
                    gridLeft + i * (slot + gap),
                    armor,
                    slot));
        }
    }

    private String carriedText() {
        if (carried == null || carried.count <= 0) {
            return "Hand: empty";
        }
        return "Hand: " + carried.name + (carried.count > 1 ? " x" + carried.count : "");
    }

    private void text(Canvas c, String s, float x, float y, float size, int color, Typeface face) {
        paint.setStyle(Paint.Style.FILL);
        paint.setTypeface(face);
        paint.setTextSize(sp(size));
        paint.setColor(color);
        c.drawText(s, x, y, paint);
    }

    private void drawItem(Canvas c, Cell cell, SlotData item) {
        if (item == null || item.id.isEmpty()) {
            return;
        }
        float pad = dp(6);
        paint.setStyle(Paint.Style.FILL);
        paint.setTypeface(Typeface.create("sans-serif-medium", Typeface.NORMAL));
        paint.setTextSize(sp(10));
        paint.setColor(INK);
        TextPaint measured = new TextPaint(paint);
        String shown = TextUtils.ellipsize(item.name, measured,
                cell.rect.width() - dp(12), TextUtils.TruncateAt.END).toString();
        c.drawText(shown, cell.rect.left + pad, cell.rect.top + dp(18), paint);
        if (item.count > 1) {
            String count = Integer.toString(item.count);
            paint.setStyle(Paint.Style.FILL);
            paint.setTypeface(Typeface.create("sans-serif-medium", Typeface.NORMAL));
            paint.setTextSize(sp(12));
            paint.setColor(INK);
            c.drawText(count, cell.rect.right - pad - paint.measureText(count),
                    cell.rect.bottom - dp(6), paint);
        }
    }

    @Override
    protected void onDraw(Canvas c) {
        super.onDraw(c);
        int w = getWidth();
        int h = getHeight();
        if (w <= 0 || h <= 0) {
            return;
        }
        if (overlay) {
            return;
        }
        c.drawColor(BG);
        ensureCells();
        Typeface medium = Typeface.create("sans-serif-medium", Typeface.NORMAL);
        float left = dp(70);
        text(c, "INVENTORY", left, dp(24) + sp(12), 13, MUTED, medium);
        text(c, carriedText(), left, dp(24) + sp(36), 14,
                carried == null ? MUTED : ACCENT, medium);

        for (Cell cell : cells) {
            fill.setColor(SLOT);
            box.set(cell.rect);
            c.drawRoundRect(box, dp(8), dp(8), fill);
            if (cell.slot == selected && cell.slot < 9) {
                c.drawRoundRect(box, dp(8), dp(8), border);
            }
            drawItem(c, cell, slots.get(cell.slot));
        }

        // The carried stack's own slot, top right.
        float side = dp(72);
        float hx = w - dp(36) - side;
        float hy = dp(24);
        box.set(hx, hy, hx + side, hy + side);
        fill.setColor(CARD);
        c.drawRoundRect(box, dp(8), dp(8), fill);
        text(c, "HAND", hx, hy + dp(18), 9, MUTED, medium);
        if (carried != null && carried.count > 0) {
            text(c, carried.name, hx + dp(6), hy + dp(40), 10, INK, medium);
            if (carried.count > 1) {
                text(c, "x" + carried.count, hx + dp(6), hy + dp(58), 12, GOOD, medium);
            }
        }
    }

    private int slotAt(float x, float y) {
        if (overlay) {
            for (Map.Entry<Integer, RectF> entry : rects.entrySet()) {
                if (entry.getValue().contains(x, y)) {
                    return entry.getKey();
                }
            }
            return -1;
        }
        ensureCells();
        for (Cell cell : cells) {
            if (cell.rect.contains(x, y)) {
                return cell.slot;
            }
        }
        return -1;
    }

    @Override
    public boolean onTouchEvent(MotionEvent event) {
        switch (event.getActionMasked()) {
            case MotionEvent.ACTION_DOWN:
                HorizonActivity.nativeInventoryTouch(true);
                HorizonActivity.nativeInventoryCursor(event.getX(), event.getY());
                downSlot = slotAt(event.getX(), event.getY());
                // Press picks up (or places the held stack) at once, so the
                // carried item follows the finger while dragging.
                if (downSlot >= 0) {
                    HorizonActivity.nativeInventoryClick(downSlot, splitMode ? 1 : 0);
                }
                return true;
            case MotionEvent.ACTION_MOVE:
                HorizonActivity.nativeInventoryTouch(true);
                HorizonActivity.nativeInventoryCursor(event.getX(), event.getY());
                return true;
            case MotionEvent.ACTION_UP: {
                HorizonActivity.nativeInventoryCursor(event.getX(), event.getY());
                HorizonActivity.nativeInventoryTouch(false);
                int upSlot = slotAt(event.getX(), event.getY());
                if (downSlot >= 0 && upSlot >= 0 && upSlot != downSlot) {
                    // Split mode splits the stack on press; the release still
                    // drops everything now carried into the target.
                    HorizonActivity.nativeInventoryClick(upSlot, 0);
                }
                downSlot = -1;
                return true;
            }
            case MotionEvent.ACTION_CANCEL:
                HorizonActivity.nativeInventoryTouch(false);
                downSlot = -1;
                return true;
            default:
                return super.onTouchEvent(event);
        }
    }
}
