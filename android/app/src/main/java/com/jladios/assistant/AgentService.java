package com.jladios.assistant;

import android.accessibilityservice.AccessibilityService;
import android.accessibilityservice.GestureDescription;
import android.content.ClipData;
import android.content.ClipboardManager;
import android.content.Context;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.content.pm.ResolveInfo;
import android.graphics.Path;
import android.graphics.Rect;
import android.net.Uri;
import android.os.Build;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.media.AudioManager;
import android.util.DisplayMetrics;
import android.view.KeyEvent;
import android.view.accessibility.AccessibilityEvent;
import android.view.accessibility.AccessibilityNodeInfo;

import java.text.Normalizer;
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;

/**
 * Contrôle du téléphone pour /unlock : exécute les actions demandées par le
 * cerveau Rust (ouvrir une appli, lire l'écran, toucher, taper...).
 */
public class AgentService extends AccessibilityService {
    private final Handler handler = new Handler(Looper.getMainLooper());
    /** Éléments numérotés (#n) de la dernière lecture d'écran. */
    private final List<AccessibilityNodeInfo> nodes = new ArrayList<>();

    private final Runnable loop = new Runnable() {
        @Override
        public void run() {
            String a;
            int n = 0;
            while (n++ < 20 && (a = Native.pollAction()) != null) handle(a);
            handler.postDelayed(this, 120);
        }
    };

    @Override
    protected void onServiceConnected() {
        super.onServiceConnected();
        Native.ensureInit(this);
        Native.setAgentConnected(true);
        handler.post(loop);
    }

    @Override
    public boolean onUnbind(Intent intent) {
        handler.removeCallbacks(loop);
        Native.setAgentConnected(false);
        return super.onUnbind(intent);
    }

    @Override
    public void onAccessibilityEvent(AccessibilityEvent event) {}

    @Override
    public void onInterrupt() {}

    private void done(long id, String s) {
        Native.actionResult(id, s);
    }

    private void doneLater(long id, String s, long ms) {
        handler.postDelayed(() -> done(id, s), ms);
    }

    private void handle(String raw) {
        int tab = raw.indexOf('\t');
        if (tab < 0) return;
        long id;
        try {
            id = Long.parseLong(raw.substring(0, tab));
        } catch (NumberFormatException e) {
            return;
        }
        String cmd = raw.substring(tab + 1).trim();
        try {
            exec(id, cmd);
        } catch (Throwable t) {
            done(id, "ERREUR : " + t);
        }
    }

    private void exec(long id, String cmd) {
        int sp = cmd.indexOf(' ');
        String verb = (sp < 0 ? cmd : cmd.substring(0, sp)).toLowerCase(Locale.ROOT);
        String rest = sp < 0 ? "" : cmd.substring(sp + 1).trim();
        switch (verb) {
            case "screen":
                done(id, dumpScreen());
                break;
            case "open":
                openApp(id, rest);
                break;
            case "url":
                try {
                    Intent i = new Intent(Intent.ACTION_VIEW, Uri.parse(rest)).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
                    startActivity(i);
                    doneLater(id, "ok : lien ouvert", 1500);
                } catch (Exception e) {
                    done(id, "ERREUR : " + e.getMessage());
                }
                break;
            case "tap_text":
                tapText(id, rest);
                break;
            case "tap_id":
            case "long_id": {
                AccessibilityNodeInfo n = nodeAt(rest);
                if (n == null) {
                    done(id, "ERREUR : élément introuvable, relis l'écran (screen) puis réessaie");
                    break;
                }
                if (verb.equals("tap_id")) clickNode(id, n);
                else {
                    Rect r = new Rect();
                    n.getBoundsInScreen(r);
                    Path p = new Path();
                    p.moveTo(r.centerX(), r.centerY());
                    gesture(id, p, 800);
                }
                break;
            }
            case "long_press": {
                float[] xy = coords(rest);
                if (xy == null) { done(id, "ERREUR : usage long_press <x> <y>"); break; }
                Path p = new Path();
                p.moveTo(xy[0], xy[1]);
                gesture(id, p, 800);
                break;
            }
            case "double_tap": {
                float[] xy = coords(rest);
                if (xy == null) { done(id, "ERREUR : usage double_tap <x> <y>"); break; }
                Path p = new Path();
                p.moveTo(xy[0], xy[1]);
                GestureDescription.Builder b = new GestureDescription.Builder();
                b.addStroke(new GestureDescription.StrokeDescription(p, 0, 50));
                b.addStroke(new GestureDescription.StrokeDescription(p, 150, 50));
                boolean sent = dispatchGesture(b.build(), null, null);
                doneLater(id, sent ? "ok" : "ERREUR : geste refusé", 500);
                break;
            }
            case "type_id": {
                int s2 = rest.indexOf(' ');
                if (s2 < 0) { done(id, "ERREUR : usage type_id <n> <texte>"); break; }
                AccessibilityNodeInfo n = nodeAt(rest.substring(0, s2));
                if (n == null) { done(id, "ERREUR : élément introuvable, relis l'écran"); break; }
                n.performAction(AccessibilityNodeInfo.ACTION_FOCUS);
                setText(id, n, rest.substring(s2 + 1).trim());
                break;
            }
            case "append": {
                AccessibilityNodeInfo n = inputNode();
                if (n == null) { done(id, "ERREUR : aucun champ de saisie actif"); break; }
                CharSequence cur = n.getText();
                setText(id, n, (cur == null ? "" : cur.toString()) + rest);
                break;
            }
            case "clear": {
                AccessibilityNodeInfo n = inputNode();
                if (n == null) { done(id, "ERREUR : aucun champ de saisie actif"); break; }
                setText(id, n, "");
                break;
            }
            case "select_all": {
                AccessibilityNodeInfo n = inputNode();
                if (n == null) { done(id, "ERREUR : aucun champ de saisie actif"); break; }
                CharSequence cur = n.getText();
                Bundle b = new Bundle();
                b.putInt(AccessibilityNodeInfo.ACTION_ARGUMENT_SELECTION_START_INT, 0);
                b.putInt(AccessibilityNodeInfo.ACTION_ARGUMENT_SELECTION_END_INT, cur == null ? 0 : cur.length());
                doneLater(id, n.performAction(AccessibilityNodeInfo.ACTION_SET_SELECTION, b) ? "ok" : "ERREUR : sélection refusée", 300);
                break;
            }
            case "copy":
            case "paste": {
                AccessibilityNodeInfo n = inputNode();
                if (n == null) { done(id, "ERREUR : aucun champ de saisie actif"); break; }
                int act = verb.equals("copy") ? AccessibilityNodeInfo.ACTION_COPY : AccessibilityNodeInfo.ACTION_PASTE;
                doneLater(id, n.performAction(act) ? "ok" : "ERREUR : action refusée", 300);
                break;
            }
            case "copy_text": {
                ClipboardManager cm = (ClipboardManager) getSystemService(Context.CLIPBOARD_SERVICE);
                cm.setPrimaryClip(ClipData.newPlainText("jladios", rest));
                done(id, "ok : texte dans le presse-papiers");
                break;
            }
            case "scroll_to":
                scrollTo(id, rest, 0);
                break;
            case "apps": {
                PackageManager pm = getPackageManager();
                List<String> names = new ArrayList<>();
                for (ResolveInfo r : pm.queryIntentActivities(new Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_LAUNCHER), 0))
                    names.add(String.valueOf(r.loadLabel(pm)));
                java.util.Collections.sort(names, String.CASE_INSENSITIVE_ORDER);
                done(id, String.join(", ", names));
                break;
            }
            case "dial":
                try {
                    startActivity(new Intent(Intent.ACTION_DIAL, Uri.parse("tel:" + Uri.encode(rest))).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));
                    doneLater(id, "ok : numéroteur ouvert (touche le bouton d'appel avec tap_text ou tap_id)", 1200);
                } catch (Exception e) {
                    done(id, "ERREUR : " + e.getMessage());
                }
                break;
            case "search":
                try {
                    startActivity(new Intent(Intent.ACTION_VIEW, Uri.parse("https://www.google.com/search?q=" + Uri.encode(rest))).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));
                    doneLater(id, "ok : recherche ouverte", 1800);
                } catch (Exception e) {
                    done(id, "ERREUR : " + e.getMessage());
                }
                break;
            case "volume": {
                AudioManager am = (AudioManager) getSystemService(Context.AUDIO_SERVICE);
                int dir = rest.equals("up") ? AudioManager.ADJUST_RAISE : rest.equals("down") ? AudioManager.ADJUST_LOWER
                        : rest.equals("mute") ? AudioManager.ADJUST_MUTE : 0;
                if (dir == 0) { done(id, "ERREUR : usage volume up|down|mute"); break; }
                am.adjustStreamVolume(AudioManager.STREAM_MUSIC, dir, AudioManager.FLAG_SHOW_UI);
                done(id, "ok");
                break;
            }
            case "media": {
                int key = rest.equals("play") || rest.equals("pause") ? KeyEvent.KEYCODE_MEDIA_PLAY_PAUSE
                        : rest.equals("next") ? KeyEvent.KEYCODE_MEDIA_NEXT
                        : rest.equals("prev") ? KeyEvent.KEYCODE_MEDIA_PREVIOUS : 0;
                if (key == 0) { done(id, "ERREUR : usage media play|pause|next|prev"); break; }
                AudioManager am = (AudioManager) getSystemService(Context.AUDIO_SERVICE);
                am.dispatchMediaKeyEvent(new KeyEvent(KeyEvent.ACTION_DOWN, key));
                am.dispatchMediaKeyEvent(new KeyEvent(KeyEvent.ACTION_UP, key));
                done(id, "ok");
                break;
            }
            case "screenshot":
                if (Build.VERSION.SDK_INT >= 28) global(id, GLOBAL_ACTION_TAKE_SCREENSHOT);
                else done(id, "ERREUR : capture d'écran indisponible avant Android 9");
                break;
            case "lock":
                if (Build.VERSION.SDK_INT >= 28) global(id, GLOBAL_ACTION_LOCK_SCREEN);
                else done(id, "ERREUR : verrouillage indisponible avant Android 9");
                break;
            case "tap": {
                String[] p = rest.split("[ ,]+");
                if (p.length < 2) {
                    done(id, "ERREUR : usage tap <x> <y>");
                    break;
                }
                Path path = new Path();
                path.moveTo(Float.parseFloat(p[0]), Float.parseFloat(p[1]));
                gesture(id, path, 60);
                break;
            }
            case "swipe":
                swipe(id, rest);
                break;
            case "scroll":
                swipe(id, rest.equals("up") ? "down" : "up"); // faire défiler = balayer à l'inverse
                break;
            case "type":
                typeText(id, rest);
                break;
            case "enter":
                enter(id);
                break;
            case "back":
                global(id, GLOBAL_ACTION_BACK);
                break;
            case "home":
                global(id, GLOBAL_ACTION_HOME);
                break;
            case "recents":
                global(id, GLOBAL_ACTION_RECENTS);
                break;
            case "notifications":
                global(id, GLOBAL_ACTION_NOTIFICATIONS);
                break;
            case "quick_settings":
                global(id, GLOBAL_ACTION_QUICK_SETTINGS);
                break;
            case "wait": {
                long ms = 1000;
                try {
                    ms = Math.min(5000, Long.parseLong(rest));
                } catch (NumberFormatException ignored) {
                }
                doneLater(id, "ok", ms);
                break;
            }
            default:
                done(id, "ERREUR : commande inconnue '" + verb + "'");
        }
    }

    private void global(long id, int action) {
        doneLater(id, performGlobalAction(action) ? "ok" : "ERREUR : action refusée", 500);
    }

    private void gesture(long id, Path path, long durationMs) {
        GestureDescription.Builder b = new GestureDescription.Builder();
        b.addStroke(new GestureDescription.StrokeDescription(path, 0, durationMs));
        boolean sent = dispatchGesture(b.build(), new GestureResultCallback() {
            @Override
            public void onCompleted(GestureDescription g) {
                doneLater(id, "ok", 500);
            }

            @Override
            public void onCancelled(GestureDescription g) {
                done(id, "ERREUR : geste annulé");
            }
        }, null);
        if (!sent) done(id, "ERREUR : geste refusé");
    }

    private void swipe(long id, String dir) {
        DisplayMetrics m = getResources().getDisplayMetrics();
        float w = m.widthPixels, h = m.heightPixels;
        float x1 = w / 2, y1 = h / 2, x2 = x1, y2 = y1;
        switch (dir) {
            case "up": y1 = h * 0.75f; y2 = h * 0.25f; break;
            case "down": y1 = h * 0.25f; y2 = h * 0.75f; break;
            case "left": x1 = w * 0.8f; x2 = w * 0.2f; break;
            case "right": x1 = w * 0.2f; x2 = w * 0.8f; break;
            default:
                done(id, "ERREUR : direction attendue : up, down, left ou right");
                return;
        }
        Path p = new Path();
        p.moveTo(x1, y1);
        p.lineTo(x2, y2);
        gesture(id, p, 300);
    }

    private static String norm(String s) {
        return Normalizer.normalize(s == null ? "" : s, Normalizer.Form.NFD)
                .replaceAll("\\p{M}+", "").toLowerCase(Locale.ROOT).trim();
    }

    private void openApp(long id, String query) {
        PackageManager pm = getPackageManager();
        Intent main = new Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_LAUNCHER);
        List<ResolveInfo> apps = pm.queryIntentActivities(main, 0);
        String q = norm(query);
        String best = null;
        int bestScore = 0;
        List<String> names = new ArrayList<>();
        for (ResolveInfo r : apps) {
            String label = norm(String.valueOf(r.loadLabel(pm)));
            String pkg = r.activityInfo.packageName;
            names.add(String.valueOf(r.loadLabel(pm)));
            int score = 0;
            if (label.equals(q) || pkg.equalsIgnoreCase(query)) score = 4;
            else if (label.startsWith(q)) score = 3;
            else if (label.contains(q)) score = 2;
            else if (pkg.toLowerCase(Locale.ROOT).contains(q)) score = 1;
            if (score > bestScore) {
                bestScore = score;
                best = pkg;
            }
        }
        if (best == null) {
            done(id, "ERREUR : appli introuvable. Applis installées : " + String.join(", ", names));
            return;
        }
        Intent i = pm.getLaunchIntentForPackage(best);
        if (i == null) {
            done(id, "ERREUR : impossible de lancer " + best);
            return;
        }
        i.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
        startActivity(i);
        doneLater(id, "ok : " + best + " lancée", 1500);
    }

    private void tapText(long id, String text) {
        AccessibilityNodeInfo root = getRootInActiveWindow();
        if (root == null) {
            done(id, "ERREUR : écran illisible");
            return;
        }
        List<AccessibilityNodeInfo> found = root.findAccessibilityNodeInfosByText(text);
        if (found == null || found.isEmpty()) {
            done(id, "ERREUR : texte '" + text + "' introuvable à l'écran");
            return;
        }
        clickNode(id, found.get(0));
    }

    private AccessibilityNodeInfo findEditable(AccessibilityNodeInfo node) {
        if (node == null) return null;
        if (node.isEditable()) return node;
        for (int i = 0; i < node.getChildCount(); i++) {
            AccessibilityNodeInfo r = findEditable(node.getChild(i));
            if (r != null) return r;
        }
        return null;
    }

    private AccessibilityNodeInfo inputNode() {
        AccessibilityNodeInfo root = getRootInActiveWindow();
        if (root == null) return null;
        AccessibilityNodeInfo f = root.findFocus(AccessibilityNodeInfo.FOCUS_INPUT);
        return f != null ? f : findEditable(root);
    }

    private void typeText(long id, String text) {
        AccessibilityNodeInfo n = inputNode();
        if (n == null) {
            done(id, "ERREUR : aucun champ de saisie (touche d'abord un champ avec tap_text ou tap)");
            return;
        }
        setText(id, n, text);
    }

    private void setText(long id, AccessibilityNodeInfo n, String text) {
        Bundle b = new Bundle();
        b.putCharSequence(AccessibilityNodeInfo.ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE, text);
        boolean ok = n.performAction(AccessibilityNodeInfo.ACTION_SET_TEXT, b);
        doneLater(id, ok ? "ok" : "ERREUR : saisie refusée (champ protégé ?)", 400);
    }

    private float[] coords(String s) {
        String[] p = s.trim().split("[ ,]+");
        if (p.length < 2) return null;
        try {
            return new float[]{Float.parseFloat(p[0]), Float.parseFloat(p[1])};
        } catch (NumberFormatException e) {
            return null;
        }
    }

    private AccessibilityNodeInfo nodeAt(String s) {
        try {
            int i = Integer.parseInt(s.trim().replace("#", ""));
            if (i < 0 || i >= nodes.size()) return null;
            AccessibilityNodeInfo n = nodes.get(i);
            n.refresh();
            return n;
        } catch (NumberFormatException e) {
            return null;
        }
    }

    private void clickNode(long id, AccessibilityNodeInfo n) {
        AccessibilityNodeInfo c = n;
        while (c != null && !c.isClickable()) c = c.getParent();
        if (c != null && c.performAction(AccessibilityNodeInfo.ACTION_CLICK)) {
            doneLater(id, "ok", 700);
            return;
        }
        Rect r = new Rect();
        n.getBoundsInScreen(r);
        Path p = new Path();
        p.moveTo(r.centerX(), r.centerY());
        gesture(id, p, 60);
    }

    /** Fait défiler (jusqu'à 8 fois) jusqu'à ce que le texte soit visible. */
    private void scrollTo(long id, String text, int tries) {
        AccessibilityNodeInfo root = getRootInActiveWindow();
        if (root != null) {
            List<AccessibilityNodeInfo> f = root.findAccessibilityNodeInfosByText(text);
            if (f != null && !f.isEmpty()) {
                done(id, "ok : '" + text + "' visible");
                return;
            }
        }
        if (tries >= 8) {
            done(id, "ERREUR : '" + text + "' introuvable après défilement");
            return;
        }
        DisplayMetrics m = getResources().getDisplayMetrics();
        Path p = new Path();
        p.moveTo(m.widthPixels / 2f, m.heightPixels * 0.75f);
        p.lineTo(m.widthPixels / 2f, m.heightPixels * 0.25f);
        GestureDescription.Builder b = new GestureDescription.Builder();
        b.addStroke(new GestureDescription.StrokeDescription(p, 0, 300));
        boolean sent = dispatchGesture(b.build(), new GestureResultCallback() {
            @Override
            public void onCompleted(GestureDescription g) {
                handler.postDelayed(() -> scrollTo(id, text, tries + 1), 600);
            }

            @Override
            public void onCancelled(GestureDescription g) {
                done(id, "ERREUR : geste annulé");
            }
        }, null);
        if (!sent) done(id, "ERREUR : geste refusé");
    }

    private void enter(long id) {
        AccessibilityNodeInfo n = inputNode();
        boolean ok = false;
        if (n != null && Build.VERSION.SDK_INT >= 30) {
            ok = n.performAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_IME_ENTER.getId());
        }
        doneLater(id, ok ? "ok" : "ERREUR : touche Entrée indisponible (touche le bouton avec tap_text)", 700);
    }

    private String dumpScreen() {
        AccessibilityNodeInfo root = getRootInActiveWindow();
        if (root == null) return "ERREUR : écran illisible (appli protégée ou écran verrouillé)";
        nodes.clear();
        StringBuilder sb = new StringBuilder();
        sb.append("appli : ").append(root.getPackageName()).append('\n');
        DisplayMetrics m = getResources().getDisplayMetrics();
        sb.append("écran : ").append(m.widthPixels).append('x').append(m.heightPixels).append('\n');
        walk(root, sb, 0);
        if (sb.length() > 4000) {
            sb.setLength(4000);
            sb.append("\n(tronqué)");
        }
        return sb.toString();
    }

    private void walk(AccessibilityNodeInfo n, StringBuilder sb, int depth) {
        if (n == null || sb.length() > 4000 || depth > 40) return;
        if (n.isVisibleToUser()) {
            CharSequence t = n.getText();
            CharSequence d = n.getContentDescription();
            String label = t != null && t.length() > 0 ? t.toString() : (d != null ? d.toString() : "");
            if (n.isEditable() && label.isEmpty() && Build.VERSION.SDK_INT >= 26 && n.getHintText() != null)
                label = n.getHintText().toString();
            if ((!label.isEmpty() || n.isClickable() || n.isEditable()) && nodes.size() < 150) {
                Rect r = new Rect();
                n.getBoundsInScreen(r);
                sb.append('#').append(nodes.size()).append(" [").append(r.centerX()).append(',').append(r.centerY()).append("] ");
                nodes.add(n);
                if (n.isEditable()) sb.append("(champ) ");
                else if (n.isClickable()) sb.append("(bouton) ");
                if (n.isCheckable()) sb.append(n.isChecked() ? "(coché) " : "(décoché) ");
                if (n.isSelected()) sb.append("(sélectionné) ");
                if (!n.isEnabled()) sb.append("(inactif) ");
                if (n.isScrollable()) sb.append("(défile) ");
                sb.append('"').append(label.replace('\n', ' ')).append("\"\n");
            }
        }
        for (int i = 0; i < n.getChildCount(); i++) walk(n.getChild(i), sb, depth + 1);
    }
}
