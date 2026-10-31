//! Modèles de langage locaux (llama.cpp, hors ligne) : commande `/local`.
//! Le moteur n'est compilé que dans l'APK (feature `local-llm`, nécessite le NDK).

use crate::app::app;
use crate::config::{Config, Endpoint};
use serde_json::Value;
use std::path::PathBuf;

pub const KIND: &str = "local";

pub struct LocalModel {
    pub id: &'static str,
    pub label: &'static str,
    pub repo: &'static str,
    pub file: &'static str,
    pub approx_mb: u32,
    /// Modèles « réfléchissants » : on ferme d'office le bloc de raisonnement pour répondre vite.
    pub no_think: bool,
    /// Gabarit de secours si celui du modèle n'est pas reconnu.
    pub template: &'static str,
}

/// Petits modèles (quantifiés Q4_K_M) qui tiennent sur un téléphone.
pub const MODELS: &[LocalModel] = &[
    LocalModel { id: "qwen3.5-0.8b", label: "Qwen3.5 0.8B — minuscule et étonnant", repo: "unsloth/Qwen3.5-0.8B-GGUF", file: "Qwen3.5-0.8B-Q4_K_M.gguf", approx_mb: 533, no_think: true, template: "chatml" },
    LocalModel { id: "qwen3.5-2b", label: "Qwen3.5 2B — le meilleur équilibre (recommandé)", repo: "unsloth/Qwen3.5-2B-GGUF", file: "Qwen3.5-2B-Q4_K_M.gguf", approx_mb: 1281, no_think: true, template: "chatml" },
    LocalModel { id: "qwen3.5-4b", label: "Qwen3.5 4B — le plus intelligent (8 Go de RAM)", repo: "unsloth/Qwen3.5-4B-GGUF", file: "Qwen3.5-4B-Q4_K_M.gguf", approx_mb: 2741, no_think: true, template: "chatml" },
    LocalModel { id: "qwen3-4b", label: "Qwen3 4B Instruct 2507 — très bon, sans raisonnement", repo: "unsloth/Qwen3-4B-Instruct-2507-GGUF", file: "Qwen3-4B-Instruct-2507-Q4_K_M.gguf", approx_mb: 2497, no_think: false, template: "chatml" },
    LocalModel { id: "qwen3-1.7b", label: "Qwen3 1.7B — rapide", repo: "unsloth/Qwen3-1.7B-GGUF", file: "Qwen3-1.7B-Q4_K_M.gguf", approx_mb: 1107, no_think: true, template: "chatml" },
    LocalModel { id: "gemma3-1b", label: "Gemma 3 1B — léger, bon en français", repo: "unsloth/gemma-3-1b-it-GGUF", file: "gemma-3-1b-it-Q4_K_M.gguf", approx_mb: 806, no_think: false, template: "gemma" },
    LocalModel { id: "llama3.2-1b", label: "Llama 3.2 1B — très rapide", repo: "bartowski/Llama-3.2-1B-Instruct-GGUF", file: "Llama-3.2-1B-Instruct-Q4_K_M.gguf", approx_mb: 808, no_think: false, template: "llama3" },
    LocalModel { id: "llama3.2-3b", label: "Llama 3.2 3B — polyvalent", repo: "bartowski/Llama-3.2-3B-Instruct-GGUF", file: "Llama-3.2-3B-Instruct-Q4_K_M.gguf", approx_mb: 2019, no_think: false, template: "llama3" },
];

pub fn find(id: &str) -> Option<&'static LocalModel> {
    MODELS.iter().find(|m| m.id == id)
}

pub fn dir() -> PathBuf {
    app().files_dir.join("models").join("llm")
}

pub fn path(m: &LocalModel) -> PathBuf {
    dir().join(m.file)
}

pub fn installed(m: &LocalModel) -> bool {
    path(m).is_file()
}

pub fn chat(cfg: &Config, ep: &Endpoint, messages: &[Value], on_delta: impl FnMut(&str) -> bool) -> Result<String, String> {
    let m = find(&ep.model).ok_or("modèle local inconnu (liste avec /local)")?;
    if !installed(m) {
        return Err(format!("modèle local non téléchargé : tape /local {}", m.id));
    }
    engine::chat(cfg, m, messages, on_delta)
}

/// Charge en tâche de fond le modèle local actif (au démarrage de l'app).
pub fn preload() {
    let ep = app().cfg().active();
    if ep.kind != KIND {
        return;
    }
    let Some(m) = find(&ep.model).filter(|m| installed(m)) else { return };
    std::thread::spawn(move || {
        let a = app();
        a.info("chargement du modèle local (quelques secondes)...");
        match engine::preload(m) {
            Ok(()) => a.success("modèle local prêt"),
            Err(e) => a.error(e),
        }
    });
}

/// Libère la mémoire du modèle chargé.
pub fn unload() {
    engine::unload();
}

#[cfg(not(feature = "local-llm"))]
mod engine {
    use super::*;

    pub fn chat(_: &Config, _: &LocalModel, _: &[Value], _: impl FnMut(&str) -> bool) -> Result<String, String> {
        Err("cette version n'embarque pas le moteur de modèles locaux".into())
    }

    pub fn unload() {}

    pub fn preload(_: &LocalModel) -> Result<(), String> {
        Ok(())
    }
}

#[cfg(feature = "local-llm")]
mod engine {
    use super::*;
    use llama_cpp_2::context::params::LlamaContextParams;
    use llama_cpp_2::llama_backend::LlamaBackend;
    use llama_cpp_2::llama_batch::LlamaBatch;
    use llama_cpp_2::model::params::LlamaModelParams;
    use llama_cpp_2::model::{LlamaChatMessage, LlamaChatTemplate, LlamaModel};
    use llama_cpp_2::sampling::LlamaSampler;
    use std::num::NonZeroU32;
    use std::sync::{Mutex, OnceLock};

    const N_CTX: u32 = 3072;
    const N_BATCH: usize = 512;
    const MAX_NEW: usize = 400;

    struct Loaded {
        id: String,
        model: LlamaModel,
    }

    static BACKEND: OnceLock<LlamaBackend> = OnceLock::new();
    static LOADED: Mutex<Option<Loaded>> = Mutex::new(None);

    fn backend() -> Result<&'static LlamaBackend, String> {
        if let Some(b) = BACKEND.get() {
            return Ok(b);
        }
        let b = LlamaBackend::init().map_err(|e| format!("moteur local : {e}"))?;
        Ok(BACKEND.get_or_init(|| b))
    }

    pub fn unload() {
        *LOADED.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }

    fn prompt_for(model: &LlamaModel, m: &LocalModel, turns: &[(String, String)]) -> Result<String, String> {
        let chat: Vec<LlamaChatMessage> = turns
            .iter()
            .map(|(r, c)| LlamaChatMessage::new(r.clone(), c.clone()).map_err(|e| e.to_string()))
            .collect::<Result<_, _>>()?;
        let own = model.chat_template(None).ok();
        let mut prompt = own
            .and_then(|t| model.apply_chat_template(&t, &chat, true).ok())
            .or_else(|| {
                LlamaChatTemplate::new(m.template)
                    .ok()
                    .and_then(|t| model.apply_chat_template(&t, &chat, true).ok())
            })
            .ok_or("gabarit de conversation non reconnu pour ce modèle")?;
        if m.no_think {
            prompt.push_str("<think>\n\n</think>\n\n");
        }
        Ok(prompt)
    }

    /// Charge le modèle s'il ne l'est pas déjà.
    fn ensure_loaded(guard: &mut Option<Loaded>, backend: &LlamaBackend, m: &LocalModel, announce: bool) -> Result<(), String> {
        if guard.as_ref().map(|l| l.id != m.id).unwrap_or(true) {
            *guard = None; // libère l'ancien modèle avant d'en charger un autre
            if announce {
                app().info("chargement du modèle local (quelques secondes)...");
            }
            let model = LlamaModel::load_from_file(backend, path(m), &LlamaModelParams::default())
                .map_err(|e| format!("chargement du modèle impossible : {e}"))?;
            *guard = Some(Loaded { id: m.id.to_string(), model });
        }
        Ok(())
    }

    pub fn preload(m: &LocalModel) -> Result<(), String> {
        let backend = backend()?;
        let mut guard = LOADED.lock().unwrap_or_else(|e| e.into_inner());
        ensure_loaded(&mut guard, backend, m, false)
    }

    pub fn chat(
        cfg: &Config,
        m: &LocalModel,
        messages: &[Value],
        mut on_delta: impl FnMut(&str) -> bool,
    ) -> Result<String, String> {
        let backend = backend()?;
        let mut guard = LOADED.lock().unwrap_or_else(|e| e.into_inner());
        ensure_loaded(&mut guard, backend, m, true)?;
        let model = &guard.as_ref().ok_or("modèle non chargé")?.model;
        let vocab = model.vocab();

        // historique -> texte du gabarit ; on retire les plus anciens tours s'il est trop long
        let mut turns: Vec<(String, String)> = messages
            .iter()
            .map(|v| (v["role"].as_str().unwrap_or("user").to_string(), v["content"].as_str().unwrap_or("").to_string()))
            .collect();
        let mut tokens;
        loop {
            let prompt = prompt_for(model, m, &turns)?;
            tokens = vocab.tokenize(prompt.as_bytes(), false, true);
            if vocab.should_add_bos() && tokens.first() != Some(&vocab.bos()) {
                tokens.insert(0, vocab.bos());
            }
            if tokens.len() + MAX_NEW <= N_CTX as usize || turns.len() <= 2 {
                break;
            }
            match turns.iter().position(|(r, _)| r != "system") {
                Some(i) if turns.len() > 2 => {
                    turns.remove(i);
                }
                _ => break,
            }
        }
        if tokens.is_empty() || tokens.len() + 16 > N_CTX as usize {
            return Err("message trop long pour le modèle local".into());
        }

        let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(6) as i32;
        let params = LlamaContextParams::default()
            .with_n_ctx(NonZeroU32::new(N_CTX))
            .with_n_batch(N_BATCH as u32)
            .with_n_threads(threads)
            .with_n_threads_batch(threads);
        let mut ctx = model.new_context(backend, params).map_err(|e| format!("contexte local : {e}"))?;

        // lecture du prompt par paquets ; seul le dernier jeton demande des probabilités
        let mut batch = LlamaBatch::new(N_BATCH, 1);
        let last = tokens.len() - 1;
        let mut pos = 0usize;
        for chunk in tokens.chunks(N_BATCH) {
            batch.clear();
            for (i, t) in chunk.iter().enumerate() {
                batch.add(*t, (pos + i) as i32, &[0], pos + i == last).map_err(|e| e.to_string())?;
            }
            ctx.decode(&mut batch).map_err(|e| format!("calcul local : {e}"))?;
            pos += chunk.len();
        }

        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(1234);
        let mut sampler = LlamaSampler::chain_simple([
            LlamaSampler::penalties(model.n_vocab(), 64, 1.1, 0.0, 0.0),
            LlamaSampler::top_k(40),
            LlamaSampler::top_p(0.9, 1),
            LlamaSampler::temp(cfg.temperature.max(0.05)),
            LlamaSampler::dist(seed),
        ]);

        let mut out = String::new();
        let mut bytes: Vec<u8> = Vec::new();
        let mut n_cur = tokens.len() as i32;
        for _ in 0..MAX_NEW {
            let tok = sampler.sample(&ctx, batch.n_tokens() - 1);
            if vocab.is_eog(tok) {
                break;
            }
            bytes.extend(vocab.token_to_piece(tok, false, None));
            // n'envoie que des caractères UTF-8 complets
            let valid = match std::str::from_utf8(&bytes) {
                Ok(s) => s.len(),
                Err(e) => e.valid_up_to(),
            };
            if valid > 0 {
                let s = String::from_utf8_lossy(&bytes[..valid]).into_owned();
                bytes.drain(..valid);
                out.push_str(&s);
                if !on_delta(&s) {
                    break;
                }
            }
            batch.clear();
            batch.add(tok, n_cur, &[0], true).map_err(|e| e.to_string())?;
            n_cur += 1;
            ctx.decode(&mut batch).map_err(|e| format!("calcul local : {e}"))?;
        }
        Ok(out.trim().to_string())
    }
}
