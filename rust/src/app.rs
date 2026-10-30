//! État global de l'application + petites fonctions utilitaires partagées
//! entre les threads (STT, cerveau/LLM, TTS) et le pont JNI.

use crate::ansi::*;
use crate::config::Config;
use crate::terminal::Onboarding;
use crate::{brain, models, stt, terminal, tts};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

/// Fréquence de sortie audio (Kokoro génère en 24 kHz).
pub const PLAYBACK_RATE: i32 = 24_000;
/// Fréquence d'entrée micro attendue par Nemotron.
pub const MIC_RATE: i32 = 16_000;
const MAX_EVENTS: usize = 20_000;

pub enum BrainMsg {
    User { text: String, voice: bool },
    Reset,
}

pub enum SttMsg {
    Start,
    Audio(Vec<f32>),
    Stop,
    Unload,
}

pub enum TtsMsg {
    Speak { turn: u64, text: String },
    EndOfTurn { turn: u64 },
    Unload,
}

pub struct App {
    pub files_dir: PathBuf,
    #[allow(dead_code)]
    pub cache_dir: PathBuf,
    cfg: Mutex<Config>,
    events: Mutex<VecDeque<String>>,

    pub listening: AtomicBool,
    pub thinking: AtomicBool,
    pub loading: AtomicBool,
    pub downloading: AtomicBool,
    pub overlay_visible: AtomicBool,
    pub mouth_open: AtomicBool,
    avg_level: AtomicU32,
    pub fake_talk_until: AtomicU64,
    pub tts_pending: AtomicUsize,
    pub turn: AtomicU64,

    pub playback: Mutex<VecDeque<f32>>,
    subtitle: Mutex<String>,
    pub history: Mutex<Vec<(String, String)>>,
    pub onboarding: Mutex<Onboarding>,

    brain_tx: Mutex<Sender<BrainMsg>>,
    stt_tx: Mutex<Sender<SttMsg>>,
    tts_tx: Mutex<Sender<TtsMsg>>,
}

static APP: OnceLock<App> = OnceLock::new();

pub fn app() -> &'static App {
    APP.get().expect("Native.init() n'a pas été appelé")
}

pub fn try_app() -> Option<&'static App> {
    APP.get()
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Verrou qui ne panique pas si un thread a paniqué en le tenant.
pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn init(files_dir: String, cache_dir: String) {
    if APP.get().is_some() {
        return;
    }
    let files_dir = PathBuf::from(files_dir);
    let cache_dir = PathBuf::from(cache_dir);
    let _ = std::fs::create_dir_all(&files_dir);
    let _ = std::fs::create_dir_all(&cache_dir);
    // piper-phonemize extrait espeak-ng-data dans temp_dir() : on le redirige
    // vers le cache de l'app (le /tmp par défaut n'est pas accessible sur Android).
    std::env::set_var("TMPDIR", &cache_dir);

    let cfg = Config::load(&files_dir.join("config.json"));
    let (brain_tx, brain_rx) = channel();
    let (stt_tx, stt_rx) = channel();
    let (tts_tx, tts_rx) = channel();

    let app = App {
        files_dir,
        cache_dir,
        cfg: Mutex::new(cfg),
        events: Mutex::new(VecDeque::new()),
        listening: AtomicBool::new(false),
        thinking: AtomicBool::new(false),
        loading: AtomicBool::new(false),
        downloading: AtomicBool::new(false),
        overlay_visible: AtomicBool::new(false),
        mouth_open: AtomicBool::new(false),
        avg_level: AtomicU32::new(0f32.to_bits()),
        fake_talk_until: AtomicU64::new(0),
        tts_pending: AtomicUsize::new(0),
        turn: AtomicU64::new(0),
        playback: Mutex::new(VecDeque::new()),
        subtitle: Mutex::new(String::new()),
        history: Mutex::new(Vec::new()),
        onboarding: Mutex::new(Onboarding::None),
        brain_tx: Mutex::new(brain_tx),
        stt_tx: Mutex::new(stt_tx),
        tts_tx: Mutex::new(tts_tx),
    };
    if APP.set(app).is_err() {
        return;
    }

    brain::spawn(brain_rx);
    stt::spawn(stt_rx);
    tts::spawn(tts_rx);
    terminal::boot();
}

impl App {
    // ---------- terminal / événements vers Java ----------

    fn push_event(&self, ev: String) {
        let mut q = lock(&self.events);
        if q.len() >= MAX_EVENTS {
            q.pop_front();
        }
        q.push_back(ev);
    }

    /// Écrit du texte (avec codes ANSI) dans le terminal.
    pub fn print(&self, s: impl AsRef<str>) {
        self.push_event(format!("o:{}", s.as_ref()));
    }

    pub fn println(&self, s: impl AsRef<str>) {
        self.push_event(format!("o:{}\n", s.as_ref()));
    }

    pub fn info(&self, s: impl AsRef<str>) {
        self.println(format!("{GRAY}[i]{RESET} {}", s.as_ref()));
    }

    pub fn error(&self, s: impl AsRef<str>) {
        self.println(format!("{RED}[!] {}{RESET}", s.as_ref()));
    }

    pub fn success(&self, s: impl AsRef<str>) {
        self.println(format!("{GREEN}[ok]{RESET} {}", s.as_ref()));
    }

    /// Commande à exécuter côté Java (afficher l'overlay, permissions...).
    pub fn command(&self, c: &str) {
        self.push_event(format!("c:{c}"));
    }

    pub fn pop_event(&self) -> Option<String> {
        lock(&self.events).pop_front()
    }

    // ---------- configuration ----------

    pub fn cfg(&self) -> Config {
        lock(&self.cfg).clone()
    }

    pub fn update_cfg(&self, f: impl FnOnce(&mut Config)) {
        let mut c = lock(&self.cfg);
        f(&mut c);
        c.save(&self.files_dir.join("config.json"));
    }

    // ---------- sous-titre (bulle au-dessus du personnage) ----------

    pub fn set_subtitle(&self, s: impl Into<String>) {
        *lock(&self.subtitle) = s.into();
    }

    pub fn subtitle(&self) -> String {
        lock(&self.subtitle).clone()
    }

    // ---------- messages vers les threads ----------

    pub fn send_brain(&self, m: BrainMsg) {
        let _ = lock(&self.brain_tx).send(m);
    }

    pub fn send_stt(&self, m: SttMsg) {
        let _ = lock(&self.stt_tx).send(m);
    }

    pub fn send_tts(&self, m: TtsMsg) {
        let _ = lock(&self.tts_tx).send(m);
    }

    pub fn speak(&self, turn: u64, text: impl Into<String>) {
        let text = text.into();
        if text.trim().is_empty() {
            return;
        }
        self.tts_pending.fetch_add(1, Ordering::SeqCst);
        self.send_tts(TtsMsg::Speak { turn, text });
    }

    /// Démarre un nouveau "tour" de parole et coupe ce qui était en cours.
    pub fn new_turn(&self) -> u64 {
        let t = self.turn.fetch_add(1, Ordering::SeqCst) + 1;
        lock(&self.playback).clear();
        self.fake_talk_until.store(0, Ordering::SeqCst);
        self.mouth_open.store(false, Ordering::SeqCst);
        t
    }

    // ---------- écoute ----------

    pub fn start_listening(&self) {
        if !models::stt_ready() {
            self.error("Le modèle de reconnaissance vocale n'est pas installé. Tape /download");
            self.set_subtitle("Installe mes modèles avec /download !");
            return;
        }
        self.new_turn();
        self.send_stt(SttMsg::Start);
        self.listening.store(true, Ordering::SeqCst);
        let fr = self.cfg().is_fr();
        self.set_subtitle(if fr { "[j'écoute...]" } else { "[listening...]" });
    }

    pub fn stop_listening(&self) {
        if self.listening.swap(false, Ordering::SeqCst) {
            self.send_stt(SttMsg::Stop);
        }
    }

    pub fn mic_unavailable(&self, reason: &str) {
        if self.listening.swap(false, Ordering::SeqCst) {
            self.send_stt(SttMsg::Stop);
            self.error(format!("Micro indisponible : {reason}"));
            self.set_subtitle("Micro indisponible :(");
        }
    }

    pub fn is_speaking(&self) -> bool {
        !lock(&self.playback).is_empty()
            || self.tts_pending.load(Ordering::SeqCst) > 0
            || now_ms() < self.fake_talk_until.load(Ordering::SeqCst)
    }

    /// Appui sur le personnage flottant.
    pub fn tap(&self) {
        if self.listening.load(Ordering::SeqCst) {
            self.stop_listening();
        } else {
            // Interrompt la réponse en cours éventuelle puis écoute.
            self.start_listening();
        }
    }

    // ---------- audio ----------

    pub fn push_audio(&self, samples: &[f32]) {
        if self.listening.load(Ordering::SeqCst) && !samples.is_empty() {
            self.send_stt(SttMsg::Audio(samples.to_vec()));
        }
    }

    /// Java récupère les échantillons à jouer. On en profite pour animer la
    /// bouche selon l'énergie du signal (bouche ouverte sur les syllabes).
    pub fn pull_audio(&self, out: &mut [f32]) -> usize {
        let n = {
            let mut q = lock(&self.playback);
            let n = out.len().min(q.len());
            for (i, s) in q.drain(..n).enumerate() {
                out[i] = s;
            }
            n
        };
        if n == 0 {
            self.mouth_open.store(false, Ordering::Relaxed);
            return 0;
        }
        let rms = (out[..n].iter().map(|x| x * x).sum::<f32>() / n as f32).sqrt();
        let avg = f32::from_bits(self.avg_level.load(Ordering::Relaxed));
        let avg = avg * 0.9 + rms * 0.1;
        self.avg_level.store(avg.to_bits(), Ordering::Relaxed);
        let open = rms > 0.012 && rms > avg * 0.7;
        self.mouth_open.store(open, Ordering::Relaxed);
        n
    }

    /// Bits : 1 bouche ouverte, 2 écoute, 4 réflexion, 8 parle, 16 chargement.
    pub fn overlay_state(&self) -> i32 {
        let now = now_ms();
        let fake = now < self.fake_talk_until.load(Ordering::Relaxed);
        let speaking = self.is_speaking();
        let mouth = if fake {
            (now / 140) % 2 == 0
        } else {
            speaking && self.mouth_open.load(Ordering::Relaxed)
        };
        let mut st = 0;
        if mouth {
            st |= 1;
        }
        if self.listening.load(Ordering::Relaxed) {
            st |= 2;
        }
        if self.thinking.load(Ordering::Relaxed) {
            st |= 4;
        }
        if speaking {
            st |= 8;
        }
        if self.loading.load(Ordering::Relaxed) {
            st |= 16;
        }
        st
    }
}
