package com.jladios.assistant;

import android.Manifest;
import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.app.Service;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.content.pm.ServiceInfo;
import android.graphics.PixelFormat;
import android.graphics.Typeface;
import android.graphics.drawable.GradientDrawable;
import android.os.Build;
import android.os.Handler;
import android.os.IBinder;
import android.os.Looper;
import android.util.TypedValue;
import android.view.Gravity;
import android.view.MotionEvent;
import android.view.View;
import android.view.ViewConfiguration;
import android.view.WindowManager;
import android.widget.LinearLayout;
import android.widget.TextView;

/** Service au premier plan qui affiche le personnage par-dessus les autres applis. */
public class OverlayService extends Service {
    private static final String CHANNEL = "jladios_overlay";
    private static final String ACTION_STOP = "com.jladios.assistant.STOP";

    private WindowManager wm;
    private LinearLayout root;
    private CharacterView character;
    private TextView bubble;
    private WindowManager.LayoutParams lp;
    private final Handler handler = new Handler(Looper.getMainLooper());
    private String lastSubtitle = "";
    private int lastState = -1;
    private long lastChange, lastActive, lastCharCheck;

    private final Runnable tick = new Runnable() {
        @Override
        public void run() {
            int st = Native.overlayState();
            long t0 = System.currentTimeMillis();
            if (t0 - lastCharCheck > 500) {
                lastCharCheck = t0;
                character.setCharacter(OverlayService.this, CharacterStore.selected(OverlayService.this));
                applyPrefs();
            }
            character.setMouthOpen((st & Native.ST_MOUTH) != 0);
            String sub = Native.subtitle();
            boolean listening = (st & Native.ST_LISTEN) != 0;
            boolean active = listening || (st & (Native.ST_THINK | Native.ST_SPEAK | Native.ST_LOADING)) != 0;
            // sprite « actif » = bouche ouverte : il suit le rythme de la voix et se referme entre les mots ;
            // écoute / réflexion le laissent au repos (sauf option « aussi en écoutant »)
            character.setActive((st & Native.ST_MOUTH) != 0 || (UiPrefs.activeOnListen(OverlayService.this) && active));
            if ((st & Native.ST_LOADING) != 0 && sub.isEmpty()) sub = "[chargement...]";
            long now = System.currentTimeMillis();
            if (!sub.equals(lastSubtitle)) lastChange = now;
            if (active) lastActive = now;
            if (!sub.equals(lastSubtitle) || st != lastState) {
                lastSubtitle = sub;
                lastState = st;
                bubble.setText(sub);
                bubble.setTextColor(listening ? 0xFF5FD7FF : 0xFF4EE04E);
            }
            // la bulle reste visible pendant l'activité + 4 s après
            boolean show = UiPrefs.bubbleOn(OverlayService.this) && !sub.isEmpty() && (active || now - Math.max(lastChange, lastActive) < 4000);
            int vis = show ? View.VISIBLE : View.GONE;
            if (bubble.getVisibility() != vis) bubble.setVisibility(vis);
            handler.postDelayed(this, 33);
        }
    };

    private int lastBubbleSize = -1, lastBubbleAlpha = -1;

    /** Réglages de l'onglet custom : taille du personnage, bulle, position. */
    private void applyPrefs() {
        if (character == null || root == null || wm == null || !root.isAttachedToWindow()) return;
        int scale = UiPrefs.charScale(this);
        int w = dp(63 * scale / 100f), h = dp(104 * scale / 100f);
        LinearLayout.LayoutParams cp = (LinearLayout.LayoutParams) character.getLayoutParams();
        if (cp.width != w || cp.height != h) {
            cp.width = w;
            cp.height = h;
            character.setLayoutParams(cp);
            wm.updateViewLayout(root, lp);
        }
        int bs = UiPrefs.bubbleSize(this), ba = UiPrefs.bubbleAlpha(this);
        if (bs != lastBubbleSize) {
            lastBubbleSize = bs;
            bubble.setTextSize(bs);
        }
        if (ba != lastBubbleAlpha) {
            lastBubbleAlpha = ba;
            bubble.setAlpha(ba / 100f);
        }
        if (UiPrefs.p(this).getBoolean("reset_pos", false)) {
            UiPrefs.putBool(this, "reset_pos", false);
            lp.x = getResources().getDisplayMetrics().widthPixels - w - dp(16);
            lp.y = dp(200);
            wm.updateViewLayout(root, lp);
        }
    }

    private int dp(float v) {
        return (int) TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_DIP, v, getResources().getDisplayMetrics());
    }

    @Override
    public IBinder onBind(Intent intent) {
        return null;
    }

    @Override
    public void onCreate() {
        super.onCreate();
        Native.ensureInit(this);
        goForeground();
        buildOverlay();
        Native.setOverlayVisible(true);
        handler.post(tick);
    }

    @Override
    public int onStartCommand(Intent intent, int flags, int startId) {
        if (intent != null && ACTION_STOP.equals(intent.getAction())) {
            stopSelf();
        }
        return START_NOT_STICKY;
    }

    private void goForeground() {
        NotificationManager nm = getSystemService(NotificationManager.class);
        nm.createNotificationChannel(new NotificationChannel(CHANNEL, "Assistant flottant",
                NotificationManager.IMPORTANCE_LOW));
        PendingIntent open = PendingIntent.getActivity(this, 0,
                new Intent(this, MainActivity.class), PendingIntent.FLAG_IMMUTABLE);
        PendingIntent stop = PendingIntent.getService(this, 1,
                new Intent(this, OverlayService.class).setAction(ACTION_STOP), PendingIntent.FLAG_IMMUTABLE);
        String name = Native.assistantName();
        Notification n = new Notification.Builder(this, CHANNEL)
                .setSmallIcon(android.R.drawable.ic_btn_speak_now)
                .setContentTitle(name + I18n.t(" est à l'écran", " is on screen"))
                .setContentText(I18n.t("Appuie sur le personnage pour lui parler", "Tap the character to talk to it"))
                .setContentIntent(open)
                .addAction(new Notification.Action.Builder(null, "Cacher", stop).build())
                .setOngoing(true)
                .build();
        int type = ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK;
        if (checkSelfPermission(Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED) {
            type |= ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE;
        }
        if (Build.VERSION.SDK_INT >= 29) {
            startForeground(1, n, type);
        } else {
            startForeground(1, n);
        }
    }

    private void buildOverlay() {
        wm = getSystemService(WindowManager.class);

        root = new LinearLayout(this);
        root.setOrientation(LinearLayout.VERTICAL);
        root.setGravity(Gravity.CENTER_HORIZONTAL);

        bubble = new TextView(this);
        bubble.setTypeface(Typeface.MONOSPACE);
        bubble.setTextSize(12);
        bubble.setMaxWidth(dp(220));
        bubble.setPadding(dp(8), dp(6), dp(8), dp(6));
        GradientDrawable bg = new GradientDrawable();
        bg.setColor(0xF00C0C0C);
        bg.setCornerRadius(dp(6));
        bg.setStroke(dp(1), 0xFF2E7D32);
        bubble.setBackground(bg);
        bubble.setVisibility(View.GONE);
        LinearLayout.LayoutParams blp = new LinearLayout.LayoutParams(
                LinearLayout.LayoutParams.WRAP_CONTENT, LinearLayout.LayoutParams.WRAP_CONTENT);
        blp.bottomMargin = dp(6);
        root.addView(bubble, blp);

        character = new CharacterView(this);
        // proportions du SVG : 280 x 460
        root.addView(character, new LinearLayout.LayoutParams(dp(63), dp(104)));

        lp = new WindowManager.LayoutParams(
                WindowManager.LayoutParams.WRAP_CONTENT,
                WindowManager.LayoutParams.WRAP_CONTENT,
                WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY,
                WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE
                        | WindowManager.LayoutParams.FLAG_NOT_TOUCH_MODAL,
                PixelFormat.TRANSLUCENT);
        lp.gravity = Gravity.TOP | Gravity.START;
        lp.x = getResources().getDisplayMetrics().widthPixels - dp(63) - dp(16);
        lp.y = dp(200);

        character.setOnTouchListener(new View.OnTouchListener() {
            final int slop = ViewConfiguration.get(OverlayService.this).getScaledTouchSlop();
            float downX, downY;
            int startX, startY;
            boolean dragging, longPressed;
            final Runnable longPress = () -> {
                longPressed = true;
                Intent i = new Intent(OverlayService.this, MainActivity.class);
                i.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_REORDER_TO_FRONT);
                startActivity(i);
            };

            @Override
            public boolean onTouch(View v, MotionEvent e) {
                switch (e.getActionMasked()) {
                    case MotionEvent.ACTION_DOWN:
                        downX = e.getRawX();
                        downY = e.getRawY();
                        startX = lp.x;
                        startY = lp.y;
                        dragging = false;
                        longPressed = false;
                        handler.postDelayed(longPress, 650);
                        return true;
                    case MotionEvent.ACTION_MOVE:
                        float dx = e.getRawX() - downX, dy = e.getRawY() - downY;
                        if (!dragging && (Math.abs(dx) > slop || Math.abs(dy) > slop)) {
                            dragging = true;
                            handler.removeCallbacks(longPress);
                        }
                        if (dragging) {
                            lp.x = startX + (int) dx;
                            lp.y = startY + (int) dy;
                            wm.updateViewLayout(root, lp);
                        }
                        return true;
                    case MotionEvent.ACTION_UP:
                        handler.removeCallbacks(longPress);
                        if (!dragging && !longPressed) Native.onTap();
                        return true;
                    case MotionEvent.ACTION_CANCEL:
                        handler.removeCallbacks(longPress);
                        return true;
                    default:
                        return false;
                }
            }
        });

        wm.addView(root, lp);
    }

    @Override
    public void onDestroy() {
        handler.removeCallbacksAndMessages(null);
        Native.setOverlayVisible(false);
        if (root != null && root.isAttachedToWindow()) {
            wm.removeView(root);
        }
        super.onDestroy();
    }
}
