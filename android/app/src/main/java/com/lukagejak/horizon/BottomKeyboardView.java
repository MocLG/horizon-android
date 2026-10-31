package com.lukagejak.horizon;

import android.content.Context;
import android.graphics.Color;
import android.graphics.Typeface;
import android.graphics.drawable.GradientDrawable;
import android.util.TypedValue;
import android.view.Gravity;
import android.view.View;
import android.widget.FrameLayout;
import android.widget.LinearLayout;
import android.widget.TextView;

/**
 * A self-contained keyboard on the bottom display. Android's IME only follows
 * the top-focused display, so the game draws its own instead of trying to move
 * the system keyboard across displays.
 */
final class BottomKeyboardView extends FrameLayout {
    private static final String[][] LETTERS = {
            {"q", "w", "e", "r", "t", "y", "u", "i", "o", "p"},
            {"a", "s", "d", "f", "g", "h", "j", "k", "l"},
            {"shift", "z", "x", "c", "v", "b", "n", "m", "back"},
            {"mode", ",", "space", ".", "hide", "done"},
    };
    private static final String[][] SYMBOLS = {
            {"1", "2", "3", "4", "5", "6", "7", "8", "9", "0"},
            {"-", "/", ":", ";", "(", ")", "$", "&", "@", "\""},
            {"*", ".", ",", "?", "!", "'", "#", "+", "=", "back"},
            {"mode", ",", "space", ".", "hide", "done"},
    };

    private final Runnable hide;
    private final LinearLayout content;
    private boolean symbols;
    private boolean shift;

    BottomKeyboardView(Context context, Runnable hide) {
        super(context);
        this.hide = hide;
        GradientDrawable background = new GradientDrawable();
        background.setColor(Color.argb(242, 16, 16, 20));
        background.setCornerRadii(new float[]{
                dp(16), dp(16), dp(16), dp(16), 0, 0, 0, 0});
        setBackground(background);
        setFocusable(false);
        content = new LinearLayout(context);
        content.setOrientation(LinearLayout.VERTICAL);
        content.setPadding(dp(6), dp(8), dp(6), dp(8));
        addView(content, new LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT));
        setVisibility(GONE);
        layoutKeys();
    }

    private int dp(int value) {
        return Math.round(TypedValue.applyDimension(
                TypedValue.COMPLEX_UNIT_DIP, value, getResources().getDisplayMetrics()));
    }

    private void layoutKeys() {
        content.removeAllViews();
        String[][] rows = symbols ? SYMBOLS : LETTERS;
        for (String[] row : rows) {
            LinearLayout line = new LinearLayout(getContext());
            line.setOrientation(LinearLayout.HORIZONTAL);
            content.addView(line, new LinearLayout.LayoutParams(
                    LinearLayout.LayoutParams.MATCH_PARENT, 0, 1f));
            for (String key : row) {
                line.addView(key(key));
            }
        }
    }

    private TextView key(String name) {
        TextView key = new TextView(getContext());
        key.setGravity(Gravity.CENTER);
        key.setTextColor(Color.rgb(242, 242, 245));
        key.setTextSize(TypedValue.COMPLEX_UNIT_SP, 18);
        key.setTypeface(Typeface.DEFAULT_BOLD);
        GradientDrawable shape = new GradientDrawable();
        shape.setColor(Color.rgb(43, 43, 49));
        shape.setCornerRadius(dp(7));
        key.setBackground(shape);
        key.setClickable(true);
        key.setFocusable(false);
        float weight = 1f;
        switch (name) {
            case "shift" -> {
                key.setText("Shift");
                weight = 1.5f;
                key.setOnClickListener(v -> {
                    shift = !shift;
                    layoutKeys();
                });
            }
            case "back" -> {
                key.setText("Del");
                weight = 1.5f;
                key.setOnClickListener(v -> HorizonActivity.nativeKey(1));
            }
            case "mode" -> {
                key.setText(symbols ? "ABC" : "?123");
                weight = 1.4f;
                key.setOnClickListener(v -> {
                    symbols = !symbols;
                    shift = false;
                    layoutKeys();
                });
            }
            case "space" -> {
                key.setText("Space");
                weight = 4f;
                key.setOnClickListener(v -> HorizonActivity.nativeText(" "));
            }
            case "hide" -> {
                key.setText("Hide");
                weight = 1.3f;
                key.setOnClickListener(v -> hide.run());
            }
            case "done" -> {
                key.setText("Done");
                weight = 1.4f;
                key.setOnClickListener(v -> HorizonActivity.nativeKey(2));
            }
            default -> {
                String text = symbols ? name : shift ? name.toUpperCase() : name;
                key.setText(text);
                key.setOnClickListener(v -> {
                    HorizonActivity.nativeText(text);
                    if (shift) {
                        shift = false;
                        layoutKeys();
                    }
                });
            }
        }
        LinearLayout.LayoutParams params = new LinearLayout.LayoutParams(0, -1, weight);
        params.setMargins(dp(2), dp(2), dp(2), dp(2));
        key.setLayoutParams(params);
        return key;
    }
}
