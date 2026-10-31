package com.lukagejak.horizon;

import android.app.ActivityManager;
import android.app.NativeActivity;
import android.app.Presentation;
import android.content.ClipData;
import android.content.ClipboardManager;
import android.content.Context;
import android.content.Intent;
import android.graphics.Typeface;
import android.graphics.drawable.GradientDrawable;
import android.hardware.Sensor;
import android.hardware.SensorEvent;
import android.hardware.SensorEventListener;
import android.hardware.SensorManager;
import android.hardware.display.DisplayManager;
import android.net.Uri;
import android.os.Build;
import android.os.Bundle;
import android.os.VibrationEffect;
import android.os.Vibrator;
import android.text.InputType;
import android.util.Log;
import android.view.Display;
import android.view.DisplayCutout;
import android.view.Gravity;
import android.view.KeyEvent;
import android.view.Surface;
import android.view.SurfaceHolder;
import android.view.SurfaceView;
import android.view.View;
import android.view.ViewGroup;
import android.view.Window;
import android.view.WindowInsets;
import android.view.WindowInsetsController;
import android.view.WindowManager;
import android.view.inputmethod.BaseInputConnection;
import android.view.inputmethod.EditorInfo;
import android.view.inputmethod.InputConnection;
import android.view.inputmethod.InputMethodManager;
import android.widget.FrameLayout;
import android.widget.LinearLayout;
import android.widget.TextView;
import android.widget.Toast;
import android.window.OnBackInvokedDispatcher;

/**
 * The game's window. Everything runs in libhorizon.so (android_main); this
 * class only reaches what has no native API: the clipboard, links, the soft
 * keyboard and a few facts about the device. The native side calls
 * {@link #platform} and receives typed text through the native methods.
 */
public class HorizonActivity extends NativeActivity {
    static {
        // Loaded here as well as by NativeActivity, so the native methods below resolve.
        System.loadLibrary("horizon");
    }

    /** Text the soft keyboard committed. */
    static native void nativeText(String text);

    /** A key the soft keyboard pressed: 1 is backspace, 2 is enter, 3 is the back gesture. */
    static native void nativeKey(int key);

    /** The soft keyboard showed or hid, whoever asked it to. */
    static native void nativeKeyboard(boolean shown);

    /** A file the player picked to import: its name and its text. */
    static native void nativePicked(String name, String text);

    /** The phone turned by (x, y) radians since the last call: the gyroscope's look. */
    static native void nativeGyro(float x, float y);

    /** The second screen's Surface, or null when it went away. */
    static native void nativeSecondSurface(Surface surface);

    /** The bottom panel's tab: 0 is the map, 1 the debug view, 2 the inventory. */
    static native void nativeSecondTab(int tab);

    /** A tap on a bottom-screen inventory slot. */
    static native void nativeInventoryClick(int slot, int button);

    /** Where the finger is on the bottom inventory, in surface pixels. */
    static native void nativeInventoryCursor(float x, float y);

    /** Whether a finger is down on the bottom inventory. */
    static native void nativeInventoryTouch(boolean touching);

    private static final int SAVE_DOCUMENT = 1;
    private static final int OPEN_DOCUMENTS = 2;

    private TextSink sink;
    /** The text an export writes where the player picks. */
    private String toSave;
    private boolean keyboardShown;
    private SensorEventListener gyroscope;
    /** The second screen (the AYN Thor's bottom panel), if it has one. */
    private Presentation secondScreen;
    private FrameLayout secondRoot;
    private TextSink secondSink;
    private boolean secondKeyboardShown;
    private SecondDebugView secondDebug;
    private SecondInventoryView secondInventory;
    private TextView secondMapTab;
    private TextView secondDebugTab;
    private TextView secondItemsTab;
    private TextView secondSplitButton;
    private BottomKeyboardView secondKeyboard;
    private LinearLayout secondTabs;
    private int secondTab;

    @Override
    protected void onCreate(Bundle state) {
        super.onCreate(state);
        getWindow().addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
        sink = new TextSink(this);
        addContentView(sink, new ViewGroup.LayoutParams(1, 1));
        if (Build.VERSION.SDK_INT >= 30) {
            // The keyboard also hides on its own arrow or a swipe, which the
            // game has to know to show it again on the next press.
            getWindow().getDecorView().setOnApplyWindowInsetsListener((view, insets) -> {
                boolean shown = insets.isVisible(WindowInsets.Type.ime());
                if (shown != keyboardShown) {
                    keyboardShown = shown;
                    nativeKeyboard(shown || secondKeyboardShown);
                }
                return view.onApplyWindowInsets(insets);
            });
        }
        if (Build.VERSION.SDK_INT >= 33) {
            // Back is the game's, as Escape is: the activity never finishes on it.
            getOnBackInvokedDispatcher().registerOnBackInvokedCallback(
                    OnBackInvokedDispatcher.PRIORITY_DEFAULT, () -> nativeKey(3));
        }
        immersive();
    }

    @Override
    protected void onResume() {
        super.onResume();
        showSecondScreen();
    }

    @Override
    protected void onPause() {
        hideSecondScreen();
        super.onPause();
    }

    @Override
    @SuppressWarnings("deprecation")
    public void onBackPressed() {
        nativeKey(3);
    }

    @Override
    public void onWindowFocusChanged(boolean focused) {
        super.onWindowFocusChanged(focused);
        if (focused) {
            immersive();
        }
    }

    private void immersive() {
        if (Build.VERSION.SDK_INT >= 30) {
            WindowInsetsController controller = getWindow().getInsetsController();
            if (controller != null) {
                controller.hide(WindowInsets.Type.systemBars());
                controller.setSystemBarsBehavior(
                        WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE);
            }
        } else {
            getWindow().getDecorView().setSystemUiVisibility(
                    View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY
                            | View.SYSTEM_UI_FLAG_FULLSCREEN
                            | View.SYSTEM_UI_FLAG_HIDE_NAVIGATION
                            | View.SYSTEM_UI_FLAG_LAYOUT_STABLE
                            | View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION
                            | View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN);
        }
    }

    /**
     * Shows the app's content on a second screen when the device has a
     * presentation display (the AYN Thor's bottom panel), and hands its
     * Surface to the native renderer.
     */
    private void showSecondScreen() {
        if (secondScreen != null) {
            return;
        }
        DisplayManager manager = getSystemService(DisplayManager.class);
        if (manager == null) {
            return;
        }
        int main = getWindowManager().getDefaultDisplay().getDisplayId();
        for (Display display : manager.getDisplays(DisplayManager.DISPLAY_CATEGORY_PRESENTATION)) {
            if (display.getDisplayId() == main) {
                continue;
            }
            try {
                Presentation presentation = new Presentation(this, display);
                SurfaceView view = new SurfaceView(this);
                view.getHolder().addCallback(new SurfaceHolder.Callback() {
                    @Override
                    public void surfaceCreated(SurfaceHolder holder) {
                        Log.i("Horizon", "Second screen surface on display "
                                + display.getDisplayId());
                        nativeSecondSurface(holder.getSurface());
                    }

                    @Override
                    public void surfaceChanged(SurfaceHolder holder, int format, int width,
                            int height) {
                    }

                    @Override
                    public void surfaceDestroyed(SurfaceHolder holder) {
                        Log.i("Horizon", "Second screen surface destroyed");
                        nativeSecondSurface(null);
                    }
                });
                SecondDebugView debug = new SecondDebugView(this);
                debug.setVisibility(View.GONE);
                SecondInventoryView inventory = new SecondInventoryView(this);
                inventory.setVisibility(View.GONE);
                TextView mapTab = secondTabView("Map");
                TextView debugTab = secondTabView("Debug");
                TextView itemsTab = secondTabView("Items");
                TextView splitButton = secondTabView("Drag");
                splitButton.setTextSize(12);
                TextSink second = new TextSink(this);
                FrameLayout root = new FrameLayout(this);
                root.addView(view, new FrameLayout.LayoutParams(
                        ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT));
                root.addView(second, new FrameLayout.LayoutParams(1, 1));
                if (Build.VERSION.SDK_INT >= 30) {
                    presentation.getWindow().getDecorView().setOnApplyWindowInsetsListener(
                            (v, insets) -> {
                                boolean shown = insets.isVisible(WindowInsets.Type.ime());
                                if (shown != secondKeyboardShown) {
                                    secondKeyboardShown = shown;
                                    nativeKeyboard(shown || keyboardShown);
                                }
                                return v.onApplyWindowInsets(insets);
                            });
                }
                root.addView(debug, new FrameLayout.LayoutParams(
                        ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT));
                root.addView(inventory, new FrameLayout.LayoutParams(
                        ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT));
                LinearLayout tabs = new LinearLayout(this);
                tabs.setOrientation(LinearLayout.VERTICAL);
                LinearLayout.LayoutParams mapTabAt =
                        new LinearLayout.LayoutParams(dp(60), dp(56));
                LinearLayout.LayoutParams debugTabAt =
                        new LinearLayout.LayoutParams(dp(60), dp(56));
                debugTabAt.topMargin = dp(8);
                LinearLayout.LayoutParams itemsTabAt =
                        new LinearLayout.LayoutParams(dp(60), dp(56));
                itemsTabAt.topMargin = dp(8);
                LinearLayout.LayoutParams splitAt =
                        new LinearLayout.LayoutParams(dp(60), dp(44));
                splitAt.topMargin = dp(8);
                tabs.addView(mapTab, mapTabAt);
                tabs.addView(debugTab, debugTabAt);
                tabs.addView(itemsTab, itemsTabAt);
                tabs.addView(splitButton, splitAt);
                FrameLayout.LayoutParams tabsAt = new FrameLayout.LayoutParams(
                        ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT,
                        Gravity.LEFT | Gravity.CENTER_VERTICAL);
                tabsAt.leftMargin = dp(8);
                root.addView(tabs, tabsAt);
                mapTab.setOnClickListener(v -> selectSecondTab(0));
                debugTab.setOnClickListener(v -> selectSecondTab(1));
                itemsTab.setOnClickListener(v -> selectSecondTab(2));
                splitButton.setOnClickListener(v -> {
                    boolean split = inventory.toggleSplit();
                    splitButton.setText(split ? "Split" : "Drag");
                    splitButton.setBackground(tabBackground(split));
                });
                BottomKeyboardView keyboard =
                        new BottomKeyboardView(this, this::hideBottomKeyboard);
                FrameLayout.LayoutParams keyboardAt = new FrameLayout.LayoutParams(
                        ViewGroup.LayoutParams.MATCH_PARENT, dp(310), Gravity.BOTTOM);
                root.addView(keyboard, keyboardAt);

                presentation.setContentView(root);
                presentation.show();
                secondScreen = presentation;
                secondRoot = root;
                secondSink = second;
                secondDebug = debug;
                secondInventory = inventory;
                secondMapTab = mapTab;
                secondDebugTab = debugTab;
                secondItemsTab = itemsTab;
                secondSplitButton = splitButton;
                secondKeyboard = keyboard;
                secondTabs = tabs;
                selectSecondTab(0);
                Log.i("Horizon", "Second screen shown on display " + display.getDisplayId());
                return;
            } catch (RuntimeException e) {
                Log.w("Horizon", "Second screen unavailable: " + e);
            }
        }
    }

    private void hideSecondScreen() {
        if (secondScreen != null) {
            secondScreen.dismiss();
            secondScreen = null;
        }
        secondRoot = null;
        secondSink = null;
        secondKeyboardShown = false;
        secondDebug = null;
        secondInventory = null;
        secondMapTab = null;
        secondDebugTab = null;
        secondItemsTab = null;
        secondSplitButton = null;
        secondKeyboard = null;
        secondTabs = null;
        nativeSecondSurface(null);
    }

    private void selectSecondTab(int tab) {
        secondTab = tab;
        if (tab != 2) {
            nativeInventoryTouch(false);
        }
        if (secondDebug != null) {
            secondDebug.setVisibility(tab == 1 ? View.VISIBLE : View.GONE);
        }
        if (secondInventory != null) {
            secondInventory.setVisibility(tab == 2 ? View.VISIBLE : View.GONE);
        }
        if (secondMapTab != null) {
            secondMapTab.setBackground(tabBackground(tab == 0));
        }
        if (secondDebugTab != null) {
            secondDebugTab.setBackground(tabBackground(tab == 1));
        }
        if (secondItemsTab != null) {
            secondItemsTab.setBackground(tabBackground(tab == 2));
        }
        if (secondSplitButton != null) {
            secondSplitButton.setVisibility(tab == 2 ? View.VISIBLE : View.INVISIBLE);
        }
        nativeSecondTab(tab);
        Log.i("Horizon", "Second screen tab " + tab);
    }

    private TextView secondTabView(String label) {
        TextView view = new TextView(this);
        view.setText(label);
        view.setTextSize(14);
        view.setTextColor(0xFFF2F2F6);
        view.setGravity(Gravity.CENTER);
        view.setTypeface(Typeface.create("sans-serif-medium", Typeface.NORMAL));
        view.setBackground(tabBackground(false));
        view.setClickable(true);
        view.setFocusable(true);
        return view;
    }

    private GradientDrawable tabBackground(boolean selected) {
        GradientDrawable drawable = new GradientDrawable();
        drawable.setShape(GradientDrawable.RECTANGLE);
        drawable.setCornerRadius(dp(12));
        drawable.setColor(selected ? 0xFF7A6CD6 : 0xFF1C1C22);
        drawable.setStroke(dp(1), selected ? 0xFF9C90EA : 0xFF34343E);
        return drawable;
    }

    private int dp(float value) {
        return Math.round(value * getResources().getDisplayMetrics().density);
    }

    /**
     * One entry point for the native side, so it needs a single signature.
     * Runs on the game's thread; anything that touches views is posted to the
     * UI thread.
     */
    public String platform(String op, String arg) {
        switch (op) {
            case "clipboard_get":
                return clipboard();
            case "clipboard_set":
                runOnUiThread(() -> {
                    ClipboardManager manager = getSystemService(ClipboardManager.class);
                    if (manager != null) {
                        manager.setPrimaryClip(ClipData.newPlainText("Horizon", arg));
                    }
                });
                return null;
            case "open_url":
                runOnUiThread(() -> {
                    try {
                        startActivity(new Intent(Intent.ACTION_VIEW, Uri.parse(arg))
                                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));
                    } catch (RuntimeException e) {
                        Toast.makeText(this, arg, Toast.LENGTH_LONG).show();
                    }
                });
                return null;
            case "keyboard":
                runOnUiThread(() -> keyboard(arg.equals("1")));
                return null;
            case "gyroscope":
                runOnUiThread(() -> gyroscope(arg.equals("1")));
                return null;
            case "toast":
                runOnUiThread(() -> Toast.makeText(this, arg, Toast.LENGTH_LONG).show());
                return null;
            case "vibrate":
                vibrate(Integer.parseInt(arg));
                return null;
            case "second_debug":
                runOnUiThread(() -> {
                    if (secondDebug != null) {
                        secondDebug.setStats(arg);
                    }
                });
                return null;
            case "second_inventory":
                runOnUiThread(() -> {
                    if (secondInventory != null) {
                        secondInventory.setData(arg);
                    }
                });
                return null;
            case "second_inventory_overlay":
                runOnUiThread(() -> {
                    if (secondInventory != null) {
                        secondInventory.setOverlay(arg.equals("1"));
                    }
                });
                return null;
            case "second_inventory_rects":
                runOnUiThread(() -> {
                    if (secondInventory != null) {
                        secondInventory.setRects(arg);
                    }
                });
                return null;
            case "native_lib_dir":
                return getApplicationInfo().nativeLibraryDir;
            case "files_dir":
                return getFilesDir().getAbsolutePath();
            case "external_dir": {
                java.io.File dir = getExternalFilesDir(null);
                return dir == null ? null : dir.getAbsolutePath();
            }
            case "total_memory": {
                ActivityManager.MemoryInfo info = new ActivityManager.MemoryInfo();
                getSystemService(ActivityManager.class).getMemoryInfo(info);
                return Long.toString(info.totalMem);
            }
            case "insets":
                return insets();
            case "density":
                return Float.toString(getResources().getDisplayMetrics().density);
            case "dark":
                return (getResources().getConfiguration().uiMode
                        & android.content.res.Configuration.UI_MODE_NIGHT_MASK)
                        == android.content.res.Configuration.UI_MODE_NIGHT_YES ? "1" : "0";
            case "language":
                return java.util.Locale.getDefault().toLanguageTag();
            case "save_document": {
                // The suggested file name, a new line, then the text.
                int cut = arg.indexOf('\n');
                String name = arg.substring(0, cut);
                String text = arg.substring(cut + 1);
                runOnUiThread(() -> saveDocument(name, text));
                return null;
            }
            case "open_documents":
                runOnUiThread(this::openDocuments);
                return null;
            case "version_code":
                try {
                    return Long.toString(getPackageManager()
                            .getPackageInfo(getPackageName(), 0).getLongVersionCode());
                } catch (android.content.pm.PackageManager.NameNotFoundException e) {
                    return null;
                }
            case "install_update":
                runOnUiThread(this::installUpdate);
                return null;
            case "model":
                return Build.MANUFACTURER + " " + Build.MODEL + " (Android " + Build.VERSION.RELEASE + ")";
            default:
                return null;
        }
    }

    private String clipboard() {
        ClipboardManager manager = getSystemService(ClipboardManager.class);
        if (manager == null || !manager.hasPrimaryClip()) {
            return "";
        }
        ClipData clip = manager.getPrimaryClip();
        if (clip == null || clip.getItemCount() == 0) {
            return "";
        }
        CharSequence text = clip.getItemAt(0).coerceToText(this);
        return text == null ? "" : text.toString();
    }

    /** Turning the phone looks around, as in Zalith Launcher 2: its X axis turns, its Y axis tilts. */
    private void gyroscope(boolean on) {
        SensorManager sensors = getSystemService(SensorManager.class);
        if (sensors == null) {
            return;
        }
        if (gyroscope != null) {
            sensors.unregisterListener(gyroscope);
            gyroscope = null;
        }
        Sensor sensor = sensors.getDefaultSensor(Sensor.TYPE_GYROSCOPE);
        if (!on || sensor == null) {
            return;
        }
        gyroscope = new SensorEventListener() {
            private long last;

            @Override
            public void onSensorChanged(SensorEvent event) {
                if (last != 0) {
                    float seconds = (event.timestamp - last) / 1e9f;
                    // The other landscape turns the axes round.
                    float sign = getWindowManager().getDefaultDisplay().getRotation()
                            == android.view.Surface.ROTATION_270 ? -1f : 1f;
                    nativeGyro(sign * event.values[0] * seconds, sign * event.values[1] * seconds);
                }
                last = event.timestamp;
            }

            @Override
            public void onAccuracyChanged(Sensor sensor, int accuracy) {
            }
        };
        sensors.registerListener(gyroscope, sensor, SensorManager.SENSOR_DELAY_GAME);
    }

    private void vibrate(int millis) {
        Vibrator vibrator = getSystemService(Vibrator.class);
        if (vibrator != null && vibrator.hasVibrator()) {
            vibrator.vibrate(VibrationEffect.createOneShot(millis, VibrationEffect.DEFAULT_AMPLITUDE));
        }
    }

    /** The display cutout's margins in pixels: "left,top,right,bottom". */
    private String insets() {
        WindowInsets insets = getWindow().getDecorView().getRootWindowInsets();
        DisplayCutout cutout = insets == null ? null : insets.getDisplayCutout();
        if (cutout == null) {
            return "0,0,0,0";
        }
        return cutout.getSafeInsetLeft() + "," + cutout.getSafeInsetTop() + ","
                + cutout.getSafeInsetRight() + "," + cutout.getSafeInsetBottom();
    }

    /** Asks where to save `text`, Downloads or anywhere else, then writes it there. */
    private void saveDocument(String name, String text) {
        toSave = text;
        Intent intent = new Intent(Intent.ACTION_CREATE_DOCUMENT)
                .addCategory(Intent.CATEGORY_OPENABLE)
                .setType(name.endsWith(".json") ? "application/json" : "text/plain")
                .putExtra(Intent.EXTRA_TITLE, name);
        startActivityForResult(intent, SAVE_DOCUMENT);
    }

    /** Lets the player pick files to import, which go to the game as text. */
    private void openDocuments() {
        Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT)
                .addCategory(Intent.CATEGORY_OPENABLE)
                .setType("*/*")
                .putExtra(Intent.EXTRA_ALLOW_MULTIPLE, true);
        startActivityForResult(intent, OPEN_DOCUMENTS);
    }

    @Override
    protected void onActivityResult(int request, int result, Intent data) {
        super.onActivityResult(request, result, data);
        if (result != RESULT_OK || data == null) {
            return;
        }
        if (request == SAVE_DOCUMENT && data.getData() != null && toSave != null) {
            try (java.io.OutputStream out =
                    getContentResolver().openOutputStream(data.getData(), "wt")) {
                out.write(toSave.getBytes(java.nio.charset.StandardCharsets.UTF_8));
                Toast.makeText(this, "Saved", Toast.LENGTH_SHORT).show();
            } catch (java.io.IOException | RuntimeException e) {
                Toast.makeText(this, "Not saved: " + e.getMessage(), Toast.LENGTH_LONG).show();
            }
            toSave = null;
        } else if (request == OPEN_DOCUMENTS) {
            java.util.List<Uri> uris = new java.util.ArrayList<>();
            if (data.getClipData() != null) {
                for (int i = 0; i < data.getClipData().getItemCount(); i++) {
                    uris.add(data.getClipData().getItemAt(i).getUri());
                }
            } else if (data.getData() != null) {
                uris.add(data.getData());
            }
            for (Uri uri : uris) {
                try (java.io.InputStream in = getContentResolver().openInputStream(uri)) {
                    // Layouts and settings are small: anything larger is not one.
                    byte[] bytes = new byte[4 << 20];
                    int length = 0;
                    for (int n; length < bytes.length
                            && (n = in.read(bytes, length, bytes.length - length)) > 0; ) {
                        length += n;
                    }
                    nativePicked(displayName(uri),
                            new String(bytes, 0, length, java.nio.charset.StandardCharsets.UTF_8));
                } catch (java.io.IOException | RuntimeException e) {
                    nativePicked(displayName(uri), "");
                }
            }
        }
    }

    private String displayName(Uri uri) {
        try (android.database.Cursor c = getContentResolver().query(uri,
                new String[] {android.provider.OpenableColumns.DISPLAY_NAME}, null, null, null)) {
            if (c != null && c.moveToFirst()) {
                return c.getString(0);
            }
        } catch (RuntimeException e) {
            // The file's name only labels it.
        }
        String last = uri.getLastPathSegment();
        return last == null ? "file" : last;
    }

    /** Opens Android's installer on the downloaded update, or its setting to allow it first. */
    private void installUpdate() {
        if (!getPackageManager().canRequestPackageInstalls()) {
            Toast.makeText(this, "Allow Horizon to install apps, then press Update again",
                    Toast.LENGTH_LONG).show();
            startActivity(new Intent(android.provider.Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES,
                    Uri.parse("package:" + getPackageName())));
            return;
        }
        Intent intent = new Intent(Intent.ACTION_VIEW)
                .setDataAndType(ApkProvider.uri(), "application/vnd.android.package-archive")
                .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION | Intent.FLAG_ACTIVITY_NEW_TASK);
        startActivity(intent);
    }

    private void keyboard(boolean show) {
        boolean second = secondScreen != null && secondSink != null;
        if (second && secondKeyboard != null) {
            // Android's IME only follows the top-focused display, so the
            // bottom keyboard is drawn here instead of moving the system one.
            if (show) {
                InputMethodManager main = getSystemService(InputMethodManager.class);
                sink.setFocusable(false);
                sink.setFocusableInTouchMode(false);
                sink.clearFocus();
                secondSink.clearFocus();
                if (main != null) {
                    hideIme(main, getWindow(), sink);
                }
                secondKeyboard.setVisibility(View.VISIBLE);
                if (secondTabs != null) {
                    secondTabs.setVisibility(View.GONE);
                }
            } else {
                secondKeyboard.setVisibility(View.GONE);
                if (secondTabs != null) {
                    secondTabs.setVisibility(View.VISIBLE);
                }
            }
            return;
        }
        InputMethodManager manager = getSystemService(InputMethodManager.class);
        Window window = getWindow();
        sink.setFocusable(show);
        sink.setFocusableInTouchMode(show);
        if (show) {
            if (manager == null) {
                return;
            }
            sink.requestFocus();
            manager.restartInput(sink);
            sink.post(() -> {
                sink.requestFocus();
                if (Build.VERSION.SDK_INT >= 30) {
                    WindowInsetsController controller = window.getInsetsController();
                    if (controller != null) {
                        controller.show(WindowInsets.Type.ime());
                    }
                }
                manager.showSoftInput(sink, 0);
            });
        } else if (manager != null) {
            hideIme(manager, window, sink);
            sink.clearFocus();
        }
    }

    private void hideBottomKeyboard() {
        if (secondKeyboard != null) {
            secondKeyboard.setVisibility(View.GONE);
        }
        if (secondTabs != null) {
            secondTabs.setVisibility(View.VISIBLE);
        }
        nativeKeyboard(false);
        // A change just after the game asked for the keyboard is ignored as a
        // stale echo; send it again once that window has passed, unless the
        // player has already reopened it.
        if (secondKeyboard != null) {
            secondKeyboard.postDelayed(() -> {
                if (secondKeyboard != null && secondKeyboard.getVisibility() != View.VISIBLE) {
                    nativeKeyboard(false);
                }
            }, 700);
        }
    }

    private static void hideIme(InputMethodManager manager, Window window, View target) {
        if (Build.VERSION.SDK_INT >= 30) {
            WindowInsetsController controller = window.getInsetsController();
            if (controller != null) {
                controller.hide(WindowInsets.Type.ime());
            }
        }
        manager.hideSoftInputFromWindow(target.getWindowToken(), 0);
    }

    /**
     * A view the soft keyboard types into. It keeps no text: what is committed
     * goes to the game's focused field, and a word still being composed is
     * typed and erased there as it changes, so the field always shows it.
     */
    static final class TextSink extends View {
        TextSink(Context context) {
            super(context);
            // Only `keyboard(true)` makes a sink focusable, so it cannot
            // steal focus (and pop the IME) when the app starts.
            setFocusable(false);
            setFocusableInTouchMode(false);
        }

        @Override
        public boolean onCheckIsTextEditor() {
            return true;
        }

        @Override
        public InputConnection onCreateInputConnection(EditorInfo info) {
            info.inputType = InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS;
            info.imeOptions = EditorInfo.IME_FLAG_NO_FULLSCREEN
                    | EditorInfo.IME_FLAG_NO_EXTRACT_UI
                    | EditorInfo.IME_ACTION_DONE;
            return new Connection(this);
        }
    }

    static final class Connection extends BaseInputConnection {
        /** The word the keyboard is composing, as typed into the field so far. */
        private String composing = "";

        Connection(View view) {
            super(view, false);
        }

        private void replaceComposing(String text) {
            int same = 0;
            while (same < composing.length() && same < text.length()
                    && composing.charAt(same) == text.charAt(same)) {
                same++;
            }
            for (int i = composing.codePointCount(same, composing.length()); i > 0; i--) {
                nativeKey(1);
            }
            if (same < text.length()) {
                nativeText(text.substring(same));
            }
            composing = text;
        }

        @Override
        public boolean setComposingText(CharSequence text, int position) {
            replaceComposing(text.toString());
            return true;
        }

        @Override
        public boolean finishComposingText() {
            composing = "";
            return true;
        }

        @Override
        public boolean commitText(CharSequence text, int position) {
            replaceComposing(text.toString());
            composing = "";
            return true;
        }

        @Override
        public boolean deleteSurroundingText(int before, int after) {
            if (!composing.isEmpty()) {
                composing = "";
            }
            for (int i = 0; i < before; i++) {
                nativeKey(1);
            }
            return true;
        }

        @Override
        public boolean sendKeyEvent(KeyEvent event) {
            if (event.getAction() != KeyEvent.ACTION_DOWN) {
                return true;
            }
            switch (event.getKeyCode()) {
                case KeyEvent.KEYCODE_DEL:
                    composing = "";
                    nativeKey(1);
                    break;
                case KeyEvent.KEYCODE_ENTER:
                case KeyEvent.KEYCODE_NUMPAD_ENTER:
                    composing = "";
                    nativeKey(2);
                    break;
                default:
                    int c = event.getUnicodeChar();
                    if (c != 0) {
                        nativeText(new String(Character.toChars(c)));
                    }
            }
            return true;
        }

        @Override
        public boolean performEditorAction(int action) {
            composing = "";
            nativeKey(2);
            return true;
        }
    }
}
