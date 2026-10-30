//! Téléchargement des modèles (au premier lancement) depuis HuggingFace.
//! Les fichiers sont stockés dans le dossier privé de l'app, avec reprise
//! automatique (`Range`) si le téléchargement est interrompu.

use crate::ansi::*;
use crate::app::{app, BrainMsg};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

pub struct ModelSet {
    pub label: &'static str,
    pub repo: &'static str,
    pub dir: &'static str,
    pub files: &'static [&'static str],
    pub approx_mb: u32,
}

/// Speech-to-text : NVIDIA Nemotron 3.5 ASR streaming 0.6B (export sherpa-onnx int8).
pub const STT: ModelSet = ModelSet {
    label: "STT nemotron-3.5-asr-streaming-0.6b",
    repo: "csukuangfj2/sherpa-onnx-nemotron-3.5-asr-streaming-0.6b-560ms-int8-2026-06-11",
    dir: "stt-nemotron-3.5-asr-streaming-0.6b-560ms-int8",
    files: &["tokens.txt", "decoder.int8.onnx", "joiner.int8.onnx", "encoder.int8.onnx"],
    approx_mb: 680,
};

/// Text-to-speech : Kokoro-82M multilingue (int8).
pub const TTS: ModelSet = ModelSet {
    label: "TTS Kokoro-82M (multi-lang v1.0 int8)",
    repo: "csukuangfj/kokoro-int8-multi-lang-v1_0",
    dir: "tts-kokoro-int8-multi-lang-v1_0",
    files: &["tokens.txt", "voices.bin", "model.int8.onnx"],
    approx_mb: 140,
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Engine {
    Kokoro,
    /// VITS / Piper (une seule voix par fichier, très léger)
    Vits,
}

/// Une voix du catalogue `/tts`.
pub struct Voice {
    pub id: &'static str,
    pub label: &'static str,
    pub engine: Engine,
    /// "fr", "en" ou "multi" (Kokoro suit la langue de conversation)
    pub lang: &'static str,
    pub model_file: &'static str,
    pub set: ModelSet,
}

macro_rules! piper {
    ($id:literal, $label:literal, $lang:literal, $repo:literal, $file:literal, $mb:literal) => {
        Voice {
            id: $id,
            label: $label,
            engine: Engine::Vits,
            lang: $lang,
            model_file: $file,
            set: ModelSet {
                label: concat!("TTS Piper ", $label),
                repo: concat!("csukuangfj/", $repo),
                dir: concat!("tts-", $repo),
                files: &["tokens.txt", $file],
                approx_mb: $mb,
            },
        }
    };
}

/// Catalogue des voix qui tournent hors ligne sur le téléphone.
pub const VOICES: &[Voice] = &[
    Voice { id: "kokoro", label: "Kokoro 82M multilingue (53 voix, naturel mais lent)", engine: Engine::Kokoro, lang: "multi", model_file: "model.int8.onnx", set: TTS },
    piper!("siwis-low", "fr siwis low (rapide)", "fr", "vits-piper-fr_FR-siwis-low", "fr_FR-siwis-low.onnx", 28),
    piper!("siwis", "fr siwis medium", "fr", "vits-piper-fr_FR-siwis-medium", "fr_FR-siwis-medium.onnx", 63),
    piper!("upmc", "fr upmc medium (2 voix)", "fr", "vits-piper-fr_FR-upmc-medium", "fr_FR-upmc-medium.onnx", 77),
    piper!("tom", "fr tom medium (voix d'homme)", "fr", "vits-piper-fr_FR-tom-medium", "fr_FR-tom-medium.onnx", 64),
    piper!("gilles", "fr gilles low (voix d'homme)", "fr", "vits-piper-fr_FR-gilles-low", "fr_FR-gilles-low.onnx", 63),
    piper!("mls", "fr mls medium", "fr", "vits-piper-fr_FR-mls-medium", "fr_FR-mls-medium.onnx", 77),
    piper!("lessac", "en lessac medium", "en", "vits-piper-en_US-lessac-medium", "en_US-lessac-medium.onnx", 63),
    piper!("amy", "en amy low", "en", "vits-piper-en_US-amy-low", "en_US-amy-low.onnx", 63),
    piper!("ryan", "en ryan medium (voix d'homme)", "en", "vits-piper-en_US-ryan-medium", "en_US-ryan-medium.onnx", 63),
];

/// Voix choisie avec `/tts` (Kokoro par défaut).
pub fn tts_voice() -> &'static Voice {
    let id = app().cfg().tts_model;
    VOICES.iter().find(|v| v.id == id).unwrap_or(&VOICES[0])
}

#[derive(Clone, Copy)]
pub enum Which {
    Stt,
    Tts,
    All,
}

pub fn set_dir(set: &ModelSet) -> PathBuf {
    app().files_dir.join("models").join(set.dir)
}

pub fn file(set: &ModelSet, name: &str) -> String {
    set_dir(set).join(name).to_string_lossy().into_owned()
}

fn ready(set: &ModelSet) -> bool {
    let d = set_dir(set);
    set.files.iter().all(|f| d.join(f).is_file())
}

pub fn stt_ready() -> bool {
    ready(&STT)
}

pub fn tts_ready() -> bool {
    ready(&tts_voice().set)
}

pub fn voice_ready(v: &Voice) -> bool {
    ready(&v.set)
}

pub fn delete(which: Which) {
    let a = app();
    let sets: Vec<&ModelSet> = match which {
        Which::Stt => vec![&STT],
        Which::Tts => vec![&tts_voice().set],
        Which::All => vec![&STT, &tts_voice().set],
    };
    if matches!(which, Which::Stt | Which::All) {
        a.send_stt(crate::app::SttMsg::Unload);
    }
    if matches!(which, Which::Tts | Which::All) {
        a.send_tts(crate::app::TtsMsg::Unload);
    }
    for s in sets {
        let _ = fs::remove_dir_all(set_dir(s));
        a.success(format!("{} supprimé", s.label));
    }
}

pub fn download(which: Which, greet_after: bool) {
    let a = app();
    if a.downloading.swap(true, Ordering::SeqCst) {
        a.error("Un téléchargement est déjà en cours.");
        return;
    }
    std::thread::Builder::new()
        .name("download".into())
        .spawn(move || {
            let a = app();
            let sets: Vec<&ModelSet> = match which {
                Which::Stt => vec![&STT],
                Which::Tts => vec![&tts_voice().set],
                Which::All => vec![&tts_voice().set, &STT],
            };
            let mut ok = true;
            for set in sets {
                if ready(set) {
                    a.success(format!("{} déjà installé", set.label));
                    continue;
                }
                a.println(format!(
                    "{CYAN}{BOLD}==>{RESET} {BOLD}{}{RESET} {GRAY}(~{} Mo){RESET}",
                    set.label, set.approx_mb
                ));
                let dir = set_dir(set);
                if let Err(e) = fs::create_dir_all(&dir) {
                    a.error(format!("mkdir: {e}"));
                    ok = false;
                    break;
                }
                for f in set.files {
                    let url = format!("https://huggingface.co/{}/resolve/main/{}", set.repo, f);
                    let mut attempt = 0;
                    loop {
                        attempt += 1;
                        match download_file(&url, &dir.join(f), f) {
                            Ok(()) => break,
                            Err(e) if attempt < 4 => {
                                a.error(format!("{f}: {e} — nouvel essai ({attempt}/3)..."));
                                std::thread::sleep(Duration::from_secs(3));
                            }
                            Err(e) => {
                                a.error(format!("{f}: {e}"));
                                ok = false;
                                break;
                            }
                        }
                    }
                    if !ok {
                        break;
                    }
                }
                if !ok {
                    break;
                }
                a.success(format!("{} installé", set.label));
            }
            a.downloading.store(false, Ordering::SeqCst);
            if ok {
                a.println(format!("{BRIGHT_GREEN}{BOLD}Modèles prêts !{RESET} Tape {YELLOW}/show{RESET} pour faire apparaître ton assistant."));
                if greet_after {
                    let cfg = a.cfg();
                    let hello = if cfg.is_fr() {
                        format!("Dis bonjour à {} et présente-toi en une phrase.", cfg.user_name)
                    } else {
                        format!("Say hello to {} and introduce yourself in one sentence.", cfg.user_name)
                    };
                    if !cfg.active().api_key.is_empty() || !cfg.active().builtin {
                        a.send_brain(BrainMsg::User { text: hello, voice: false });
                    }
                }
            } else {
                a.error("Téléchargement incomplet. Relance /download (il reprendra où il s'est arrêté).");
            }
        })
        .expect("spawn download");
}

fn bar(frac: f64) -> String {
    let w = 20;
    let filled = ((frac * w as f64).round() as usize).min(w);
    format!("{}{}", "#".repeat(filled), ".".repeat(w - filled))
}

fn download_file(url: &str, dest: &Path, label: &str) -> Result<(), String> {
    let a = app();
    if dest.is_file() {
        return Ok(());
    }
    let part = PathBuf::from(format!("{}.part", dest.display()));
    let existing = fs::metadata(&part).map(|m| m.len()).unwrap_or(0);

    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(20))
        .timeout_read(Duration::from_secs(60))
        .redirects(10)
        .user_agent("jladios/0.1")
        .build();
    let mut req = agent.get(url);
    if existing > 0 {
        req = req.set("Range", &format!("bytes={existing}-"));
    }
    let resp = match req.call() {
        Ok(r) => r,
        Err(ureq::Error::Status(416, _)) if existing > 0 => {
            // Déjà entièrement téléchargé.
            fs::rename(&part, dest).map_err(|e| e.to_string())?;
            return Ok(());
        }
        Err(ureq::Error::Status(code, _)) => return Err(format!("HTTP {code}")),
        Err(e) => return Err(e.to_string()),
    };

    let resumed = resp.status() == 206;
    let mut done: u64 = if resumed { existing } else { 0 };
    let len: u64 = resp
        .header("Content-Length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let total = if len > 0 { len + done } else { 0 };

    let mut out: File = if resumed {
        OpenOptions::new().append(true).open(&part).map_err(|e| e.to_string())?
    } else {
        File::create(&part).map_err(|e| e.to_string())?
    };

    let mut reader = resp.into_reader();
    let mut buf = vec![0u8; 256 * 1024];
    let mut last = Instant::now() - Duration::from_secs(1);
    let started = Instant::now();
    let start_done = done;
    loop {
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        done += n as u64;
        if last.elapsed() >= Duration::from_millis(400) {
            last = Instant::now();
            let secs = started.elapsed().as_secs_f64().max(0.1);
            let speed = (done - start_done) as f64 / secs / 1e6;
            if total > 0 {
                let frac = done as f64 / total as f64;
                a.print(format!(
                    "\r  {GREEN}[{}]{RESET} {:>3}%  {}/{} Mo  {GRAY}{:.1} Mo/s{RESET}  {}",
                    bar(frac),
                    (frac * 100.0) as u32,
                    done / 1_000_000,
                    total / 1_000_000,
                    speed,
                    label
                ));
            } else {
                a.print(format!("\r  {} Mo  {GRAY}{:.1} Mo/s{RESET}  {}", done / 1_000_000, speed, label));
            }
        }
    }
    out.flush().map_err(|e| e.to_string())?;
    drop(out);
    if total > 0 && done < total {
        return Err(format!("connexion coupée ({}/{} Mo)", done / 1_000_000, total / 1_000_000));
    }
    fs::rename(&part, dest).map_err(|e| e.to_string())?;
    a.print(format!(
        "\r  {GREEN}[{}]{RESET} 100%  {} Mo  {}\n",
        bar(1.0),
        done / 1_000_000,
        label
    ));
    Ok(())
}
