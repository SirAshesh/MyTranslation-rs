const localHardware = document.getElementById("local-hardware");
const localAutoBtn = document.getElementById("local-auto-btn");
const localProgress = document.getElementById("local-progress");
const localProgressFill = document.getElementById("local-progress-fill");
const localStatus = document.getElementById("local-status");
const localStartBtn = document.getElementById("local-start-btn");
const localStopBtn = document.getElementById("local-stop-btn");

let localPollTimer = null;
let hardwareModels = [];
let recommendedLocalQuant = "";

function formatLocalOptionName(quant) {
    const match = hardwareModels.find(m => m.quant === quant);
    if (match && match.name) return match.name;
    if (typeof quant === "string" && quant.toUpperCase().startsWith("HY-MT1.5-1.8B-")) {
        return "Tencent HY-MT1.5 1.8B (" + quant.slice("HY-MT1.5-1.8B-".length) + ")";
    }
    return quant ? "LiquidAI LFM3 1.7B (" + quant + ")" : "Local";
}

function buildLocalOptionHtml(m) {
    const rec = m.recommended ? " \u2014 Recommended" : "";
    const gb = (m.size_mb / 1024).toFixed(2);
    const label = m.name || formatLocalOptionName(m.quant);
    return `<option value="${escapeHtml(m.quant)}">${escapeHtml(label)} (${gb} GB)${rec}</option>`;
}

function renderLocalModelOptions(models) {
    const hymt = models.filter(m => m.family === "hy-mt1.5");
    const lfm3 = models.filter(m => m.family !== "hy-mt1.5");
    const sections = [];
    if (hymt.length > 0) {
        sections.push(`<optgroup label="Tencent HY-MT1.5 1.8B (Recommended for translation)">${hymt.map(buildLocalOptionHtml).join("")}</optgroup>`);
    }
    if (lfm3.length > 0) {
        sections.push(`<optgroup label="LiquidAI LFM3 1.7B (General compact LLM)">${lfm3.map(buildLocalOptionHtml).join("")}</optgroup>`);
    }
    return sections.join("");
}

async function loadHardwareInfo(forceRefresh, selectRecommended) {
    try {
        const endpoint = forceRefresh ? "/api/hardware?refresh=true" : "/api/hardware";
        const res = await fetch(endpoint);
        if (!res.ok) throw new Error("HTTP " + res.status);
        const data = await res.json();

        hardwareModels = Array.isArray(data.models) ? data.models : [];
        const recModel = hardwareModels.find(m => m.recommended) || hardwareModels[0] || null;
        recommendedLocalQuant = recModel ? recModel.quant : "";

        if (localHardware) {
            const gpuText = data.gpu
                ? `${data.gpu.name} (${data.gpu.vendor}, ${Math.round(data.gpu.vram_mb / 1024)} GB VRAM)`
                : "None";
            const cpuText = `${data.cpu.name} (${data.cpu.cores}c/${data.cpu.threads}t)`;
            const backendName = String(data.backend || "cpu").toUpperCase();
            const recText = recModel ? recModel.name : "HY-MT1.5 1.8B (Q4_K_M)";
            localHardware.innerHTML = `<strong>GPU:</strong> ${escapeHtml(gpuText)} &bull; <strong>CPU:</strong> ${escapeHtml(cpuText)} &bull; <strong>RAM:</strong> ${data.ram_gb} GB &bull; <strong>Backend:</strong> ${escapeHtml(backendName)} &bull; <strong>Recommended:</strong> ${escapeHtml(recText)}`;
        }

        if (localModelSelect && hardwareModels.length > 0) {
            const previousVal = localModelSelect.value;
            localModelSelect.innerHTML = renderLocalModelOptions(hardwareModels);
            const candidate = selectRecommended
                ? recommendedLocalQuant
                : (previousVal || (typeof savedLocalModel === "string" ? savedLocalModel : ""));
            const isValidCandidate = hardwareModels.some(m => m.quant === candidate);
            if (isValidCandidate) {
                localModelSelect.value = candidate;
            } else if (recommendedLocalQuant) {
                localModelSelect.value = recommendedLocalQuant;
            }
            if (localModelSelect.value && !savedLocalModel) {
                savedLocalModel = localModelSelect.value;
            }
        }
        pollLocalStatus();
    } catch (err) {
        if (localHardware) {
            localHardware.textContent = "Hardware detection unavailable: " + err.message;
        }
    }
}

async function pollLocalStatus() {
    try {
        const res = await fetch("/api/local/status");
        if (!res.ok) return;
        const status = await res.json();
        updateLocalUi(status);

        if (status.phase === "downloading_model" || status.phase === "installing_engine" || status.phase === "starting") {
            clearTimeout(localPollTimer);
            localPollTimer = setTimeout(pollLocalStatus, 800);
        } else if (status.phase === "running") {
            clearTimeout(localPollTimer);
            localPollTimer = setTimeout(pollLocalStatus, 5000);
        }
    } catch (_) {}
}

function updateLocalUi(status) {
    if (!localStatus) return;
    localStatus.setAttribute("data-phase", status.phase);

    const isInstalled = status.installed_quants && status.installed_quants.includes(localModelSelect ? localModelSelect.value : "");

    if (status.phase === "downloading_model" || status.phase === "installing_engine") {
        if (localProgress) localProgress.hidden = false;
        const pct = status.total_bytes > 0
            ? Math.min(100, Math.round((status.downloaded_bytes / status.total_bytes) * 100))
            : 0;
        if (localProgressFill) localProgressFill.style.width = pct + "%";
        localStatus.textContent = `${status.message} (${pct}%)`;
        if (localStartBtn) {
            localStartBtn.disabled = true;
            localStartBtn.textContent = "Downloading...";
        }
        if (localStopBtn) localStopBtn.disabled = false;
    } else if (status.phase === "starting") {
        if (localProgress) localProgress.hidden = true;
        localStatus.textContent = status.message || "Starting server...";
        if (localStartBtn) {
            localStartBtn.disabled = true;
            localStartBtn.textContent = "Starting...";
        }
        if (localStopBtn) localStopBtn.disabled = false;
    } else if (status.phase === "running") {
        if (localProgress) localProgress.hidden = true;
        localStatus.textContent = `Running (${formatLocalOptionName(status.active_quant)}) \u2022 Port 51057`;
        if (localStartBtn) {
            localStartBtn.disabled = true;
            localStartBtn.textContent = "Running";
        }
        if (localStopBtn) localStopBtn.disabled = false;
    } else if (status.phase === "failed") {
        if (localProgress) localProgress.hidden = true;
        localStatus.textContent = "Error: " + (status.message || "Execution failed");
        if (localStartBtn) {
            localStartBtn.disabled = false;
            localStartBtn.textContent = isInstalled ? "Start model" : "Download & start";
        }
        if (localStopBtn) localStopBtn.disabled = true;
    } else {
        if (localProgress) localProgress.hidden = true;
        localStatus.textContent = status.message || "Stopped";
        if (localStartBtn) {
            localStartBtn.disabled = false;
            localStartBtn.textContent = isInstalled ? "Start model" : "Download & start";
        }
        if (localStopBtn) localStopBtn.disabled = true;
    }
}

if (localStartBtn) {
    localStartBtn.addEventListener("click", async () => {
        localStartBtn.disabled = true;
        localStartBtn.textContent = "Requesting...";
        const quant = (localModelSelect && localModelSelect.value) || recommendedLocalQuant || "HY-MT1.5-1.8B-Q4_K_M";
        try {
            const res = await fetch("/api/local/start", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({ quant })
            });
            const data = await res.json();
            if (!res.ok) {
                setBanner(settingsStatusBox, "err", "ERROR", data.message || "Failed to start local model.");
            }
        } catch (err) {
            setBanner(settingsStatusBox, "err", "ERROR", "Network error: " + err.message);
        }
        pollLocalStatus();
    });
}

if (localStopBtn) {
    localStopBtn.addEventListener("click", async () => {
        localStopBtn.disabled = true;
        localStopBtn.textContent = "Stopping...";
        try {
            await fetch("/api/local/stop", { method: "POST" });
        } catch (_) {}
        pollLocalStatus();
    });
}

if (localModelSelect) {
    localModelSelect.addEventListener("change", () => {
        pollLocalStatus();
    });
}

if (localAutoBtn) {
    localAutoBtn.addEventListener("click", async () => {
        localAutoBtn.disabled = true;
        localAutoBtn.textContent = "Detecting...";
        await loadHardwareInfo(true, true);
        localAutoBtn.disabled = false;
        localAutoBtn.textContent = "Auto-detect";
    });
}

loadHardwareInfo(false, false);
pollLocalStatus();

