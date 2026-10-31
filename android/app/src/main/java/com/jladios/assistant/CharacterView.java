package com.jladios.assistant;

import android.content.Context;
import android.graphics.Bitmap;
import android.graphics.Canvas;
import android.graphics.Paint;
import android.graphics.Rect;
import android.view.View;

/** Affiche le personnage dont les pixels sont dessinés par Rust. */
final class CharacterView extends View {
    private Bitmap closed, open;
    private boolean mouthOpen;
    /** Personnage importé : sprite inactif / actif (null = personnage par défaut de Rust). */
    private Bitmap idleSprite, activeSprite;
    private boolean active;
    private String customId = "";
    private final Paint paint = new Paint(Paint.FILTER_BITMAP_FLAG | Paint.ANTI_ALIAS_FLAG);
    private final Rect dst = new Rect();

    CharacterView(Context ctx) {
        super(ctx);
    }

    private static Bitmap render(int w, int h, boolean mouth) {
        int[] px = new int[w * h];
        Native.renderCharacter(px, w, h, mouth);
        return Bitmap.createBitmap(px, w, h, Bitmap.Config.ARGB_8888);
    }

    @Override
    protected void onSizeChanged(int w, int h, int oldw, int oldh) {
        if (w > 0 && h > 0) {
            closed = render(w, h, false);
            open = render(w, h, true);
        }
    }

    /** Choisit le personnage ("" = défaut). Ne recharge que si l'identifiant change. */
    void setCharacter(Context ctx, String id) {
        if (id.equals(customId)) return;
        customId = id;
        if (id.isEmpty()) {
            idleSprite = activeSprite = null;
        } else {
            idleSprite = CharacterStore.bitmap(ctx, id, "idle");
            activeSprite = CharacterStore.bitmap(ctx, id, "active");
            if (idleSprite == null || activeSprite == null) idleSprite = activeSprite = null;
        }
        invalidate();
    }

    void setActive(boolean a) {
        if (a != active) {
            active = a;
            if (idleSprite != null) invalidate();
        }
    }

    void setMouthOpen(boolean m) {
        if (m != mouthOpen) {
            mouthOpen = m;
            invalidate();
        }
    }

    @Override
    protected void onDraw(Canvas c) {
        if (idleSprite != null) {
            Bitmap s = active ? activeSprite : idleSprite;
            float k = Math.min(getWidth() / (float) s.getWidth(), getHeight() / (float) s.getHeight());
            int w = Math.round(s.getWidth() * k), h = Math.round(s.getHeight() * k);
            int x = (getWidth() - w) / 2, y = getHeight() - h;
            dst.set(x, y, x + w, y + h);
            c.drawBitmap(s, null, dst, paint);
            return;
        }
        Bitmap b = mouthOpen ? open : closed;
        if (b != null) c.drawBitmap(b, 0, 0, null);
    }
}
