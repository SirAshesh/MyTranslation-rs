# Agent Guidelines: Code Quality & Architecture Standards

This document establishes the mandatory engineering standards and constraints for all AI coding agents working in this repository. Every agent must strictly adhere to these rules without exception.

---

## 1. Clean Code Standards

* **Meaningful Naming:**
  * Use clear, descriptive nouns for types, structs, and classes (`Translator`, `HardwareInfo`, `BubbleGeometry`).
  * Use active, unambiguous verbs for functions and methods (`detect_hardware`, `resolve_openai_url`, `apply_settings`).
  * Avoid abbreviations, single-letter variables (except standard loop indices `i`, `j`), or ambiguous names (`data`, `temp`, `info` without qualifier).

* **Single Responsibility Principle (SRP):**
  * Each function and method must perform exactly one logical operation.
  * Keep functions focused, concise, and easy to test.

* **Linear Flow & Guard Clauses:**
  * Avoid deep nested `if-else` branching. Use guard clauses, early returns, and pattern matching (`match` / `if let`).
  * Keep happy-path logic unindented at the root level of the function body.

* **Robust Error Handling:**
  * Never use `.unwrap()` or `.expect()` in non-test production Rust paths where inputs or runtime environments could fail.
  * Always return descriptive typed errors (`Result<T, String>` or custom error types).
  * In frontend code, always handle promise rejections and network failures gracefully with clear user-facing feedback.

* **Zero Dead Code:**
  * Never leave dead, uncalled functions, unused variables, or orphaned imports.
  * Ensure the compiler and linters produce zero warnings (`cargo check`).

---

## 2. Full English Language Requirement

* **Strict English Policy:**
  * All source code, identifier names, variables, functions, structs, classes, modules, and constants must be written entirely in English.
  * All error messages, log statements (`tracing::info`, `console.error`), commit messages, and documentation files must be written in professional English.
  * No mixed-language or Romanized foreign words in variable names or code structures.
  * **Sole Exception:** Localized end-user translation datasets, dictionaries, or prompt output strings where a specific target language (e.g., natural Indonesian dialogue for comic translation) is explicitly mandated by the product specification.

---

## 4. No Unnecessary Function Declarations

* **YAGNI (You Aren't Gonna Need It):**
  * Never declare speculative, future-proofing, or hypothetical functions that are not actively invoked in the codebase.
  * Every declared function, struct, or method must serve an immediate, concrete purpose in the current architecture.

* **No Redundant One-Liner Wrappers:**
  * Avoid declaring functions that merely forward arguments to another function without adding any abstraction, validation, or business logic.
  * Call the underlying function directly unless an abstraction layer is genuinely justified.

* **Avoid Premature Extraction:**
  * Do not extract trivial single-use logic into standalone functions when keeping it inline maintains better locality and clarity.

---

## 5. Mandatory Function Reuse (DRY Principle)

* **Audit Before Writing:**
  * Before creating any new function, method, or helper, always audit existing modules and utilities to see if an equivalent function already exists.
  * Check shared modules first (`utilities/`, `pipe/`, `model/`).

* **Generalize Rather Than Duplicate:**
  * If an existing function almost fits the requirement, generalize its parameters or add an optional parameter rather than creating a duplicate or near-identical copy.
  * Never maintain two versions of the same algorithm or logic in different files.

* **Centralize Common Logic:**
  * Place shared network, formatting, or parsing utilities in dedicated utility modules (`src/utilities/`), making them easily reusable across handlers and pipeline components.

---

## 6. Strict File Length Limit: Maximum 600 Lines (No God Files)

* **Hard Constraint:**
  * **No single file may exceed 600 lines of code under any circumstances.**
  * God files, monolithic scripts, and oversized classes are strictly forbidden.

* **Proactive Decomposition at 500 Lines:**
  * When a file approaches 500 lines, it must be proactively refactored and split into cohesive sub-modules:
    * **Backend (Rust):**
      * Domain schemas and serialization models belong in `src/model/`.
      * Pipeline processing steps belong in individual modules under `src/pipe/` (e.g., `ocr.rs`, `llm_client.rs`, `renderer.rs`).
      * Shared system and hardware routines belong in `src/utilities/` (e.g., `hardware.rs`, `formatting.rs`).
      * HTTP endpoints belong in `src/handlers.rs` or domain-specific handler modules.
    * **Frontend:**
      * Decompose large JavaScript and CSS files by separating component logic, event listeners, and API clients.
      * Replace repetitive manual DOM construction with clean, concise template mapping.

---

## Agent Pre-Flight Checklist

Before completing any task, agents must verify:

1. [ ] **Line Count:** Run line count checks across all touched files. No file exceeds 600 lines.
2. [ ] **Zero Comment Tags:** Search for `TODO`, `FIXME`, `NOTE`, `HACK`, and eliminate all unnecessary comments.
3. [ ] **Language:** Confirm all code, types, logs, and docs are in 100% English.
4. [ ] **No Dead Code:** Ensure `cargo check` / build completes with zero errors and zero warnings.
5. [ ] **Function Reuse:** Confirm newly introduced logic does not duplicate existing helper functions.

