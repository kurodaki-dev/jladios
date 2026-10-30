package com.jladios.assistant;

import android.content.Context;
import android.graphics.Bitmap;
import android.graphics.Canvas;
import android.view.View;

/** Affiche le personnage dont les pixels sont dessinés par Rust. */
final class CharacterView extends View {
    private Bitmap closed, open;
    private boolean mouthOpen;

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

    void setMouthOpen(boolean m) {
        if (m != mouthOpen) {
            mouthOpen = m;
            invalidate();
        }
    }

    @Override
    protected void onDraw(Canvas c) {
        Bitmap b = mouthOpen ? open : closed;
        if (b != null) c.drawBitmap(b, 0, 0, null);
    }
}
