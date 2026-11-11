import os
import sys
import time
import warnings
import logging
from pathlib import Path

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

while True:
    try:
        user_input = input("\n[EN] > ").strip()
        if not user_input:
            continue
        if user_input.lower() in ["q", "exit", "quit"]:
            print("Goodbye!")
            break

        t_start = time.perf_counter()
        inputs = tokenizer([user_input], return_tensors="pt", padding=True)
        with torch.inference_mode():
            output_ids = model.generate(**inputs, max_new_tokens=60, num_beams=1)
        translated = tokenizer.decode(output_ids[0], skip_special_tokens=True)
        elapsed_ms = (time.perf_counter() - t_start) * 1000

        print(f"[ID] > {translated}")
        print(f"[SPEED] > {elapsed_ms:.1f} ms ({elapsed_ms / 1000:.3f}s)")
    except (KeyboardInterrupt, EOFError):
        print("\nGoodbye!")
        break
