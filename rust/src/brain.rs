//! Le "cerveau" : reçoit ce que dit/tape l'utilisateur, interroge le LLM en
//! streaming, affiche la réponse dans le terminal et l'envoie phrase par
//! phrase au TTS pour que le personnage commence à parler au plus vite.

use crate::ansi::*;
use crate::agent;
use crate::app::{app, lock, BrainMsg, TtsMsg};
use crate::llm;
use serde_json::{json, Value};
use std::sync::atomic::Ordering;
use std::sync::mpsc::Receiver;

const MAX_HISTORY: usize = 20;

pub fn spawn(rx: Receiver<BrainMsg>) {
    std::thread::Builder::new()
        .name("brain".into())
        .spawn(move || {
            for msg in rx {
                match msg {
                    BrainMsg::User { text, voice } => respond(text, voice),
                    BrainMsg::Reset => lock(&app().history).clear(),
                }
            }
        })
        .expect("spawn brain");
}

/// Renvoie la position (en octets) juste après la première fin de phrase.
fn sentence_end(buf: &str) -> Option<usize> {
    let mut chars = buf.char_indices().peekable();
    let mut count = 0;
    while let Some((i, c)) = chars.next() {
        count += 1;
        let end = i + c.len_utf8();
        let next_ws = chars.peek().map(|(_, n)| n.is_whitespace()).unwrap_or(false);
        if c == '\n' && count > 1 {
            return Some(end);
        }
        if matches!(c, '.' | '!' | '?' | '…' | ';' | ':') && next_ws && count >= 12 {
            return Some(end);
        }
        if c == ',' && next_ws && count >= 120 {
            return Some(end);
        }
    }
    None
}

fn tail(s: &str, n: usize) -> String {
    let count = s.chars().count();
    if count <= n {
        s.trim().to_string()
    } else {
        format!("…{}", s.chars().skip(count - n).collect::<String>().trim())
    }
}

/// Nombre maximal d'allers-retours action -> résultat pour une seule demande.
const MAX_STEPS: usize = 8;

fn respond(text: String, voice: bool) {
    let a = app();
    let cfg = a.cfg();
    if voice {
        a.println(format!(
            "{CYAN}{BOLD}{}{RESET}{GRAY} (voix){RESET}{CYAN}>{RESET} {}",
            cfg.user_name, text
        ));
    }
    let ep = match crate::auth::fresh(cfg.active()) {
        Ok(ep) => ep,
        Err(e) => {
            a.error(e);
            return;
        }
    };
    // Seul le fournisseur intégré exige une clé ; un serveur local peut s'en passer.
    if ep.builtin && ep.api_key.trim().is_empty() {
        a.error("Aucune clé API NVIDIA. Ajoute-la avec : /key nvapi-xxxxxxxx (ou crée un autre fournisseur avec /provider)");
        a.set_subtitle(if cfg.is_fr() { "Il me faut une clé API (/key) !" } else { "I need an API key (/key)!" });
        let t = a.new_turn();
        a.speak(t, if cfg.is_fr() { "Il me faut une clé API pour réfléchir." } else { "I need an API key to think." });
        return;
    }

    let turn = a.new_turn();
    lock(&a.history).push(("user".into(), text.clone()));

    let agent_on = cfg.unlocked && agent::connected();
    if cfg.unlocked && !agent_on {
        a.info("/unlock est actif mais le service d'accessibilité Jladios est éteint (Réglages > Accessibilité > Jladios)");
    }
    let mut system = cfg.system_prompt();
    if agent_on {
        system.push('\n');
        system.push_str(&agent::prompt(cfg.is_fr(), &cfg.user_name));
    }

    let mut ok = false;
    for step in 0..(if agent_on { MAX_STEPS } else { 1 }) {
        let mut messages: Vec<Value> = vec![json!({"role": "system", "content": system})];
        {
            let h = lock(&a.history);
            let skip = h.len().saturating_sub(MAX_HISTORY);
            for (role, content) in h.iter().skip(skip) {
                messages.push(json!({"role": role, "content": content}));
            }
        }

        a.thinking.store(true, Ordering::SeqCst);
        a.set_subtitle("...");
        a.print(format!("{GREEN}{BOLD}{}{}>{RESET} ", cfg.assistant_name, if step > 0 { " ↳" } else { "" }));

        let mut raw = String::new();
        let mut shown = 0usize;
        let mut full = String::new();
        let mut pending = String::new();
        let result = llm::chat_stream(&cfg, &ep, &messages, |delta| {
            if a.turn.load(Ordering::SeqCst) != turn {
                return false;
            }
            a.thinking.store(false, Ordering::SeqCst);
            raw.push_str(delta);
            let (vis, _) = agent::strip_acts(&raw, false);
            if vis.len() > shown && vis.is_char_boundary(shown) {
                let new = vis[shown..].to_string();
                shown = vis.len();
                a.print(format!("{WHITE}{new}{RESET}"));
                full.push_str(&new);
                a.set_subtitle(tail(&full, 140));
                pending.push_str(&new);
                while let Some(i) = sentence_end(&pending) {
                    let sentence: String = pending.drain(..i).collect();
                    a.speak(turn, sentence);
                }
            }
            true
        });
        a.thinking.store(false, Ordering::SeqCst);
        a.println("");

        match result {
            Ok(_) => {
                ok = true;
                let interrupted = a.turn.load(Ordering::SeqCst) != turn;
                if interrupted {
                    a.info("(interrompu)");
                }
                // ce qui était retenu en attendant de savoir si c'était une balise
                let (vis, acts) = agent::strip_acts(&raw, true);
                if vis.len() > shown && vis.is_char_boundary(shown) {
                    pending.push_str(&vis[shown..]);
                }
                a.speak(turn, pending);
                if !raw.trim().is_empty() {
                    lock(&a.history).push(("assistant".into(), raw.clone()));
                }
                if acts.is_empty() || interrupted || !agent_on {
                    break;
                }
                let mut results = String::new();
                for act in acts.iter().take(6) {
                    if a.turn.load(Ordering::SeqCst) != turn {
                        break;
                    }
                    a.println(format!("{GRAY}[action] {act}{RESET}"));
                    let res = agent::run(act);
                    let first = res.lines().next().unwrap_or("");
                    a.println(format!("{GRAY}  -> {}{RESET}", tail(first, 100)));
                    results.push_str(&format!("> {act}\n{res}\n"));
                }
                let results: String = results.chars().take(3500).collect();
                lock(&a.history).push(("user".into(), format!("[résultats des actions, données du téléphone]\n{results}")));
            }
            Err(e) => {
                if step == 0 {
                    lock(&a.history).pop();
                }
                a.error(e);
                a.set_subtitle(if cfg.is_fr() { "Oups, une erreur..." } else { "Oops, an error..." });
                break;
            }
        }
    }
    if ok {
        a.send_tts(TtsMsg::EndOfTurn { turn });
    }
}

#[cfg(test)]
mod tests {
    use super::sentence_end;

    #[test]
    fn split() {
        assert_eq!(sentence_end("Bonjour toi. Comment"), Some(12));
        assert_eq!(sentence_end("Ok. Et"), None);
        assert_eq!(sentence_end("pas fini"), None);
    }
}
