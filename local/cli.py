import os
import sys
import time
import warnings
import logging
import re
from pathlib import Path
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

model_name = "Helsinki-NLP/opus-mt-en-id"

print("=" * 60)
print("Loading MarianMT model from local cache...")
t0 = time.time()

tokenizer = MarianTokenizer.from_pretrained(model_name, cache_dir=str(cache_dir))
model = MarianMTModel.from_pretrained(model_name, cache_dir=str(cache_dir))
model.eval()

dummy = tokenizer(["Warmup"], return_tensors="pt", padding=True)
with torch.inference_mode():
    _ = model.generate(**dummy, max_new_tokens=10)

print(f"Model ready in {time.time() - t0:.2f}s!")
print("=" * 60)
print("MarianMT Interactive CLI (EN -> ID)")
print("Type any English text to translate. Type 'q' or 'exit' to quit.")
print("=" * 60)

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

while True:
    try:
        user_input = input("\n[EN] > ").strip()
        if not user_input:
            continue
        if user_input.lower() in ["q", "exit", "quit"]:
            print("Goodbye!")
            break

        t_start = time.perf_counter()
        cleaned_input = preprocess_text(user_input)
        inputs = tokenizer([cleaned_input], return_tensors="pt", padding=True)
        with torch.inference_mode():
            output_ids = model.generate(**inputs, max_new_tokens=60, num_beams=1)
        translated = tokenizer.decode(output_ids[0], skip_special_tokens=True)
        elapsed_ms = (time.perf_counter() - t_start) * 1000

        print(f"[ID] ({elapsed_ms:.1f}ms) > {translated}")
    except KeyboardInterrupt:
        print("\nGoodbye!")
        break
    except Exception as e:
        print(f"Error: {e}")
