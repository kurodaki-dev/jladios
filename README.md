# Jladios — assistant vocal flottant pour Android (en Rust)

Une app au style **terminal** (fond noir, couleurs ANSI classiques) qui fait apparaître
un **petit personnage par-dessus toutes tes applis**. Quand il parle, sa bouche s'anime
au rythme de la voix.

| au repos | quand il parle |
|---|---|
| bouche fermée (petit trait) | bouche ouverte (rectangle) |

## Pipeline

```
micro ──► Nemotron 3.5 ASR streaming 0.6B (local) ──► LLM NVIDIA (API) ──► Kokoro-82M (local) ──► haut-parleur
                                                                                │
                                                       bouche animée ◄──────────┘ (énergie du signal)
```

- **Speech-to-text** : `nvidia/nemotron-3.5-asr-streaming-0.6b` (export sherpa-onnx int8, chunk 560 ms), 100 % local.
- **LLM** : API NVIDIA (`integrate.api.nvidia.com`, compatible OpenAI, streaming). La clé se configure dans l'app.
  Modèle par défaut : `nvidia/nemotron-3.5-lightning-30b-a3b` (changeable avec `/models` et `/model`).
- **Text-to-speech** : Kokoro-82M multilingue (int8), voix française `ff_siwis` par défaut, 100 % local.

Les modèles (~820 Mo) sont **téléchargés au premier lancement** depuis HuggingFace (avec reprise
si la connexion coupe). Ils ne sont pas inclus dans l'APK.

## Architecture

```
rust/                 ← toute la logique (lib native libjladios.so)
  src/lib.rs          pont JNI
  src/app.rs          état global, files de messages, audio, animation de la bouche
  src/terminal.rs     terminal : config initiale (noms, clé), commandes /xxx
  src/brain.rs        conversation + découpage en phrases pour parler au plus vite
  src/llm.rs          client API NVIDIA (SSE)
  src/stt.rs          reconnaissance vocale Nemotron (sherpa-onnx)
  src/tts.rs          synthèse Kokoro (sherpa-onnx + espeak-ng)
  src/models.rs       téléchargement des modèles
  src/character.rs    rendu pixel du personnage (fidèle aux SVG)
android/              ← fine couche Java obligatoire sur Android
  MainActivity        affiche le terminal (interprète les codes ANSI envoyés par Rust)
  OverlayService      fenêtre flottante (TYPE_APPLICATION_OVERLAY) + service au premier plan
  AudioIO             transporte les échantillons micro/haut-parleur vers/depuis Rust
.github/workflows/    ← compilation de l'APK par GitHub Actions
```

> Android impose du Java/Kotlin pour les fenêtres flottantes, les services et les permissions :
> cette partie se limite au strict nécessaire, tout le reste (IA, audio, terminal, dessin du
> personnage) est en Rust.

## Utilisation

1. Installe l'APK (onglet **Actions** → dernier build → artefact `jladios-apk`, ou la page **Releases**).
2. Au premier lancement : donne un nom à l'assistant, ton prénom, ta clé API NVIDIA
   (gratuite sur <https://build.nvidia.com>, bouton *Get API Key*), puis accepte le téléchargement des modèles.
3. Tape `/show` (ou le bouton `[ afficher ]`) et accepte les permissions *superposition* et *micro*.
4. **Appuie sur le personnage** pour lui parler. Appui long = ouvrir le terminal, glisser = le déplacer.

Commandes principales : `/help`, `/show`, `/hide`, `/listen`, `/stop`, `/say`, `/name`, `/me`, `/key`,
`/model`, `/models`, `/provider`, `/lang fr|en`, `/voice`, `/speed`, `/auto on|off`, `/mute on|off`, `/prompt`,
`/download`, `/delete`, `/status`, `/reset`, `/setup`, `/clear`.

## Fournisseurs d'IA (texte)

NVIDIA est intégré, mais tu peux ajouter n'importe quel service **compatible OpenAI**
(OpenAI, OpenRouter, Groq, Together, Ollama / LM Studio / llama.cpp en local...) :

```
/provider add openrouter https://openrouter.ai/api/v1 sk-or-xxxx openai/gpt-4o-mini
/provider add ollama http://127.0.0.1:11434/v1 - llama3.2     # "-" = pas de clé
/provider use ollama        # ou : /provider use nvidia
/models                     # liste les modèles du fournisseur actif, /model <id> pour choisir
```

`/key` et `/model` agissent sur le fournisseur actif. La conversation est remise à zéro quand tu changes de fournisseur.

### Utiliser ton abonnement Claude ou ChatGPT (Codex)

Directement dans l'app, sans clé API :

```
/login codex     # ChatGPT : le navigateur s'ouvre, tu entres le code affiché, c'est tout
/login claude    # Claude : connecte-toi dans le navigateur, puis colle le code affiché dans le terminal
/login gemini    # Gemini : connexion avec ton compte Google (navigateur)
/login aistudio  # Gemini avec une clé API gratuite Google AI Studio
/logout claude   # déconnecter
```

Les jetons restent dans la config privée de l'app et sont renouvelés automatiquement. Change de modèle avec
`/models` puis `/model <id>`. Ça consomme le quota de ton abonnement.

> Ces connexions réutilisent les identifiants OAuth publics des CLI officiels (Codex, Claude Code) et des
> endpoints non documentés : ils peuvent changer, et les conditions d'utilisation de Anthropic / OpenAI
> restreignent l'usage des abonnements hors de leurs outils. À tes risques.

Alternative sans OAuth dans l'app : `tools/cli-bridge.py claude|codex` lance le CLI officiel déjà connecté et
l'expose comme un serveur OpenAI-compatible local (`/provider add claude http://127.0.0.1:8787/v1 - sonnet`).

### Contrôle du téléphone (`/unlock`) et mémoire (`/memory`)

- `/unlock` (avec confirmation) laisse l'IA lire l'écran et agir : ouvrir des applis, toucher, taper, balayer,
  copier/coller, volume, médias, capture d'écran... Il faut activer le service d'accessibilité **Jladios**
  dans les réglages Android. L'écran lu est envoyé au fournisseur d'IA actif. `/lock` coupe tout.
- `/memory on|off` : garde la conversation après un redémarrage et laisse l'IA retenir des faits sur toi.
  `/memory` liste les souvenirs, `add`, `forget <n>`, `clear` pour les gérer ; `/reset` oublie la conversation.

### Modèles d'IA locaux (`/local`)

`/local` liste de petits modèles qui tournent hors ligne sur le téléphone (llama.cpp) : Qwen3.5 0.8B / 2B / 4B,
Qwen3, Gemma 3, Llama 3.2. `/local qwen3.5-2b` télécharge le fichier puis l'active ; `/local off` revient au
fournisseur précédent, `/local delete <id>` libère la place. Compte la taille du fichier + ~1 Go de RAM libre.

### Personnages (onglet *characters*)

L'onglet **characters** permet de créer un personnage avec **deux images obligatoires** : une au repos
(inactif) et une quand il parle, écoute ou réfléchit (actif). Chaque personnage peut être exporté en fichier
`.jladios` (un zip : `manifest.json`, `idle.png`, `active.png`), partagé, puis importé d'un simple appui sur le
fichier ou avec le bouton *importer .jladios*.

### Langues

`/lang fr|en` change la langue de l'interface *et* de la conversation (la langue est demandée au premier lancement).

## Build

Le build se fait automatiquement par GitHub Actions à chaque push sur `main`
(et une Release est créée pour chaque tag `v*`).

### Signature

L'APK est signé avec `android/app/jladios.jks` (mot de passe `jladios`) pour que les mises à jour
s'installent par-dessus sans perdre les modèles téléchargés. Pour utiliser ta propre clé, ajoute
les secrets GitHub `KEYSTORE_BASE64`, `KEYSTORE_PASSWORD`, `KEY_ALIAS`, `KEY_PASSWORD`.

### En local (Linux)

```bash
rustup target add aarch64-linux-android
cargo install cargo-ndk
export ANDROID_NDK_HOME=/chemin/vers/ndk
cd rust
cargo ndk -t arm64-v8a -P 26 -o ../android/app/src/main/jniLibs build --release
cp target/aarch64-linux-android/release/*.so ../android/app/src/main/jniLibs/arm64-v8a/
cp $ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/sysroot/usr/lib/aarch64-linux-android/libc++_shared.so ../android/app/src/main/jniLibs/arm64-v8a/
cd ../android && gradle assembleRelease
```

Appareil requis : Android 8+ en arm64, ~1,5 Go de RAM libre conseillé pour les modèles.
