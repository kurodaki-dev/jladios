package com.jladios.assistant;

import android.accessibilityservice.AccessibilityService;
import android.accessibilityservice.GestureDescription;
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
import android.util.DisplayMetrics;
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
        AccessibilityNodeInfo n = found.get(0);
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
        Bundle b = new Bundle();
        b.putCharSequence(AccessibilityNodeInfo.ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE, text);
        boolean ok = n.performAction(AccessibilityNodeInfo.ACTION_SET_TEXT, b);
        doneLater(id, ok ? "ok" : "ERREUR : saisie refusée", 400);
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
            if (!label.isEmpty() || n.isClickable() || n.isEditable()) {
                Rect r = new Rect();
                n.getBoundsInScreen(r);
                sb.append('[').append(r.centerX()).append(',').append(r.centerY()).append("] ");
                if (n.isEditable()) sb.append("(champ) ");
                else if (n.isClickable()) sb.append("(bouton) ");
                sb.append('"').append(label.replace('\n', ' ')).append("\"\n");
            }
        }
        for (int i = 0; i < n.getChildCount(); i++) walk(n.getChild(i), sb, depth + 1);
    }
}
