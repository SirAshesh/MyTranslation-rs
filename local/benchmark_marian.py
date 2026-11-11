import os
import sys
import time
from pathlib import Path

local_dir = Path(__file__).resolve().parent
cache_dir = local_dir / "cache"
cache_dir.mkdir(parents=True, exist_ok=True)

os.environ["HF_HOME"] = str(cache_dir)
os.environ["TRANSFORMERS_CACHE"] = str(cache_dir)

import torch
from transformers import MarianMTModel, MarianTokenizer

model_name = "Helsinki-NLP/opus-mt-en-id"

print("=" * 60)
print("LOADING MARIAN MT (en -> id) INTO LOCAL CACHE...")
print("=" * 60)
t_load_start = time.time()

tokenizer = MarianTokenizer.from_pretrained(model_name, cache_dir=str(cache_dir))
model = MarianMTModel.from_pretrained(model_name, cache_dir=str(cache_dir))
model.eval()

t_load = time.time() - t_load_start
print(f"Model loaded in {t_load:.2f}s\n")

comic_dialogues = [
    "But I thought he got what he ordered?",
    "Why he...?",
    "Thanks for letting me in.",
    "He has no reason to come out here anymore.",
    "Oh!",
    "...Sure.",
    "Like the behavior of those who drink the potion.",
    "Still, you did warn me three times about potential side effects.",
    "Before you drink the potion, you said it doesn't work well on witches.",
    "I have no reason to doubt its efficacy."
]

dummy_tokens = tokenizer(["Hello world"], return_tensors="pt", padding=True)
with torch.inference_mode():
    _ = model.generate(**dummy_tokens, max_new_tokens=20)

print("=" * 60)
print("TEST 1: SINGLE BUBBLE LATENCY (One by one)")
print("=" * 60)

single_latencies = []
for i, text in enumerate(comic_dialogues):
    t0 = time.time()
    inputs = tokenizer([text], return_tensors="pt", padding=True)
    with torch.inference_mode():
        translated_ids = model.generate(**inputs, max_new_tokens=40, num_beams=1)
    output = tokenizer.decode(translated_ids[0], skip_special_tokens=True)
    elapsed = (time.time() - t0) * 1000
    single_latencies.append(elapsed)
    print(f"[{i+1:2d}] {elapsed:6.1f}ms | EN: {text}")
    print(f"     -> ID: {output}")

avg_single = sum(single_latencies) / len(single_latencies)
print(f"\nAverage single-bubble translation: {avg_single:.1f}ms")

print("\n" + "=" * 60)
print("TEST 2: BATCH BUBBLE LATENCY (Entire comic page at once - 10 bubbles)")
print("=" * 60)

batch_latencies = []
batch_results = []
for run in range(5):
    t0 = time.time()
    inputs = tokenizer(comic_dialogues, return_tensors="pt", padding=True)
    with torch.inference_mode():
        translated_ids = model.generate(**inputs, max_new_tokens=40, num_beams=1)
    outputs = tokenizer.batch_decode(translated_ids, skip_special_tokens=True)
    elapsed = (time.time() - t0) * 1000
    batch_latencies.append(elapsed)
    if run == 0:
        batch_results = outputs

for en, id_text in zip(comic_dialogues, batch_results):
    print(f"EN: {en}\nID: {id_text}\n")

avg_batch = sum(batch_latencies) / len(batch_latencies)
print(f"10 Bubbles translated in parallel: {avg_batch:.1f}ms ({avg_batch/1000:.3f} seconds)")
print(f"Per-bubble cost in batch: {avg_batch/len(comic_dialogues):.1f}ms")

print("\n" + "=" * 60)
print("SUMMARY & SPEED EVALUATION")
print("=" * 60)
print(f"1 Page (10 speech bubbles in batch): {avg_batch:.1f}ms")
print(f"Single speech bubble:                {avg_single:.1f}ms")
if avg_batch < 1000:
    print(f"RESULT: PURE UNDER 1 SECOND ACHIEVED! ({avg_batch/1000:.2f}s << 1.0s)")
