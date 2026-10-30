#!/usr/bin/env python3
"""Pont OpenAI-compatible vers ton abonnement Claude ou Codex.

Lance le CLI officiel (déjà connecté à TON compte) et expose
http://127.0.0.1:PORT/v1 pour que Jladios l'utilise comme un provider.
Aucun jeton n'est extrait ni copié : c'est le CLI qui s'authentifie.

    python3 tools/cli-bridge.py claude          # abonnement Claude (claude -p)
    python3 tools/cli-bridge.py codex           # abonnement ChatGPT (codex exec)

Puis dans Jladios :  /provider add claude http://127.0.0.1:8787/v1
                     /provider use claude
"""
import json, os, subprocess, sys, tempfile, time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

BACKEND = sys.argv[1] if len(sys.argv) > 1 else "claude"
PORT = int(sys.argv[2]) if len(sys.argv) > 2 else 8787
if BACKEND not in ("claude", "codex"):
    sys.exit("usage: cli-bridge.py claude|codex [port]")

MODELS = {"claude": ["sonnet", "opus", "haiku"], "codex": ["default"]}[BACKEND]


def text_of(content):
    if isinstance(content, list):
        return "".join(p.get("text", "") for p in content if isinstance(p, dict))
    return content or ""


def split(messages):
    """-> (system, transcript) ; le CLI n'a qu'un seul prompt."""
    system = "\n".join(text_of(m["content"]) for m in messages if m["role"] == "system")
    turns = [m for m in messages if m["role"] != "system"]
    if len(turns) == 1:
        return system, text_of(turns[0]["content"])
    lines = [("Utilisateur" if m["role"] == "user" else "Assistant") + " : " + text_of(m["content"]) for m in turns]
    return system, "Conversation jusqu'ici :\n\n" + "\n\n".join(lines) + "\n\nRéponds au dernier message de l'utilisateur."


def run_claude(model, system, prompt):
    """Génère les morceaux de texte au fil de l'eau."""
    cmd = ["claude", "-p", prompt, "--output-format", "stream-json", "--verbose",
           "--include-partial-messages", "--no-session-persistence", "--tools", "",
           "--setting-sources", ""]
    if system:
        cmd += ["--system-prompt", system]
    if model and model != "default":
        cmd += ["--model", model]
    p = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    got = False
    for line in p.stdout:
        try:
            ev = json.loads(line)
        except ValueError:
            continue
        if ev.get("type") == "stream_event":
            d = ev["event"].get("delta", {})
            if d.get("type") == "text_delta":
                got = True
                yield d["text"]
        elif ev.get("type") == "result" and not got and ev.get("result"):
            yield ev["result"]
            got = True
    p.wait()
    if p.returncode and not got:
        raise RuntimeError(p.stderr.read().strip()[-300:] or f"claude a quitté avec le code {p.returncode}")


def run_codex(model, system, prompt):
    full = (system + "\n\n" if system else "") + prompt
    with tempfile.NamedTemporaryFile("r", suffix=".txt") as out:
        cmd = ["codex", "exec", "--skip-git-repo-check", "--sandbox", "read-only", "-o", out.name]
        if model and model != "default":
            cmd += ["--model", model]
        cmd.append(full)
        r = subprocess.run(cmd, capture_output=True, text=True)
        text = out.read().strip()
    if not text:
        raise RuntimeError(r.stderr.strip()[-300:] or "codex n'a rien répondu")
    yield text  # codex ne streame pas : un seul morceau


RUN = run_claude if BACKEND == "claude" else run_codex


class H(BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def send_json(self, code, obj):
        b = json.dumps(obj).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(b)))
        self.end_headers()
        self.wfile.write(b)

    def do_GET(self):
        if self.path.rstrip("/").endswith("/models"):
            self.send_json(200, {"object": "list", "data": [{"id": m, "object": "model"} for m in MODELS]})
        else:
            self.send_json(404, {"error": {"message": "not found"}})

    def do_POST(self):
        if not self.path.rstrip("/").endswith("/chat/completions"):
            return self.send_json(404, {"error": {"message": "not found"}})
        req = json.loads(self.rfile.read(int(self.headers.get("Content-Length", 0))))
        system, prompt = split(req.get("messages", []))
        model = req.get("model", "")
        cid = f"chatcmpl-{int(time.time() * 1000)}"

        def chunk(delta, finish=None):
            return {"id": cid, "object": "chat.completion.chunk", "created": int(time.time()), "model": model,
                    "choices": [{"index": 0, "delta": delta, "finish_reason": finish}]}

        try:
            gen = RUN(model, system, prompt)
            if not req.get("stream"):
                text = "".join(gen)
                return self.send_json(200, {"id": cid, "object": "chat.completion", "model": model,
                    "choices": [{"index": 0, "message": {"role": "assistant", "content": text}, "finish_reason": "stop"}]})
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.send_header("Cache-Control", "no-cache")
            self.end_headers()
            for part in gen:
                self.wfile.write(b"data: " + json.dumps(chunk({"content": part})).encode() + b"\n\n")
                self.wfile.flush()
            self.wfile.write(b"data: " + json.dumps(chunk({}, "stop")).encode() + b"\n\ndata: [DONE]\n\n")
        except Exception as e:  # noqa
            if not req.get("stream"):
                return self.send_json(500, {"error": {"message": str(e)}})
            self.wfile.write(b"data: " + json.dumps({"error": {"message": str(e)}}).encode() + b"\n\n")


if __name__ == "__main__":
    print(f"Pont {BACKEND} -> http://127.0.0.1:{PORT}/v1  (Ctrl+C pour arrêter)")
    ThreadingHTTPServer(("127.0.0.1", PORT), H).serve_forever()
