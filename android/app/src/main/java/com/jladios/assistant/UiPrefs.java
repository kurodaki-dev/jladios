package com.jladios.assistant;

import android.content.Context;
import android.content.SharedPreferences;

/** Réglages d'apparence (onglet « custom »), lus par le terminal et par le personnage flottant. */
final class UiPrefs {
    static final int ACCENT = 0xFF4EE04E, BG = 0xFF0C0C0C, FG = 0xFFCCCCCC;
    static final int FONT = 13, CHAR_SCALE = 100, BUBBLE_SIZE = 12, BUBBLE_ALPHA = 94;

    private UiPrefs() {}

    static SharedPreferences p(Context c) {
        return c.getSharedPreferences("jladios_ui", Context.MODE_PRIVATE);
    }

    static int accent(Context c) { return p(c).getInt("accent", ACCENT); }
    static int bg(Context c) { return p(c).getInt("bg", BG); }
    static int fg(Context c) { return p(c).getInt("fg", FG); }
    static int font(Context c) { return p(c).getInt("font", FONT); }
    static int charScale(Context c) { return p(c).getInt("char_scale", CHAR_SCALE); }
    static boolean activeOnListen(Context c) { return p(c).getBoolean("active_on_listen", false); }
    static boolean bubbleOn(Context c) { return p(c).getBoolean("bubble_on", true); }
    static int bubbleSize(Context c) { return p(c).getInt("bubble_size", BUBBLE_SIZE); }
    static int bubbleAlpha(Context c) { return p(c).getInt("bubble_alpha", BUBBLE_ALPHA); }

    static void putInt(Context c, String k, int v) { p(c).edit().putInt(k, v).apply(); }
    static void putBool(Context c, String k, boolean v) { p(c).edit().putBoolean(k, v).apply(); }
    static void resetAll(Context c) { p(c).edit().clear().apply(); }

    /** Version plus sombre d'une couleur (bordures). */
    static int dark(int color) {
        int r = (color >> 16 & 0xFF) * 6 / 10, g = (color >> 8 & 0xFF) * 6 / 10, b = (color & 0xFF) * 6 / 10;
        return 0xFF000000 | r << 16 | g << 8 | b;
    }
}
