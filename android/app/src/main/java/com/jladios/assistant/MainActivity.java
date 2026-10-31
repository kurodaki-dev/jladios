package com.jladios.assistant;

import android.Manifest;
import android.app.Activity;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.graphics.Color;
import android.graphics.Typeface;
import android.graphics.drawable.GradientDrawable;
import android.net.Uri;
import android.os.Build;
import android.os.Environment;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.provider.Settings;
import android.text.Editable;
import android.text.InputType;
import android.text.Spanned;
import android.text.TextUtils;
import android.text.style.ForegroundColorSpan;
import android.text.style.StyleSpan;
import android.util.TypedValue;
import android.view.Gravity;
import android.view.KeyEvent;
import android.view.View;
import android.view.inputmethod.EditorInfo;
import android.widget.EditText;
import android.widget.HorizontalScrollView;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.TextView;

/** Le terminal. Tout le contenu affiché est produit par Rust (avec codes ANSI). */
public class MainActivity extends Activity {
    private static final int BG = 0xFF0C0C0C;
    private static final int FG = 0xFFCCCCCC;
    private static final int GREEN = 0xFF4EE04E;
    private static final int[] PALETTE = {
            0xFF555555, 0xFFFF5555, 0xFF4EE04E, 0xFFFFD75F, 0xFF5F87FF, 0xFFFF79C6, 0xFF5FD7FF, 0xFFD0D0D0
    };
    private static final int[] BRIGHT = {
            0xFF808080, 0xFFFF8787, 0xFF87FF87, 0xFFFFFF87, 0xFF87AFFF, 0xFFFF87FF, 0xFF87FFFF, 0xFFFFFFFF
    };
    private static final int MAX_CHARS = 60000;

    private TextView term;
    private ScrollView scroll;
    private EditText input;
    private Editable buf;
    private final Handler handler = new Handler(Looper.getMainLooper());
    private int color = FG;
    private boolean bold;
    private boolean pendingOverlay;
    private boolean afterFilesOpenAccessibility;

    private final Runnable poller = new Runnable() {
        @Override
        public void run() {
            boolean changed = false;
            String ev;
            int count = 0;
            while (count < 300 && (ev = Native.pollEvent()) != null) {
                count++;
                if (ev.startsWith("o:")) {
                    appendAnsi(ev.substring(2));
                    changed = true;
                } else if (ev.startsWith("c:")) {
                    runCommand(ev.substring(2));
                }
            }
            if (changed) {
                if (buf.length() > MAX_CHARS) buf.delete(0, buf.length() - MAX_CHARS);
                scroll.post(() -> scroll.fullScroll(View.FOCUS_DOWN));
            }
            handler.postDelayed(this, 40);
        }
    };

    private int dp(float v) {
        return (int) TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_DIP, v, getResources().getDisplayMetrics());
    }

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        getWindow().setStatusBarColor(BG);
        getWindow().setNavigationBarColor(BG);
        Native.ensureInit(this);

        LinearLayout root = new LinearLayout(this);
        root.setOrientation(LinearLayout.VERTICAL);
        root.setBackgroundColor(BG);

        // barre de titre façon fenêtre de terminal
        TextView title = new TextView(this);
        title.setTypeface(Typeface.MONOSPACE, Typeface.BOLD);
        title.setTextSize(12);
        title.setTextColor(0xFF9E9E9E);
        title.setBackgroundColor(0xFF1E1E1E);
        title.setPadding(dp(10), dp(6), dp(10), dp(6));
        title.setText("\u25CF \u25CF \u25CF   jladios — ~/assistant");
        root.addView(title);

        // boutons rapides
        HorizontalScrollView hs = new HorizontalScrollView(this);
        hs.setHorizontalScrollBarEnabled(false);
        LinearLayout bar = new LinearLayout(this);
        bar.setPadding(dp(6), dp(4), dp(6), dp(4));
        String[][] buttons = {
                {"[ afficher ]", "/show"}, {"[ parler ]", "/listen"}, {"[ stop ]", "/stop"},
                {"[ cacher ]", "/hide"}, {"[ état ]", "/status"}, {"[ aide ]", "/help"}
        };
        for (String[] b : buttons) {
            TextView t = new TextView(this);
            t.setText(b[0]);
            t.setTypeface(Typeface.MONOSPACE, Typeface.BOLD);
            t.setTextSize(13);
            t.setTextColor(GREEN);
            t.setPadding(dp(6), dp(6), dp(6), dp(6));
            final String cmd = b[1];
            t.setOnClickListener(v -> Native.onInput(cmd));
            bar.addView(t);
        }
        hs.addView(bar);
        root.addView(hs);

        scroll = new ScrollView(this);
        scroll.setFillViewport(true);
        term = new TextView(this);
        term.setTypeface(Typeface.MONOSPACE);
        term.setTextSize(13);
        term.setTextColor(FG);
        term.setTextIsSelectable(true);
        term.setPadding(dp(8), dp(4), dp(8), dp(8));
        term.setText("", TextView.BufferType.EDITABLE);
        buf = term.getEditableText();
        scroll.addView(term);
        root.addView(scroll, new LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, 0, 1f));

        // ligne de saisie
        LinearLayout row = new LinearLayout(this);
        row.setGravity(Gravity.CENTER_VERTICAL);
        row.setPadding(dp(8), dp(2), dp(8), dp(2));
        GradientDrawable border = new GradientDrawable();
        border.setColor(0xFF111111);
        border.setStroke(dp(1), 0xFF2E7D32);
        row.setBackground(border);
        TextView prompt = new TextView(this);
        prompt.setTypeface(Typeface.MONOSPACE, Typeface.BOLD);
        prompt.setTextSize(15);
        prompt.setTextColor(GREEN);
        prompt.setText("$ ");
        row.addView(prompt);
        input = new EditText(this);
        input.setTypeface(Typeface.MONOSPACE);
        input.setTextSize(14);
        input.setTextColor(Color.WHITE);
        input.setHintTextColor(0xFF666666);
        input.setHint("parle à ton assistant ou tape /help");
        input.setBackground(null);
        input.setSingleLine(true);
        input.setInputType(InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS);
        input.setImeOptions(EditorInfo.IME_ACTION_SEND);
        input.setOnEditorActionListener((v, actionId, event) -> {
            boolean enter = event != null && event.getKeyCode() == KeyEvent.KEYCODE_ENTER
                    && event.getAction() == KeyEvent.ACTION_DOWN;
            if (actionId == EditorInfo.IME_ACTION_SEND || actionId == EditorInfo.IME_ACTION_DONE || enter) {
                String line = input.getText().toString();
                input.setText("");
                Native.onInput(line);
                return true;
            }
            return false;
        });
        row.addView(input, new LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f));
        root.addView(row);

        setContentView(root);

        if (Build.VERSION.SDK_INT >= 33
                && checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) {
            requestPermissions(new String[]{Manifest.permission.POST_NOTIFICATIONS}, 2);
        }
        handler.post(poller);
    }

    @Override
    protected void onResume() {
        super.onResume();
        if (afterFilesOpenAccessibility && hasAllFiles()) {
            afterFilesOpenAccessibility = false;
            startActivity(new Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS));
        }
        if (pendingOverlay && Settings.canDrawOverlays(this)) {
            tryStartOverlay();
        }
    }

    @Override
    protected void onDestroy() {
        handler.removeCallbacks(poller);
        super.onDestroy();
    }

    private void sysPrint(String s) {
        appendAnsi("\u001b[33m[android]\u001b[0m " + s + "\n");
        scroll.post(() -> scroll.fullScroll(View.FOCUS_DOWN));
    }

    private boolean hasAllFiles() {
        if (Build.VERSION.SDK_INT >= 30) return Environment.isExternalStorageManager();
        return checkSelfPermission(Manifest.permission.WRITE_EXTERNAL_STORAGE) == PackageManager.PERMISSION_GRANTED;
    }

    private void requestAllFiles() {
        if (Build.VERSION.SDK_INT >= 30) {
            sysPrint("Autorise \"Accès à tous les fichiers\" pour Jladios, puis reviens ici.");
            startActivity(new Intent(Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION,
                    Uri.parse("package:" + getPackageName())));
        } else {
            requestPermissions(new String[]{Manifest.permission.READ_EXTERNAL_STORAGE,
                    Manifest.permission.WRITE_EXTERNAL_STORAGE}, 3);
        }
    }

    private void runCommand(String c) {
        if (c.startsWith("unlock_setup:")) {
            boolean needAcc = c.contains("acc");
            if (!hasAllFiles()) {
                afterFilesOpenAccessibility = needAcc;
                requestAllFiles();
            } else if (needAcc) {
                startActivity(new Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS));
            }
            return;
        }
        if (c.equals("accessibility")) {
            startActivity(new Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS));
            return;
        }
        if (c.startsWith("open:")) {
            try {
                startActivity(new Intent(Intent.ACTION_VIEW, Uri.parse(c.substring(5))));
            } catch (Exception e) {
                sysPrint("impossible d'ouvrir le navigateur, copie le lien ci-dessus");
            }
            return;
        }
        switch (c) {
            case "overlay_start":
                pendingOverlay = true;
                tryStartOverlay();
                break;
            case "overlay_stop":
                pendingOverlay = false;
                stopService(new Intent(this, OverlayService.class));
                Native.setOverlayVisible(false);
                sysPrint("personnage caché");
                break;
            case "clear":
                buf.clear();
                break;
            default:
                break;
        }
    }

    private void tryStartOverlay() {
        if (!Settings.canDrawOverlays(this)) {
            sysPrint("Autorise \"Afficher par-dessus les autres applis\" pour Jladios, puis reviens ici.");
            startActivity(new Intent(Settings.ACTION_MANAGE_OVERLAY_PERMISSION,
                    Uri.parse("package:" + getPackageName())));
            return;
        }
        if (checkSelfPermission(Manifest.permission.RECORD_AUDIO) != PackageManager.PERMISSION_GRANTED) {
            sysPrint("J'ai besoin du micro pour t'entendre.");
            requestPermissions(new String[]{Manifest.permission.RECORD_AUDIO}, 1);
            return;
        }
        pendingOverlay = false;
        Intent i = new Intent(this, OverlayService.class);
        startForegroundService(i);
        sysPrint("personnage affiché \u2713  (appui = parler, appui long = terminal, glisser = déplacer)");
    }

    @Override
    public void onRequestPermissionsResult(int requestCode, String[] permissions, int[] results) {
        if (requestCode == 1) {
            boolean ok = results.length > 0 && results[0] == PackageManager.PERMISSION_GRANTED;
            if (ok) {
                if (pendingOverlay) tryStartOverlay();
            } else {
                sysPrint("\u001b[31mmicro refusé : le personnage s'affichera mais ne pourra pas t'entendre.\u001b[0m");
                pendingOverlay = false;
                startForegroundService(new Intent(this, OverlayService.class));
            }
        }
    }

    // ---------------- interprétation ANSI ----------------

    private void flush(StringBuilder run) {
        if (run.length() == 0) return;
        int start = buf.length();
        buf.append(run);
        int end = buf.length();
        buf.setSpan(new ForegroundColorSpan(color), start, end, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE);
        if (bold) buf.setSpan(new StyleSpan(Typeface.BOLD), start, end, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE);
        run.setLength(0);
    }

    private void applySgr(String params) {
        if (params.isEmpty()) params = "0";
        for (String p : params.split(";")) {
            int n;
            try { n = Integer.parseInt(p); } catch (NumberFormatException e) { continue; }
            if (n == 0) { color = FG; bold = false; }
            else if (n == 1) bold = true;
            else if (n == 22) bold = false;
            else if (n == 39) color = FG;
            else if (n >= 30 && n <= 37) color = PALETTE[n - 30];
            else if (n >= 90 && n <= 97) color = BRIGHT[n - 90];
        }
    }

    private void appendAnsi(String s) {
        StringBuilder run = new StringBuilder();
        int i = 0;
        int len = s.length();
        while (i < len) {
            char c = s.charAt(i);
            if (c == '\u001b' && i + 1 < len && s.charAt(i + 1) == '[') {
                flush(run);
                int j = i + 2;
                while (j < len && s.charAt(j) != 'm') j++;
                applySgr(s.substring(i + 2, Math.min(j, len)));
                i = j + 1;
                continue;
            }
            if (c == '\r') {
                // retour chariot : on réécrit la ligne courante (barres de progression)
                flush(run);
                int last = TextUtils.lastIndexOf(buf, '\n');
                buf.delete(last + 1, buf.length());
                i++;
                continue;
            }
            run.append(c);
            i++;
        }
        flush(run);
    }
}
