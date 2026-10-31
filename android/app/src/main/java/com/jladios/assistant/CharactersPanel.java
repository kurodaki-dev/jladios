package com.jladios.assistant;

import android.app.Activity;
import android.app.AlertDialog;
import android.content.ContentValues;
import android.content.Intent;
import android.graphics.Bitmap;
import android.graphics.Color;
import android.graphics.Typeface;
import android.graphics.drawable.GradientDrawable;
import android.net.Uri;
import android.os.Build;
import android.provider.MediaStore;
import android.util.TypedValue;
import android.view.Gravity;
import android.view.View;
import android.widget.EditText;
import android.widget.ImageView;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.TextView;
import android.widget.Toast;

import java.io.InputStream;
import java.io.OutputStream;
import java.util.List;

/** Onglet « characters » : choisir, créer (2 sprites obligatoires), importer, exporter, partager. */
final class CharactersPanel extends ScrollView {
    static final int REQ_IDLE = 201, REQ_ACTIVE = 202, REQ_IMPORT = 203;
    private static final int GRAY = 0xFF9E9E9E;

    private final Activity act;
    private final LinearLayout list;
    private String pendingName;
    private Uri pendingIdle;

    CharactersPanel(Activity a) {
        super(a);
        act = a;
        setFillViewport(true);
        list = new LinearLayout(a);
        list.setOrientation(LinearLayout.VERTICAL);
        list.setPadding(dp(10), dp(8), dp(10), dp(16));
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
        t.setPadding(dp(4), dp(8), dp(8), dp(8));
        t.setOnClickListener(l);
        return t;
    }

    void refresh() {
        list.removeAllViews();
        String sel = CharacterStore.selected(act);
        list.addView(text(I18n.t("Personnages", "Characters"), Color.WHITE, 16, true));
        list.addView(text(I18n.t(
                "Un personnage = 2 images : une au repos (inactif), une quand il parle ou écoute (actif).",
                "A character = 2 images: one at rest (idle), one when it talks or listens (active)."), GRAY, 12, false));

        TextView hint = text(I18n.t(
                "Taille recommandée de chaque image : 312 × 512 px, PNG avec fond transparent (maximum 512 px de côté, 6 Mo). Les deux images doivent avoir la même taille et le même cadrage.",
                "Recommended size for each image: 312 × 512 px, PNG with a transparent background (max 512 px per side, 6 MB). Both images should have the same size and framing."),
                UiPrefs.accent(act), 12, true);
        hint.setPadding(0, dp(8), 0, 0);
        list.addView(hint);

        row(null, I18n.t("Pixel (par défaut)", "Pixel (default)"), sel.isEmpty(), "");
        List<CharacterStore.Info> all = CharacterStore.list(act);
        for (CharacterStore.Info c : all) row(c, c.name, c.id.equals(sel), c.id);

        LinearLayout actions = new LinearLayout(act);
        actions.setPadding(0, dp(12), 0, 0);
        actions.addView(button(I18n.t("+ nouveau", "+ new"), v -> askName()));
        actions.addView(button(I18n.t("importer .jladios", "import .jladios"), v -> {
            Intent i = new Intent(Intent.ACTION_OPEN_DOCUMENT).addCategory(Intent.CATEGORY_OPENABLE).setType("*/*");
            act.startActivityForResult(i, REQ_IMPORT);
        }));
        list.addView(actions);
    }

    private void row(CharacterStore.Info c, String name, boolean selected, String id) {
        LinearLayout box = new LinearLayout(act);
        box.setOrientation(LinearLayout.VERTICAL);
        box.setPadding(dp(8), dp(8), dp(8), dp(4));
        GradientDrawable bg = new GradientDrawable();
        bg.setColor(0xFF111111);
        bg.setStroke(dp(1), selected ? UiPrefs.accent(act) : 0xFF333333);
        bg.setCornerRadius(dp(6));
        box.setBackground(bg);
        LinearLayout.LayoutParams lp = new LinearLayout.LayoutParams(-1, -2);
        lp.topMargin = dp(10);

        LinearLayout top = new LinearLayout(act);
        top.setGravity(Gravity.CENTER_VERTICAL);
        if (c != null) {
            top.addView(preview(CharacterStore.bitmap(act, id, "idle")));
            top.addView(preview(CharacterStore.bitmap(act, id, "active")));
        }
        TextView n = text((selected ? "● " : "") + name, selected ? UiPrefs.accent(act) : Color.WHITE, 14, true);
        n.setPadding(dp(8), 0, 0, 0);
        top.addView(n);
        box.addView(top);

        LinearLayout btns = new LinearLayout(act);
        if (!selected) btns.addView(button(I18n.t("utiliser", "use"), v -> {
            CharacterStore.select(act, id);
            refresh();
        }));
        if (c != null) {
            btns.addView(button(I18n.t("exporter", "export"), v -> exportToDownloads(id, name, false)));
            btns.addView(button(I18n.t("partager", "share"), v -> exportToDownloads(id, name, true)));
            btns.addView(button(I18n.t("supprimer", "delete"), v -> new AlertDialog.Builder(act)
                    .setMessage(I18n.t("Supprimer « " + name + " » ?", "Delete \"" + name + "\"?"))
                    .setPositiveButton(I18n.t("Supprimer", "Delete"), (d, w) -> {
                        CharacterStore.delete(act, id);
                        refresh();
                    })
                    .setNegativeButton(I18n.t("Annuler", "Cancel"), null).show()));
        }
        box.addView(btns);
        list.addView(box, lp);
    }

    private ImageView preview(Bitmap b) {
        ImageView v = new ImageView(act);
        v.setImageBitmap(b);
        v.setScaleType(ImageView.ScaleType.FIT_CENTER);
        v.setLayoutParams(new LinearLayout.LayoutParams(dp(48), dp(64)));
        return v;
    }

    // ---------- création : nom, puis 2 sprites obligatoires ----------

    private void askName() {
        EditText e = new EditText(act);
        e.setSingleLine(true);
        e.setHint(I18n.t("Nom du personnage", "Character name"));
        new AlertDialog.Builder(act)
                .setTitle(I18n.t("Nouveau personnage", "New character"))
                .setMessage(I18n.t("2 images obligatoires : repos puis actif.\nTaille conseillée : 312 × 512 px, PNG transparent.", "2 images required: idle then active.\nRecommended size: 312 × 512 px, transparent PNG."))
                .setView(e)
                .setPositiveButton(I18n.t("Suivant", "Next"), (d, w) -> {
                    pendingName = CharacterStore.cleanName(e.getText().toString());
                    pendingIdle = null;
                    Toast.makeText(act, I18n.t("1/2 : image au repos (inactif)", "1/2: resting image (idle)"), Toast.LENGTH_LONG).show();
                    pick(REQ_IDLE);
                })
                .setNegativeButton(I18n.t("Annuler", "Cancel"), null).show();
    }

    private void pick(int req) {
        Intent i = new Intent(Intent.ACTION_OPEN_DOCUMENT).addCategory(Intent.CATEGORY_OPENABLE).setType("image/*");
        act.startActivityForResult(i, req);
    }

    /** Résultat d'un sélecteur de fichier ; renvoie un message à afficher dans le terminal (ou null). */
    String onResult(int req, int result, Intent data) {
        if (result != Activity.RESULT_OK || data == null || data.getData() == null) {
            if (req == REQ_ACTIVE || req == REQ_IDLE) pendingName = null;
            return null;
        }
        Uri u = data.getData();
        switch (req) {
            case REQ_IDLE:
                pendingIdle = u;
                Toast.makeText(act, I18n.t("2/2 : image active (quand il parle)", "2/2: active image (when it talks)"), Toast.LENGTH_LONG).show();
                pick(REQ_ACTIVE);
                return null;
            case REQ_ACTIVE:
                if (pendingIdle == null || pendingName == null) return null;
                try {
                    String id = CharacterStore.create(act, pendingName, pendingIdle, u);
                    CharacterStore.select(act, id);
                    refresh();
                    return I18n.t("personnage « " + pendingName + " » créé et sélectionné", "character \"" + pendingName + "\" created and selected");
                } catch (Exception e) {
                    return I18n.t("échec de la création : ", "creation failed: ") + e.getMessage();
                } finally {
                    pendingName = null;
                    pendingIdle = null;
                }
            case REQ_IMPORT:
                return importUri(u);
            default:
                return null;
        }
    }

    /** Importe un .jladios (depuis le sélecteur ou en l'ouvrant depuis une autre appli). */
    String importUri(Uri u) {
        try (InputStream in = act.getContentResolver().openInputStream(u)) {
            String id = CharacterStore.importJladios(act, in);
            CharacterStore.select(act, id);
            refresh();
            return I18n.t("personnage importé et sélectionné : ", "character imported and selected: ") + nameOf(id);
        } catch (Exception e) {
            return I18n.t("import impossible : ", "import failed: ") + e.getMessage();
        }
    }

    private String nameOf(String id) {
        for (CharacterStore.Info c : CharacterStore.list(act)) if (c.id.equals(id)) return c.name;
        return id;
    }

    // ---------- export / partage ----------

    private void exportToDownloads(String id, String name, boolean share) {
        if (Build.VERSION.SDK_INT < 29) {
            Toast.makeText(act, I18n.t("Export disponible dès Android 10", "Export needs Android 10+"), Toast.LENGTH_LONG).show();
            return;
        }
        try {
            String file = name.replaceAll("[^\\p{L}\\p{N}._ -]", "_").trim() + ".jladios";
            ContentValues v = new ContentValues();
            v.put(MediaStore.MediaColumns.DISPLAY_NAME, file);
            v.put(MediaStore.MediaColumns.MIME_TYPE, CharacterStore.MIME);
            v.put(MediaStore.MediaColumns.RELATIVE_PATH, "Download");
            Uri uri = act.getContentResolver().insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI, v);
            if (uri == null) throw new java.io.IOException("insertion refusée");
            try (OutputStream out = act.getContentResolver().openOutputStream(uri)) {
                CharacterStore.export(act, id, out);
            }
            Toast.makeText(act, I18n.t("Enregistré : Download/", "Saved: Download/") + file, Toast.LENGTH_LONG).show();
            if (share) {
                Intent s = new Intent(Intent.ACTION_SEND).setType(CharacterStore.MIME)
                        .putExtra(Intent.EXTRA_STREAM, uri)
                        .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION);
                act.startActivity(Intent.createChooser(s, I18n.t("Partager le personnage", "Share the character")));
            }
        } catch (Exception e) {
            Toast.makeText(act, I18n.t("Export impossible : ", "Export failed: ") + e.getMessage(), Toast.LENGTH_LONG).show();
        }
    }
}
