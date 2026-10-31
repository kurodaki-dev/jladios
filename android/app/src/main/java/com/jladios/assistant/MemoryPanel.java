package com.jladios.assistant;

import android.app.Activity;
import android.app.AlertDialog;
import android.content.ContentValues;
import android.content.Intent;
import android.graphics.Color;
import android.graphics.Typeface;
import android.net.Uri;
import android.os.Build;
import android.provider.MediaStore;
import android.util.TypedValue;
import android.view.View;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.TextView;
import android.widget.Toast;

import org.json.JSONArray;
import org.json.JSONObject;

import java.io.File;
import java.io.FileInputStream;
import java.io.InputStream;
import java.io.OutputStream;

/**
 * Onglet « memory » : état de la mémoire (/memory on), souvenirs, export, partage
 * (Bluetooth, ou tout autre canal du menu Partager) et import depuis un autre téléphone.
 */
final class MemoryPanel extends ScrollView {
    static final int REQ_IMPORT = 301;
    private static final int GRAY = 0xFF9E9E9E;
    static final String MIME = "application/x-jladios-memory";

    private final Activity act;
    private final LinearLayout list;
    private boolean shareAfterExport;

    MemoryPanel(Activity a) {
        super(a);
        act = a;
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

    private TextView text(String s, int color, float size, boolean bold) {
        TextView t = new TextView(act);
        t.setText(s);
        t.setTextColor(color);
        t.setTextSize(size);
        t.setTypeface(Typeface.MONOSPACE, bold ? Typeface.BOLD : Typeface.NORMAL);
        return t;
    }

    private TextView button(String label, View.OnClickListener l) {
        TextView t = text("[ " + label + " ]", UiPrefs.accent(act), 13, true);
        t.setPadding(dp(2), dp(8), dp(10), dp(8));
        t.setOnClickListener(l);
        return t;
    }

    /** La config est écrite par Rust dans le même dossier : on la relit pour afficher l'état. */
    private JSONObject config() {
        try (InputStream in = new FileInputStream(new File(act.getFilesDir(), "config.json"))) {
            java.io.ByteArrayOutputStream b = new java.io.ByteArrayOutputStream();
            byte[] buf = new byte[8192];
            int n;
            while ((n = in.read(buf)) > 0) b.write(buf, 0, n);
            return new JSONObject(b.toString("UTF-8"));
        } catch (Exception e) {
            return new JSONObject();
        }
    }

    /** Envoie une commande au terminal puis réaffiche l'onglet. */
    private void run(String cmd) {
        Native.onInput(cmd);
        postDelayed(this::refresh, 700);
    }

    void refresh() {
        list.removeAllViews();
        JSONObject c = config();
        boolean on = c.optBoolean("memory", false);
        JSONArray facts = c.optJSONArray("facts");
        int n = facts == null ? 0 : facts.length();

        list.addView(text(I18n.t("Mémoire", "Memory"), Color.WHITE, 16, true));
        TextView state = text(on ? I18n.t("● activée", "● on") : I18n.t("○ désactivée", "○ off"), on ? UiPrefs.accent(act) : GRAY, 14, true);
        state.setPadding(0, dp(4), 0, 0);
        list.addView(state);
        if (!on) {
            list.addView(text(I18n.t("Pour avoir de la mémoire, tape /memory on (ou appuie sur le bouton ci-dessous).",
                    "To have memory, type /memory on (or tap the button below)."), GRAY, 12, false));
        }
        LinearLayout toggle = new LinearLayout(act);
        toggle.addView(button(on ? I18n.t("désactiver", "turn off") : I18n.t("activer", "turn on"), v -> run(on ? "/memory off" : "/memory on")));
        list.addView(toggle);

        TextView h = text(I18n.t("Souvenirs", "Memories") + " (" + n + ")", Color.WHITE, 14, true);
        h.setPadding(0, dp(14), 0, dp(2));
        list.addView(h);
        if (n == 0) {
            list.addView(text(I18n.t("Aucun souvenir pour l'instant.", "No memories yet."), GRAY, 12, false));
        }
        for (int i = 0; i < n && i < 60; i++) {
            TextView t = text((i + 1) + ". " + facts.optString(i), 0xFFCCCCCC, 12, false);
            t.setPadding(0, dp(2), 0, dp(2));
            list.addView(t);
        }

        TextView h2 = text(I18n.t("Transférer vers / depuis un autre téléphone", "Transfer to / from another phone"), Color.WHITE, 14, true);
        h2.setPadding(0, dp(16), 0, dp(2));
        list.addView(h2);
        list.addView(text(I18n.t(
                "Bluetooth : \"partager\" puis choisis Bluetooth ; sur l'autre téléphone, ouvre le fichier reçu avec Jladios ou utilise \"importer\".\n"
                        + "Câble USB : branche les deux téléphones en \"transfert de fichiers\", copie le fichier .jladiosmem du dossier Download de l'un vers l'autre, puis \"importer\".",
                "Bluetooth: tap \"share\" and pick Bluetooth; on the other phone, open the received file with Jladios or use \"import\".\n"
                        + "USB cable: connect both phones in \"file transfer\" mode, copy the .jladiosmem file from one Download folder to the other, then \"import\"."),
                GRAY, 12, false));
        LinearLayout row = new LinearLayout(act);
        row.addView(button(I18n.t("exporter", "export"), v -> export(false)));
        row.addView(button(I18n.t("partager", "share"), v -> export(true)));
        row.addView(button(I18n.t("importer", "import"), v -> {
            Intent i = new Intent(Intent.ACTION_OPEN_DOCUMENT).addCategory(Intent.CATEGORY_OPENABLE).setType("*/*");
            act.startActivityForResult(i, REQ_IMPORT);
        }));
        list.addView(row);

        LinearLayout row2 = new LinearLayout(act);
        row2.setPadding(0, dp(12), 0, 0);
        row2.addView(button(I18n.t("tout effacer", "erase all"), v -> new AlertDialog.Builder(act)
                .setMessage(I18n.t("Effacer tous les souvenirs et l'historique ?", "Erase all memories and history?"))
                .setPositiveButton(I18n.t("Effacer", "Erase"), (d, w) -> run("/memory clear"))
                .setNegativeButton(I18n.t("Annuler", "Cancel"), null).show()));
        list.addView(row2);
    }

    // ---------- export / partage ----------

    private void export(boolean share) {
        shareAfterExport = share;
        Native.onInput("/memory export"); // Rust écrit le fichier puis renvoie la commande mem_exported:<chemin>
    }

    /** Appelé quand Rust a écrit le fichier : on le copie dans Download et on ouvre le partage si demandé. */
    void onExported(String path) {
        if (Build.VERSION.SDK_INT < 29) {
            Toast.makeText(act, I18n.t("Export disponible dès Android 10", "Export needs Android 10+"), Toast.LENGTH_LONG).show();
            return;
        }
        try {
            String name = "jladios-memory-" + new java.text.SimpleDateFormat("yyyyMMdd-HHmm", java.util.Locale.US).format(new java.util.Date()) + ".jladiosmem";
            ContentValues v = new ContentValues();
            v.put(MediaStore.MediaColumns.DISPLAY_NAME, name);
            v.put(MediaStore.MediaColumns.MIME_TYPE, MIME);
            v.put(MediaStore.MediaColumns.RELATIVE_PATH, "Download");
            Uri uri = act.getContentResolver().insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI, v);
            if (uri == null) throw new java.io.IOException("insertion refusée");
            try (OutputStream out = act.getContentResolver().openOutputStream(uri); InputStream in = new FileInputStream(path)) {
                byte[] buf = new byte[16384];
                int n;
                while ((n = in.read(buf)) > 0) out.write(buf, 0, n);
            }
            Toast.makeText(act, I18n.t("Enregistré : Download/", "Saved: Download/") + name, Toast.LENGTH_LONG).show();
            if (shareAfterExport) {
                Intent s = new Intent(Intent.ACTION_SEND).setType(MIME)
                        .putExtra(Intent.EXTRA_STREAM, uri)
                        .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION);
                act.startActivity(Intent.createChooser(s, I18n.t("Envoyer la mémoire (Bluetooth...)", "Send the memory (Bluetooth...)")));
            }
        } catch (Exception e) {
            Toast.makeText(act, I18n.t("Export impossible : ", "Export failed: ") + e.getMessage(), Toast.LENGTH_LONG).show();
        }
        shareAfterExport = false;
    }

    // ---------- import ----------

    /** Copie le fichier reçu dans le cache puis demande à Rust de l'importer. */
    void importUri(Uri u) {
        try (InputStream in = act.getContentResolver().openInputStream(u)) {
            File dir = new File(act.getCacheDir(), "import");
            dir.mkdirs();
            File f = new File(dir, "memory.jladiosmem");
            try (OutputStream out = new java.io.FileOutputStream(f)) {
                byte[] buf = new byte[16384];
                int n, total = 0;
                while ((n = in.read(buf)) > 0) {
                    total += n;
                    if (total > 2_000_000) throw new java.io.IOException("fichier trop gros");
                    out.write(buf, 0, n);
                }
            }
            run("/memory import " + f.getAbsolutePath());
        } catch (Exception e) {
            Toast.makeText(act, I18n.t("Import impossible : ", "Import failed: ") + e.getMessage(), Toast.LENGTH_LONG).show();
        }
    }

    String onResult(int req, int result, Intent data) {
        if (req == REQ_IMPORT && result == Activity.RESULT_OK && data != null && data.getData() != null) {
            importUri(data.getData());
        }
        return null;
    }
}
