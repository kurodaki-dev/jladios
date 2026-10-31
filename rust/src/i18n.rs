//! Interface en anglais : le code écrit ses messages en français ; quand la langue
//! est « en », chaque ligne affichée dans le terminal passe par cette table de
//! fragments (les valeurs dynamiques, couleurs et commandes restent intactes).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::LazyLock;

static FR: AtomicBool = AtomicBool::new(true);

pub fn set_fr(fr: bool) {
    FR.store(fr, Ordering::Relaxed);
}

pub fn is_fr() -> bool {
    FR.load(Ordering::Relaxed)
}

/// Choisit le texte selon la langue (pour les petits libellés).
pub fn tr<'a>(fr: &'a str, en: &'a str) -> &'a str {
    if is_fr() {
        fr
    } else {
        en
    }
}

/// (français, anglais) : les fragments les plus longs sont appliqués en premier.
const TABLE: &[(&str, &str)] = &[
    // --- démarrage / configuration ---
    ("Bienvenue !", "Welcome!"),
    (" On va configurer ton assistant.", " Let's set up your assistant."),
    ("Re-bonjour ", "Welcome back "),
    (" est prêt.", " is ready."),
    ("Tape ", "Type "),
    (" pour l'aide, ", " for help, "),
    (" pour faire apparaître le personnage.", " to show the character."),
    ("Comment veux-tu appeler ton assistant ?", "What do you want to call your assistant?"),
    ("Et toi, comment tu t'appelles ?", "And you, what's your name?"),
    ("Colle ta clé API NVIDIA ", "Paste your NVIDIA API key "),
    ("(build.nvidia.com → Get API Key, commence par nvapi-)", "(build.nvidia.com → Get API Key, starts with nvapi-)"),
    ("ou tape 'passer' pour le faire plus tard avec /key", "or type 'skip' to do it later with /key"),
    ("Télécharger les modèles vocaux maintenant ?", "Download the voice models now?"),
    ("(~820 Mo, Wi-Fi conseillé)", "(~820 MB, Wi-Fi recommended)"),
    ("[o/n]", "[y/n]"),
    ("Super, je m'appelle ", "Great, my name is "),
    ("Enchanté ", "Nice to meet you, "),
    ("OK, tu pourras l'ajouter avec /key nvapi-...", "OK, you can add it later with /key nvapi-..."),
    ("OK, lance /download quand tu veux.", "OK, run /download whenever you like."),
    ("Configuration terminée. Tape ", "Setup complete. Type "),
    (" pour voir les commandes.", " to see the commands."),
    ("La clé ne commence pas par 'nvapi-', je l'enregistre quand même.", "The key doesn't start with 'nvapi-', saving it anyway."),
    ("Clé API enregistrée", "API key saved"),
    // --- statut ---
    ("(service d'accessibilité : ", "(accessibility service: "),
    ("(aucune)", "(none)"),
    ("(vitesse ", "(speed "),
    ("souvenirs)", "memories)"),
    // --- aide ---
    ("Commandes :", "Commands:"),
    ("cette aide", "this help"),
    ("afficher / cacher le personnage flottant", "show / hide the floating character"),
    ("écouter le micro / tout arrêter", "listen to the microphone / stop everything"),
    ("faire parler le personnage", "make the character talk"),
    ("renommer l'assistant", "rename the assistant"),
    ("changer ton prénom", "change your name"),
    ("clé API du fournisseur actif", "API key of the active provider"),
    ("voir / changer le modèle du fournisseur actif", "view / change the active provider's model"),
    ("lister les modèles du fournisseur actif", "list the active provider's models"),
    ("fournisseurs d'IA (add, use, del...) : /provider help", "AI providers (add, use, del...): /provider help"),
    ("connecter Claude, ChatGPT (Codex) ou Gemini (compte Google)", "sign in to Claude, ChatGPT (Codex) or Gemini (Google account)"),
    ("Gemini avec une clé API gratuite Google AI Studio", "Gemini with a free Google AI Studio API key"),
    ("déconnecter", "sign out"),
    ("langue de conversation", "language (interface and conversation)"),
    ("mémoire : garde la conversation et retient des faits (/memory pour la voir)", "memory: keeps the conversation and remembers facts (/memory to view)"),
    ("laisser l'IA contrôler tout le téléphone (confirmation) / couper", "let the AI control the whole phone (asks to confirm) / turn off"),
    ("catalogue des voix hors ligne (Kokoro, Piper...)", "catalog of offline voices (Kokoro, Piper...)"),
    ("voix Kokoro (30=ff_siwis FR, 3=af_heart EN)", "Kokoro voice (30=ff_siwis FR, 3=af_heart EN)"),
    ("vitesse de la voix", "voice speed"),
    ("réécoute automatique après chaque réponse", "listen again automatically after each answer"),
    ("couper la voix (la bouche bouge quand même)", "mute the voice (the mouth still moves)"),
    ("consignes perso pour l'IA (/prompt clear)", "custom instructions for the AI (/prompt clear)"),
    ("télécharger les modèles", "download the models"),
    ("supprimer les modèles", "delete the models"),
    ("état de l'app", "app status"),
    ("oublier la conversation", "forget the conversation"),
    ("refaire la configuration initiale", "redo the initial setup"),
    ("effacer l'écran", "clear the screen"),
    ("quitter complètement l'app (personnage, micro, modèles, tout)", "quit the app completely (character, microphone, models, everything)"),
    ("Tout texte sans '/' est envoyé à l'assistant.", "Any text without '/' is sent to the assistant."),
    ("Sur le personnage : appui = parler/interrompre, appui long = ouvrir le terminal, glisser = déplacer.", "On the character: tap = talk/interrupt, long press = open the terminal, drag = move."),
    // --- commandes ---
    ("arrêté", "stopped"),
    ("usage : ", "usage: "),
    ("<texte>", "<text>"),
    ("[clé|-] [modèle]", "[key|-] [model]"),
    ("(voir /local)", "(see /local)"),
    ("<nom>", "<name>"),
    ("<ton prénom>", "<your name>"),
    ("<clé>", "<key>"),
    ("<numéro>", "<number>"),
    ("clé actuelle : ", "current key: "),
    ("modèle actuel : ", "current model: "),
    ("modèle : ", "model: "),
    ("récupération de la liste...", "fetching the list..."),
    ("Change avec /model <id>", "Change with /model <id>"),
    ("L'assistant s'appelle maintenant ", "The assistant is now called "),
    ("langue : ", "language: "),
    ("voix : ", "voice: "),
    ("vitesse : ", "speed: "),
    ("écoute auto : ", "auto-listen: "),
    ("muet : ", "muted: "),
    ("consignes effacées", "instructions cleared"),
    ("consignes enregistrées", "instructions saved"),
    ("(aucune consigne perso)", "(no custom instructions)"),
    ("conversation oubliée", "conversation forgotten"),
    ("commande inconnue : ", "unknown command: "),
    (" (tape /help)", " (type /help)"),
    ("OK, rien n'est activé", "OK, nothing was turned on"),
    // --- /unlock ---
    ("Attention :", "Warning:"),
    (" donne à l'IA le contrôle de TOUT le téléphone :", " gives the AI control of the ENTIRE phone:"),
    ("  - elle lit ce qui s'affiche à l'écran (et l'envoie au fournisseur d'IA actif)", "  - it reads what is shown on screen (and sends it to the active AI provider)"),
    ("  - elle ouvre les applis, touche, tape, balaie, revient en arrière...", "  - it opens apps, taps, types, swipes, goes back..."),
    ("  - elle lit, crée, modifie, déplace et supprime tes fichiers (tout le stockage)", "  - it reads, creates, edits, moves and deletes your files (all storage)"),
    ("  - elle peut donc lire des messages, envoyer, supprimer, acheter...", "  - so it can read messages, send, delete, buy..."),
    ("Un texte affiché à l'écran peut tenter de la manipuler. Coupe tout avec /lock.", "Text shown on screen may try to manipulate it. Turn everything off with /lock."),
    ("Veux-tu vraiment donner l'accès complet ?", "Do you really want to give full access?"),
    ("contrôle du téléphone activé", "phone control enabled"),
    ("contrôle du téléphone coupé", "phone control turned off"),
    ("Demande-lui par exemple : « ouvre YouTube » ou « que contient mon dossier Download ? ». /lock pour tout couper.", "Ask it for example: \"open YouTube\" or \"what's in my Download folder?\". /lock turns everything off."),
    ("Étapes Android :", "Android steps:"),
    ("Étape Android :", "Android step:"),
    ("1) « Accès à tous les fichiers » pour Jladios, 2) service d'accessibilité ", "1) \"All files access\" for Jladios, 2) accessibility service "),
    ("« Accès à tous les fichiers » pour Jladios", "\"All files access\" for Jladios"),
    ("(Réglages > Accessibilité > Applis installées > Jladios). Si Android le grise :", "(Settings > Accessibility > Installed apps > Jladios). If Android greys it out:"),
    ("Infos de l'appli Jladios > ⋮ > Autoriser les paramètres restreints.", "Jladios app info > ⋮ > Allow restricted settings."),
    ("contrôle de l'écran inactif : active le service d'accessibilité Jladios (Réglages > Accessibilité). Les fichiers restent accessibles.", "screen control is off: enable the Jladios accessibility service (Settings > Accessibility). Files remain accessible."),
    ("[mémoire] noté : ", "[memory] noted: "),
    ("[action] ", "[action] "),
    // --- /login ---
    ("connecter Claude", "sign in to Claude"),
    ("Connecte-toi à Claude dans le navigateur :", "Sign in to Claude in your browser:"),
    ("Connecte-toi à Google dans le navigateur :", "Sign in to Google in your browser:"),
    ("Connecte-toi à Google et crée une clé API gratuite :", "Sign in to Google and create a free API key:"),
    ("Colle ici le code affiché ensuite ", "Paste here the code shown afterwards "),
    ("Colle ici ta clé ", "Paste your key here "),
    ("(ou 'annuler')", "(or 'cancel')"),
    ("(commence par AIza, ou 'annuler')", "(starts with AIza, or 'cancel')"),
    ("La clé ne commence pas par 'AIza', je l'enregistre quand même.", "The key doesn't start with 'AIza', saving it anyway."),
    ("connexion annulée", "sign-in cancelled"),
    ("connecté : fournisseur '", "signed in: provider '"),
    ("' actif, modèle ", "' active, model "),
    ("' actif", "' active"),
    ("Change de modèle avec /models puis /model <id>", "Change the model with /models then /model <id>"),
    ("Liste les modèles avec /models puis /model <id>", "List the models with /models then /model <id>"),
    ("Ouvre ", "Open "),
    ("  et entre le code : ", "  and enter the code: "),
    ("(valable 15 min)", "(valid for 15 min)"),
    ("j'attends ta validation...", "waiting for you to approve..."),
    ("code reçu, finalisation...", "code received, finishing up..."),
    ("aucun projet trouvé, création (free-tier)...", "no project found, creating one (free tier)..."),
    ("connexion Codex : ", "Codex sign-in: "),
    ("connexion Claude : ", "Claude sign-in: "),
    ("connexion Gemini : ", "Gemini sign-in: "),
    ("délai dépassé (15 min), relance /login codex", "timed out (15 min), run /login codex again"),
    ("délai dépassé (5 min), relance /login gemini", "timed out (5 min), run /login gemini again"),
    ("le code ne correspond pas à cette connexion, relance /login claude", "the code doesn't match this sign-in, run /login claude again"),
    ("aucune connexion en cours, relance /login claude", "no sign-in in progress, run /login claude again"),
    ("session expirée : refais /login ", "session expired: run /login again for "),
    ("renouvellement du jeton impossible (", "could not refresh the token ("),
    ("réponse sans ", "response without "),
    ("projet Google introuvable (compte non éligible ?)", "Google project not found (account not eligible?)"),
    ("cette version n'embarque pas le secret client (build sans JL_GEMINI_SECRET)", "this build doesn't include the client secret (built without JL_GEMINI_SECRET)"),
    ("secret client absent de cette version", "client secret missing from this build"),
    ("le port ", "port "),
    (" est occupé (une autre connexion est en cours ?)", " is busy (another sign-in in progress?)"),
    ("Google a refusé : ", "Google refused: "),
    ("identifiant de compte ChatGPT introuvable dans le jeton", "ChatGPT account id not found in the token"),
    ("réseau : ", "network: "),
    ("accès refusé : clé API invalide, ou session expirée (refais /login)", "access denied: invalid API key or expired session (run /login again)"),
    ("limite de requêtes atteinte, réessaie dans un moment", "rate limit reached, try again in a moment"),
    ("modèle introuvable (vérifie /model) : ", "model not found (check /model): "),
    ("réponse vide : ", "empty response: "),
    // --- /memory ---
    ("mémoire activée : la conversation est gardée et l'IA retient ce qui compte", "memory on: the conversation is kept and the AI remembers what matters"),
    ("/memory pour voir ce qu'elle sait, /memory off pour couper", "/memory shows what it knows, /memory off turns it off"),
    ("mémoire coupée (les souvenirs déjà gardés restent sur le téléphone, /memory clear pour les effacer)", "memory off (saved memories stay on the phone, /memory clear erases them)"),
    ("mémoire : ", "memory: "),
    ("(aucun souvenir pour l'instant)", "(no memories yet)"),
    ("souvenir ajouté", "memory added"),
    ("déjà connu", "already known"),
    ("oublié", "forgotten"),
    ("souvenirs et historique effacés", "memories and history erased"),
    ("voir /memory", "see /memory"),
    // --- /tts ---
    ("installée", "installed"),
    ("Choisis avec /tts <id> (téléchargement automatique)", "Pick with /tts <id> (downloads automatically)"),
    ("voix inconnue (liste avec /tts)", "unknown voice (list with /tts)"),
    ("Impossible de charger la voix (essaie /delete tts puis /download tts)", "Could not load the voice (try /delete tts then /download tts)"),
    ("Échec de la synthèse vocale", "Speech synthesis failed"),
    // --- /provider ---
    ("liste les fournisseurs", "list providers"),
    ("ajoute un fournisseur compatible OpenAI", "add an OpenAI-compatible provider"),
    ("active un fournisseur (nvidia = intégré)", "activate a provider (nvidia = built-in)"),
    ("change la clé d'un fournisseur", "change a provider's key"),
    ("supprime un fournisseur", "delete a provider"),
    ("Abonnement Claude / ChatGPT : /login claude  ou  /login codex", "Claude / ChatGPT subscription: /login claude  or  /login codex"),
    ("pour les commandes", "for the commands"),
    ("'nvidia' est réservé au fournisseur intégré", "'nvidia' is reserved for the built-in provider"),
    ("l'URL doit commencer par http:// ou https:// (et finir par /v1 en général)", "the URL must start with http:// or https:// (and usually end with /v1)"),
    ("' ajouté et activé", "' added and activated"),
    ("fournisseur '", "provider '"),
    ("Choisis un modèle avec /models puis /model <id>", "Pick a model with /models then /model <id>"),
    ("fournisseur inconnu : ", "unknown provider: "),
    (" (liste avec /provider)", " (list with /provider)"),
    ("clé enregistrée", "key saved"),
    ("' supprimé", "' deleted"),
    ("fournisseur actif : ", "active provider: "),
    // --- cerveau / modèles / audio ---
    ("Aucune clé API NVIDIA. Ajoute-la avec : /key nvapi-xxxxxxxx (ou crée un autre fournisseur avec /provider)", "No NVIDIA API key. Add it with: /key nvapi-xxxxxxxx (or create another provider with /provider)"),
    ("(interrompu)", "(interrupted)"),
    ("Un téléchargement est déjà en cours.", "A download is already in progress."),
    (" déjà installé", " already installed"),
    (" installé", " installed"),
    (" supprimé", " deleted"),
    ("Modèles prêts !", "Models ready!"),
    (" pour faire apparaître ton assistant.", " to bring your assistant to life."),
    ("Téléchargement incomplet. Relance /download (il reprendra où il s'est arrêté).", "Download incomplete. Run /download again (it resumes where it stopped)."),
    ("connexion coupée (", "connection lost ("),
    (" Mo)", " MB)"),
    ("Chargement du modèle de reconnaissance vocale (quelques secondes)...", "Loading the speech recognition model (a few seconds)..."),
    ("Reconnaissance vocale prête", "Speech recognition ready"),
    ("Impossible de charger le modèle STT (fichiers corrompus ? essaie /delete stt puis /download stt)", "Could not load the STT model (corrupted files? try /delete stt then /download stt)"),
    ("Le modèle de reconnaissance vocale n'est pas installé. Tape /download", "The speech recognition model is not installed. Type /download"),
    ("(voix)", "(voice)"),
    ("assistant vocal flottant — coeur en Rust — v", "floating voice assistant — Rust core — v"),
    ("[j'écoute...]", "[listening...]"),
    ("Installe mes modèles avec /download !", "Install my models with /download!"),
    // --- modèles locaux ---
    ("modèle local prêt", "local model ready"),
    ("voix française : elle lira l'anglais avec un accent français", "French voice: it will read English with a French accent"),
    ("voix anglaise : elle lira le français avec un accent anglais", "English voice: it will read French with an English accent"),
    ("Pour des modèles voix adaptés à cette langue, installe : ", "For voice models suited to this language, install: "),
    ("modèles d'IA locaux, hors ligne (Qwen, Gemma, Llama...) : télécharge et active", "local AI models, offline (Qwen, Gemma, Llama...): downloads and activates"),
    (" : télécharge puis active (hors ligne, gratuit). /local delete <id>, /local off", ": downloads then activates (offline, free). /local delete <id>, /local off"),
    ("Prévois la taille du fichier + ~1 Go de RAM libre ; les 4B demandent un téléphone avec 8 Go de RAM.", "Allow the file size + ~1 GB of free RAM; the 4B models need a phone with 8 GB of RAM."),
    ("modèle local déchargé, retour au fournisseur nvidia", "local model unloaded, back to the nvidia provider"),
    ("modèle inconnu (liste avec /local)", "unknown model (list with /local)"),
    ("modèle local actif : ", "local model active: "),
    (" (hors ligne)", " (offline)"),
    ("Téléchargement incomplet. Relance la même commande (il reprendra où il s'est arrêté).", "Download incomplete. Run the same command again (it resumes where it stopped)."),
    ("chargement du modèle local (quelques secondes)...", "loading the local model (a few seconds)..."),
    ("chargement du modèle impossible : ", "could not load the model: "),
    ("modèle local non téléchargé : tape /local ", "local model not downloaded: type /local "),
    ("modèle local inconnu (liste avec /local)", "unknown local model (list with /local)"),
    ("cette version n'embarque pas le moteur de modèles locaux", "this build doesn't include the local model engine"),
    ("message trop long pour le modèle local", "message too long for the local model"),
    ("gabarit de conversation non reconnu pour ce modèle", "chat template not recognized for this model"),
];

static SORTED: LazyLock<Vec<(&'static str, &'static str)>> = LazyLock::new(|| {
    let mut v: Vec<_> = TABLE.iter().copied().collect();
    v.sort_by_key(|(fr, _)| std::cmp::Reverse(fr.len()));
    v
});

/// Libellés du catalogue `/local` (français -> anglais).
pub fn translate_label(label: &str) -> String {
    label
        .replace("minuscule et étonnant", "tiny and surprisingly good")
        .replace("le meilleur équilibre (recommandé)", "best balance (recommended)")
        .replace("le plus intelligent (8 Go de RAM)", "the smartest (needs 8 GB RAM)")
        .replace("très bon, sans raisonnement", "very good, no reasoning")
        .replace("rapide", "fast")
        .replace("léger, bon en français", "light, good at French")
        .replace("très fast", "very fast")
        .replace("polyvalent", "all-round")
}

/// Traduit une ligne du terminal en anglais (sans effet en français).
pub fn translate(line: &str) -> String {
    if is_fr() {
        return line.to_string();
    }
    let mut s = line.to_string();
    for (fr, en) in SORTED.iter() {
        if s.contains(fr) {
            s = s.replace(fr, en);
        }
    }
    // typographie française : espace avant ! ? :
    s.replace(" !", "!").replace(" ?", "?").replace(" :", ":")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_fragments_and_keeps_values() {
        set_fr(false);
        assert_eq!(translate("Re-bonjour Nhat ! Pixel est prêt."), "Welcome back Nhat! Pixel is ready.");
        assert_eq!(translate("usage : /say <texte>"), "usage: /say <text>");
        set_fr(true);
        assert_eq!(translate("Re-bonjour"), "Re-bonjour");
    }
}
