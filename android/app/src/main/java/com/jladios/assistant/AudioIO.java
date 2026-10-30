package com.jladios.assistant;

import android.Manifest;
import android.content.Context;
import android.content.pm.PackageManager;
import android.media.AudioAttributes;
import android.media.AudioFormat;
import android.media.AudioRecord;
import android.media.AudioTrack;
import android.media.MediaRecorder;

/**
 * Entrées/sorties audio brutes. Java ne fait que transporter les échantillons :
 * la reconnaissance (Nemotron) et la synthèse (Kokoro) sont faites en Rust.
 */
final class AudioIO {
    private static boolean started;

    private AudioIO() {}

    static synchronized void start(Context ctx) {
        if (started) return;
        started = true;
        Thread p = new Thread(AudioIO::playerLoop, "jl-player");
        p.setDaemon(true);
        p.start();
        Thread m = new Thread(() -> micLoop(ctx), "jl-mic");
        m.setDaemon(true);
        m.start();
    }

    private static void sleep(long ms) {
        try { Thread.sleep(ms); } catch (InterruptedException ignored) {}
    }

    private static void playerLoop() {
        int rate = Native.playbackRate();
        int min = AudioTrack.getMinBufferSize(rate, AudioFormat.CHANNEL_OUT_MONO, AudioFormat.ENCODING_PCM_FLOAT);
        AudioTrack track = new AudioTrack.Builder()
                .setAudioAttributes(new AudioAttributes.Builder()
                        .setUsage(AudioAttributes.USAGE_MEDIA)
                        .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH)
                        .build())
                .setAudioFormat(new AudioFormat.Builder()
                        .setSampleRate(rate)
                        .setEncoding(AudioFormat.ENCODING_PCM_FLOAT)
                        .setChannelMask(AudioFormat.CHANNEL_OUT_MONO)
                        .build())
                .setBufferSizeInBytes(Math.max(min, rate / 10 * 4))
                .setTransferMode(AudioTrack.MODE_STREAM)
                .build();
        track.play();
        float[] buf = new float[1024];
        while (true) {
            int n = Native.pullAudio(buf);
            if (n > 0) {
                track.write(buf, 0, n, AudioTrack.WRITE_BLOCKING);
            } else {
                sleep(15);
            }
        }
    }

    private static boolean wantsMic() {
        return (Native.overlayState() & Native.ST_LISTEN) != 0;
    }

    private static void micLoop(Context ctx) {
        while (true) {
            if (!wantsMic()) {
                sleep(40);
                continue;
            }
            if (ctx.checkSelfPermission(Manifest.permission.RECORD_AUDIO) != PackageManager.PERMISSION_GRANTED) {
                Native.micUnavailable("permission micro refusée (relance /show pour la demander)");
                continue;
            }
            int rate = Native.micRate();
            int min = AudioRecord.getMinBufferSize(rate, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_FLOAT);
            AudioRecord rec = null;
            try {
                rec = new AudioRecord(MediaRecorder.AudioSource.VOICE_RECOGNITION, rate,
                        AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_FLOAT, Math.max(min, rate * 4 / 2));
                if (rec.getState() != AudioRecord.STATE_INITIALIZED) {
                    Native.micUnavailable("impossible d'ouvrir le micro");
                    continue;
                }
                rec.startRecording();
                float[] buf = new float[rate / 10];
                while (wantsMic()) {
                    int n = rec.read(buf, 0, buf.length, AudioRecord.READ_BLOCKING);
                    if (n > 0) Native.pushAudio(buf, n);
                    else if (n < 0) break;
                }
            } catch (Exception e) {
                Native.micUnavailable(String.valueOf(e.getMessage()));
                sleep(500);
            } finally {
                if (rec != null) {
                    try { rec.stop(); } catch (Exception ignored) {}
                    rec.release();
                }
            }
        }
    }
}
