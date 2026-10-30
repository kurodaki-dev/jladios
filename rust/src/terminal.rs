//! Le terminal : bannière, configuration initiale (noms, clé API, modèles)
//! et commandes `/xxx`.

use crate::ansi::*;
use crate::app::{app, lock, BrainMsg};
use crate::config::{VOICE_EN, VOICE_FR};
use crate::llm;
use crate::models::{self, Which};
use std::sync::atomic::Ordering;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Onboarding {
    None,
    AssistantName,
    UserName,
    ApiKey,
    Download,
}

fn banner() {
    let a = app();
    let art = [
        "   _ _           _ _           ",
        "  (_) | __ _  __| (_) ___  ___ ",
        "  | | |/ _` |/ _` | |/ _ \\/ __|",
        "  | | | (_| | (_| | | (_) \\__ \\",
        " _/ |_|\\__,_|\\__,_|_|\\___/|___/",
        "|__/                           ",
    ];
    for l in art {
        a.println(format!("{BRIGHT_GREEN}{l}{RESET}"));
    }
    a.println(format!("{GRAY}assistant vocal flottant — coeur en Rust — v{}{RESET}", env!("CARGO_PKG_VERSION")));
    a.println("");
}

pub fn boot() {
    let a = app();
    banner();
    let cfg = a.cfg();
    if !cfg.onboarded {
        a.println(format!("{YELLOW}{BOLD}Bienvenue !{RESET} On va configurer ton assistant."));
        ask(Onboarding::AssistantName);
    } else {
        a.println(format!(
            "Re-bonjour {CYAN}{BOLD}{}{RESET} ! {GREEN}{BOLD}{}{RESET} est prêt.",
            cfg.user_name, cfg.assistant_name
        ));
        status();
        a.println(format!("{GRAY}Tape {YELLOW}/help{GRAY} pour l'aide, {YELLOW}/show{GRAY} pour faire apparaître le personnage.{RESET}"));
    }
}

fn ask(step: Onboarding) {
    let a = app();
    *lock(&a.onboarding) = step;
    let cfg = a.cfg();
    let q = match step {
        Onboarding::AssistantName => format!(
            "{MAGENTA}?{RESET} Comment veux-tu appeler ton assistant ? {GRAY}[{}]{RESET}",
            cfg.assistant_name
        ),
        Onboarding::UserName => format!("{MAGENTA}?{RESET} Et toi, comment tu t'appelles ?"),
        Onboarding::ApiKey => format!(
            "{MAGENTA}?{RESET} Colle ta clé API NVIDIA {GRAY}(build.nvidia.com → Get API Key, commence par nvapi-){RESET}\n  {GRAY}ou tape 'passer' pour le faire plus tard avec /key{RESET}"
        ),
        Onboarding::Download => format!(
            "{MAGENTA}?{RESET} Télécharger les modèles vocaux maintenant ? {GRAY}(~820 Mo, Wi-Fi conseillé){RESET} {BOLD}[o/n]{RESET}"
        ),
        Onboarding::None => return,
    };
    a.println(q);
}

fn onboarding_input(step: Onboarding, line: &str) {
    let a = app();
    match step {
        Onboarding::AssistantName => {
            if !line.is_empty() {
                let name = line.to_string();
                a.update_cfg(|c| c.assistant_name = name);
            }
            let n = a.cfg().assistant_name;
            a.println(format!("{GREEN}{BOLD}{n}>{RESET} Super, je m'appelle {BOLD}{n}{RESET} !"));
            ask(Onboarding::UserName);
        }
        Onboarding::UserName => {
            if line.is_empty() {
                ask(Onboarding::UserName);
                return;
            }
            let name = line.to_string();
            a.update_cfg(|c| c.user_name = name.clone());
            let cfg = a.cfg();
            a.println(format!(
                "{GREEN}{BOLD}{}>{RESET} Enchanté {CYAN}{BOLD}{name}{RESET} !",
                cfg.assistant_name
            ));
            ask(Onboarding::ApiKey);
        }
        Onboarding::ApiKey => {
            let l = line.to_lowercase();
            if !(l.is_empty() || l == "passer" || l == "skip") {
                set_key(line);
            } else {
                a.info("OK, tu pourras l'ajouter avec /key nvapi-...");
            }
            ask(Onboarding::Download);
        }
        Onboarding::Download => {
            *lock(&a.onboarding) = Onboarding::None;
            a.update_cfg(|c| c.onboarded = true);
            let l = line.to_lowercase();
            if l.starts_with('o') || l.starts_with('y') {
                models::download(Which::All, true);
            } else {
                a.info("OK, lance /download quand tu veux.");
            }
            a.println(format!("{GRAY}Configuration terminée. Tape {YELLOW}/help{GRAY} pour voir les commandes.{RESET}"));
        }
        Onboarding::None => {}
    }
}

fn set_key(k: &str) {
    let a = app();
    let k = k.trim().to_string();
    if !k.starts_with("nvapi-") {
        a.println(format!("{YELLOW}[?]{RESET} La clé ne commence pas par 'nvapi-', je l'enregistre quand même."));
    }
    a.update_cfg(|c| c.api_key = k);
    a.success("Clé API enregistrée");
}

fn mask(k: &str) -> String {
    if k.is_empty() {
        return format!("{RED}(aucune){RESET}");
    }
    let start: String = k.chars().take(8).collect();
    format!("{start}****")
}

fn yes_no(b: bool) -> String {
    if b {
        format!("{GREEN}oui{RESET}")
    } else {
        format!("{RED}non{RESET}")
    }
}

fn status() {
    let a = app();
    let c = a.cfg();
    let row = |k: &str, v: String| a.println(format!("  {GRAY}{k:<14}{RESET} {v}"));
    row("assistant", format!("{GREEN}{}{RESET}", c.assistant_name));
    row("toi", format!("{CYAN}{}{RESET}", c.user_name));
    row("clé API", mask(&c.api_key));
    row("modèle LLM", format!("{YELLOW}{}{RESET}", c.model));
    row("langue", c.lang.clone());
    row("voix", format!("{} {GRAY}(vitesse {:.2}){RESET}", c.voice_sid, c.speed));
    row("STT installé", yes_no(models::stt_ready()));
    row("TTS installé", yes_no(models::tts_ready()));
    row("écoute auto", yes_no(c.auto_listen));
    row("muet", yes_no(c.mute));
    row("overlay", yes_no(a.overlay_visible.load(Ordering::SeqCst)));
}

fn help() {
    let a = app();
    let cmds: &[(&str, &str)] = &[
        ("/help", "cette aide"),
        ("/show  /hide", "afficher / cacher le personnage flottant"),
        ("/listen  /stop", "écouter le micro / tout arrêter"),
        ("/say <texte>", "faire parler le personnage"),
        ("/name <nom>", "renommer l'assistant"),
        ("/me <nom>", "changer ton prénom"),
        ("/key <nvapi-...>", "clé API NVIDIA"),
        ("/model [id]", "voir / changer le modèle LLM"),
        ("/models", "lister les modèles NVIDIA dispo"),
        ("/lang fr|en", "langue de conversation"),
        ("/voice <0-52>", "voix Kokoro (30=ff_siwis FR, 3=af_heart EN)"),
        ("/speed <0.5-2>", "vitesse de la voix"),
        ("/auto on|off", "réécoute automatique après chaque réponse"),
        ("/mute on|off", "couper la voix (la bouche bouge quand même)"),
        ("/prompt <texte>", "consignes perso pour l'IA (/prompt clear)"),
        ("/download [stt|tts|all]", "télécharger les modèles"),
        ("/delete stt|tts|all", "supprimer les modèles"),
        ("/status", "état de l'app"),
        ("/reset", "oublier la conversation"),
        ("/setup", "refaire la configuration initiale"),
        ("/clear", "effacer l'écran"),
    ];
    a.println(format!("{BOLD}Commandes :{RESET}"));
    for (c, d) in cmds {
        a.println(format!("  {YELLOW}{c:<24}{RESET} {GRAY}{d}{RESET}"));
    }
    a.println(format!("{GRAY}Tout texte sans '/' est envoyé à l'assistant.{RESET}"));
    a.println(format!("{GRAY}Sur le personnage : appui = parler/interrompre, appui long = ouvrir le terminal, glisser = déplacer.{RESET}"));
}

fn on_off(arg: &str) -> Option<bool> {
    match arg {
        "on" | "oui" | "1" | "true" => Some(true),
        "off" | "non" | "0" | "false" => Some(false),
        _ => None,
    }
}

fn which(arg: &str) -> Option<Which> {
    match arg {
        "" | "all" | "tout" => Some(Which::All),
        "stt" => Some(Which::Stt),
        "tts" => Some(Which::Tts),
        _ => None,
    }
}

fn command(cmd: &str, arg: &str) {
    let a = app();
    match cmd {
        "help" | "aide" | "h" | "?" => help(),
        "show" | "afficher" => a.command("overlay_start"),
        "hide" | "cacher" => a.command("overlay_stop"),
        "listen" | "ecoute" | "écoute" => a.start_listening(),
        "stop" => {
            a.listening.store(false, Ordering::SeqCst);
            a.send_stt(crate::app::SttMsg::Stop);
            a.new_turn();
            a.set_subtitle("");
            a.info("arrêté");
        }
        "say" | "dis" => {
            if arg.is_empty() {
                a.error("usage : /say <texte>");
            } else {
                let t = a.new_turn();
                a.set_subtitle(arg.to_string());
                a.speak(t, arg.to_string());
            }
        }
        "name" | "nom" => {
            if arg.is_empty() {
                a.error("usage : /name <nom>");
            } else {
                let n = arg.to_string();
                a.update_cfg(|c| c.assistant_name = n);
                a.success(format!("L'assistant s'appelle maintenant {}", arg));
            }
        }
        "me" | "moi" => {
            if arg.is_empty() {
                a.error("usage : /me <ton prénom>");
            } else {
                let n = arg.to_string();
                a.update_cfg(|c| c.user_name = n);
                a.success(format!("Enchanté {arg} !"));
            }
        }
        "key" | "cle" | "clé" => {
            if arg.is_empty() {
                a.println(format!("clé actuelle : {}", mask(&a.cfg().api_key)));
            } else {
                set_key(arg);
            }
        }
        "model" | "modele" | "modèle" => {
            if arg.is_empty() {
                a.println(format!("modèle actuel : {YELLOW}{}{RESET}", a.cfg().model));
            } else {
                let m = arg.to_string();
                a.update_cfg(|c| c.model = m);
                a.success(format!("modèle : {arg}"));
            }
        }
        "models" | "modeles" | "modèles" => {
            let key = a.cfg().api_key;
            std::thread::spawn(move || {
                let a = app();
                a.info("récupération de la liste...");
                match llm::list_models(&key) {
                    Ok(list) => {
                        let cur = a.cfg().model;
                        for m in list {
                            let mark = if m == cur { format!("{GREEN}*{RESET}") } else { " ".into() };
                            a.println(format!(" {mark} {m}"));
                        }
                        a.println(format!("{GRAY}Change avec /model <id>{RESET}"));
                    }
                    Err(e) => a.error(e),
                }
            });
        }
        "lang" | "langue" => match arg {
            "fr" | "en" => {
                let l = arg.to_string();
                a.update_cfg(|c| {
                    if l == "en" && c.voice_sid == VOICE_FR {
                        c.voice_sid = VOICE_EN;
                    }
                    if l == "fr" && c.voice_sid == VOICE_EN {
                        c.voice_sid = VOICE_FR;
                    }
                    c.lang = l;
                });
                a.success(format!("langue : {arg}"));
            }
            _ => a.error("usage : /lang fr|en"),
        },
        "voice" | "voix" => match arg.parse::<i32>() {
            Ok(v) if (0..=52).contains(&v) => {
                a.update_cfg(|c| c.voice_sid = v);
                a.success(format!("voix : {v}"));
            }
            _ => a.error("usage : /voice <0-52>"),
        },
        "speed" | "vitesse" => match arg.replace(',', ".").parse::<f32>() {
            Ok(v) if (0.5..=2.0).contains(&v) => {
                a.update_cfg(|c| c.speed = v);
                a.success(format!("vitesse : {v}"));
            }
            _ => a.error("usage : /speed <0.5-2.0>"),
        },
        "auto" => match on_off(arg) {
            Some(b) => {
                a.update_cfg(|c| c.auto_listen = b);
                a.success(format!("écoute auto : {}", yes_no(b)));
            }
            None => a.error("usage : /auto on|off"),
        },
        "mute" | "muet" => match on_off(arg) {
            Some(b) => {
                a.update_cfg(|c| c.mute = b);
                a.success(format!("muet : {}", yes_no(b)));
            }
            None => a.error("usage : /mute on|off"),
        },
        "prompt" => {
            if arg.is_empty() {
                let p = a.cfg().extra_prompt;
                a.println(if p.is_empty() { format!("{GRAY}(aucune consigne perso){RESET}") } else { p });
            } else if arg == "clear" {
                a.update_cfg(|c| c.extra_prompt.clear());
                a.success("consignes effacées");
            } else {
                let p = arg.to_string();
                a.update_cfg(|c| c.extra_prompt = p);
                a.success("consignes enregistrées");
            }
        }
        "download" | "dl" => match which(arg) {
            Some(w) => models::download(w, false),
            None => a.error("usage : /download [stt|tts|all]"),
        },
        "delete" => match which(arg) {
            Some(w) if !arg.is_empty() => models::delete(w),
            _ => a.error("usage : /delete stt|tts|all"),
        },
        "status" | "etat" | "état" => status(),
        "reset" => {
            a.send_brain(BrainMsg::Reset);
            a.success("conversation oubliée");
        }
        "setup" => ask(Onboarding::AssistantName),
        "clear" | "cls" => a.command("clear"),
        _ => a.error(format!("commande inconnue : /{cmd} (tape /help)")),
    }
}

/// Une ligne saisie dans le terminal.
pub fn handle_input(line: &str) {
    let a = app();
    let line = line.trim();
    let step = *lock(&a.onboarding);
    let cfg = a.cfg();

    // écho de la ligne (en masquant les clés API)
    let shown = if step == Onboarding::ApiKey && line.starts_with("nvapi") {
        mask(line)
    } else if let Some(k) = line.strip_prefix("/key ") {
        format!("/key {}", mask(k.trim()))
    } else {
        line.to_string()
    };
    if step != Onboarding::None {
        a.println(format!("{MAGENTA}>{RESET} {shown}"));
        onboarding_input(step, line);
        return;
    }
    a.println(format!(
        "{BRIGHT_GREEN}{BOLD}{}@{}{RESET}:{BLUE}{BOLD}~{RESET}$ {shown}",
        cfg.user_name.to_lowercase().replace(' ', "_"),
        cfg.assistant_name.to_lowercase().replace(' ', "_")
    ));
    if line.is_empty() {
        return;
    }
    if let Some(rest) = line.strip_prefix('/') {
        let (cmd, arg) = match rest.split_once(char::is_whitespace) {
            Some((c, r)) => (c, r.trim()),
            None => (rest, ""),
        };
        command(&cmd.to_lowercase(), arg);
    } else {
        a.send_brain(BrainMsg::User { text: line.to_string(), voice: false });
    }
}
