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

// ---------- AI Studio : clé API Google (gratuite) ----------

const AISTUDIO_KEYS: &str = "https://aistudio.google.com/apikey";
pub const AISTUDIO_URL: &str = "https://generativelanguage.googleapis.com/v1beta/openai";

pub fn login_aistudio() {
    let a = app();
    GEN.fetch_add(1, Ordering::SeqCst);
    a.println(format!("{MAGENTA}?{RESET} Connecte-toi à Google et crée une clé API gratuite :"));
    a.println(format!("  {CYAN}{AISTUDIO_KEYS}{RESET}"));
    a.command(&format!("open:{AISTUDIO_KEYS}"));
    a.println(format!("{MAGENTA}?{RESET} Colle ici ta clé {GRAY}(commence par AIza, ou 'annuler'){RESET}"));
    *lock(&a.onboarding) = crate::terminal::Onboarding::LoginAiStudio;
}

pub fn finish_aistudio(input: &str) {
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
            .find(|p| p.name == "aistudio")
            .map(|p| p.model.clone())
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| "gemini-flash-latest".into());
        c.providers.retain(|p| p.name != "aistudio");
        c.providers.push(Provider {
            name: "aistudio".into(),
            base_url: AISTUDIO_URL.into(),
            api_key: key,
            model,
            ..Default::default()
        });
        c.provider = "aistudio".into();
    });
    a.send_brain(BrainMsg::Reset);
    a.success("connecté : fournisseur 'aistudio' actif");
    a.info("Liste les modèles avec /models puis /model <id>");
}

// ---------- Gemini : OAuth Google (Antigravity) ----------

pub const GEMINI: &str = "gemini";
pub const GEMINI_HOST: &str = "https://daily-cloudcode-pa.googleapis.com";
pub const GEMINI_MODELS: &[&str] = &["gemini-3.8-flash-low", "gemini-3.8-flash-medium", "gemini-3.8-flash-high", "gemini-3.1-pro-high"];
const GOOGLE_AUTH: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const GOOGLE_TOKEN: &str = "https://oauth2.googleapis.com/token";
const GEMINI_CLIENT: &str = "1071006060591-tmhssin2h21lcre235vtolojh4g403ep.apps.googleusercontent.com";
/// Injecté à la compilation (secret GitHub JL_GEMINI_SECRET), jamais dans le dépôt.
const GEMINI_SECRET: Option<&str> = option_env!("JL_GEMINI_SECRET");
const GEMINI_PORT: u16 = 51121;
const GEMINI_REDIRECT: &str = "http://localhost:51121/oauth-callback";
const GEMINI_SCOPES: &str = "https://www.googleapis.com/auth/cloud-platform https://www.googleapis.com/auth/userinfo.email https://www.googleapis.com/auth/userinfo.profile https://www.googleapis.com/auth/cclog https://www.googleapis.com/auth/experimentsandconfigs";

fn dec(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() => {
                match u8::from_str_radix(&s[i + 1..i + 3], 16) {
                    Ok(v) => {
                        out.push(v);
                        i += 2;
                    }
                    Err(_) => out.push(b'%'),
                }
            }
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn request_id() -> String {
    let mut b = [0u8; 16];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        let _ = f.read_exact(&mut b);
    }
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
}

/// Réessaie tant que le réseau est coupé (app en arrière-plan pendant la connexion).
fn retry_net<T>(mut f: impl FnMut() -> Result<T, String>) -> Result<T, String> {
    let mut n = 0;
    loop {
        match f() {
            Err(e) if e.starts_with("réseau") && n < 30 => {
                n += 1;
                std::thread::sleep(Duration::from_secs(2));
            }
            r => return r,
        }
    }
}

/// Appel de l'API Code Assist (même en-têtes que l'app Antigravity).
pub fn gemini_call(token: &str, path: &str, body: &Value, timeout_s: u64) -> Result<Value, ureq::Error> {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(timeout_s))
        .build()
        .post(&format!("{GEMINI_HOST}{path}"))
        .set("Authorization", &format!("Bearer {token}"))
        .set("Accept", "application/json")
        .set("User-Agent", "antigravity/2.9.1 windows/amd64")
        .set("X-Client-Name", "antigravity")
        .set("X-Client-Version", "2.9.1")
        .set("X-Goog-Api-Client", "google-cloud-sdk vscode_cloudshelleditor/0.1")
        .set("Client-Metadata", r#"{"ideType":"IDE_UNSPECIFIED","platform":"PLATFORM_UNSPECIFIED","pluginType":"GEMINI"}"#)
        .send_json(body.clone())?
        .into_json()
        .map_err(|e| ureq::Error::from(e))
}

fn extract_project(v: &Value) -> Option<String> {
    for k in ["cloudaicompanionProject", "currentCloudaicompanionProject"] {
        match &v[k] {
            Value::String(s) if !s.trim().is_empty() => return Some(s.trim().to_string()),
            Value::Object(o) => {
                if let Some(Value::String(s)) = o.get("id").or_else(|| o.get("projectId")) {
                    if !s.trim().is_empty() {
                        return Some(s.trim().to_string());
                    }
                }
            }
            _ => {}
        }
    }
    None
}

fn resolve_project(token: &str) -> Result<String, String> {
    let meta = json!({"ideType": "IDE_UNSPECIFIED", "platform": "PLATFORM_UNSPECIFIED", "pluginType": "GEMINI"});
    if let Ok(v) = gemini_call(token, "/v1internal:loadCodeAssist", &json!({"metadata": meta}), 30) {
        if let Some(p) = extract_project(&v) {
            return Ok(p);
        }
    }
    app().info("aucun projet trouvé, création (free-tier)...");
    let v = gemini_call(token, "/v1internal:onboardUser", &json!({"tierId": "free-tier", "metadata": meta}), 30).map_err(http_err)?;
    let mut v = v;
    if v["done"] != true {
        if let Some(name) = v["name"].as_str().map(String::from) {
            for _ in 0..12 {
                std::thread::sleep(Duration::from_secs(5));
                if let Ok(p) = gemini_call(token, &format!("/v1internal/{name}"), &json!({}), 30) {
                    if p["done"] == true {
                        v = p;
                        break;
                    }
                }
            }
        }
    }
    extract_project(if v["response"].is_object() { &v["response"] } else { &v }).ok_or_else(|| "projet Google introuvable (compte non éligible ?)".into())
}

pub fn login_gemini() {
    let gen = GEN.fetch_add(1, Ordering::SeqCst) + 1;
    *lock(&PENDING) = None;
    *lock(&app().onboarding) = crate::terminal::Onboarding::None;
    std::thread::spawn(move || {
        if let Err(e) = gemini_flow(gen) {
            app().error(format!("connexion Gemini : {e}"));
        }
    });
}

fn gemini_flow(gen: u64) -> Result<(), String> {
    use std::io::Write;
    use std::net::TcpListener;
    let a = app();
    let secret = GEMINI_SECRET.ok_or("cette version n'embarque pas le secret client (build sans JL_GEMINI_SECRET)")?;
    let listener = TcpListener::bind(("127.0.0.1", GEMINI_PORT))
        .map_err(|_| format!("le port {GEMINI_PORT} est occupé (une autre connexion est en cours ?)"))?;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let state = random_b64(16);
    let url = format!(
        "{GOOGLE_AUTH}?client_id={GEMINI_CLIENT}&redirect_uri={}&response_type=code&scope={}&state={}&access_type=offline&prompt=consent",
        enc(GEMINI_REDIRECT),
        enc(GEMINI_SCOPES),
        enc(&state)
    );
    a.println(format!("{MAGENTA}?{RESET} Connecte-toi à Google dans le navigateur :"));
    a.println(format!("  {CYAN}{url}{RESET}"));
    a.command(&format!("open:{url}"));
    a.info("j'attends ta validation...");

    let deadline = now() + 5 * 60;
    let code = loop {
        if GEN.load(Ordering::SeqCst) != gen {
            return Ok(());
        }
        if now() > deadline {
            return Err("délai dépassé (5 min), relance /login gemini".into());
        }
        let mut stream = match listener.accept() {
            Ok((s, _)) => s,
            Err(_) => {
                std::thread::sleep(Duration::from_millis(250));
                continue;
            }
        };
        let _ = stream.set_nonblocking(false);
        let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
        let mut buf = [0u8; 8192];
        let n = stream.read(&mut buf).unwrap_or(0);
        let req = String::from_utf8_lossy(&buf[..n]).into_owned();
        let target = req.lines().next().and_then(|l| l.split_whitespace().nth(1)).unwrap_or("");
        let (path, query) = target.split_once('?').unwrap_or((target, ""));
        let mut reply = |status: &str, body: &str| {
            let _ = write!(stream, "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
        };
        if path != "/oauth-callback" {
            reply("404 Not Found", "");
            continue;
        }
        let get = |k: &str| query.split('&').filter_map(|kv| kv.split_once('=')).find(|(key, _)| *key == k).map(|(_, v)| dec(v));
        if let Some(err) = get("error") {
            reply("200 OK", "<h1>Connexion refusée</h1>");
            return Err(format!("Google a refusé : {err}"));
        }
        if get("state").as_deref() != Some(state.as_str()) {
            reply("400 Bad Request", "<h1>État invalide</h1>");
            continue;
        }
        match get("code") {
            Some(c) => {
                reply("200 OK", "<h1>Connexion r&eacute;ussie !</h1><p>Tu peux revenir dans Jladios.</p>");
                break c;
            }
            None => reply("400 Bad Request", ""),
        }
    };
    drop(listener);
    a.info("code reçu, finalisation...");
    let resp = retry_net(|| {
        token_request(
            GOOGLE_TOKEN,
            &[
                ("client_id", GEMINI_CLIENT),
                ("client_secret", secret),
                ("code", &code),
                ("grant_type", "authorization_code"),
                ("redirect_uri", GEMINI_REDIRECT),
            ],
        )
    })?;
    let t = parse_tokens(&resp, "")?;
    let project = retry_net(|| resolve_project(&t.access))?;
    store(GEMINI, GEMINI_HOST, GEMINI_MODELS[0], t, project);
    Ok(())
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
    let (url, client) = match ep.kind.as_str() {
        CLAUDE => (CLAUDE_TOKEN.to_string(), CLAUDE_CLIENT),
        GEMINI => (GOOGLE_TOKEN.to_string(), GEMINI_CLIENT),
        _ => (format!("{OPENAI}/oauth/token"), CODEX_CLIENT),
    };
    let mut fields = vec![("grant_type", "refresh_token"), ("refresh_token", ep.refresh_token.as_str()), ("client_id", client)];
    if ep.kind == GEMINI {
        fields.push(("client_secret", GEMINI_SECRET.ok_or("secret client absent de cette version")?));
    }
    let v = token_request(&url, &fields)
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
    fn url_decoding() {
        assert_eq!(dec("a%20b+c%3A"), "a b c:");
        assert_eq!(dec("100%"), "100%");
    }

    #[test]
    fn url_encoding() {
        assert_eq!(enc("user:profile user:inference"), "user%3Aprofile%20user%3Ainference");
    }
}
