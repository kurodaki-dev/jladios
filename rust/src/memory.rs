//! Mémoire (`/memory on`) : la conversation survit au redémarrage et l'IA
//! retient des faits sur toi (balises `<mem>`), injectés dans son prompt.

use std::fs;
use std::path::Path;

const FILE: &str = "history.json";
/// Nombre de messages gardés sur disque.
pub const KEEP: usize = 40;
pub const MAX_FACTS: usize = 60;

pub fn load_history(dir: &Path) -> Vec<(String, String)> {
    fs::read_to_string(dir.join(FILE))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_history(dir: &Path, h: &[(String, String)]) {
    let start = h.len().saturating_sub(KEEP);
    if let Ok(s) = serde_json::to_string(&h[start..]) {
        let tmp = dir.join("history.tmp");
        if fs::write(&tmp, s).is_ok() {
            let _ = fs::rename(&tmp, dir.join(FILE));
        }
    }
}

pub fn clear_history(dir: &Path) {
    let _ = fs::remove_file(dir.join(FILE));
}

/// Ajoute un fait (sans doublon) ; renvoie vrai s'il était nouveau.
pub fn add_fact(facts: &mut Vec<String>, fact: &str) -> bool {
    let f: String = fact.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(200).collect();
    if f.is_empty() || facts.iter().any(|x| x.eq_ignore_ascii_case(&f)) {
        return false;
    }
    if facts.len() >= MAX_FACTS {
        facts.remove(0);
    }
    facts.push(f);
    true
}

/// Consignes mémoire ajoutées au prompt système.
pub fn prompt(fr: bool, user: &str, facts: &[String]) -> String {
    let mut p = String::new();
    if !facts.is_empty() {
        p.push_str(&if fr { format!("Ce que tu sais déjà sur {user} :\n") } else { format!("What you already know about {user}:\n") });
        for f in facts {
            p.push_str(&format!("- {f}\n"));
        }
    }
    p.push_str(&if fr {
        format!(
            "Si {user} te dit quelque chose à retenir durablement (préférence, prénom d'un proche, habitude, projet), \
             ajoute dans ta réponse une balise <mem>fait court</mem> : elle n'est jamais lue à voix haute."
        )
    } else {
        format!(
            "If {user} tells you something worth remembering long-term (preference, a relative's name, habit, project), \
             add a <mem>short fact</mem> tag to your reply: it is never read aloud."
        )
    });
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn facts_dedup() {
        let mut f = vec![];
        assert!(add_fact(&mut f, "Aime  le café"));
        assert!(!add_fact(&mut f, "aime le café"));
        assert!(!add_fact(&mut f, "   "));
        assert_eq!(f, vec!["Aime le café"]);
    }
}
