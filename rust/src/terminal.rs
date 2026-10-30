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
    /// attend le code collé après /login claude
    LoginClaude,
    /// attend la clé collée après /login aistudio
    LoginAiStudio,
    /// attend o/n après /unlock
    Unlock,
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
        Onboarding::None | Onboarding::LoginClaude | Onboarding::LoginAiStudio | Onboarding::Unlock => return,
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
        Onboarding::LoginClaude => crate::auth::finish_claude(line),
        Onboarding::LoginAiStudio => crate::auth::finish_aistudio(line),
        Onboarding::Unlock => {
            *lock(&a.onboarding) = Onboarding::None;
            let l = line.to_lowercase();
            if !(l.starts_with('o') || l.starts_with('y')) {
                a.info("OK, rien n'est activé");
                return;
            }
            a.update_cfg(|c| c.unlocked = true);
            a.success("contrôle du téléphone activé");
            if crate::agent::connected() {
                a.println(format!("{GRAY}Demande-lui par exemple : « ouvre YouTube ». /lock pour tout couper.{RESET}"));
            } else {
                a.println(format!("{YELLOW}Dernière étape :{RESET} active le service d'accessibilité {BOLD}Jladios{RESET}"));
                a.println(format!("{GRAY}(Réglages > Accessibilité > Applis installées > Jladios). Si Android le grise :{RESET}"));
                a.println(format!("{GRAY}Infos de l'appli Jladios > ⋮ > Autoriser les paramètres restreints.{RESET}"));
                a.command("accessibility");
            }
        }
        Onboarding::None => {}
    }
}

fn set_key(k: &str) {
    let a = app();
    let k = k.trim().to_string();
    if a.cfg().active().builtin && !k.starts_with("nvapi-") {
        a.println(format!("{YELLOW}[?]{RESET} La clé ne commence pas par 'nvapi-', je l'enregistre quand même."));
    }
    a.update_cfg(|c| c.set_key(k));
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
    let ep = c.active();
    row("fournisseur", format!("{MAGENTA}{}{RESET} {GRAY}{}{RESET}", ep.name, ep.base_url));
    row("clé API", mask(&ep.api_key));
    row("modèle LLM", format!("{YELLOW}{}{RESET}", ep.model));
    row("langue", c.lang.clone());
    row("voix", format!("{} {GRAY}(vitesse {:.2}){RESET}", c.voice_sid, c.speed));
    row("STT installé", yes_no(models::stt_ready()));
    row("TTS installé", yes_no(models::tts_ready()));
    row("écoute auto", yes_no(c.auto_listen));
    row("muet", yes_no(c.mute));
    row("overlay", yes_no(a.overlay_visible.load(Ordering::SeqCst)));
    row("mémoire", format!("{} {GRAY}({} souvenirs){RESET}", yes_no(c.memory), c.facts.len()));
    row("contrôle tel", format!("{} {GRAY}(service d'accessibilité : {}){RESET}", yes_no(c.unlocked), yes_no(crate::agent::connected())));
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
        ("/key <clé>", "clé API du fournisseur actif"),
        ("/model [id]", "voir / changer le modèle du fournisseur actif"),
        ("/models", "lister les modèles du fournisseur actif"),
        ("/login claude|codex|gemini", "connecter Claude, ChatGPT (Codex) ou Gemini (compte Google)"),
        ("/login aistudio", "Gemini avec une clé API gratuite Google AI Studio"),
        ("/logout claude|codex|gemini", "déconnecter"),
        ("/provider", "fournisseurs d'IA (add, use, del...) : /provider help"),
        ("/lang fr|en", "langue de conversation"),
        ("/memory on|off", "mémoire : garde la conversation et retient des faits (/memory pour la voir)"),
        ("/unlock  /lock", "laisser l'IA contrôler tout le téléphone (confirmation) / couper"),
        ("/tts [id]", "catalogue des voix hors ligne (Kokoro, Piper...)"),
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
                a.println(format!("clé actuelle : {}", mask(&a.cfg().active().api_key)));
            } else {
                set_key(arg);
            }
        }
        "model" | "modele" | "modèle" => {
            if arg.is_empty() {
                a.println(format!("modèle actuel : {YELLOW}{}{RESET}", a.cfg().active().model));
            } else {
                let m = arg.to_string();
                a.update_cfg(|c| c.set_model(m));
                a.success(format!("modèle : {arg}"));
            }
        }
        "models" | "modeles" | "modèles" => {
            let ep = a.cfg().active();
            std::thread::spawn(move || {
                let a = app();
                a.info("récupération de la liste...");
                match llm::list_models(&ep) {
                    Ok(list) => {
                        let cur = a.cfg().active().model;
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
        "memory" | "memoire" | "mémoire" => memory_cmd(arg),
        "unlock" | "debloquer" | "débloquer" => {
            a.println(format!("{RED}{BOLD}Attention :{RESET} {BOLD}/unlock{RESET} donne à l'IA le contrôle de TOUT le téléphone :"));
            a.println("  - elle lit ce qui s'affiche à l'écran (et l'envoie au fournisseur d'IA actif)");
            a.println("  - elle ouvre les applis, touche, tape, balaie, revient en arrière...");
            a.println("  - elle peut donc lire des messages, envoyer, supprimer, acheter...");
            a.println(format!("{GRAY}Un texte affiché à l'écran peut tenter de la manipuler. Coupe tout avec /lock.{RESET}"));
            a.println(format!("{MAGENTA}?{RESET} Veux-tu vraiment donner l'accès complet ? {BOLD}[o/n]{RESET}"));
            *lock(&a.onboarding) = Onboarding::Unlock;
        }
        "lock" | "bloquer" | "verrouiller" => {
            a.update_cfg(|c| c.unlocked = false);
            a.success("contrôle du téléphone coupé");
        }
        "provider" | "providers" | "fournisseur" => provider(arg),
        "login" | "connexion" => match arg.to_lowercase().as_str() {
            "claude" | "anthropic" => crate::auth::login_claude(),
            "codex" | "chatgpt" | "openai" => crate::auth::login_codex(),
            "gemini" | "google" | "antigravity" => crate::auth::login_gemini(),
            "aistudio" => crate::auth::login_aistudio(),
            _ => a.error("usage : /login claude|codex|gemini|aistudio"),
        },
        "logout" | "deconnexion" | "déconnexion" => {
            let name = match arg.to_lowercase().as_str() {
                "claude" | "anthropic" => crate::auth::CLAUDE,
                "codex" | "chatgpt" | "openai" => crate::auth::CODEX,
                "gemini" | "google" | "antigravity" => crate::auth::GEMINI,
                "aistudio" => "aistudio",
                _ => {
                    a.error("usage : /logout claude|codex|gemini|aistudio");
                    return;
                }
            };
            provider(&format!("del {name}"));
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
        "tts" => tts_cmd(arg),
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
            a.send_brain(BrainMsg::Forget);
            a.success("conversation oubliée");
        }
        "setup" => ask(Onboarding::AssistantName),
        "clear" | "cls" => a.command("clear"),
        _ => a.error(format!("commande inconnue : /{cmd} (tape /help)")),
    }
}

fn memory_cmd(arg: &str) {
    let a = app();
    let (sub, rest) = arg.split_once(char::is_whitespace).map(|(s, r)| (s, r.trim())).unwrap_or((arg, ""));
    match sub.to_lowercase().as_str() {
        "on" | "oui" | "1" => {
            a.update_cfg(|c| c.memory = true);
            a.success("mémoire activée : la conversation est gardée et l'IA retient ce qui compte");
            a.info("/memory pour voir ce qu'elle sait, /memory off pour couper");
        }
        "off" | "non" | "0" => {
            a.update_cfg(|c| c.memory = false);
            a.success("mémoire coupée (les souvenirs déjà gardés restent sur le téléphone, /memory clear pour les effacer)");
        }
        "" | "list" | "ls" => {
            let c = a.cfg();
            a.println(format!("mémoire : {}", yes_no(c.memory)));
            if c.facts.is_empty() {
                a.println(format!("{GRAY}(aucun souvenir pour l'instant){RESET}"));
            }
            for (i, f) in c.facts.iter().enumerate() {
                a.println(format!("  {YELLOW}{}{RESET} {f}", i + 1));
            }
            a.println(format!("{GRAY}/memory on|off  add <texte>  forget <n>  clear{RESET}"));
        }
        "add" | "ajouter" => {
            if rest.is_empty() {
                a.error("usage : /memory add <texte>");
                return;
            }
            let mut added = false;
            a.update_cfg(|c| added = crate::memory::add_fact(&mut c.facts, rest));
            if added {
                a.success("souvenir ajouté");
            } else {
                a.info("déjà connu");
            }
        }
        "forget" | "oublier" | "del" => match rest.parse::<usize>() {
            Ok(n) if n >= 1 && n <= a.cfg().facts.len() => {
                a.update_cfg(|c| {
                    c.facts.remove(n - 1);
                });
                a.success("oublié");
            }
            _ => a.error("usage : /memory forget <numéro> (voir /memory)"),
        },
        "clear" | "effacer" => {
            a.update_cfg(|c| c.facts.clear());
            a.send_brain(BrainMsg::Forget);
            a.success("souvenirs et historique effacés");
        }
        _ => a.error("usage : /memory [on|off|add <texte>|forget <n>|clear]"),
    }
}

fn tts_cmd(arg: &str) {
    let a = app();
    let cur = models::tts_voice().id;
    if arg.is_empty() {
        for v in models::VOICES {
            let mark = if v.id == cur { format!("{GREEN}*{RESET}") } else { " ".into() };
            let inst = if models::voice_ready(v) { format!("{GREEN}installée{RESET}") } else { format!("{GRAY}~{} Mo{RESET}", v.set.approx_mb) };
            a.println(format!(" {mark} {YELLOW}{:<10}{RESET} {}  {inst}", v.id, v.label));
        }
        a.println(format!("{GRAY}Choisis avec /tts <id> (téléchargement automatique){RESET}"));
        return;
    }
    let Some(v) = models::VOICES.iter().find(|v| v.id == arg.to_lowercase()) else {
        a.error("voix inconnue (liste avec /tts)");
        return;
    };
    let id = v.id.to_string();
    a.update_cfg(|c| c.tts_model = id);
    a.send_tts(crate::app::TtsMsg::Unload);
    a.success(format!("voix : {}", v.label));
    if !models::voice_ready(v) {
        models::download(Which::Tts, false);
    }
}

fn provider_help() {
    let a = app();
    let rows: &[(&str, &str)] = &[
        ("/provider", "liste les fournisseurs"),
        ("/provider add <nom> <url> [clé|-] [modèle]", "ajoute un fournisseur compatible OpenAI"),
        ("/provider use <nom>", "active un fournisseur (nvidia = intégré)"),
        ("/provider key <nom> <clé>", "change la clé d'un fournisseur"),
        ("/provider del <nom>", "supprime un fournisseur"),
    ];
    for (c, d) in rows {
        a.println(format!("  {YELLOW}{c}{RESET}"));
        a.println(format!("      {GRAY}{d}{RESET}"));
    }
    a.println(format!("{GRAY}URL : https://api.openai.com/v1, https://openrouter.ai/api/v1,{RESET}"));
    a.println(format!("{GRAY}https://api.groq.com/openai/v1, http://127.0.0.1:11434/v1 (Ollama)...{RESET}"));
    a.println(format!("{GRAY}Abonnement Claude / ChatGPT : /login claude  ou  /login codex{RESET}"));
}

fn provider(arg: &str) {
    use crate::config::{Provider, NVIDIA, NVIDIA_URL};
    let a = app();
    let w: Vec<&str> = arg.split_whitespace().collect();
    // "-" = pas de clé / pas de modèle
    let opt = |s: Option<&&str>| match s {
        Some(&"-") | None => String::new(),
        Some(s) => s.to_string(),
    };
    match w.first().copied().unwrap_or("") {
        "" | "list" | "ls" => {
            let c = a.cfg();
            let active = c.active().name;
            let row = |name: &str, url: &str, model: &str| {
                let mark = if active == name { format!("{GREEN}*{RESET}") } else { " ".into() };
                a.println(format!(" {mark} {BOLD}{name}{RESET} {GRAY}{url}{RESET} {YELLOW}{model}{RESET}"));
            };
            row(NVIDIA, NVIDIA_URL, &c.model);
            for p in &c.providers {
                row(&p.name, &p.base_url, &p.model);
            }
            a.println(format!("{GRAY}/provider help pour les commandes{RESET}"));
        }
        "help" | "aide" => provider_help(),
        "add" | "ajouter" => {
            if w.len() < 3 {
                a.error("usage : /provider add <nom> <url> [clé|-] [modèle]");
                return;
            }
            let name = w[1].to_lowercase();
            let url = w[2].trim_end_matches('/').to_string();
            if name == NVIDIA {
                a.error("'nvidia' est réservé au fournisseur intégré");
                return;
            }
            if !(url.starts_with("http://") || url.starts_with("https://")) {
                a.error("l'URL doit commencer par http:// ou https:// (et finir par /v1 en général)");
                return;
            }
            let p = Provider { name: name.clone(), base_url: url, api_key: opt(w.get(3)), model: opt(w.get(4)), ..Default::default() };
            let no_model = p.model.is_empty();
            a.update_cfg(|c| {
                c.providers.retain(|x| x.name != name);
                c.providers.push(p);
                c.provider = name.clone();
            });
            a.send_brain(BrainMsg::Reset);
            a.success(format!("fournisseur '{name}' ajouté et activé"));
            if no_model {
                a.info("Choisis un modèle avec /models puis /model <id>");
            }
        }
        "use" | "set" | "utiliser" => {
            let Some(name) = w.get(1).map(|s| s.to_lowercase()) else {
                a.error("usage : /provider use <nom>");
                return;
            };
            if name != NVIDIA && !a.cfg().providers.iter().any(|p| p.name == name) {
                a.error(format!("fournisseur inconnu : {name} (liste avec /provider)"));
                return;
            }
            a.update_cfg(|c| c.provider = name.clone());
            a.send_brain(BrainMsg::Reset);
            a.success(format!("fournisseur actif : {name}"));
        }
        "key" | "cle" | "clé" => {
            let (Some(name), Some(key)) = (w.get(1).map(|s| s.to_lowercase()), w.get(2)) else {
                a.error("usage : /provider key <nom> <clé>");
                return;
            };
            let key = key.to_string();
            if name != NVIDIA && !a.cfg().providers.iter().any(|p| p.name == name) {
                a.error(format!("fournisseur inconnu : {name}"));
                return;
            }
            a.update_cfg(|c| {
                if name == NVIDIA {
                    c.api_key = key;
                } else if let Some(p) = c.providers.iter_mut().find(|p| p.name == name) {
                    p.api_key = key;
                }
            });
            a.success("clé enregistrée");
        }
        "del" | "rm" | "remove" | "supprimer" => {
            let Some(name) = w.get(1).map(|s| s.to_lowercase()) else {
                a.error("usage : /provider del <nom>");
                return;
            };
            if !a.cfg().providers.iter().any(|p| p.name == name) {
                a.error(format!("fournisseur inconnu : {name}"));
                return;
            }
            a.update_cfg(|c| {
                c.providers.retain(|p| p.name != name);
                if c.provider == name {
                    c.provider = NVIDIA.into();
                }
            });
            a.success(format!("'{name}' supprimé"));
        }
        _ => provider_help(),
    }
}

/// Une ligne saisie dans le terminal.
pub fn handle_input(line: &str) {
    let a = app();
    let line = line.trim();
    let step = *lock(&a.onboarding);
    let cfg = a.cfg();

    // écho de la ligne (en masquant les clés API)
    let shown = if matches!(step, Onboarding::LoginClaude | Onboarding::LoginAiStudio) {
        mask(line)
    } else if step == Onboarding::ApiKey && line.starts_with("nvapi") {
        mask(line)
    } else if let Some(k) = line.strip_prefix("/key ") {
        format!("/key {}", mask(k.trim()))
    } else if line.starts_with("/provider ") || line.starts_with("/fournisseur ") {
        // masque la clé : 4e mot de "/provider add <nom> <url> <clé>" et de "/provider key <nom> <clé>"
        let mut w: Vec<String> = line.split_whitespace().map(String::from).collect();
        let (sub, key_at) = (w.get(1).cloned().unwrap_or_default(), if w.get(1).map(|s| s.as_str()) == Some("add") { 4 } else { 3 });
        if matches!(sub.as_str(), "add" | "key") && w.len() > key_at && w[key_at] != "-" {
            w[key_at] = mask(&w[key_at]);
        }
        w.join(" ")
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
