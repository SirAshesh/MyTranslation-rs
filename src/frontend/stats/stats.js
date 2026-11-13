(function () {
    const statsPanel = document.getElementById("panel-statistics");
    const statsTabBtn = document.getElementById("tab-statistics");
    if (!statsPanel || !statsTabBtn) {
        return;
    }

    if (typeof viewPanels === "object" && viewPanels !== null) {
        viewPanels.statistics = statsPanel;
    }

    const COLORS = {
        ocr: "#60a5fa",
        tr: "#4ade80",
        ren: "#facc15",
        err: "#f87171",
        accent: "#a78bfa"
    };

    const el = {
        uptime: document.getElementById("stats-uptime"),
        autoRefresh: document.getElementById("stats-auto-refresh"),
        refreshBtn: document.getElementById("stats-refresh-btn"),
        resetBtn: document.getElementById("stats-reset-btn"),
        kpiRequests: document.getElementById("stats-kpi-requests"),
        kpiPages: document.getElementById("stats-kpi-pages"),
        kpiSuccess: document.getElementById("stats-kpi-success"),
        kpiErrors: document.getElementById("stats-kpi-errors"),
        kpiBubbles: document.getElementById("stats-kpi-bubbles"),
        kpiMode: document.getElementById("stats-kpi-mode"),
        kpiLatency: document.getElementById("stats-kpi-latency"),
        kpiLast: document.getElementById("stats-kpi-last"),
        chartLatency: document.getElementById("chart-latency"),
        chartTimeline: document.getElementById("chart-timeline"),
        chartStages: document.getElementById("chart-stages"),
        chartModels: document.getElementById("chart-models"),
        recentTbody: document.getElementById("stats-recent-tbody"),
        tooltip: document.getElementById("stats-tooltip"),
        statusBox: document.getElementById("stats-status-box")
    };

    function escapeHtml(str) {
        return String(str)
            .replace(/&/g, "&amp;")
            .replace(/</g, "&lt;")
            .replace(/>/g, "&gt;")
            .replace(/"/g, "&quot;");
    }

    function formatDuration(secs) {
        const s = Math.max(0, Math.floor(Number(secs) || 0));
        if (s < 60) return s + "s";
        const m = Math.floor(s / 60);
        const remS = s % 60;
        if (m < 60) return m + "m " + remS + "s";
        const h = Math.floor(m / 60);
        return h + "h " + (m % 60) + "m";
    }

    function formatClockTime(ms) {
        if (!ms) return "-";
        const d = new Date(ms);
        return [d.getHours(), d.getMinutes(), d.getSeconds()]
            .map((v) => String(v).padStart(2, "0"))
            .join(":");
    }

    function formatBucketMinute(minuteEpoch) {
        const d = new Date(minuteEpoch * 60000);
        return [d.getHours(), d.getMinutes()]
            .map((v) => String(v).padStart(2, "0"))
            .join(":");
    }

    function setStatsStatus(state, prefix, text) {
        if (typeof setBanner === "function" && el.statusBox) {
            setBanner(el.statusBox, state, prefix, text);
        }
    }

    function attachTooltips(container) {
        if (!container || !el.tooltip) return;
        container.querySelectorAll("[data-tip]").forEach((node) => {
            node.addEventListener("mouseenter", () => {
                el.tooltip.textContent = node.getAttribute("data-tip") || "";
                el.tooltip.hidden = false;
            });
            node.addEventListener("mousemove", (e) => {
                el.tooltip.style.left = Math.min(window.innerWidth - 220, e.clientX + 12) + "px";
                el.tooltip.style.top = Math.max(8, e.clientY - 36) + "px";
            });
            node.addEventListener("mouseleave", () => {
                el.tooltip.hidden = true;
            });
        });
    }

    function renderEmptyChart(container, msg) {
        container.innerHTML = '<div class="empty-text">' + escapeHtml(msg) + "</div>";
    }

    function renderLatencyChart(recent) {
        if (!Array.isArray(recent) || recent.length === 0) {
            renderEmptyChart(el.chartLatency, "No translation requests recorded yet.");
            return;
        }
        const items = recent.slice(-16);
        const w = 520;
        const h = 210;
        const pad = { top: 16, right: 14, bottom: 28, left: 46 };
        const plotW = w - pad.left - pad.right;
        const plotH = h - pad.top - pad.bottom;
        const maxMs = Math.max(100, ...items.map((r) => r.total_ms || 0));
        const slotW = plotW / items.length;
        const barW = Math.max(8, Math.min(24, slotW * 0.62));

        let gridSvg = "";
        [0, 0.5, 1].forEach((ratio) => {
            const y = pad.top + plotH * (1 - ratio);
            const val = Math.round(maxMs * ratio);
            gridSvg += '<line class="chart-grid-line" x1="' + pad.left + '" y1="' + y + '" x2="' + (w - pad.right) + '" y2="' + y + '" />';
            gridSvg += '<text class="chart-axis-text" x="' + (pad.left - 6) + '" y="' + (y + 3) + '" text-anchor="end">' + val + "</text>";
        });

        let barsSvg = "";
        items.forEach((item, i) => {
            const x = pad.left + i * slotW + (slotW - barW) / 2;
            const total = Math.max(1, item.total_ms || 0);
            const tip = [
                "#" + item.seq + " (" + item.endpoint + ") - " + formatClockTime(item.timestamp_ms),
                "Model: " + item.provider_model,
                "Total: " + item.total_ms + " ms | Bubbles: " + item.bubbles,
                item.success ? ("OCR " + item.ocr_ms + "ms / LLM " + item.translate_ms + "ms / Render " + item.render_ms + "ms") : "Status: FAILED"
            ].join("\n");

            if (!item.success) {
                const bh = Math.max(4, (total / maxMs) * plotH);
                const y = pad.top + plotH - bh;
                barsSvg += '<rect x="' + x + '" y="' + y + '" width="' + barW + '" height="' + bh + '" rx="2" fill="' + COLORS.err + '" data-tip="' + escapeHtml(tip) + '" />';
            } else {
                const segs = [
                    { v: item.ocr_ms || 0, c: COLORS.ocr },
                    { v: item.translate_ms || 0, c: COLORS.tr },
                    { v: item.render_ms || 0, c: COLORS.ren }
                ];
                let cursorY = pad.top + plotH;
                segs.forEach((seg) => {
                    if (seg.v <= 0) return;
                    const sh = Math.max(2, (seg.v / maxMs) * plotH);
                    cursorY -= sh;
                    barsSvg += '<rect x="' + x + '" y="' + cursorY + '" width="' + barW + '" height="' + sh + '" fill="' + seg.c + '" data-tip="' + escapeHtml(tip) + '" />';
                });
                const accounted = (item.ocr_ms || 0) + (item.translate_ms || 0) + (item.render_ms || 0);
                if (accounted === 0) {
                    const sh = Math.max(4, (total / maxMs) * plotH);
                    barsSvg += '<rect x="' + x + '" y="' + (pad.top + plotH - sh) + '" width="' + barW + '" height="' + sh + '" rx="2" fill="' + COLORS.tr + '" data-tip="' + escapeHtml(tip) + '" />';
                }
            }
            if (items.length <= 10 || i % 2 === 0 || i === items.length - 1) {
                barsSvg += '<text class="chart-axis-text" x="' + (x + barW / 2) + '" y="' + (h - 8) + '" text-anchor="middle">#' + item.seq + "</text>";
            }
        });

        el.chartLatency.innerHTML = '<svg viewBox="0 0 ' + w + " " + h + '" role="img" aria-label="Latency per request chart">' + gridSvg + barsSvg + "</svg>";
        attachTooltips(el.chartLatency);
    }

    function renderTimelineChart(timeline) {
        if (!Array.isArray(timeline) || timeline.length === 0) {
            renderEmptyChart(el.chartTimeline, "Waiting for traffic to plot timeline.");
            return;
        }
        const w = 520;
        const h = 210;
        const pad = { top: 16, right: 16, bottom: 28, left: 36 };
        const plotW = w - pad.left - pad.right;
        const plotH = h - pad.top - pad.bottom;
        const maxVal = Math.max(2, ...timeline.map((b) => Math.max(b.requests || 0, b.bubbles || 0)));
        const slotW = plotW / timeline.length;
        const barW = Math.max(10, Math.min(28, slotW * 0.52));

        let gridSvg = "";
        [0, 0.5, 1].forEach((ratio) => {
            const y = pad.top + plotH * (1 - ratio);
            const val = Math.round(maxVal * ratio);
            gridSvg += '<line class="chart-grid-line" x1="' + pad.left + '" y1="' + y + '" x2="' + (w - pad.right) + '" y2="' + y + '" />';
            gridSvg += '<text class="chart-axis-text" x="' + (pad.left - 6) + '" y="' + (y + 3) + '" text-anchor="end">' + val + "</text>";
        });

        let barsSvg = "";
        const pts = [];
        timeline.forEach((b, i) => {
            const cx = pad.left + i * slotW + slotW / 2;
            const bx = cx - barW / 2;
            const reqH = ((b.requests || 0) / maxVal) * plotH;
            const errH = ((b.errors || 0) / maxVal) * plotH;
            const tip = [
                "Minute " + formatBucketMinute(b.minute_epoch),
                "Requests: " + b.requests + " (" + b.errors + " failed)",
                "Bubbles: " + b.bubbles + " | Avg: " + b.avg_ms + " ms"
            ].join("\n");

            if (reqH > 0) {
                barsSvg += '<rect x="' + bx + '" y="' + (pad.top + plotH - reqH) + '" width="' + barW + '" height="' + reqH + '" rx="2" fill="' + COLORS.ocr + '" fill-opacity="0.85" data-tip="' + escapeHtml(tip) + '" />';
            }
            if (errH > 0) {
                barsSvg += '<rect x="' + bx + '" y="' + (pad.top + plotH - errH) + '" width="' + barW + '" height="' + errH + '" rx="2" fill="' + COLORS.err + '" data-tip="' + escapeHtml(tip) + '" />';
            }
            const bubY = pad.top + plotH - ((b.bubbles || 0) / maxVal) * plotH;
            pts.push({ x: cx, y: bubY, tip });
            if (timeline.length <= 8 || i % 2 === 0 || i === timeline.length - 1) {
                barsSvg += '<text class="chart-axis-text" x="' + cx + '" y="' + (h - 8) + '" text-anchor="middle">' + formatBucketMinute(b.minute_epoch) + "</text>";
            }
        });

        let lineSvg = "";
        if (pts.length > 1) {
            lineSvg += '<polyline fill="none" stroke="' + COLORS.tr + '" stroke-width="2" points="' + pts.map((p) => p.x.toFixed(1) + "," + p.y.toFixed(1)).join(" ") + '" />';
        }
        pts.forEach((p) => {
            lineSvg += '<circle cx="' + p.x.toFixed(1) + '" cy="' + p.y.toFixed(1) + '" r="3.5" fill="' + COLORS.tr + '" data-tip="' + escapeHtml(p.tip) + '" />';
        });

        el.chartTimeline.innerHTML = '<svg viewBox="0 0 ' + w + " " + h + '" role="img" aria-label="Throughput over time chart">' + gridSvg + barsSvg + lineSvg + "</svg>";
        attachTooltips(el.chartTimeline);
    }

    function renderStagesChart(stages, totalRequests) {
        if (!totalRequests || !stages) {
            renderEmptyChart(el.chartStages, "Run at least one request to view stage breakdown.");
            return;
        }
        const items = [
            { label: "OCR Detection", ms: stages.ocr_ms || 0, color: COLORS.ocr },
            { label: "LLM Translation", ms: stages.translate_ms || 0, color: COLORS.tr },
            { label: "Inpaint & Render", ms: stages.render_ms || 0, color: COLORS.ren }
        ];
        const sumMs = Math.max(1, items.reduce((acc, s) => acc + s.ms, 0));
        const w = 520;
        const h = 195;
        const cx = 92;
        const cy = 96;
        const r = 56;
        const strokeW = 20;
        const circ = 2 * Math.PI * r;

        let offset = 0;
        let arcsSvg = '<circle cx="' + cx + '" cy="' + cy + '" r="' + r + '" fill="none" stroke="var(--rule)" stroke-width="' + strokeW + '" />';
        items.forEach((seg) => {
            if (seg.ms <= 0) return;
            const len = (seg.ms / sumMs) * circ;
            arcsSvg += '<circle cx="' + cx + '" cy="' + cy + '" r="' + r + '" fill="none" stroke="' + seg.color + '" stroke-width="' + strokeW + '" stroke-dasharray="' + len.toFixed(2) + " " + circ.toFixed(2) + '" stroke-dashoffset="' + (-offset).toFixed(2) + '" transform="rotate(-90 ' + cx + " " + cy + ')" data-tip="' + escapeHtml(seg.label + ": " + seg.ms + " ms avg") + '" />';
            offset += len;
        });

        arcsSvg += '<text class="chart-val-text" x="' + cx + '" y="' + (cy - 2) + '" text-anchor="middle" style="font-size:14px;font-weight:600">' + (stages.total_ms || sumMs) + " ms</text>";
        arcsSvg += '<text class="chart-axis-text" x="' + cx + '" y="' + (cy + 14) + '" text-anchor="middle">avg total</text>';

        let rowsSvg = "";
        items.forEach((seg, idx) => {
            const pct = Math.round((seg.ms / sumMs) * 100);
            const y = 36 + idx * 48;
            const barMaxW = 290;
            const fillW = Math.max(seg.ms > 0 ? 4 : 0, (seg.ms / sumMs) * barMaxW);
            rowsSvg += '<text class="chart-val-text" x="196" y="' + y + '">' + seg.label + "</text>";
            rowsSvg += '<text class="chart-axis-text" x="486" y="' + y + '" text-anchor="end">' + seg.ms + " ms (" + pct + "%)</text>";
            rowsSvg += '<rect x="196" y="' + (y + 8) + '" width="' + barMaxW + '" height="10" rx="3" fill="var(--surface-alt)" />';
            rowsSvg += '<rect x="196" y="' + (y + 8) + '" width="' + fillW + '" height="10" rx="3" fill="' + seg.color + '" />';
        });

        el.chartStages.innerHTML = '<svg viewBox="0 0 ' + w + " " + h + '" role="img" aria-label="Pipeline stage breakdown chart">' + arcsSvg + rowsSvg + "</svg>";
        attachTooltips(el.chartStages);
    }

    function renderModelsChart(byModel, byEndpoint) {
        if ((!Array.isArray(byModel) || byModel.length === 0) && (!Array.isArray(byEndpoint) || byEndpoint.length === 0)) {
            renderEmptyChart(el.chartModels, "No model or endpoint usage recorded yet.");
            return;
        }
        const models = (byModel || []).slice(0, 5);
        const maxReq = Math.max(1, ...models.map((m) => m.requests || 0));
        const w = 520;
        const rowH = 34;
        const h = Math.max(195, 58 + models.length * rowH);
        const barW = 240;

        let svg = "";
        models.forEach((m, idx) => {
            const y = 26 + idx * rowH;
            const label = m.name.length > 26 ? m.name.slice(0, 23) + "..." : m.name;
            const okW = ((m.success || 0) / maxReq) * barW;
            const errW = ((m.errors || 0) / maxReq) * barW;
            const tip = m.name + "\nRequests: " + m.requests + " (" + m.errors + " errors)\nBubbles: " + m.bubbles + " | Avg: " + m.avg_ms + " ms";
            svg += '<text class="chart-axis-text" x="8" y="' + (y + 11) + '">' + escapeHtml(label) + "</text>";
            svg += '<rect x="180" y="' + y + '" width="' + barW + '" height="14" rx="3" fill="var(--surface-alt)" />';
            if (okW > 0) {
                svg += '<rect x="180" y="' + y + '" width="' + okW + '" height="14" rx="3" fill="' + COLORS.accent + '" data-tip="' + escapeHtml(tip) + '" />';
            }
            if (errW > 0) {
                svg += '<rect x="' + (180 + okW) + '" y="' + y + '" width="' + errW + '" height="14" rx="3" fill="' + COLORS.err + '" data-tip="' + escapeHtml(tip) + '" />';
            }
            svg += '<text class="chart-axis-text" x="430" y="' + (y + 11) + '">' + m.requests + "x (" + m.avg_ms + "ms)</text>";
        });

        const epSummary = (byEndpoint || []).map((ep) => ep.name + ": " + ep.requests).join("  |  ");
        svg += '<text class="chart-axis-text" x="8" y="' + (h - 10) + '">' + escapeHtml("Endpoints: " + (epSummary || "-")) + "</text>";

        el.chartModels.innerHTML = '<svg viewBox="0 0 ' + w + " " + h + '" role="img" aria-label="Models and endpoints usage chart">' + svg + "</svg>";
        attachTooltips(el.chartModels);
    }

    function renderRecentTable(recent) {
        if (!Array.isArray(recent) || recent.length === 0) {
            el.recentTbody.innerHTML = '<tr><td colspan="8" class="empty-text">No requests recorded yet.</td></tr>';
            return;
        }
        el.recentTbody.innerHTML = recent
            .slice(-12)
            .reverse()
            .map((r) => {
                const badge = r.success
                    ? '<span class="stats-badge-ok">OK</span>'
                    : '<span class="stats-badge-err">FAIL</span>';
                const stageStr = r.ocr_ms + " / " + r.translate_ms + " / " + r.render_ms + " ms";
                return (
                    "<tr>" +
                    '<td class="mono">#' + r.seq + "</td>" +
                    '<td class="mono">' + escapeHtml(formatClockTime(r.timestamp_ms)) + "</td>" +
                    '<td class="mono">' + escapeHtml(r.endpoint) + "</td>" +
                    '<td class="mono">' + escapeHtml(r.provider_model) + "</td>" +
                    '<td class="mono">' + r.bubbles + "</td>" +
                    '<td class="mono">' + escapeHtml(stageStr) + "</td>" +
                    '<td class="mono">' + r.total_ms + " ms</td>" +
                    "<td>" + badge + "</td>" +
                    "</tr>"
                );
            })
            .join("");
    }

    function renderSnapshot(st) {
        const total = st.total_requests ?? 0;
        const ok = st.total_success ?? 0;
        const err = st.total_errors ?? 0;
        const rate = total > 0 ? ((ok / total) * 100).toFixed(1) + "%" : "100%";

        el.uptime.textContent = "Uptime: " + formatDuration(st.uptime_secs || 0);
        el.kpiRequests.textContent = String(total);
        el.kpiPages.textContent = (st.total_processed ?? 0) + " pages/regions ok";
        el.kpiSuccess.textContent = rate;
        el.kpiErrors.textContent = err + " failed";
        el.kpiBubbles.textContent = String(st.total_bubbles ?? 0);
        el.kpiMode.textContent = "Mode: " + (st.translation_mode || "-");
        el.kpiLatency.textContent = (st.avg_total_ms ?? 0) + " ms";
        el.kpiLast.textContent = "Last: " + (st.last_total_ms ?? 0) + " ms";

        renderLatencyChart(st.recent_requests || []);
        renderTimelineChart(st.timeline || []);
        renderStagesChart(st.stage_averages, total);
        renderModelsChart(st.by_model, st.by_endpoint);
        renderRecentTable(st.recent_requests || []);
    }

    async function loadStatistics(announce) {
        if (announce) {
            setStatsStatus("loading", "LOADING", "Refreshing telemetry...");
        }
        try {
            const res = await fetch("/stats");
            if (res.status === 401) {
                window.location.href = "/login";
                return;
            }
            if (!res.ok) {
                throw new Error("HTTP " + res.status);
            }
            const data = await res.json();
            renderSnapshot(data);
            setStatsStatus("ok", "LIVE", "Telemetry updated (" + (data.total_requests ?? 0) + " requests tracked).");
        } catch (err) {
            setStatsStatus("err", "ERROR", "Failed to load statistics: " + err.message);
        }
    }

    el.refreshBtn.addEventListener("click", () => loadStatistics(true));
    el.resetBtn.addEventListener("click", async () => {
        try {
            const res = await fetch("/api/stats/reset", { method: "POST" });
            if (!res.ok) {
                throw new Error("HTTP " + res.status);
            }
            const data = await res.json();
            renderSnapshot(data);
            setStatsStatus("ok", "RESET", "In-memory statistics cleared.");
        } catch (err) {
            setStatsStatus("err", "ERROR", "Failed to reset statistics: " + err.message);
        }
    });

    statsTabBtn.addEventListener("click", () => {
        loadStatistics(false);
    });

    setInterval(() => {
        if (statsPanel.classList.contains("active") && el.autoRefresh && el.autoRefresh.checked) {
            loadStatistics(false);
        }
    }, 5000);

    loadStatistics(false);
})();

