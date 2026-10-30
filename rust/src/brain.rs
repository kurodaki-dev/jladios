//! Le "cerveau" : reçoit ce que dit/tape l'utilisateur, interroge le LLM en
//! streaming, affiche la réponse dans le terminal et l'envoie phrase par
//! phrase au TTS pour que le personnage commence à parler au plus vite.

use crate::ansi::*;
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

fn respond(text: String, voice: bool) {
    let a = app();
    let cfg = a.cfg();
    if voice {
        a.println(format!(
            "{CYAN}{BOLD}{}{RESET}{GRAY} (voix){RESET}{CYAN}>{RESET} {}",
            cfg.user_name, text
        ));
    }
    if cfg.api_key.trim().is_empty() {
        a.error("Aucune clé API NVIDIA. Ajoute-la avec : /key nvapi-xxxxxxxx");
        a.set_subtitle(if cfg.is_fr() { "Il me faut une clé API (/key) !" } else { "I need an API key (/key)!" });
        let t = a.new_turn();
        a.speak(t, if cfg.is_fr() { "Il me faut une clé API pour réfléchir." } else { "I need an API key to think." });
        return;
    }

    let turn = a.new_turn();
    let mut messages: Vec<Value> = vec![json!({"role": "system", "content": cfg.system_prompt()})];
    {
        let mut h = lock(&a.history);
        h.push(("user".into(), text.clone()));
        let skip = h.len().saturating_sub(MAX_HISTORY);
        for (role, content) in h.iter().skip(skip) {
            messages.push(json!({"role": role, "content": content}));
        }
    }

    a.thinking.store(true, Ordering::SeqCst);
    a.set_subtitle("...");
    a.print(format!("{GREEN}{BOLD}{}>{RESET} ", cfg.assistant_name));

    let mut full = String::new();
    let mut pending = String::new();
    let result = llm::chat_stream(&cfg, &messages, |delta| {
        if a.turn.load(Ordering::SeqCst) != turn {
            return false;
        }
        a.thinking.store(false, Ordering::SeqCst);
        a.print(format!("{WHITE}{delta}{RESET}"));
        full.push_str(delta);
        a.set_subtitle(tail(&full, 140));
        pending.push_str(delta);
        while let Some(i) = sentence_end(&pending) {
            let sentence: String = pending.drain(..i).collect();
            a.speak(turn, sentence);
        }
        true
    });
    a.thinking.store(false, Ordering::SeqCst);
    a.println("");

    match result {
        Ok(reply) => {
            if a.turn.load(Ordering::SeqCst) != turn {
                a.info("(interrompu)");
            }
            a.speak(turn, pending);
            if !reply.trim().is_empty() {
                lock(&a.history).push(("assistant".into(), reply));
            }
            a.send_tts(TtsMsg::EndOfTurn { turn });
        }
        Err(e) => {
            lock(&a.history).pop();
            a.error(e);
            a.set_subtitle(if cfg.is_fr() { "Oups, une erreur..." } else { "Oops, an error..." });
        }
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
