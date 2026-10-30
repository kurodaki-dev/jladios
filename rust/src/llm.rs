//! Génération de texte via l'API NVIDIA (compatible OpenAI, streaming SSE).

use crate::config::{Config, Endpoint};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader};
use std::time::Duration;

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(120))
        .user_agent("jladios/0.1")
        .build()
}

fn short(s: &str) -> String {
    let s = s.trim();
    if s.chars().count() > 300 {
        format!("{}…", s.chars().take(300).collect::<String>())
    } else {
        s.to_string()
    }
}

fn map_err(e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(401, _) | ureq::Error::Status(403, _) => {
            "accès refusé : clé API invalide, ou session expirée (refais /login)".into()
        }
        ureq::Error::Status(404, r) => format!(
            "modèle introuvable (vérifie /model) : {}",
            short(&r.into_string().unwrap_or_default())
        ),
        ureq::Error::Status(code, r) => {
            format!("HTTP {code} : {}", short(&r.into_string().unwrap_or_default()))
        }
        e => format!("réseau : {e}"),
    }
}

/// Retire les blocs de raisonnement `<think>...</think>`.
/// Si `final_` est faux, on retient aussi un éventuel début de balise en fin
/// de texte (pour ne pas l'afficher avant de savoir).
fn visible(raw: &str, final_: bool) -> String {
    const OPEN: &str = "<think>";
    const CLOSE: &str = "</think>";
    let mut out = String::new();
    let mut rest = raw;
    loop {
        match rest.find(OPEN) {
            Some(i) => {
                out.push_str(&rest[..i]);
                let after = &rest[i + OPEN.len()..];
                match after.find(CLOSE) {
                    Some(j) => rest = &after[j + CLOSE.len()..],
                    None => break,
                }
            }
            None => {
                out.push_str(rest);
                break;
            }
        }
    }
    if let Some(j) = out.find(CLOSE) {
        out = out[j + CLOSE.len()..].to_string();
    }
    if !final_ {
        for k in (1..OPEN.len()).rev() {
            if out.ends_with(&OPEN[..k]) {
                out.truncate(out.len() - k);
                break;
            }
        }
    }
    out.trim_start().to_string()
}

/// Envoie la conversation et appelle `on_delta` pour chaque morceau de texte
/// visible reçu. `on_delta` renvoie `false` pour interrompre.
pub fn chat_stream(
    cfg: &Config,
    ep: &Endpoint,
    messages: &[Value],
    mut on_delta: impl FnMut(&str) -> bool,
) -> Result<String, String> {
    match ep.kind.as_str() {
        crate::auth::CLAUDE => return claude_stream(cfg, ep, messages, on_delta),
        crate::auth::CODEX => return codex_stream(ep, messages, on_delta),
        _ => {}
    }
    let mut body = json!({
        "model": ep.model,
        "messages": messages,
        "temperature": cfg.temperature,
        "top_p": 0.95,
        "max_tokens": 1024,
        "stream": true,
        // Pas de raisonnement : sinon le modèle "réfléchit" en silence pendant
        // de longues secondes avant d'écrire le premier mot de sa réponse.
        "chat_template_kwargs": {"enable_thinking": false},
    });
    // Ce paramètre est spécifique aux modèles NVIDIA/vLLM.
    if !ep.builtin {
        body.as_object_mut().unwrap().remove("chat_template_kwargs");
    }
    let send = |body: &Value| {
        let mut req = agent()
            .post(&format!("{}/chat/completions", ep.base_url.trim_end_matches('/')))
            .set("Accept", "text/event-stream");
        // Les serveurs locaux n'ont pas forcément de clé.
        if !ep.api_key.trim().is_empty() {
            req = req.set("Authorization", &format!("Bearer {}", ep.api_key.trim()));
        }
        req.send_json(body)
    };
    let resp = match send(&body) {
        // Certains modèles refusent ce paramètre : on réessaie sans.
        Err(ureq::Error::Status(400 | 422, _)) => {
            body.as_object_mut().unwrap().remove("chat_template_kwargs");
            send(&body)
        }
        r => r,
    }
    .map_err(map_err)?;

    let reader = BufReader::new(resp.into_reader());
    let mut raw = String::new();
    let mut shown = 0usize;
    for line in reader.lines() {
        let line = line.map_err(|e| format!("lecture : {e}"))?;
        let line = line.trim();
        let Some(data) = line.strip_prefix("data:") else { continue };
        let data = data.trim();
        if data == "[DONE]" {
            break;
        }
        let Ok(v) = serde_json::from_str::<Value>(data) else { continue };
        if let Some(err) = v.get("error") {
            return Err(short(&err.to_string()));
        }
        if let Some(d) = v["choices"][0]["delta"]["content"].as_str() {
            raw.push_str(d);
            let vis = visible(&raw, false);
            if vis.len() > shown && vis.is_char_boundary(shown) {
                if !on_delta(&vis[shown..]) {
                    return Ok(vis);
                }
                shown = vis.len();
            }
        }
    }
    let vis = visible(&raw, true);
    if vis.len() > shown && vis.is_char_boundary(shown) {
        on_delta(&vis[shown..]);
    }
    Ok(vis)
}

/// Lit un flux SSE : `pick` renvoie `Ok(Some(texte))` pour un morceau de réponse.
fn read_sse(
    resp: ureq::Response,
    pick: impl Fn(&Value) -> Result<Option<String>, String>,
    mut on_delta: impl FnMut(&str) -> bool,
) -> Result<String, String> {
    let mut full = String::new();
    for line in BufReader::new(resp.into_reader()).lines() {
        let line = line.map_err(|e| format!("lecture : {e}"))?;
        let Some(data) = line.trim().strip_prefix("data:") else { continue };
        let Ok(v) = serde_json::from_str::<Value>(data.trim()) else { continue };
        if let Some(t) = pick(&v)? {
            full.push_str(&t);
            if !on_delta(&t) {
                break;
            }
        }
    }
    Ok(full.trim().to_string())
}

fn split_system(messages: &[Value]) -> (String, Vec<&Value>) {
    let system = messages
        .iter()
        .filter(|m| m["role"] == "system")
        .filter_map(|m| m["content"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    (system, messages.iter().filter(|m| m["role"] != "system").collect())
}

/// Abonnement Claude (API Messages, jeton OAuth).
fn claude_stream(
    cfg: &Config,
    ep: &Endpoint,
    messages: &[Value],
    on_delta: impl FnMut(&str) -> bool,
) -> Result<String, String> {
    let (system, turns) = split_system(messages);
    // Les jetons d'abonnement exigent cette première consigne système.
    let body = json!({
        "model": ep.model,
        "max_tokens": 1024,
        "temperature": cfg.temperature,
        "stream": true,
        "system": [
            {"type": "text", "text": "You are Claude Code, Anthropic's official CLI for Claude."},
            {"type": "text", "text": system},
        ],
        "messages": turns,
    });
    let resp = agent()
        .post(&format!("{}/v1/messages", ep.base_url.trim_end_matches('/')))
        .set("Authorization", &format!("Bearer {}", ep.api_key.trim()))
        .set("anthropic-version", "2023-06-01")
        .set("anthropic-beta", "oauth-2025-04-20")
        .set("Accept", "text/event-stream")
        .send_json(body)
        .map_err(map_err)?;
    read_sse(
        resp,
        |v| match v["type"].as_str() {
            Some("content_block_delta") => Ok(v["delta"]["text"].as_str().map(String::from)),
            Some("error") => Err(short(&v["error"].to_string())),
            _ => Ok(None),
        },
        on_delta,
    )
}

/// Abonnement ChatGPT (API Responses de Codex, jeton OAuth).
fn codex_stream(
    ep: &Endpoint,
    messages: &[Value],
    on_delta: impl FnMut(&str) -> bool,
) -> Result<String, String> {
    let (system, turns) = split_system(messages);
    let input: Vec<Value> = turns
        .iter()
        .map(|m| {
            let kind = if m["role"] == "assistant" { "output_text" } else { "input_text" };
            json!({"role": m["role"], "content": [{"type": kind, "text": m["content"]}]})
        })
        .collect();
    let body = json!({
        "model": ep.model,
        "instructions": if system.is_empty() { "You are a helpful assistant.".to_string() } else { system },
        "input": input,
        "store": false,
        "stream": true,
    });
    let resp = agent()
        .post(&format!("{}/codex/responses", ep.base_url.trim_end_matches('/')))
        .set("Authorization", &format!("Bearer {}", ep.api_key.trim()))
        .set("chatgpt-account-id", &ep.account_id)
        .set("originator", "codex_cli_rs")
        .set("OpenAI-Beta", "responses=experimental")
        .set("Accept", "text/event-stream")
        .send_json(body)
        .map_err(map_err)?;
    read_sse(
        resp,
        |v| match v["type"].as_str() {
            Some("response.output_text.delta") => Ok(v["delta"].as_str().map(String::from)),
            Some("error") => Err(short(v["message"].as_str().unwrap_or("erreur Codex"))),
            Some("response.failed") => Err(short(&v["response"]["error"].to_string())),
            _ => Ok(None),
        },
        on_delta,
    )
}

/// Liste les modèles de chat disponibles sur l'API NVIDIA.
pub fn list_models(ep: &Endpoint) -> Result<Vec<String>, String> {
    match ep.kind.as_str() {
        crate::auth::CLAUDE => return Ok(crate::auth::CLAUDE_MODELS.iter().map(|s| s.to_string()).collect()),
        crate::auth::CODEX => return Ok(crate::auth::CODEX_MODELS.iter().map(|s| s.to_string()).collect()),
        _ => {}
    }
    let mut req = agent().get(&format!("{}/models", ep.base_url.trim_end_matches('/')));
    if !ep.api_key.trim().is_empty() {
        req = req.set("Authorization", &format!("Bearer {}", ep.api_key.trim()));
    }
    let v: Value = req
        .call()
        .map_err(map_err)?
        .into_json()
        .map_err(|e| e.to_string())?;
    let skip = ["embed", "rerank", "reward", "safety", "guard", "parse", "clip", "retriever", "nv-dino", "vila", "deplot", "kosmos", "paligemma"];
    let mut ids: Vec<String> = v["data"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|m| m["id"].as_str().map(String::from))
                .filter(|id| !skip.iter().any(|k| id.contains(k)))
                .collect()
        })
        .unwrap_or_default();
    ids.sort();
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::visible;

    #[test]
    fn think_is_hidden() {
        assert_eq!(visible("<think>hmm</think> Salut", true), "Salut");
        assert_eq!(visible("Salut <thi", false), "Salut ");
        assert_eq!(visible("<think>en cours", false), "");
        assert_eq!(visible("raisonnement</think>Bonjour", true), "Bonjour");
    }
}
