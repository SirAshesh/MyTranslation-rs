(function () {
  if (window.MyTransUi) {
    return;
  }

  const activeOverlays = new Map();
  let floatingButton = null;

  function showToast(text, isError) {
    const existing = document.getElementById("mytrans-toast");
    if (existing) {
      existing.remove();
    }
    const toast = document.createElement("div");
    toast.id = "mytrans-toast";
    toast.className = "mytrans-toast" + (isError ? " is-error" : "");
    toast.textContent = text;
    document.body.appendChild(toast);
    setTimeout(() => {
      if (toast.isConnected) {
        toast.remove();
      }
    }, 3200);
  }

  function positionOverlay(overlay, img) {
    const rect = img.getBoundingClientRect();
    overlay.style.left = Math.max(0, rect.left) + "px";
    overlay.style.top = Math.max(0, rect.top) + "px";
    overlay.style.width = Math.max(0, Math.min(rect.right, window.innerWidth) - Math.max(0, rect.left)) + "px";
    overlay.style.height = Math.max(0, Math.min(rect.bottom, window.innerHeight) - Math.max(0, rect.top)) + "px";
  }

  function showOverlay(img, label) {
    hideOverlay(img);
    const overlay = document.createElement("div");
    overlay.className = "mytrans-overlay";
    const spinner = document.createElement("div");
    spinner.className = "mytrans-spinner";
    const caption = document.createElement("div");
    caption.textContent = label || "Translating...";
    overlay.appendChild(spinner);
    overlay.appendChild(caption);
    positionOverlay(overlay, img);
    document.body.appendChild(overlay);
    activeOverlays.set(img, overlay);
  }

  function hideOverlay(img) {
    const existing = activeOverlays.get(img);
    if (existing) {
      existing.remove();
      activeOverlays.delete(img);
    }
  }

  window.addEventListener("scroll", () => {
    activeOverlays.forEach((overlay, img) => {
      positionOverlay(overlay, img);
    });
  }, { passive: true });

  function showRegionResult(sourceText, targetText) {
    const existing = document.getElementById("mytrans-region-card");
    if (existing) {
      existing.remove();
    }
    const card = document.createElement("div");
    card.id = "mytrans-region-card";
    card.className = "mytrans-region-card";

    const header = document.createElement("div");
    header.className = "mytrans-region-header";
    const title = document.createElement("span");
    title.textContent = "Screen Capture OCR";
    const closeBtn = document.createElement("button");
    closeBtn.className = "mytrans-region-close";
    closeBtn.textContent = "x";
    closeBtn.onclick = () => card.remove();
    header.append(title, closeBtn);

    const body = document.createElement("div");
    body.className = "mytrans-region-body";

    const srcLabel = document.createElement("div");
    srcLabel.className = "mytrans-region-label";
    srcLabel.textContent = "Detected Text";
    const srcBox = document.createElement("div");
    srcBox.className = "mytrans-region-text";
    srcBox.textContent = sourceText || "(No text detected)";

    const tgtLabel = document.createElement("div");
    tgtLabel.className = "mytrans-region-label";
    tgtLabel.textContent = "Translation";
    const tgtBox = document.createElement("div");
    tgtBox.className = "mytrans-region-text";
    tgtBox.textContent = targetText || "(Empty)";

    const actions = document.createElement("div");
    actions.className = "mytrans-region-actions";
    const copyBtn = document.createElement("button");
    copyBtn.className = "mytrans-region-btn";
    copyBtn.textContent = "Copy Translation";
    copyBtn.onclick = () => {
      navigator.clipboard.writeText(targetText || "").then(() => showToast("Translation copied"));
    };
    actions.appendChild(copyBtn);

    body.append(srcLabel, srcBox, tgtLabel, tgtBox, actions);
    card.append(header, body);
    document.body.appendChild(card);
  }

  function syncFloatingButton(visible, onClick) {
    if (!visible) {
      if (floatingButton) {
        floatingButton.remove();
        floatingButton = null;
      }
      return;
    }
    if (floatingButton || !document.body) {
      return;
    }

    const btn = document.createElement("div");
    btn.id = "mytrans-floating-btn";
    btn.textContent = "MT";
    btn.title = "MyTranslation Quick Action";
    btn.style.right = "20px";
    btn.style.bottom = "80px";

    let dragging = false;
    let moved = false;
    let startX = 0;
    let startY = 0;
    let origLeft = 0;
    let origTop = 0;

    btn.addEventListener("mousedown", (event) => {
      dragging = true;
      moved = false;
      startX = event.clientX;
      startY = event.clientY;
      const rect = btn.getBoundingClientRect();
      btn.style.right = "auto";
      btn.style.bottom = "auto";
      origLeft = rect.left;
      origTop = rect.top;
      btn.style.left = origLeft + "px";
      btn.style.top = origTop + "px";
      event.preventDefault();
    });

    window.addEventListener("mousemove", (event) => {
      if (!dragging) {
        return;
      }
      const dx = event.clientX - startX;
      const dy = event.clientY - startY;
      if (Math.abs(dx) > 3 || Math.abs(dy) > 3) {
        moved = true;
      }
      btn.style.left = Math.max(0, Math.min(window.innerWidth - 44, origLeft + dx)) + "px";
      btn.style.top = Math.max(0, Math.min(window.innerHeight - 44, origTop + dy)) + "px";
    });

    window.addEventListener("mouseup", () => {
      if (!dragging) {
        return;
      }
      dragging = false;
      if (!moved && typeof onClick === "function") {
        onClick();
      }
    });

    document.body.appendChild(btn);
    floatingButton = btn;
  }

  window.MyTransUi = {
    showToast,
    showOverlay,
    hideOverlay,
    showRegionResult,
    syncFloatingButton
  };
})();

