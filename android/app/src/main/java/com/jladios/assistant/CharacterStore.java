package com.jladios.assistant;

import android.content.Context;
import android.content.SharedPreferences;
import android.graphics.Bitmap;
import android.graphics.BitmapFactory;
import android.net.Uri;

import org.json.JSONObject;

import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.security.SecureRandom;
import java.text.Normalizer;
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import java.util.zip.ZipEntry;
import java.util.zip.ZipInputStream;
import java.util.zip.ZipOutputStream;

/**
 * Personnages personnalisés : un nom + deux sprites (inactif / actif).
 * Stockés dans files/characters/&lt;id&gt;/{idle.png, active.png, meta.json}.
 * Format d'échange : fichier .jladios = zip {manifest.json, idle.png, active.png}.
 */
final class CharacterStore {
    static final String FORMAT = "jladios-character";
    static final String MIME = "application/x-jladios";
    private static final int MAX_SIDE = 512;
    private static final int MAX_ENTRY_BYTES = 6 * 1024 * 1024;

    static final class Info {
        final String id, name;
        Info(String id, String name) {
            this.id = id;
            this.name = name;
        }
    }

    private CharacterStore() {}

    private static File root(Context c) {
        File d = new File(c.getFilesDir(), "characters");
        d.mkdirs();
        return d;
    }

    static File dir(Context c, String id) {
        return new File(root(c), id);
    }

    private static SharedPreferences prefs(Context c) {
        return c.getSharedPreferences("jladios", Context.MODE_PRIVATE);
    }

    /** "" = personnage par défaut dessiné par Rust. */
    static String selected(Context c) {
        String id = prefs(c).getString("character", "");
        return (id.isEmpty() || new File(dir(c, id), "idle.png").isFile()) ? id : "";
    }

    static void select(Context c, String id) {
        prefs(c).edit().putString("character", id).apply();
    }

    static List<Info> list(Context c) {
        List<Info> out = new ArrayList<>();
        File[] dirs = root(c).listFiles();
        if (dirs == null) return out;
        for (File d : dirs) {
            if (!new File(d, "idle.png").isFile() || !new File(d, "active.png").isFile()) continue;
            out.add(new Info(d.getName(), nameOf(d)));
        }
        out.sort((a, b) -> a.name.compareToIgnoreCase(b.name));
        return out;
    }

    private static String nameOf(File d) {
        try {
            return new JSONObject(new String(readAll(new java.io.FileInputStream(new File(d, "meta.json"))), "UTF-8")).optString("name", d.getName());
        } catch (Exception e) {
            return d.getName();
        }
    }

    static String cleanName(String n) {
        n = n == null ? "" : n.replaceAll("[\\p{Cntrl}]", " ").trim();
        if (n.length() > 40) n = n.substring(0, 40).trim();
        return n.isEmpty() ? "Sans nom" : n;
    }

    private static String newId(String name) {
        String slug = Normalizer.normalize(name, Normalizer.Form.NFD).replaceAll("\\p{M}+", "")
                .toLowerCase(Locale.ROOT).replaceAll("[^a-z0-9]+", "-").replaceAll("^-|-$", "");
        if (slug.isEmpty()) slug = "perso";
        if (slug.length() > 20) slug = slug.substring(0, 20);
        return slug + "-" + Long.toString(new SecureRandom().nextInt(0xFFFFFF), 36);
    }

    private static byte[] readAll(InputStream in) throws IOException {
        try (InputStream i = in) {
            ByteArrayOutputStream b = new ByteArrayOutputStream();
            byte[] buf = new byte[16384];
            int n, total = 0;
            while ((n = i.read(buf)) > 0) {
                total += n;
                if (total > MAX_ENTRY_BYTES) throw new IOException("fichier trop gros");
                b.write(buf, 0, n);
            }
            return b.toByteArray();
        }
    }

    /** Décode une image, la réduit (max 512 px) et la ré-encode en PNG (transparence conservée). */
    static byte[] normalizeImage(byte[] raw) throws IOException {
        BitmapFactory.Options o = new BitmapFactory.Options();
        o.inJustDecodeBounds = true;
        BitmapFactory.decodeByteArray(raw, 0, raw.length, o);
        if (o.outWidth <= 0 || o.outHeight <= 0 || o.outWidth > 8192 || o.outHeight > 8192)
            throw new IOException("image invalide");
        int sample = 1;
        while (Math.max(o.outWidth, o.outHeight) / sample > MAX_SIDE * 2) sample *= 2;
        BitmapFactory.Options d = new BitmapFactory.Options();
        d.inSampleSize = sample;
        Bitmap b = BitmapFactory.decodeByteArray(raw, 0, raw.length, d);
        if (b == null) throw new IOException("image illisible");
        int side = Math.max(b.getWidth(), b.getHeight());
        if (side > MAX_SIDE) {
            float k = MAX_SIDE / (float) side;
            b = Bitmap.createScaledBitmap(b, Math.max(1, Math.round(b.getWidth() * k)), Math.max(1, Math.round(b.getHeight() * k)), true);
        }
        ByteArrayOutputStream out = new ByteArrayOutputStream();
        b.compress(Bitmap.CompressFormat.PNG, 100, out);
        return out.toByteArray();
    }

    private static String save(Context c, String name, byte[] idlePng, byte[] activePng) throws IOException {
        name = cleanName(name);
        String id = newId(name);
        File d = dir(c, id);
        if (!d.mkdirs()) throw new IOException("création impossible");
        try {
            write(new File(d, "idle.png"), idlePng);
            write(new File(d, "active.png"), activePng);
            write(new File(d, "meta.json"), new JSONObject().put("name", name).toString().getBytes("UTF-8"));
        } catch (Exception e) {
            delete(c, id);
            throw new IOException(e.getMessage());
        }
        return id;
    }

    private static void write(File f, byte[] data) throws IOException {
        try (FileOutputStream o = new FileOutputStream(f)) {
            o.write(data);
        }
    }

    /** Crée un personnage depuis deux images choisies par l'utilisateur. */
    static String create(Context c, String name, Uri idle, Uri active) throws IOException {
        byte[] a = normalizeImage(readAll(c.getContentResolver().openInputStream(idle)));
        byte[] b = normalizeImage(readAll(c.getContentResolver().openInputStream(active)));
        return save(c, name, a, b);
    }

    static void delete(Context c, String id) {
        File d = dir(c, id);
        File[] fs = d.listFiles();
        if (fs != null) for (File f : fs) f.delete();
        d.delete();
        if (id.equals(prefs(c).getString("character", ""))) select(c, "");
    }

    /** Écrit un fichier .jladios (zip). */
    static void export(Context c, String id, OutputStream out) throws IOException {
        File d = dir(c, id);
        try (ZipOutputStream z = new ZipOutputStream(out)) {
            String manifest;
            try {
                manifest = new JSONObject().put("format", FORMAT).put("version", 1).put("name", nameOf(d)).toString();
            } catch (Exception e) {
                throw new IOException(e.getMessage());
            }
            put(z, "manifest.json", manifest.getBytes("UTF-8"));
            put(z, "idle.png", readAll(new java.io.FileInputStream(new File(d, "idle.png"))));
            put(z, "active.png", readAll(new java.io.FileInputStream(new File(d, "active.png"))));
        }
    }

    private static void put(ZipOutputStream z, String name, byte[] data) throws IOException {
        z.putNextEntry(new ZipEntry(name));
        z.write(data);
        z.closeEntry();
    }

    /** Importe un fichier .jladios ; renvoie l'id du personnage créé. */
    static String importJladios(Context c, InputStream in) throws IOException {
        byte[] manifest = null, idle = null, active = null;
        try (ZipInputStream z = new ZipInputStream(in)) {
            ZipEntry e;
            int entries = 0;
            while ((e = z.getNextEntry()) != null) {
                if (++entries > 16) throw new IOException("archive invalide");
                String n = e.getName();
                if (e.isDirectory()) continue;
                // seuls ces trois noms exacts sont lus : aucun chemin de l'archive n'est utilisé
                byte[] data = readAll(new java.io.ByteArrayInputStream(readAll(new NoClose(z))));
                if (n.equals("manifest.json")) manifest = data;
                else if (n.equals("idle.png")) idle = data;
                else if (n.equals("active.png")) active = data;
            }
        }
        if (manifest == null || idle == null || active == null)
            throw new IOException("ce n'est pas un personnage .jladios valide (manifest, idle ou active manquant)");
        String name;
        try {
            JSONObject m = new JSONObject(new String(manifest, "UTF-8"));
            if (!FORMAT.equals(m.optString("format"))) throw new IOException("format inconnu");
            if (m.optInt("version", 1) > 1) throw new IOException("version plus récente, mets l'app à jour");
            name = m.optString("name", "Sans nom");
        } catch (org.json.JSONException ex) {
            throw new IOException("manifest illisible");
        }
        return save(c, name, normalizeImage(idle), normalizeImage(active));
    }

    /** Empêche readAll de fermer le ZipInputStream. */
    private static final class NoClose extends java.io.FilterInputStream {
        NoClose(InputStream in) {
            super(in);
        }

        @Override
        public void close() {}
    }

    static Bitmap bitmap(Context c, String id, String which) {
        return BitmapFactory.decodeFile(new File(dir(c, id), which + ".png").getAbsolutePath());
    }
}
