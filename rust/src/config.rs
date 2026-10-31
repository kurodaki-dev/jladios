//! Configuration persistante (JSON dans le dossier privé de l'app).

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

pub const DEFAULT_MODEL: &str = "nvidia/nemotron-3.5-lightning-30b-a3b";
/// Kokoro v1.0 : 30 = ff_siwis (voix française), 3 = af_heart (anglais).
pub const VOICE_FR: i32 = 30;
pub const VOICE_EN: i32 = 3;

/// Fournisseur de texte compatible OpenAI (`{base_url}/chat/completions`).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Provider {
    pub name: String,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    /// "" = compatible OpenAI ; "claude" / "codex" = abonnement connecté par /login
    /// (`api_key` contient alors le jeton d'accès).
    pub kind: String,
    pub refresh_token: String,
    pub expires_at: u64,
    pub account_id: String,
}

/// Fournisseur actif, prêt à l'emploi.
pub struct Endpoint {
    pub name: String,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub builtin: bool,
    pub kind: String,
    pub refresh_token: String,
    pub expires_at: u64,
    pub account_id: String,
}

/// Nom du fournisseur intégré (clé et modèle = `api_key` / `model` de la config).
pub const NVIDIA: &str = "nvidia";
pub const NVIDIA_URL: &str = "https://integrate.api.nvidia.com/v1";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub assistant_name: String,
    pub user_name: String,
    pub api_key: String,
    pub model: String,
    /// Fournisseur actif : "nvidia" (intégré) ou le nom d'un élément de `providers`.
    pub provider: String,
    pub providers: Vec<Provider>,
    pub lang: String,
    pub voice_sid: i32,
    /// Voix du catalogue `/tts` (id de `models::VOICES`)
    pub tts_model: String,
    /// Voix utilisée quand la langue est « en »
    pub tts_model_en: String,
    pub speed: f32,
    pub temperature: f32,
    pub auto_listen: bool,
    pub mute: bool,
    pub onboarded: bool,
    /// /unlock : l'IA peut contrôler le téléphone (service d'accessibilité)
    pub unlocked: bool,
    /// /memory on : historique conservé + faits retenus
    pub memory: bool,
    pub facts: Vec<String>,
    pub extra_prompt: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            assistant_name: "Pixel".into(),
            user_name: "ami".into(),
            api_key: String::new(),
            model: DEFAULT_MODEL.into(),
            provider: NVIDIA.into(),
            providers: Vec::new(),
            lang: "fr".into(),
            voice_sid: VOICE_FR,
            tts_model: "kokoro".into(),
            tts_model_en: "lessac".into(),
            speed: 1.0,
            temperature: 0.6,
            auto_listen: false,
            mute: false,
            onboarded: false,
            unlocked: false,
            memory: false,
            facts: Vec::new(),
            extra_prompt: String::new(),
        }
    }
}

impl Config {
    pub fn load(path: &Path) -> Self {
        fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) {
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let tmp = path.with_extension("tmp");
            if fs::write(&tmp, s).is_ok() {
                let _ = fs::rename(&tmp, path);
            }
        }
    }

    /// Fournisseur actif (retombe sur NVIDIA si le nom n'existe plus).
    pub fn active(&self) -> Endpoint {
        if let Some(p) = self.providers.iter().find(|p| p.name == self.provider) {
            return Endpoint {
                name: p.name.clone(),
                base_url: p.base_url.clone(),
                api_key: p.api_key.clone(),
                model: p.model.clone(),
                builtin: false,
                kind: p.kind.clone(),
                refresh_token: p.refresh_token.clone(),
                expires_at: p.expires_at,
                account_id: p.account_id.clone(),
            };
        }
        Endpoint {
            name: NVIDIA.into(),
            base_url: NVIDIA_URL.into(),
            api_key: self.api_key.clone(),
            model: self.model.clone(),
            builtin: true,
            kind: String::new(),
            refresh_token: String::new(),
            expires_at: 0,
            account_id: String::new(),
        }
    }

    /// Change le modèle du fournisseur actif.
    pub fn set_model(&mut self, m: String) {
        match self.providers.iter_mut().find(|p| p.name == self.provider) {
            Some(p) => p.model = m,
            None => self.model = m,
        }
    }

    /// Change la clé API du fournisseur actif.
    pub fn set_key(&mut self, k: String) {
        match self.providers.iter_mut().find(|p| p.name == self.provider) {
            Some(p) => p.api_key = k,
            None => self.api_key = k,
        }
    }

    pub fn is_fr(&self) -> bool {
        self.lang != "en"
    }

    /// Prompt système envoyé au LLM.
    pub fn system_prompt(&self) -> String {
        let a = &self.assistant_name;
        let u = &self.user_name;
        let mut p = if self.is_fr() {
            format!(
                "Tu es {a}, un petit assistant vocal en forme de personnage qui vit sur l'écran \
                 du téléphone de {u}. Tu parles à {u} de façon chaleureuse, naturelle et concise \
                 (1 à 3 phrases courtes, sauf si on te demande plus de détails). Tes réponses sont \
                 lues à voix haute : n'utilise jamais de markdown, de listes, d'emojis ni de \
                 symboles spéciaux. Réponds toujours en français."
            )
        } else {
            format!(
                "You are {a}, a small voice assistant character living on {u}'s phone screen. \
                 Talk to {u} in a warm, natural and concise way (1 to 3 short sentences unless \
                 asked for more). Your answers are read aloud: never use markdown, lists, emojis \
                 or special symbols. Always answer in English."
            )
        };
        if !self.extra_prompt.trim().is_empty() {
            p.push('\n');
            p.push_str(self.extra_prompt.trim());
        }
        p
    }
}
