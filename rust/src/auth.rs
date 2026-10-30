//! Connexion OAuth aux abonnements ChatGPT (Codex) et Claude : `/login`.
//!
//! Les jetons sont stockés dans la config privée de l'app (comme une clé API)
//! et renouvelés automatiquement avant chaque requête.

use crate::ansi::*;
use crate::app::{app, lock, BrainMsg};
use crate::config::{Endpoint, Provider};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD as B64, Engine};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const CLAUDE: &str = "claude";
pub const CODEX: &str = "codex";
pub const CLAUDE_URL: &str = "https://api.anthropic.com";
pub const CODEX_URL: &str = "https://chatgpt.com/backend-api";
pub const CLAUDE_MODELS: &[&str] = &["claude-sonnet-5-5", "claude-opus-5-5", "claude-haiku-5-5"];
pub const CODEX_MODELS: &[&str] = &["gpt-5.5", "gpt-5.4", "gpt-5.4-mini", "gpt-5.3-codex"];

const CODEX_CLIENT: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const OPENAI: &str = "https://auth.openai.com";

const CLAUDE_CLIENT: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
const CLAUDE_AUTHORIZE: &str = "https://claude.ai/oauth/authorize";
const CLAUDE_TOKEN: &str = "https://platform.claude.com/v1/oauth/token";
const CLAUDE_REDIRECT: &str = "https://platform.claude.com/oauth/code/callback";
const CLAUDE_SCOPE: &str = "user:profile user:inference";

/// Incrémenté à chaque `/login` : une ancienne tentative en cours s'arrête.
static GEN: AtomicU64 = AtomicU64::new(0);
/// (code_verifier, state) du `/login claude` en attente du code collé.
static PENDING: Mutex<Option<(String, String)>> = Mutex::new(None);

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(30))
        .user_agent("jladios/0.1")
        .build()
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn short(s: &str) -> String {
    let s = s.trim();
    if s.chars().count() > 300 {
        format!("{}…", s.chars().take(300).collect::<String>())
    } else {
        s.to_string()
    }
}

fn http_err(e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(c, r) => format!("HTTP {c} : {}", short(&r.into_string().unwrap_or_default())),
        e => format!("réseau : {e}"),
    }
}

fn enc(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn random_b64(n: usize) -> String {
    let mut buf = vec![0u8; n];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        let _ = f.read_exact(&mut buf);
    }
    B64.encode(buf)
}

fn challenge(verifier: &str) -> String {
    B64.encode(Sha256::digest(verifier.as_bytes()))
}

struct Tokens {
    access: String,
    refresh: String,
    id_token: Option<String>,
    expires_at: u64,
}

fn parse_tokens(v: &Value, old_refresh: &str) -> Result<Tokens, String> {
    let access = v["access_token"].as_str().ok_or("réponse sans access_token")?.to_string();
    let refresh = v["refresh_token"].as_str().unwrap_or(old_refresh).to_string();
    let ttl = v["expires_in"].as_u64().unwrap_or(3600);
    Ok(Tokens {
        access,
        refresh,
        id_token: v["id_token"].as_str().map(String::from),
        expires_at: now() + ttl,
    })
}

/// POST du endpoint de jetons : formulaire d'abord, JSON si le serveur refuse.
fn token_request(url: &str, fields: &[(&str, &str)]) -> Result<Value, String> {
    match agent().post(url).send_form(fields) {
        Ok(r) => r.into_json().map_err(|e| e.to_string()),
        Err(first @ ureq::Error::Status(400 | 415 | 422, _)) => {
            let body: serde_json::Map<String, Value> =
                fields.iter().map(|(k, v)| (k.to_string(), json!(v))).collect();
            match agent().post(url).send_json(Value::Object(body)) {
                Ok(r) => r.into_json().map_err(|e| e.to_string()),
                Err(_) => Err(http_err(first)),
            }
        }
        Err(e) => Err(http_err(e)),
    }
}

/// Identifiant de compte ChatGPT contenu dans le JWT.
fn account_id(jwt: &str) -> Option<String> {
    let payload = jwt.split('.').nth(1)?;
    let bytes = B64.decode(payload.trim_end_matches('=')).ok()?;
    let v: Value = serde_json::from_slice(&bytes).ok()?;
    v["https://api.openai.com/auth"]["chatgpt_account_id"]
        .as_str()
        .or_else(|| v["chatgpt_account_id"].as_str())
        .or_else(|| v["organizations"][0]["id"].as_str())
        .map(String::from)
}

fn store(name: &str, base_url: &str, default_model: &str, t: Tokens, account: String) {
    let a = app();
    let (n, url, dm) = (name.to_string(), base_url.to_string(), default_model.to_string());
    a.update_cfg(|c| {
        let model = c
            .providers
            .iter()
            .find(|p| p.name == n)
            .map(|p| p.model.clone())
            .filter(|m| !m.is_empty())
            .unwrap_or(dm);
        c.providers.retain(|p| p.name != n);
        c.providers.push(Provider {
            name: n.clone(),
            kind: n.clone(),
            base_url: url,
            api_key: t.access,
            model,
            refresh_token: t.refresh,
            expires_at: t.expires_at,
            account_id: account,
        });
        c.provider = n;
    });
    a.send_brain(BrainMsg::Reset);
    let model = a.cfg().active().model;
    a.success(format!("connecté : fournisseur '{name}' actif, modèle {model}"));
    a.info("Change de modèle avec /models puis /model <id>");
}

// ---------- Codex / ChatGPT : code d'appareil ----------

pub fn login_codex() {
    let gen = GEN.fetch_add(1, Ordering::SeqCst) + 1;
    *lock(&PENDING) = None;
    *lock(&app().onboarding) = crate::terminal::Onboarding::None;
    std::thread::spawn(move || {
        let a = app();
        if let Err(e) = codex_flow(gen) {
            a.error(format!("connexion Codex : {e}"));
        }
    });
}

fn codex_flow(gen: u64) -> Result<(), String> {
    let a = app();
    let v: Value = agent()
        .post(&format!("{OPENAI}/api/accounts/deviceauth/usercode"))
        .send_json(json!({"client_id": CODEX_CLIENT}))
        .map_err(http_err)?
        .into_json()
        .map_err(|e| e.to_string())?;
    let id = v["device_auth_id"].as_str().ok_or("réponse sans device_auth_id")?.to_string();
    let code = v["user_code"].as_str().or_else(|| v["usercode"].as_str()).ok_or("réponse sans user_code")?.to_string();
    let interval = v["interval"]
        .as_u64()
        .or_else(|| v["interval"].as_str().and_then(|s| s.parse().ok()))
        .unwrap_or(5)
        .clamp(2, 30);
    let url = format!("{OPENAI}/codex/device");
    a.println(format!("{MAGENTA}?{RESET} Ouvre {CYAN}{url}{RESET}"));
    a.println(format!("  et entre le code : {YELLOW}{BOLD}{code}{RESET} {GRAY}(valable 15 min){RESET}"));
    a.command(&format!("open:{url}"));
    a.info("j'attends ta validation...");

    let deadline = now() + 15 * 60;
    let poll = loop {
        if GEN.load(Ordering::SeqCst) != gen {
            return Ok(()); // remplacé par un autre /login
        }
        if now() > deadline {
            return Err("délai dépassé (15 min), relance /login codex".into());
        }
        std::thread::sleep(Duration::from_secs(interval));
        match agent()
            .post(&format!("{OPENAI}/api/accounts/deviceauth/token"))
            .send_json(json!({"device_auth_id": id, "user_code": code}))
        {
            Ok(r) => break r.into_json::<Value>().map_err(|e| e.to_string())?,
            Err(ureq::Error::Status(403 | 404, _)) => continue,
            // Réseau coupé (app passée en arrière-plan pendant que tu valides) : on réessaie.
            Err(ureq::Error::Transport(_)) => continue,
            Err(e) => return Err(http_err(e)),
        }
    };
    let auth_code = poll["authorization_code"].as_str().ok_or("réponse sans authorization_code")?;
    let verifier = poll["code_verifier"].as_str().ok_or("réponse sans code_verifier")?;
    let redirect = format!("{OPENAI}/deviceauth/callback");
    let resp = token_request(
        &format!("{OPENAI}/oauth/token"),
        &[
            ("grant_type", "authorization_code"),
            ("code", auth_code),
            ("redirect_uri", &redirect),
            ("client_id", CODEX_CLIENT),
            ("code_verifier", verifier),
        ],
    )?;
    let t = parse_tokens(&resp, "")?;
    let account = t
        .id_token
        .as_deref()
        .and_then(account_id)
        .or_else(|| account_id(&t.access))
        .ok_or("identifiant de compte ChatGPT introuvable dans le jeton")?;
    store(CODEX, CODEX_URL, CODEX_MODELS[0], t, account);
    Ok(())
}

// ---------- Claude : PKCE, code collé à la main ----------

pub fn login_claude() {
    let a = app();
    GEN.fetch_add(1, Ordering::SeqCst);
    let verifier = random_b64(32);
    let state = random_b64(32);
    let url = format!(
        "{CLAUDE_AUTHORIZE}?code=true&client_id={CLAUDE_CLIENT}&response_type=code&redirect_uri={}&scope={}&code_challenge={}&code_challenge_method=S256&state={}",
        enc(CLAUDE_REDIRECT),
        enc(CLAUDE_SCOPE),
        challenge(&verifier),
        state
    );
    *lock(&PENDING) = Some((verifier, state));
    a.println(format!("{MAGENTA}?{RESET} Connecte-toi à Claude dans le navigateur :"));
    a.println(format!("  {CYAN}{url}{RESET}"));
    a.command(&format!("open:{url}"));
    a.println(format!(
        "{MAGENTA}?{RESET} Colle ici le code affiché ensuite {GRAY}(ou 'annuler'){RESET}"
    ));
    *lock(&a.onboarding) = crate::terminal::Onboarding::LoginClaude;
}

/// Le code collé par l'utilisateur (`code#state`).
pub fn finish_claude(input: &str) {
    let a = app();
    *lock(&a.onboarding) = crate::terminal::Onboarding::None;
    let l = input.to_lowercase();
    if l == "annuler" || l == "cancel" || l.is_empty() {
        *lock(&PENDING) = None;
        a.info("connexion annulée");
        return;
    }
    let Some((verifier, state)) = lock(&PENDING).take() else {
        a.error("aucune connexion en cours, relance /login claude");
        return;
    };
    let input = input.to_string();
    std::thread::spawn(move || {
        let a = app();
        let (code, got_state) = input.split_once('#').unwrap_or((input.as_str(), state.as_str()));
        if got_state != state {
            a.error("le code ne correspond pas à cette connexion, relance /login claude");
            return;
        }
        let res = token_request(
            CLAUDE_TOKEN,
            &[
                ("grant_type", "authorization_code"),
                ("code", code.trim()),
                ("state", &state),
                ("client_id", CLAUDE_CLIENT),
                ("redirect_uri", CLAUDE_REDIRECT),
                ("code_verifier", &verifier),
            ],
        )
        .and_then(|v| parse_tokens(&v, ""));
        match res {
            Ok(t) => store(CLAUDE, CLAUDE_URL, CLAUDE_MODELS[0], t, String::new()),
            Err(e) => a.error(format!("connexion Claude : {e}")),
        }
    });
}

// ---------- Gemini : clé API Google AI Studio (gratuite) ----------

const GEMINI_KEYS: &str = "https://aistudio.google.com/apikey";
pub const GEMINI_URL: &str = "https://generativelanguage.googleapis.com/v1beta/openai";

pub fn login_gemini() {
    let a = app();
    GEN.fetch_add(1, Ordering::SeqCst);
    a.println(format!("{MAGENTA}?{RESET} Connecte-toi à Google et crée une clé API gratuite :"));
    a.println(format!("  {CYAN}{GEMINI_KEYS}{RESET}"));
    a.command(&format!("open:{GEMINI_KEYS}"));
    a.println(format!("{MAGENTA}?{RESET} Colle ici ta clé {GRAY}(commence par AIza, ou 'annuler'){RESET}"));
    *lock(&a.onboarding) = crate::terminal::Onboarding::LoginGemini;
}

pub fn finish_gemini(input: &str) {
    let a = app();
    *lock(&a.onboarding) = crate::terminal::Onboarding::None;
    let key = input.trim().to_string();
    if key.is_empty() || key.eq_ignore_ascii_case("annuler") || key.eq_ignore_ascii_case("cancel") {
        a.info("connexion annulée");
        return;
    }
    if !key.starts_with("AIza") {
        a.println(format!("{YELLOW}[?]{RESET} La clé ne commence pas par 'AIza', je l'enregistre quand même."));
    }
    a.update_cfg(|c| {
        let model = c
            .providers
            .iter()
            .find(|p| p.name == "gemini")
            .map(|p| p.model.clone())
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| "gemini-flash-latest".into());
        c.providers.retain(|p| p.name != "gemini");
        c.providers.push(Provider {
            name: "gemini".into(),
            base_url: GEMINI_URL.into(),
            api_key: key,
            model,
            ..Default::default()
        });
        c.provider = "gemini".into();
    });
    a.send_brain(BrainMsg::Reset);
    a.success("connecté : fournisseur 'gemini' actif");
    a.info("Liste les modèles avec /models puis /model <id>");
}

// ---------- renouvellement ----------

/// Renvoie un `Endpoint` dont le jeton est valide (le renouvelle si besoin).
pub fn fresh(ep: Endpoint) -> Result<Endpoint, String> {
    if ep.kind.is_empty() || ep.expires_at > now() + 120 {
        return Ok(ep);
    }
    if ep.refresh_token.is_empty() {
        return Err(format!("session expirée : refais /login {}", ep.kind));
    }
    let (url, client) = if ep.kind == CLAUDE { (CLAUDE_TOKEN.to_string(), CLAUDE_CLIENT) } else { (format!("{OPENAI}/oauth/token"), CODEX_CLIENT) };
    let v = token_request(
        &url,
        &[("grant_type", "refresh_token"), ("refresh_token", &ep.refresh_token), ("client_id", client)],
    )
    .map_err(|e| format!("renouvellement du jeton impossible ({e}) : refais /login {}", ep.kind))?;
    let t = parse_tokens(&v, &ep.refresh_token)?;
    let account = t.id_token.as_deref().and_then(account_id).unwrap_or_else(|| ep.account_id.clone());
    let mut ep = ep;
    ep.api_key = t.access.clone();
    ep.refresh_token = t.refresh.clone();
    ep.expires_at = t.expires_at;
    ep.account_id = account.clone();
    let name = ep.name.clone();
    app().update_cfg(|c| {
        if let Some(p) = c.providers.iter_mut().find(|p| p.name == name) {
            p.api_key = t.access;
            p.refresh_token = t.refresh;
            p.expires_at = t.expires_at;
            p.account_id = account;
        }
    });
    Ok(ep)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_rfc7636_vector() {
        assert_eq!(
            challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn jwt_account() {
        let payload = B64.encode(r#"{"https://api.openai.com/auth":{"chatgpt_account_id":"acc-1"}}"#);
        assert_eq!(account_id(&format!("h.{payload}.s")), Some("acc-1".into()));
    }

    #[test]
    fn url_encoding() {
        assert_eq!(enc("user:profile user:inference"), "user%3Aprofile%20user%3Ainference");
    }
}
