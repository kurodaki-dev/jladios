package com.jladios.assistant;

import android.app.Activity;
import android.graphics.Color;
import android.graphics.Typeface;
import android.util.TypedValue;
import android.view.View;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.SeekBar;
import android.widget.TextView;

/** Onglet « custom » : apparence du terminal, du personnage flottant et de sa bulle. */
final class CustomPanel extends ScrollView {
    private static final int GRAY = 0xFF9E9E9E;
    private static final int[] ACCENTS = {0xFF4EE04E, 0xFF5FD7FF, 0xFFFF79C6, 0xFFFFB454, 0xFFB48EFF, 0xFFFFFFFF};
    private static final int[] BGS = {0xFF0C0C0C, 0xFF000000, 0xFF0B1020, 0xFF1A1A1A, 0xFF140B1F, 0xFF0B1A12};
    private static final int[] FGS = {0xFFCCCCCC, 0xFFFFFFFF, 0xFFA8FFA8, 0xFFFFD9A0, 0xFFA8E6FF, 0xFF999999};

    private final Activity act;
    private final Runnable onChanged;
    private final LinearLayout list;

    CustomPanel(Activity a, Runnable onChanged) {
        super(a);
        this.act = a;
        this.onChanged = onChanged;
        setFillViewport(true);
        list = new LinearLayout(a);
        list.setOrientation(LinearLayout.VERTICAL);
        list.setPadding(dp(10), dp(8), dp(10), dp(24));
        addView(list);
        refresh();
    }

    private int dp(float v) {
        return (int) TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_DIP, v, act.getResources().getDisplayMetrics());
    }

    private int accent() {
        return UiPrefs.accent(act);
    }

    private TextView text(String s, int color, float size, boolean bold) {
        TextView t = new TextView(act);
        t.setText(s);
        t.setTextColor(color);
        t.setTextSize(size);
        t.setTypeface(Typeface.MONOSPACE, bold ? Typeface.BOLD : Typeface.NORMAL);
        return t;
    }

    private void section(String s) {
        TextView t = text(s, Color.WHITE, 15, true);
        t.setPadding(0, dp(16), 0, dp(4));
        list.addView(t);
    }

    private TextView button(String label, View.OnClickListener l) {
        TextView t = text("[ " + label + " ]", accent(), 13, true);
        t.setPadding(dp(2), dp(8), dp(10), dp(8));
        t.setOnClickListener(l);
        return t;
    }

    /** Pastilles de couleurs ; la pastille choisie est entourée. */
    private void colors(String label, String key, int[] options, int current) {
        list.addView(text(label, GRAY, 12, false));
        LinearLayout row = new LinearLayout(act);
        row.setPadding(0, dp(4), 0, dp(4));
        for (int c : options) {
            TextView dot = text(c == current ? "◉" : "●", c, 30, false);
            if (c == 0xFF000000 || c == 0xFF0C0C0C || c == 0xFF0B1020 || c == 0xFF140B1F || c == 0xFF0B1A12 || c == 0xFF1A1A1A) {
                dot.setBackgroundColor(0xFF444444); // pour voir les fonds sombres
            }
            dot.setPadding(dp(8), 0, dp(8), 0);
            dot.setOnClickListener(v -> {
                UiPrefs.putInt(act, key, c);
                changed();
            });
            LinearLayout.LayoutParams lp = new LinearLayout.LayoutParams(-2, -2);
            lp.rightMargin = dp(8);
            row.addView(dot, lp);
        }
        list.addView(row);
    }

    private interface Fmt {
        String show(int v);
    }

    private void slider(String label, String key, int min, int max, int current, Fmt fmt, Runnable after) {
        TextView t = text(label + " : " + fmt.show(current), GRAY, 12, false);
        list.addView(t);
        SeekBar s = new SeekBar(act);
        s.setMax(max - min);
        s.setProgress(current - min);
        s.getProgressDrawable().setColorFilter(accent(), android.graphics.PorterDuff.Mode.SRC_IN);
        s.getThumb().setColorFilter(accent(), android.graphics.PorterDuff.Mode.SRC_IN);
        s.setOnSeekBarChangeListener(new SeekBar.OnSeekBarChangeListener() {
            @Override
            public void onProgressChanged(SeekBar sb, int p, boolean user) {
                t.setText(label + " : " + fmt.show(p + min));
            }

            @Override
            public void onStartTrackingTouch(SeekBar sb) {}

            @Override
            public void onStopTrackingTouch(SeekBar sb) {
                if (key != null) UiPrefs.putInt(act, key, sb.getProgress() + min);
                if (after != null) after.run();
                if (key != null) changed();
            }
        });
        list.addView(s);
    }

    private void changed() {
        onChanged.run();
        refresh();
    }

    void refresh() {
        list.removeAllViews();
        list.addView(text(I18n.t("Personnaliser", "Customize"), Color.WHITE, 16, true));
        list.addView(text(I18n.t("Les changements s'appliquent tout de suite.", "Changes apply immediately."), GRAY, 12, false));

        section(I18n.t("Terminal", "Terminal"));
        colors(I18n.t("Couleur d'accent", "Accent color"), "accent", ACCENTS, UiPrefs.accent(act));
        colors(I18n.t("Fond", "Background"), "bg", BGS, UiPrefs.bg(act));
        colors(I18n.t("Texte", "Text"), "fg", FGS, UiPrefs.fg(act));
        slider(I18n.t("Taille du texte", "Text size"), "font", 10, 22, UiPrefs.font(act), v -> v + " sp", null);

        section(I18n.t("Personnage flottant", "Floating character"));
        slider(I18n.t("Taille", "Size"), "char_scale", 50, 200, UiPrefs.charScale(act), v -> v + " %", null);
        list.addView(text(I18n.t("Sprites importés : 312 × 512 px conseillés (PNG transparent).", "Imported sprites: 312 × 512 px recommended (transparent PNG)."), GRAY, 11, false));
        LinearLayout row = new LinearLayout(act);
        row.addView(button(UiPrefs.bubbleOn(act) ? I18n.t("bulle : oui", "bubble: on") : I18n.t("bulle : non", "bubble: off"), v -> {
            UiPrefs.putBool(act, "bubble_on", !UiPrefs.bubbleOn(act));
            changed();
        }));
        row.addView(button(I18n.t("réinitialiser la position", "reset position"), v -> {
            UiPrefs.putBool(act, "reset_pos", true);
        }));
        list.addView(row);
        slider(I18n.t("Texte de la bulle", "Bubble text"), "bubble_size", 9, 20, UiPrefs.bubbleSize(act), v -> v + " sp", null);
        slider(I18n.t("Opacité de la bulle", "Bubble opacity"), "bubble_alpha", 30, 100, UiPrefs.bubbleAlpha(act), v -> v + " %", null);

        section(I18n.t("Assistant", "Assistant"));
        LinearLayout r2 = new LinearLayout(act);
        r2.addView(button(I18n.t("muet", "mute") + " on", v -> Native.onInput("/mute on")));
        r2.addView(button(I18n.t("muet", "mute") + " off", v -> Native.onInput("/mute off")));
        list.addView(r2);
        LinearLayout r3 = new LinearLayout(act);
        r3.addView(button(I18n.t("écoute auto", "auto-listen") + " on", v -> Native.onInput("/auto on")));
        r3.addView(button(I18n.t("écoute auto", "auto-listen") + " off", v -> Native.onInput("/auto off")));
        list.addView(r3);
        slider(I18n.t("Vitesse de la voix", "Voice speed"), null, 5, 20, 10, v -> String.format(java.util.Locale.US, "%.1f", v / 10f),
                null);
        // la vitesse est envoyée en relâchant le curseur
        final SeekBar speed = (SeekBar) list.getChildAt(list.getChildCount() - 1);
        speed.setOnSeekBarChangeListener(new SeekBar.OnSeekBarChangeListener() {
            @Override
            public void onProgressChanged(SeekBar sb, int p, boolean user) {
                TextView label = (TextView) list.getChildAt(list.getChildCount() - 2);
                label.setText(I18n.t("Vitesse de la voix", "Voice speed") + " : " + String.format(java.util.Locale.US, "%.1f", (p + 5) / 10f));
            }

            @Override
            public void onStartTrackingTouch(SeekBar sb) {}

            @Override
            public void onStopTrackingTouch(SeekBar sb) {
                Native.onInput("/speed " + String.format(java.util.Locale.US, "%.1f", (sb.getProgress() + 5) / 10f));
            }
        });

        LinearLayout reset = new LinearLayout(act);
        reset.setPadding(0, dp(16), 0, 0);
        reset.addView(button(I18n.t("tout réinitialiser", "reset everything"), v -> {
            UiPrefs.resetAll(act);
            changed();
        }));
        list.addView(reset);
    }
}
