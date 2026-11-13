(function () {
  if (window.__myTranslationContentLoaded) {
    return;
  }
  window.__myTranslationContentLoaded = true;

  const cap = window.MyTransCapture;
  const ui = window.MyTransUi;
  const settings = {
    serverURL: "http://127.0.0.1:5000",
    sourceLang: "auto",
    targetLang: "id",
    pickingWay: "1",
    useCanvas: true,
    sendRequestsViaBackground: true,
    showFloatingButton: false,
    floatingBtnAction: "translate"
  };

  let autoTranslating = false;
  let autoObserver = null;
  let autoMutationObserver = null;
  let processingQueue = [];
  let isQueueActive = false;
  const translatedSrcs = new Set();

  function normalizeBaseUrl(raw) {
    return String(raw || "http://127.0.0.1:5000").trim().replace(/\/+$/, "");
  }

  function toImageDataUri(raw) {
    if (!raw) {
      return "";
    }
    if (raw.startsWith("data:")) {
      return raw;
    }
    if (raw.startsWith("/9j/")) {
      return "data:image/jpeg;base64," + raw;
    }
    if (raw.startsWith("iVBOR")) {
      return "data:image/png;base64," + raw;
    }
    return "data:image/webp;base64," + raw;
  }

  function proxyFetch(url, options) {
    return new Promise((resolve, reject) => {
      chrome.runtime.sendMessage({ action: "proxyFetch", url, options }, (resp) => {
        if (chrome.runtime.lastError) {
          reject(new Error(chrome.runtime.lastError.message));
          return;
        }
        if (!resp || resp.error) {
          reject(new Error((resp && resp.error) || "Background fetch failed"));
          return;
        }
        resolve(resp);
      });
    });
  }

  async function requestServer(path, formObject) {
    const params = new URLSearchParams();
    Object.entries(formObject).forEach(([key, val]) => {
      if (val !== undefined && val !== null) {
        params.append(key, String(val));
      }
    });
    const url = normalizeBaseUrl(settings.serverURL) + path;
    const options = {
      method: "POST",
      headers: {
        "Content-Type": "application/x-www-form-urlencoded; charset=UTF-8"
      },
      body: params.toString(),
      cache: "no-store"
    };

    if (settings.sendRequestsViaBackground) {
      const resp = await proxyFetch(url, options);
      if (!resp.ok) {
        throw new Error("Server returned HTTP " + resp.status);
      }
      return typeof resp.data === "string" ? JSON.parse(resp.data) : resp.data;
    }

    const resp = await fetch(url, options);
    if (!resp.ok) {
      throw new Error("Server returned HTTP " + resp.status);
    }
    return resp.json();
  }

  function findLargestVisibleImage() {
    const imgs = Array.from(document.getElementsByTagName("img"));
    let bestImg = null;
    let bestArea = 0;
    for (const img of imgs) {
      const rect = img.getBoundingClientRect();
      if (rect.width < 80 || rect.height < 80) {
        continue;
      }
      const visibleW = Math.min(rect.right, window.innerWidth) - Math.max(rect.left, 0);
      const visibleH = Math.min(rect.bottom, window.innerHeight) - Math.max(rect.top, 0);
      if (visibleW <= 0 || visibleH <= 0) {
        continue;
      }
      const area = visibleW * visibleH;
      if (area > bestArea) {
        bestArea = area;
        bestImg = img;
      }
    }
    return bestImg;
  }

  function pickTargetImage(checkTop) {
    const point = settings.pickingWay === "1"
      ? { x: window.innerWidth / 2, y: window.innerHeight / 2 }
      : cap.getLastMousePosition();
    const direct = cap.getImageAtPoint(point.x, point.y, Boolean(checkTop));
    if (direct) {
      return direct;
    }
    return checkTop ? null : findLargestVisibleImage();
  }

  async function translateImageElement(img, forcedSrc, withOverlay) {
    const originalSrc = forcedSrc || cap.getImageSrc(img);
    if (!originalSrc) {
      throw new Error("No image found at selection");
    }
    if (withOverlay && img) {
      ui.showOverlay(img, "Translating...");
    }
    document.body.classList.add("mytrans-wait");

    try {
      const imagePayload = await cap.captureImageDataUrl(img, originalSrc, settings.useCanvas);
      const resp = await requestServer("/translate", {
        src: imagePayload,
        sourceLang: settings.sourceLang || "auto",
        targetLang: settings.targetLang || "id",
        displayName: "default",
        headless: "true"
      });

      if (!resp || resp.success === false || !resp.img) {
        throw new Error((resp && resp.message) || "Empty translation result");
      }

      const targetUri = toImageDataUri(resp.img);
      const targetEl = img || cap.getImageBySrc(originalSrc, true);
      if (targetEl) {
        targetEl.setAttribute("original-src", originalSrc);
        targetEl.setAttribute("target-src", targetUri);
        targetEl.src = targetUri;
      }
      translatedSrcs.add(originalSrc);
    } finally {
      document.body.classList.remove("mytrans-wait");
      if (img) {
        ui.hideOverlay(img);
      }
    }
  }

  function alterLanguage(img) {
    if (!img) {
      ui.showToast("No translated image found", true);
      return;
    }
    const originalSrc = img.getAttribute("original-src");
    const targetSrc = img.getAttribute("target-src");
    if (!originalSrc || !targetSrc) {
      ui.showToast("Image has not been translated yet", true);
      return;
    }
    const currentSrc = img.currentSrc || img.src;
    img.src = currentSrc === targetSrc ? originalSrc : targetSrc;
  }

  function observeCandidate(img) {
    if (!autoObserver || img.hasAttribute("target-src")) {
      return;
    }
    if (img.naturalWidth > 0 && img.naturalHeight > 0 && (img.naturalWidth < 100 || img.naturalHeight < 100)) {
      return;
    }
    autoObserver.observe(img);
  }

  function enqueueNextImage() {
    if (isQueueActive || !autoTranslating || processingQueue.length === 0) {
      return;
    }
    isQueueActive = true;
    const img = processingQueue.shift();
    const src = cap.getImageSrc(img);

    if (!src || translatedSrcs.has(src)) {
      ui.hideOverlay(img);
      isQueueActive = false;
      enqueueNextImage();
      return;
    }

    translateImageElement(img, src, true)
      .catch((err) => {
        console.error("Auto-translate failed:", err);
        ui.showToast("Auto-translate error: " + err.message, true);
      })
      .finally(() => {
        isQueueActive = false;
        enqueueNextImage();
      });
  }

  function startAutoTranslate() {
    if (autoTranslating) {
      return;
    }
    autoTranslating = true;

    autoObserver = new IntersectionObserver((entries) => {
      for (const entry of entries) {
        if (!entry.isIntersecting) {
          continue;
        }
        const img = entry.target;
        const src = cap.getImageSrc(img);
        if (!src || img.hasAttribute("target-src") || translatedSrcs.has(src)) {
          ui.hideOverlay(img);
          continue;
        }
        if (!processingQueue.includes(img)) {
          processingQueue.push(img);
          ui.showOverlay(img, "Queued...");
          enqueueNextImage();
        }
      }
    }, { rootMargin: "150px" });

    Array.from(document.getElementsByTagName("img")).forEach(observeCandidate);

    autoMutationObserver = new MutationObserver((mutations) => {
      for (const mutation of mutations) {
        if (mutation.type === "attributes" && mutation.target.tagName === "IMG") {
          observeCandidate(mutation.target);
          continue;
        }
        for (const node of mutation.addedNodes) {
          if (node.nodeType !== 1) {
            continue;
          }
          if (node.tagName === "IMG") {
            observeCandidate(node);
          } else if (node.getElementsByTagName) {
            Array.from(node.getElementsByTagName("img")).forEach(observeCandidate);
          }
        }
      }
    });
    autoMutationObserver.observe(document.body, {
      childList: true,
      subtree: true,
      attributes: true,
      attributeFilter: ["src"]
    });
    ui.showToast("Auto-translate started");
  }

  function stopAutoTranslate() {
    autoTranslating = false;
    if (autoObserver) {
      autoObserver.disconnect();
      autoObserver = null;
    }
    if (autoMutationObserver) {
      autoMutationObserver.disconnect();
      autoMutationObserver = null;
    }
    processingQueue.forEach(ui.hideOverlay);
    processingQueue = [];
    isQueueActive = false;
    ui.showToast("Auto-translate stopped");
  }

  function triggerScreenCapture() {
    cap.startScreenCapture(async (croppedDataUrl, _rect, err) => {
      if (err || !croppedDataUrl) {
        ui.showToast("Capture failed: " + (err ? err.message : "empty crop"), true);
        return;
      }
      document.body.classList.add("mytrans-wait");
      try {
        const resp = await requestServer("/translateRegion", {
          base64: croppedDataUrl,
          sourceLang: settings.sourceLang || "auto",
          targetLang: settings.targetLang || "id"
        });
        const region = resp && resp.regionMap ? resp.regionMap : {};
        const target = Array.isArray(region.target) && region.target[0] ? region.target[0].text : "";
        ui.showRegionResult(region.source || "", target);
      } catch (reqErr) {
        ui.showToast("Screen OCR failed: " + reqErr.message, true);
      } finally {
        document.body.classList.remove("mytrans-wait");
      }
    });
  }

  function refreshFloatingButton() {
    ui.syncFloatingButton(settings.showFloatingButton, () => {
      const img = pickTargetImage(false);
      translateImageElement(img, null, true).catch((err) => ui.showToast(err.message, true));
    });
  }

  chrome.runtime.onMessage.addListener((request, _sender, sendResponse) => {
    const msg = request && request.message;
    if (msg === "ping") {
      sendResponse({ ok: true });
      return false;
    }
    if (msg === "getAutoTranslateState") {
      sendResponse({ active: autoTranslating });
      return false;
    }
    if (msg === "toggleAutoTranslate") {
      if (autoTranslating) {
        stopAutoTranslate();
      } else {
        startAutoTranslate();
      }
      sendResponse({ active: autoTranslating });
      return false;
    }
    if (msg === "translate") {
      const img = pickTargetImage(request.check);
      translateImageElement(img, null, true).catch((err) => ui.showToast(err.message, true));
      sendResponse({ ok: true });
      return false;
    }
    if (msg === "translateWithMenu" && request.info) {
      const img = cap.getImageBySrc(request.info.srcUrl, false);
      translateImageElement(img, request.info.srcUrl, true).catch((err) => ui.showToast(err.message, true));
      sendResponse({ ok: true });
      return false;
    }
    if (msg === "alterlanguage") {
      alterLanguage(pickTargetImage(request.check));
      sendResponse({ ok: true });
      return false;
    }
    if (msg === "alterWithMenu" && request.info) {
      alterLanguage(cap.getImageBySrc(request.info.srcUrl, false));
      sendResponse({ ok: true });
      return false;
    }
    if (msg === "getsrconly") {
      const img = pickTargetImage(request.check);
      const src = cap.getImageSrc(img);
      if (!src) {
        ui.showToast("No image source found", true);
      } else {
        navigator.clipboard.writeText(src)
          .then(() => ui.showToast("Copied: " + src))
          .catch(() => ui.showToast(src));
      }
      sendResponse({ ok: true, src });
      return false;
    }
    if (msg === "startScreenCapture") {
      triggerScreenCapture();
      sendResponse({ ok: true });
      return false;
    }
    return false;
  });

  chrome.storage.sync.get(settings, (items) => {
    Object.assign(settings, items);
    refreshFloatingButton();
  });

  chrome.storage.onChanged.addListener((changes, areaName) => {
    if (areaName !== "sync") {
      return;
    }
    Object.keys(settings).forEach((key) => {
      if (changes[key] !== undefined) {
        settings[key] = changes[key].newValue;
      }
    });
    refreshFloatingButton();
  });
})();
