package com.lukagejak.horizon;

import android.app.ActivityManager;
import android.app.GameManager;
import android.app.NativeActivity;
import android.content.ClipData;
import android.content.ClipboardManager;
import android.content.Context;
import android.content.Intent;
import android.hardware.Sensor;
import android.hardware.SensorEvent;
import android.hardware.SensorEventListener;
import android.hardware.SensorManager;
import android.net.Uri;
import android.os.Build;
import android.os.Bundle;
import android.os.VibrationEffect;
import android.os.Vibrator;
import android.text.InputType;
import android.view.DisplayCutout;
import android.view.KeyEvent;
import android.view.View;
import android.view.ViewGroup;
import android.view.WindowInsets;
import android.view.WindowInsetsController;
import android.view.WindowManager;
import android.view.inputmethod.BaseInputConnection;
import android.view.inputmethod.EditorInfo;
import android.view.inputmethod.InputConnection;
import android.view.inputmethod.InputMethodManager;
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

    /** The phone turned by (x, y) radians since the last call: the gyroscope's look. */
    static native void nativeGyro(float x, float y);

    private TextSink sink;
    private boolean keyboardShown;
    private SensorEventListener gyroscope;

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
                    nativeKeyboard(shown);
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
            case "sustained_performance":
                boolean sustained = arg.equals("1");
                runOnUiThread(() -> getWindow().setSustainedPerformanceMode(sustained));
                return null;
            case "game_mode":
                return gameMode();
            case "model":
                return Build.MANUFACTURER + " " + Build.MODEL + " (Android " + Build.VERSION.RELEASE + ")";
            default:
                return null;
        }
    }

    private String gameMode() {
        if (Build.VERSION.SDK_INT < 31) {
            return "unsupported";
        }
        GameManager manager = getSystemService(GameManager.class);
        if (manager == null) {
            return "unsupported";
        }
        switch (manager.getGameMode()) {
            case GameManager.GAME_MODE_PERFORMANCE:
                return "performance";
            case GameManager.GAME_MODE_BATTERY:
                return "battery";
            case GameManager.GAME_MODE_STANDARD:
                return "standard";
            case GameManager.GAME_MODE_CUSTOM:
                return "custom";
            default:
                return "unsupported";
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
        InputMethodManager manager = getSystemService(InputMethodManager.class);
        if (manager == null) {
            return;
        }
        if (show) {
            sink.setFocusableInTouchMode(true);
            sink.requestFocus();
            manager.restartInput(sink);
            // showSoftInput is ignored until the view has taken focus.
            sink.post(() -> {
                if (Build.VERSION.SDK_INT >= 30) {
                    WindowInsetsController controller = getWindow().getInsetsController();
                    if (controller != null) {
                        controller.show(WindowInsets.Type.ime());
                    }
                }
                manager.showSoftInput(sink, 0);
            });
        } else {
            if (Build.VERSION.SDK_INT >= 30) {
                WindowInsetsController controller = getWindow().getInsetsController();
                if (controller != null) {
                    controller.hide(WindowInsets.Type.ime());
                }
            }
            manager.hideSoftInputFromWindow(sink.getWindowToken(), 0);
            sink.clearFocus();
        }
    }

    /**
     * A view the soft keyboard types into. It keeps no text: what is committed
     * goes to the game's focused field, and a word still being composed is
     * typed and erased there as it changes, so the field always shows it.
     */
    static final class TextSink extends View {
        TextSink(Context context) {
            super(context);
            setFocusable(true);
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
