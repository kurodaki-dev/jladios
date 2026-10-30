package com.jladios.assistant;

import android.content.Context;

/** Fonctions natives implémentées en Rust (rust/src/lib.rs). */
public final class Native {
    static {
        System.loadLibrary("jladios");
    }

    private Native() {}

    public static native void init(String filesDir, String cacheDir);
    public static native void onInput(String line);
    /** Prochain événement pour le terminal ("o:texte" ou "c:commande"), ou null. */
    public static native String pollEvent();
    public static native void pushAudio(float[] buf, int n);
    public static native int pullAudio(float[] buf);
    public static native void renderCharacter(int[] pixels, int w, int h, boolean mouthOpen);
    /** Bits : 1 bouche ouverte, 2 écoute, 4 réflexion, 8 parle, 16 chargement. */
    public static native int overlayState();
    public static native String subtitle();
    public static native String assistantName();
    public static native void onTap();
    public static native void setOverlayVisible(boolean visible);
    public static native void micUnavailable(String reason);
    public static native int playbackRate();
    public static native int micRate();
    /** Prochaine action de contrôle du téléphone ("id\taction"), ou null. */
    public static native String pollAction();
    public static native void actionResult(long id, String text);
    public static native void setAgentConnected(boolean connected);

    public static final int ST_MOUTH = 1, ST_LISTEN = 2, ST_THINK = 4, ST_SPEAK = 8, ST_LOADING = 16;

    private static boolean inited;

    public static synchronized void ensureInit(Context ctx) {
        if (inited) return;
        Context app = ctx.getApplicationContext();
        init(app.getFilesDir().getAbsolutePath(), app.getCacheDir().getAbsolutePath());
        inited = true;
        AudioIO.start(app);
    }
}
