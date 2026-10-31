package com.jladios.assistant;

import android.app.Activity;
import android.app.AlertDialog;
import android.app.PendingIntent;
import android.content.Context;
import android.content.Intent;
import android.content.pm.PackageInstaller;
import android.net.Uri;
import android.os.Build;
import android.provider.Settings;

import org.json.JSONArray;
import org.json.JSONObject;

import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.HttpURLConnection;
import java.net.URL;
import java.util.function.Consumer;

/**
 * Mise à jour intégrée : lit la dernière release GitHub, propose de l'installer,
 * télécharge l'APK puis le passe au PackageInstaller (Android demande la confirmation).
 */
final class Updater {
    static final String REPO = "kurodaki-dev/jladios";
    private static final long CHECK_EVERY_MS = 60 * 60 * 1000L;
    private static volatile boolean busy;

    private Updater() {}

    private static int installedCode(Context c) {
        try {
            return c.getPackageManager().getPackageInfo(c.getPackageName(), 0).versionCode;
        } catch (Exception e) {
            return Integer.MAX_VALUE;
        }
    }

    private static String installedName(Context c) {
        try {
            return c.getPackageManager().getPackageInfo(c.getPackageName(), 0).versionName;
        } catch (Exception e) {
            return "?";
        }
    }

    /** Au lancement (au plus une fois par heure) ou à la demande (/update). */
    static void check(Activity act, boolean manual, Consumer<String> log) {
        android.content.SharedPreferences p = UiPrefs.p(act);
        long now = System.currentTimeMillis();
        if (!manual && now - p.getLong("update_checked", 0) < CHECK_EVERY_MS) return;
        if (busy) return;
        busy = true;
        new Thread(() -> {
            try {
                HttpURLConnection c = (HttpURLConnection) new URL("https://api.github.com/repos/" + REPO + "/releases/latest").openConnection();
                c.setConnectTimeout(10000);
                c.setReadTimeout(15000);
                c.setRequestProperty("Accept", "application/vnd.github+json");
                if (c.getResponseCode() != 200) throw new Exception("HTTP " + c.getResponseCode());
                JSONObject rel = new JSONObject(new String(readAll(c.getInputStream()), "UTF-8"));
                p.edit().putLong("update_checked", now).apply();
                String tag = rel.getString("tag_name"); // v0.1.<numéro de build>
                int latest = Integer.parseInt(tag.substring(tag.lastIndexOf('.') + 1));
                String url = null;
                long size = 0;
                JSONArray assets = rel.optJSONArray("assets");
                for (int i = 0; assets != null && i < assets.length(); i++) {
                    JSONObject a = assets.getJSONObject(i);
                    if (a.getString("name").endsWith(".apk")) {
                        url = a.getString("browser_download_url");
                        size = a.optLong("size");
                    }
                }
                if (latest <= installedCode(act) || url == null) {
                    if (manual) log.accept(I18n.t("Jladios est à jour (version " + installedName(act) + ").", "Jladios is up to date (version " + installedName(act) + ")."));
                    return;
                }
                if (!manual && p.getInt("update_skip", 0) == latest) return;
                final String fUrl = url;
                final long fSize = size;
                final String notes = rel.optString("body", "");
                act.runOnUiThread(() -> ask(act, tag.substring(1), latest, fUrl, fSize, notes, log));
            } catch (Exception e) {
                if (manual) log.accept(I18n.t("Recherche de mise à jour impossible : ", "Could not check for updates: ") + e.getMessage());
            } finally {
                busy = false;
            }
        }, "jl-update-check").start();
    }

    private static void ask(Activity act, String version, int code, String url, long size, String notes, Consumer<String> log) {
        String msg = I18n.t("Une nouvelle version est disponible : ", "A new version is available: ") + version
                + " (" + I18n.t("installée : ", "installed: ") + installedName(act) + ", " + (size / 1_000_000) + " Mo)";
        if (!notes.trim().isEmpty()) msg += "\n\n" + (notes.length() > 600 ? notes.substring(0, 600) + "…" : notes);
        new AlertDialog.Builder(act)
                .setTitle(I18n.t("Mise à jour", "Update"))
                .setMessage(msg)
                .setPositiveButton(I18n.t("Mettre à jour", "Update"), (d, w) -> download(act, version, url, log))
                .setNegativeButton(I18n.t("Plus tard", "Later"), null)
                .setNeutralButton(I18n.t("Ignorer cette version", "Skip this version"), (d, w) -> UiPrefs.putInt(act, "update_skip", code))
                .show();
    }

    private static void download(Activity act, String version, String url, Consumer<String> log) {
        if (Build.VERSION.SDK_INT >= 26 && !act.getPackageManager().canRequestPackageInstalls()) {
            log.accept(I18n.t("Autorise Jladios à installer des applis (\"Installer des applis inconnues\"), puis relance /update.",
                    "Allow Jladios to install apps (\"Install unknown apps\"), then run /update again."));
            act.startActivity(new Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, Uri.parse("package:" + act.getPackageName())));
            return;
        }
        if (busy) return;
        busy = true;
        log.accept(I18n.t("Téléchargement de ", "Downloading ") + version + "...");
        new Thread(() -> {
            try {
                File dir = new File(act.getCacheDir(), "update");
                dir.mkdirs();
                File[] old = dir.listFiles();
                if (old != null) for (File f : old) f.delete();
                File apk = new File(dir, "jladios.apk");
                HttpURLConnection c = (HttpURLConnection) new URL(url).openConnection(); // suit les redirections GitHub
                c.setConnectTimeout(15000);
                c.setReadTimeout(30000);
                long total = c.getContentLengthLong();
                long done = 0;
                int lastPct = -1;
                try (InputStream in = c.getInputStream(); FileOutputStream out = new FileOutputStream(apk)) {
                    byte[] buf = new byte[65536];
                    int n;
                    while ((n = in.read(buf)) > 0) {
                        out.write(buf, 0, n);
                        done += n;
                        int pct = total > 0 ? (int) (done * 100 / total) : -1;
                        if (pct >= 0 && pct / 20 != lastPct / 20) {
                            lastPct = pct;
                            log.accept(pct + " %");
                        }
                    }
                }
                if (apk.length() < 1_000_000 || (total > 0 && apk.length() != total)) throw new Exception("téléchargement incomplet");
                install(act, apk, log);
            } catch (Exception e) {
                log.accept(I18n.t("Mise à jour échouée : ", "Update failed: ") + e.getMessage());
            } finally {
                busy = false;
            }
        }, "jl-update-download").start();
    }

    private static void install(Context ctx, File apk, Consumer<String> log) throws Exception {
        PackageInstaller pi = ctx.getPackageManager().getPackageInstaller();
        PackageInstaller.SessionParams params = new PackageInstaller.SessionParams(PackageInstaller.SessionParams.MODE_FULL_INSTALL);
        params.setAppPackageName(ctx.getPackageName());
        int id = pi.createSession(params);
        try (PackageInstaller.Session s = pi.openSession(id)) {
            try (OutputStream out = s.openWrite("jladios.apk", 0, apk.length()); InputStream in = new FileInputStream(apk)) {
                byte[] buf = new byte[65536];
                int n;
                while ((n = in.read(buf)) > 0) out.write(buf, 0, n);
                s.fsync(out);
            }
            Intent i = new Intent(ctx, InstallReceiver.class).setAction("com.jladios.assistant.INSTALL_RESULT");
            int flags = PendingIntent.FLAG_UPDATE_CURRENT | (Build.VERSION.SDK_INT >= 31 ? PendingIntent.FLAG_MUTABLE : 0);
            PendingIntent pend = PendingIntent.getBroadcast(ctx, id, i, flags);
            log.accept(I18n.t("Confirme l'installation dans la fenêtre Android.", "Confirm the installation in the Android window."));
            s.commit(pend.getIntentSender());
        }
    }

    private static byte[] readAll(InputStream in) throws Exception {
        try (InputStream i = in) {
            java.io.ByteArrayOutputStream b = new java.io.ByteArrayOutputStream();
            byte[] buf = new byte[8192];
            int n;
            while ((n = i.read(buf)) > 0) b.write(buf, 0, n);
            return b.toByteArray();
        }
    }
}
