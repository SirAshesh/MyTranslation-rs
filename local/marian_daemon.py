import sys
import json
import os
import warnings
import logging
import re
from pathlib import Path
from http.server import HTTPServer, BaseHTTPRequestHandler
import wordninja

warnings.filterwarnings("ignore")
logging.getLogger("transformers").setLevel(logging.ERROR)

local_dir = Path(__file__).resolve().parent
cache_dir = local_dir / "cache"

os.environ["HF_HOME"] = str(cache_dir)
os.environ["TRANSFORMERS_CACHE"] = str(cache_dir)
os.environ["HF_HUB_DISABLE_SYMLINKS_WARNING"] = "1"

import torch
from transformers import MarianMTModel, MarianTokenizer

PORT = 51056
model_name = "Helsinki-NLP/opus-mt-en-id"

print(f"[Marian Daemon] Initializing MarianMT model from {cache_dir}...", flush=True)
tokenizer = MarianTokenizer.from_pretrained(model_name, cache_dir=str(cache_dir))
model = MarianMTModel.from_pretrained(model_name, cache_dir=str(cache_dir))
model.eval()

dummy = tokenizer(["Warmup"], return_tensors="pt", padding=True)
with torch.inference_mode():
    _ = model.generate(**dummy, max_new_tokens=10)

print(f"[Marian Daemon] Model ready. Starting HTTP server on port {PORT}...", flush=True)

def preprocess_text(text: str) -> str:
    tokens = text.split()
    fixed_tokens = []
    for token in tokens:
        m = re.match(r'^([^\w]*)([\w\'-]+)([^\w]*)$', token)
        if not m:
            fixed_tokens.append(token)
            continue
        prefix, word, suffix = m.groups()
        if len(word) >= 4 and word.isupper():
            parts = wordninja.split(word)
            if len(parts) > 1 and all(len(p) >= 1 for p in parts):
                fixed_tokens.append(prefix + ' '.join(parts) + suffix)
            else:
                fixed_tokens.append(token)
        else:
            fixed_tokens.append(token)
    cleaned = ' '.join(fixed_tokens)
    s = cleaned.strip()
    if s.isupper():
        s = s.lower()
        s = re.sub(r'(?:^|[.!?]\s+)([a-z])', lambda m: m.group(0).upper(), s)
        s = re.sub(r'\bi\b', 'I', s)
        s = re.sub(r"\bi'", "I'", s)
    return s

class MarianHandler(BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path == "/health":
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(b'{"status":"ok","ready":true}')
        else:
            self.send_response(404)
            self.end_headers()

    def do_POST(self):
        if self.path == "/translate":
            try:
                length = int(self.headers.get("Content-Length", 0))
                body = self.rfile.read(length)
                data = json.loads(body.decode("utf-8"))
                items = data.get("items", [])
                if not items:
                    self.send_response(200)
                    self.send_header("Content-Type", "application/json")
                    self.end_headers()
                    self.wfile.write(b'{"translations":[]}')
                    return

                texts = [preprocess_text(item.get("text", "")) for item in items]
                inputs = tokenizer(texts, return_tensors="pt", padding=True)
                with torch.inference_mode():
                    output_ids = model.generate(**inputs, max_new_tokens=60, num_beams=1)
                outputs = tokenizer.batch_decode(output_ids, skip_special_tokens=True)

                res_items = []
                for item, translated in zip(items, outputs):
                    res_items.append({
                        "id": item.get("id"),
                        "target": translated
                    })

                resp_bytes = json.dumps({"translations": res_items}).encode("utf-8")
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(resp_bytes)))
                self.end_headers()
                self.wfile.write(resp_bytes)
            except Exception as e:
                err_bytes = json.dumps({"error": str(e)}).encode("utf-8")
                self.send_response(500)
                self.send_header("Content-Type", "application/json")
                self.end_headers()
                self.wfile.write(err_bytes)
        else:
            self.send_response(404)
            self.end_headers()

    def log_message(self, format, *args):
        sys.stderr.write(f"[Marian Daemon] {args[0]} {args[1]} -> {args[2]}\n")

def run():
    server_address = ("127.0.0.1", PORT)
    httpd = HTTPServer(server_address, MarianHandler)
    print(f"[Marian Daemon] Listening on http://127.0.0.1:{PORT}/translate", flush=True)
    try:
        httpd.serve_forever()
    except KeyboardInterrupt:
        pass

if __name__ == "__main__":
    run()
