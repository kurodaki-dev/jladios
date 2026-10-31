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
    private int bg = UiPrefs.BG, fg = UiPrefs.FG, green = UiPrefs.ACCENT;
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
    private int color = UiPrefs.FG;
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
        bg = UiPrefs.bg(this);
        fg = UiPrefs.fg(this);
        green = UiPrefs.accent(this);
        color = fg;
        getWindow().setStatusBarColor(bg);
        getWindow().setNavigationBarColor(bg);
        Native.ensureInit(this);

        LinearLayout root = new LinearLayout(this);
        root.setOrientation(LinearLayout.VERTICAL);
        root.setBackgroundColor(bg);
        rootView = root;

        // barre de titre façon fenêtre de terminal
        TextView title = new TextView(this);
        title.setTypeface(Typeface.MONOSPACE, Typeface.BOLD);
        title.setTextSize(12);
        title.setTextColor(0xFF9E9E9E);
        title.setBackgroundColor(0xFF1E1E1E);
        title.setPadding(dp(10), dp(6), dp(10), dp(6));
        title.setText("\u25CF \u25CF \u25CF   jladios — ~/assistant");
        root.addView(title);

        // onglets : terminal | characters
        LinearLayout tabs = new LinearLayout(this);
        tabs.setBackgroundColor(0xFF161616);
        tabTerm = tab(I18n.t("terminal", "terminal"), 0);
        tabChars = tab("characters", 1);
        tabCustom = tab("custom", 2);
        tabs.addView(tabTerm);
        tabs.addView(tabChars);
        tabs.addView(tabCustom);
        root.addView(tabs);
        termPane = new LinearLayout(this);
        termPane.setOrientation(LinearLayout.VERTICAL);
        root.addView(termPane, new LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, 0, 1f));
        chars = new CharactersPanel(this);
        chars.setVisibility(View.GONE);
        root.addView(chars, new LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, 0, 1f));
        custom = new CustomPanel(this, this::applyTheme);
        custom.setVisibility(View.GONE);
        root.addView(custom, new LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, 0, 1f));

        // boutons rapides
        HorizontalScrollView hs = new HorizontalScrollView(this);
        hs.setHorizontalScrollBarEnabled(false);
        LinearLayout bar = new LinearLayout(this);
        bar.setPadding(dp(6), dp(4), dp(6), dp(4));
        String[][] buttons = {
                {I18n.t("[ afficher ]", "[ show ]"), "/show"}, {I18n.t("[ parler ]", "[ talk ]"), "/listen"}, {"[ stop ]", "/stop"},
                {I18n.t("[ cacher ]", "[ hide ]"), "/hide"}, {I18n.t("[ état ]", "[ status ]"), "/status"}, {I18n.t("[ aide ]", "[ help ]"), "/help"}, {I18n.t("[ quitter ]", "[ quit ]"), "/quit"}
        };
        for (String[] b : buttons) {
            TextView t = new TextView(this);
            t.setText(b[0]);
            t.setTypeface(Typeface.MONOSPACE, Typeface.BOLD);
            t.setTextSize(13);
            t.setTextColor(green);
            accentViews.add(t);
            t.setPadding(dp(6), dp(6), dp(6), dp(6));
            final String cmd = b[1];
            t.setOnClickListener(v -> {
                if (cmd.equals("/quit")) quitAll();
                else Native.onInput(cmd);
            });
            bar.addView(t);
        }
        hs.addView(bar);
        termPane.addView(hs);

        scroll = new ScrollView(this);
        scroll.setFillViewport(true);
        term = new TextView(this);
        term.setTypeface(Typeface.MONOSPACE);
        term.setTextSize(UiPrefs.font(this));
        term.setTextColor(fg);
        term.setTextIsSelectable(true);
        term.setPadding(dp(8), dp(4), dp(8), dp(8));
        term.setText("", TextView.BufferType.EDITABLE);
        buf = term.getEditableText();
        scroll.addView(term);
        termPane.addView(scroll, new LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, 0, 1f));

        // ligne de saisie
        LinearLayout row = new LinearLayout(this);
        row.setGravity(Gravity.CENTER_VERTICAL);
        row.setPadding(dp(8), dp(2), dp(8), dp(2));
        GradientDrawable border = new GradientDrawable();
        border.setColor(0xFF111111);
        border.setStroke(dp(1), UiPrefs.dark(green));
        inputBorder = border;
        row.setBackground(border);
        TextView prompt = new TextView(this);
        prompt.setTypeface(Typeface.MONOSPACE, Typeface.BOLD);
        prompt.setTextSize(15);
        prompt.setTextColor(green);
        accentViews.add(prompt);
        prompt.setText("$ ");
        row.addView(prompt);
        input = new EditText(this);
        input.setTypeface(Typeface.MONOSPACE);
        input.setTextSize(14);
        input.setTextColor(Color.WHITE);
        input.setHintTextColor(0xFF666666);
        input.setHint(I18n.t("parle à ton assistant ou tape /help", "talk to your assistant or type /help"));
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
        termPane.addView(row);

        setContentView(root);
        showTab(0);

        if (Build.VERSION.SDK_INT >= 33
                && checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) {
            requestPermissions(new String[]{Manifest.permission.POST_NOTIFICATIONS}, 2);
        }
        handler.post(poller);
        if (savedInstanceState == null) handleIntent(getIntent());
    }

    private TextView tabTerm, tabChars, tabCustom;
    private LinearLayout rootView;
    private CustomPanel custom;
    private int currentTab;
    private final java.util.List<TextView> accentViews = new java.util.ArrayList<>();
    private GradientDrawable inputBorder;
    private LinearLayout termPane;
    private CharactersPanel chars;

    private TextView tab(String label, int idx) {
        TextView t = new TextView(this);
        t.setText(label);
        t.setTypeface(Typeface.MONOSPACE, Typeface.BOLD);
        t.setTextSize(13);
        t.setPadding(dp(14), dp(8), dp(14), dp(8));
        t.setOnClickListener(v -> showTab(idx));
        return t;
    }

    private void showTab(int idx) {
        currentTab = idx;
        termPane.setVisibility(idx == 0 ? View.VISIBLE : View.GONE);
        chars.setVisibility(idx == 1 ? View.VISIBLE : View.GONE);
        custom.setVisibility(idx == 2 ? View.VISIBLE : View.GONE);
        TextView[] ts = {tabTerm, tabChars, tabCustom};
        for (int i = 0; i < ts.length; i++) ts[i].setTextColor(i == idx ? green : 0xFF777777);
        if (idx == 1) chars.refresh();
        if (idx == 2) custom.refresh();
    }

    /** Applique les réglages de l'onglet custom sans perdre le contenu du terminal. */
    private void applyTheme() {
        int oldFg = fg;
        bg = UiPrefs.bg(this);
        fg = UiPrefs.fg(this);
        green = UiPrefs.accent(this);
        if (color == oldFg) color = fg;
        getWindow().setStatusBarColor(bg);
        getWindow().setNavigationBarColor(bg);
        rootView.setBackgroundColor(bg);
        term.setTextColor(fg);
        term.setTextSize(UiPrefs.font(this));
        for (TextView t : accentViews) t.setTextColor(green);
        if (inputBorder != null) inputBorder.setStroke(dp(1), UiPrefs.dark(green));
        showTab(currentTab);
    }

    /** Ouverture d'un fichier .jladios depuis une autre appli (VIEW ou partage). */
    private void handleIntent(Intent i) {
        if (i == null) return;
        Uri u = null;
        if (Intent.ACTION_VIEW.equals(i.getAction())) u = i.getData();
        else if (Intent.ACTION_SEND.equals(i.getAction())) u = i.getParcelableExtra(Intent.EXTRA_STREAM);
        if (u == null) return;
        String msg = chars.importUri(u);
        sysPrint(msg);
        showTab(1);
    }

    @Override
    protected void onNewIntent(Intent intent) {
        super.onNewIntent(intent);
        setIntent(intent);
        handleIntent(intent);
    }

    @Override
    protected void onActivityResult(int req, int result, Intent data) {
        super.onActivityResult(req, result, data);
        String msg = chars.onResult(req, result, data);
        if (msg != null) sysPrint(msg);
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
            sysPrint(I18n.t("Autorise \"Accès à tous les fichiers\" pour Jladios, puis reviens ici.", "Allow \"All files access\" for Jladios, then come back here."));
            startActivity(new Intent(Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION,
                    Uri.parse("package:" + getPackageName())));
        } else {
            requestPermissions(new String[]{Manifest.permission.READ_EXTERNAL_STORAGE,
                    Manifest.permission.WRITE_EXTERNAL_STORAGE}, 3);
        }
    }

    /** Ferme tout : personnage flottant, micro, service d'accessibilité, modèles chargés, processus. */
    private void quitAll() {
        sysPrint(I18n.t("fermeture de Jladios...", "closing Jladios..."));
        handler.removeCallbacks(poller);
        Native.setOverlayVisible(false);
        stopService(new Intent(this, OverlayService.class));
        AgentService.shutdown();
        finishAndRemoveTask();
        // laisse le temps aux services de s'arrêter, puis tue le processus (libère aussi les modèles en mémoire)
        new Handler(Looper.getMainLooper()).postDelayed(() -> {
            android.os.Process.killProcess(android.os.Process.myPid());
            System.exit(0);
        }, 400);
    }

    private void runCommand(String c) {
        if (c.equals("quit")) {
            quitAll();
            return;
        }
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
                sysPrint(I18n.t("impossible d'ouvrir le navigateur, copie le lien ci-dessus", "could not open the browser, copy the link above"));
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
                sysPrint(I18n.t("personnage caché", "character hidden"));
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
            sysPrint(I18n.t("Autorise \"Afficher par-dessus les autres applis\" pour Jladios, puis reviens ici.", "Allow \"Display over other apps\" for Jladios, then come back here."));
            startActivity(new Intent(Settings.ACTION_MANAGE_OVERLAY_PERMISSION,
                    Uri.parse("package:" + getPackageName())));
            return;
        }
        if (checkSelfPermission(Manifest.permission.RECORD_AUDIO) != PackageManager.PERMISSION_GRANTED) {
            sysPrint(I18n.t("J'ai besoin du micro pour t'entendre.", "I need the microphone to hear you."));
            requestPermissions(new String[]{Manifest.permission.RECORD_AUDIO}, 1);
            return;
        }
        pendingOverlay = false;
        Intent i = new Intent(this, OverlayService.class);
        startForegroundService(i);
        sysPrint(I18n.t("personnage affiché \u2713  (appui = parler, appui long = terminal, glisser = déplacer)", "character shown \u2713  (tap = talk, long press = terminal, drag = move)"));
    }

    @Override
    public void onRequestPermissionsResult(int requestCode, String[] permissions, int[] results) {
        if (requestCode == 1) {
            boolean ok = results.length > 0 && results[0] == PackageManager.PERMISSION_GRANTED;
            if (ok) {
                if (pendingOverlay) tryStartOverlay();
            } else {
                sysPrint("\u001b[31m" + I18n.t("micro refusé : le personnage s'affichera mais ne pourra pas t'entendre.", "microphone denied: the character will show but cannot hear you.") + "\u001b[0m");
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
            if (n == 0) { color = fg; bold = false; }
            else if (n == 1) bold = true;
            else if (n == 22) bold = false;
            else if (n == 39) color = fg;
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
