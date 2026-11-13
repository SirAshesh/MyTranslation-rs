const localHardware = document.getElementById("local-hardware");
const localProgress = document.getElementById("local-progress");
const localProgressFill = document.getElementById("local-progress-fill");
const localStatus = document.getElementById("local-status");
const localStartBtn = document.getElementById("local-start-btn");
const localStopBtn = document.getElementById("local-stop-btn");

let localPollTimer = null;

async function loadHardwareInfo() {
    try {
        const res = await fetch("/api/hardware");
        if (!res.ok) throw new Error("HTTP " + res.status);
        const data = await res.json();

        if (localHardware) {
            const gpuText = data.gpu
                ? `${data.gpu.name} (${data.gpu.vendor}, ${Math.round(data.gpu.vram_mb / 1024)} GB VRAM)`
                : "None";
            const cpuText = `${data.cpu.name} (${data.cpu.cores}c/${data.cpu.threads}t)`;
            const backendName = String(data.backend || "cpu").toUpperCase();
            localHardware.innerHTML = `<strong>GPU:</strong> ${escapeHtml(gpuText)} &bull; <strong>CPU:</strong> ${escapeHtml(cpuText)} &bull; <strong>RAM:</strong> ${data.ram_gb} GB &bull; <strong>Backend:</strong> ${escapeHtml(backendName)}`;
        }

        if (localModelSelect && Array.isArray(data.models)) {
            const currentVal = localModelSelect.value;
            localModelSelect.innerHTML = data.models.map(m => {
                const rec = m.recommended ? " \u2014 Recommended" : "";
                const gb = (m.size_mb / 1024).toFixed(2);
                return `<option value="${escapeHtml(m.quant)}">LiquidAI LFM3 1.7B (${escapeHtml(m.quant)}, ${gb} GB)${rec}</option>`;
            }).join("");
            if (currentVal) {
                localModelSelect.value = currentVal;
            }
        }
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
        localStatus.textContent = `Running (${status.active_quant || "Local"}) \u2022 Port 51057`;
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
        const quant = localModelSelect ? localModelSelect.value : "Q6_K";
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

loadHardwareInfo();
pollLocalStatus();

