//! Reconnaissance vocale locale en streaming : nemotron-3.5-asr-streaming-0.6b
//! (via sherpa-onnx, transducteur NeMo cache-aware).

use crate::app::{app, BrainMsg, SttMsg, MIC_RATE};
use crate::models::{self, STT};
use sherpa_onnx::{OnlineRecognizer, OnlineRecognizerConfig, OnlineStream};
use std::sync::atomic::Ordering;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

/// Si personne ne parle pendant ce délai, on arrête d'écouter.
const NO_SPEECH_TIMEOUT: Duration = Duration::from_secs(10);

pub fn spawn(rx: Receiver<SttMsg>) {
    std::thread::Builder::new()
        .name("stt".into())
        .spawn(move || run(rx))
        .expect("spawn stt");
}

fn create() -> Option<OnlineRecognizer> {
    let mut c = OnlineRecognizerConfig::default();
    c.model_config.transducer.encoder = Some(models::file(&STT, "encoder.int8.onnx"));
    c.model_config.transducer.decoder = Some(models::file(&STT, "decoder.int8.onnx"));
    c.model_config.transducer.joiner = Some(models::file(&STT, "joiner.int8.onnx"));
    c.model_config.tokens = Some(models::file(&STT, "tokens.txt"));
    c.model_config.num_threads = 4;
    c.model_config.provider = Some("cpu".into());
    c.decoding_method = Some("greedy_search".into());
    // Détection de fin de phrase.
    c.enable_endpoint = true;
    c.rule1_min_trailing_silence = 2.4;
    c.rule2_min_trailing_silence = 0.9;
    c.rule3_min_utterance_length = 25.0;
    OnlineRecognizer::create(&c)
}

fn finish(text: String) {
    let a = app();
    a.listening.store(false, Ordering::SeqCst);
    a.set_subtitle(format!("> {text}"));
    a.send_brain(BrainMsg::User { text, voice: true });
}

fn run(rx: Receiver<SttMsg>) {
    let a = app();
    let mut rec: Option<OnlineRecognizer> = None;
    let mut stream: Option<OnlineStream> = None;
    let mut last_text = String::new();
    let mut started = Instant::now();

    for msg in rx {
        match msg {
            SttMsg::Start => {
                stream = None;
                if rec.is_none() {
                    a.loading.store(true, Ordering::SeqCst);
                    a.info("Chargement du modèle de reconnaissance vocale (quelques secondes)...");
                    rec = create();
                    a.loading.store(false, Ordering::SeqCst);
                    if rec.is_none() {
                        a.error("Impossible de charger le modèle STT (fichiers corrompus ? essaie /delete stt puis /download stt)");
                        a.listening.store(false, Ordering::SeqCst);
                        continue;
                    }
                    a.success("Reconnaissance vocale prête");
                }
                let r = rec.as_ref().unwrap();
                let s = r.create_stream();
                let lang = a.cfg().lang;
                s.set_option("language", if lang == "en" { "en" } else { "fr" });
                stream = Some(s);
                last_text.clear();
                started = Instant::now();
            }
            SttMsg::Audio(samples) => {
                let (Some(r), Some(s)) = (rec.as_ref(), stream.as_ref()) else { continue };
                if !a.listening.load(Ordering::SeqCst) {
                    continue;
                }
                s.accept_waveform(MIC_RATE, &samples);
                while r.is_ready(s) {
                    r.decode(s);
                }
                let text = r
                    .get_result(s)
                    .map(|x| x.text.trim().to_string())
                    .unwrap_or_default();
                if text != last_text {
                    last_text = text.clone();
                    if !text.is_empty() {
                        a.set_subtitle(format!("> {text}"));
                    }
                }
                if r.is_endpoint(s) {
                    if !text.is_empty() {
                        stream = None;
                        finish(text);
                    } else {
                        r.reset(s);
                        if started.elapsed() > NO_SPEECH_TIMEOUT {
                            stream = None;
                            a.listening.store(false, Ordering::SeqCst);
                            a.set_subtitle("");
                            a.info("(rien entendu)");
                        }
                    }
                }
            }
            SttMsg::Stop => {
                if let (Some(r), Some(s)) = (rec.as_ref(), stream.as_ref()) {
                    s.input_finished();
                    while r.is_ready(s) {
                        r.decode(s);
                    }
                    let text = r
                        .get_result(s)
                        .map(|x| x.text.trim().to_string())
                        .unwrap_or_default();
                    if !text.is_empty() {
                        finish(text);
                    } else {
                        a.set_subtitle("");
                    }
                }
                stream = None;
            }
            SttMsg::Unload => {
                stream = None;
                rec = None;
            }
        }
    }
}
