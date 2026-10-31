//! Synthèse vocale locale : Kokoro-82M (via sherpa-onnx) + phonémisation
//! espeak-ng (piper-phonemize).

use crate::app::{app, lock, now_ms, TtsMsg, PLAYBACK_RATE};
use crate::models::{self, Engine};
use sherpa_onnx::{
    GenerationConfig, OfflineTts, OfflineTtsConfig, OfflineTtsKokoroModelConfig,
    OfflineTtsModelConfig, OfflineTtsVitsModelConfig,
};
use std::sync::atomic::Ordering;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

pub fn spawn(rx: Receiver<TtsMsg>) {
    std::thread::Builder::new()
        .name("tts".into())
        .spawn(move || run(rx))
        .expect("spawn tts");
}

fn create() -> Option<OfflineTts> {
    let v = models::tts_voice();
    let mut model = OfflineTtsModelConfig { num_threads: 4, debug: false, ..Default::default() };
    match v.engine {
        Engine::Kokoro => {
            model.kokoro = OfflineTtsKokoroModelConfig {
                model: Some(models::file(&v.set, "model.int8.onnx")),
                voices: Some(models::file(&v.set, "voices.bin")),
                tokens: Some(models::file(&v.set, "tokens.txt")),
                length_scale: 1.0,
                ..Default::default()
            }
        }
        Engine::Vits => {
            model.vits = OfflineTtsVitsModelConfig {
                model: Some(models::file(&v.set, v.model_file)),
                tokens: Some(models::file(&v.set, "tokens.txt")),
                ..Default::default()
            }
        }
    }
    OfflineTts::create(&OfflineTtsConfig { model, ..Default::default() })
}

/// Retire ce qui ne se prononce pas (markdown, emojis...).
pub fn clean_for_speech(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '*' | '#' | '`' | '_' | '~' | '|' | '<' | '>' | '[' | ']' | '{' | '}' => out.push(' '),
            c if (c as u32) >= 0x1F000 => {}
            c if ('\u{2600}'..='\u{27BF}').contains(&c) => {}
            '\n' | '\r' | '\t' => out.push(' '),
            c => out.push(c),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn resample(input: &[f32], from: i32, to: i32) -> Vec<f32> {
    if from == to || input.is_empty() {
        return input.to_vec();
    }
    let ratio = from as f64 / to as f64;
    let n = (input.len() as f64 / ratio) as usize;
    (0..n)
        .map(|i| {
            let pos = i as f64 * ratio;
            let i0 = pos.floor() as usize;
            let i1 = (i0 + 1).min(input.len() - 1);
            let t = (pos - i0 as f64) as f32;
            input[i0] * (1.0 - t) + input[i1] * t
        })
        .collect()
}

fn done_one() {
    let p = &app().tts_pending;
    let mut cur = p.load(Ordering::SeqCst);
    while cur > 0 {
        match p.compare_exchange(cur, cur - 1, Ordering::SeqCst, Ordering::SeqCst) {
            Ok(_) => break,
            Err(v) => cur = v,
        }
    }
}

fn run(rx: Receiver<TtsMsg>) {
    let a = app();
    let mut tts: Option<OfflineTts> = None;
    let mut loaded = "";
    // Charge Kokoro dès le démarrage plutôt qu'à la première phrase
    // (plusieurs secondes sur téléphone).
    if models::tts_ready() && !a.cfg().mute {
        tts = create();
        loaded = models::tts_voice().id;
    }

    for msg in rx {
        match msg {
            TtsMsg::Speak { turn, text } => {
                if turn != a.turn.load(Ordering::SeqCst) {
                    done_one();
                    continue;
                }
                let cfg = a.cfg();
                let clean = clean_for_speech(&text);
                if clean.is_empty() {
                    done_one();
                    continue;
                }
                if cfg.mute || !models::tts_ready() {
                    // Pas de voix : on fait quand même bouger la bouche.
                    let dur = (clean.chars().count() as u64 * 65).max(700);
                    let start = now_ms().max(a.fake_talk_until.load(Ordering::SeqCst));
                    a.fake_talk_until.store(start + dur, Ordering::SeqCst);
                    done_one();
                    continue;
                }
                // voix changée (par /tts ou par la langue) : on recharge la bonne
                if tts.is_some() && loaded != models::tts_voice().id {
                    tts = None;
                }
                if tts.is_none() {
                    a.loading.store(true, Ordering::SeqCst);
                    loaded = models::tts_voice().id;
                    tts = create();
                    a.loading.store(false, Ordering::SeqCst);
                    if tts.is_none() {
                        a.error("Impossible de charger la voix (essaie /delete tts puis /download tts)");
                        done_one();
                        continue;
                    }
                }
                let engine = tts.as_ref().unwrap();
                let v = models::tts_voice();
                let fr = if v.lang == "multi" { cfg.is_fr() } else { v.lang == "fr" };
                let voice = if fr { "fr" } else { "en-us" };
                let phonemes: Vec<Vec<i32>> = match piper_phonemize::phonemize(&clean, voice) {
                    Some(r) => (0..r.num_sentences())
                        .filter_map(|i| r.get_phonemes(i))
                        .filter(|p| !p.is_empty())
                        .collect(),
                    None => Vec::new(),
                };
                if phonemes.is_empty() {
                    done_one();
                    continue;
                }
                let gen = GenerationConfig {
                    sid: if v.engine == Engine::Kokoro {
                        cfg.voice_sid.clamp(0, engine.num_speakers().max(1) - 1)
                    } else if cfg.voice_sid < engine.num_speakers() {
                        cfg.voice_sid // upmc / mls : plusieurs voix
                    } else {
                        0
                    },
                    speed: cfg.speed.clamp(0.5, 2.0),
                    phoneme_codepoints: Some(phonemes),
                    ..Default::default()
                };
                let audio = engine.generate_with_config("", &gen, None::<fn(&[f32], f32) -> bool>);
                if let Some(audio) = audio {
                    if turn == a.turn.load(Ordering::SeqCst) {
                        let samples = resample(audio.samples(), audio.sample_rate(), PLAYBACK_RATE);
                        let mut q = lock(&a.playback);
                        q.extend(samples);
                        // petite pause entre les phrases
                        q.extend(std::iter::repeat(0.0).take((PLAYBACK_RATE / 8) as usize));
                    }
                } else {
                    a.error("Échec de la synthèse vocale");
                }
                done_one();
            }
            TtsMsg::EndOfTurn { turn } => {
                // Attend la fin de la lecture, puis réécoute si le mode auto est actif.
                let start = Instant::now();
                while turn == a.turn.load(Ordering::SeqCst)
                    && a.is_speaking()
                    && start.elapsed() < Duration::from_secs(180)
                {
                    std::thread::sleep(Duration::from_millis(60));
                }
                std::thread::sleep(Duration::from_millis(250));
                if turn == a.turn.load(Ordering::SeqCst)
                    && a.cfg().auto_listen
                    && a.overlay_visible.load(Ordering::SeqCst)
                    && !a.listening.load(Ordering::SeqCst)
                    && models::stt_ready()
                {
                    a.start_listening();
                }
            }
            TtsMsg::Unload => {
                tts = None;
                loaded = "";
            }
        }
    }
}

/// Banc d'essai manuel : `JL_BENCH=/chemin cargo test --lib bench -- --ignored --nocapture`
#[cfg(test)]
mod bench {
    use super::*;
    use std::time::Instant;

    fn run(name: &str, model: OfflineTtsModelConfig, lang: &str, speakers: i32) {
        let t0 = Instant::now();
        let tts = OfflineTts::create(&OfflineTtsConfig { model, ..Default::default() }).expect("create");
        println!("{name}: chargement {:.2}s", t0.elapsed().as_secs_f32());
        for text in ["Bonjour, comment vas-tu aujourd'hui ?", "Je suis ton assistant et je tourne entièrement sur ton téléphone, sans connexion internet."] {
            let ph: Vec<Vec<i32>> = {
                let r = piper_phonemize::phonemize(text, lang).expect("phonemize");
                (0..r.num_sentences()).filter_map(|i| r.get_phonemes(i)).filter(|p| !p.is_empty()).collect()
            };
            let gen = GenerationConfig { sid: if speakers > 30 { 30 } else { 0 }, speed: 1.0, phoneme_codepoints: Some(ph), ..Default::default() };
            let t = Instant::now();
            let a = tts.generate_with_config("", &gen, None::<fn(&[f32], f32) -> bool>).expect("audio");
            let el = t.elapsed().as_secs_f32();
            let dur = a.samples().len() as f32 / a.sample_rate() as f32;
            println!("{name}: {:.2}s de calcul pour {:.2}s d'audio (x{:.1} temps réel)", el, dur, dur / el);
        }
    }

    #[test]
    #[ignore]
    fn bench() {
        let d = std::env::var("JL_BENCH").expect("JL_BENCH");
        let f = |s: &str| format!("{d}/{s}");
        run("kokoro", OfflineTtsModelConfig { num_threads: 4, kokoro: OfflineTtsKokoroModelConfig {
            model: Some(f("kokoro/model.int8.onnx")), voices: Some(f("kokoro/voices.bin")), tokens: Some(f("kokoro/tokens.txt")), length_scale: 1.0, ..Default::default() }, ..Default::default() }, "fr", 53);
        for (n, dir) in [("siwis-low", "low"), ("siwis-medium", "med")] {
            run(n, OfflineTtsModelConfig { num_threads: 4, vits: OfflineTtsVitsModelConfig {
                model: Some(f(&format!("{dir}/m.onnx"))), tokens: Some(f(&format!("{dir}/tokens.txt"))), ..Default::default() }, ..Default::default() }, "fr", 1);
        }
    }
}
