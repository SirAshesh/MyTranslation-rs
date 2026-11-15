(function () {
    function syncSelectFromInput(modelInput, modelSelect) {
        if (!modelInput || !modelSelect || modelSelect.hidden) {
            return;
        }
        const typed = modelInput.value.trim();
        const hasMatch = Array.from(modelSelect.options).some((opt) => opt.value && opt.value === typed);
        modelSelect.value = hasMatch ? typed : "";
    }

    function populateFetchedModels(modelInput, modelSelect, models) {
        modelSelect.innerHTML = "";

        const placeholder = document.createElement("option");
        placeholder.value = "";
        placeholder.textContent = "Select a model (" + models.length + " available)...";
        modelSelect.appendChild(placeholder);

        models.forEach((modelId) => {
            const opt = document.createElement("option");
            opt.value = modelId;
            opt.textContent = modelId;
            modelSelect.appendChild(opt);
        });

        modelSelect.hidden = false;
        const current = modelInput.value.trim();
        if (!current) {
            modelInput.value = models[0];
            modelSelect.value = models[0];
            return;
        }
        syncSelectFromInput(modelInput, modelSelect);
    }

    function bindProviderModelFetcher(config) {
        const { mode, label, urlInput, keyInput, modelInput, modelSelect, fetchBtn } = config;
        if (!urlInput || !keyInput || !modelInput || !modelSelect || !fetchBtn) {
            return;
        }

        modelSelect.addEventListener("change", () => {
            if (modelSelect.value) {
                modelInput.value = modelSelect.value;
            }
        });

        modelInput.addEventListener("input", () => {
            syncSelectFromInput(modelInput, modelSelect);
        });

        fetchBtn.addEventListener("click", async () => {
            const originalText = fetchBtn.textContent;
            fetchBtn.disabled = true;
            fetchBtn.textContent = "Loading...";
            setBanner(settingsStatusBox, "loading", "LOADING", "Fetching available models from " + label + "...");

            try {
                const res = await fetch("/api/models", {
                    method: "POST",
                    headers: { "Content-Type": "application/json" },
                    body: JSON.stringify({
                        mode,
                        base_url: urlInput.value.trim(),
                        api_key: keyInput.value.trim()
                    })
                });
                if (res.status === 401) {
                    window.location.href = "/login";
                    return;
                }
                const data = await res.json();
                if (data.success && Array.isArray(data.models) && data.models.length > 0) {
                    populateFetchedModels(modelInput, modelSelect, data.models);
                    setBanner(settingsStatusBox, "ok", "MODELS", data.message || ("Loaded " + data.models.length + " models."));
                } else {
                    const msg = data.message || ("Failed to load models from " + label + ".");
                    const isEmptyKey = msg.startsWith("Enter ");
                    setBanner(settingsStatusBox, isEmptyKey ? "warn" : "err", isEmptyKey ? "EMPTY" : "ERROR", msg);
                    if (isEmptyKey) {
                        keyInput.focus();
                    }
                }
            } catch (err) {
                setBanner(settingsStatusBox, "err", "ERROR", "Failed to fetch models from " + label + ": " + err.message);
            } finally {
                fetchBtn.disabled = false;
                fetchBtn.textContent = originalText;
            }
        });
    }

    bindProviderModelFetcher({
        mode: "openai",
        label: "OpenAI Compatible",
        urlInput: document.getElementById("openai-url"),
        keyInput: document.getElementById("openai-key"),
        modelInput: document.getElementById("openai-model"),
        modelSelect: document.getElementById("openai-model-select"),
        fetchBtn: document.getElementById("openai-get-models-btn")
    });

    bindProviderModelFetcher({
        mode: "claude",
        label: "Claude",
        urlInput: document.getElementById("claude-url"),
        keyInput: document.getElementById("claude-key"),
        modelInput: document.getElementById("claude-model"),
        modelSelect: document.getElementById("claude-model-select"),
        fetchBtn: document.getElementById("claude-get-models-btn")
    });
})();
