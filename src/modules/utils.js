import { state } from './state.js';

// --- Helper to parse bold formatting in text safely ---
export function parseTextBoldFormatting(containerEl, text) {
  const boldRegex = /\*\*([^*]+)\*\*/g;
  let lastIdx = 0;
  let match;
  while ((match = boldRegex.exec(text)) !== null) {
    if (match.index > lastIdx) {
      containerEl.appendChild(document.createTextNode(text.substring(lastIdx, match.index)));
    }
    const b = document.createElement("strong");
    b.textContent = match[1];
    containerEl.appendChild(b);
    lastIdx = boldRegex.lastIndex;
  }
  if (lastIdx < text.length) {
    containerEl.appendChild(document.createTextNode(text.substring(lastIdx)));
  }
}

// --- Helper to strip HTML comments from gate names for clean display ---
export function stripComments(text) {
  return text.replace(/<!--.*?-->/g, "").trim();
}

export function getOwnerInitials(owner) {
  if (!owner || owner === "—") return "—";
  let clean = owner.replace("@", "");
  if (clean.includes("_")) {
    const parts = clean.split("_").filter(Boolean);
    const first = parts[0]?.[0] || "";
    const second = parts[1]?.[0] || "";
    return (first + second).toUpperCase() || clean.substring(0, 2).toUpperCase();
  }
  return clean.substring(0, 2).toUpperCase();
}

export function getGateDates(text) {
  const relativeDate = (days) => {
    const date = new Date();
    date.setDate(date.getDate() + days);
    return date.toISOString().slice(0, 10);
  };
  let due = relativeDate(3);
  let updated = "—";
  if (text.includes("Product")) {
    due = relativeDate(1);
    updated = relativeDate(-2);
  } else if (text.includes("Engineering")) {
    due = relativeDate(2);
    updated = relativeDate(-1);
  } else if (text.includes("QA")) {
    due = relativeDate(3);
    updated = relativeDate(-2);
  } else if (text.includes("Billing")) {
    due = relativeDate(1);
    updated = relativeDate(-2);
  } else if (text.includes("Marketing")) {
    due = relativeDate(3);
    updated = "—";
  }
  return { due, updated };
}

export function startCountdown(targetDateStr, displayEl, labelEl) {
  if (!targetDateStr) {
    if (displayEl) {
      displayEl.textContent = "NO TARGET SET";
      displayEl.style.color = "var(--text-muted)";
      displayEl.style.textShadow = "none";
    }
    if (labelEl) labelEl.textContent = "Target: —";
    return null;
  }

  let isoString = targetDateStr;
  if (!targetDateStr.includes("T")) {
    isoString += "T00:00:00";
  }

  const targetDate = new Date(isoString).getTime();

  if (isNaN(targetDate)) {
    if (displayEl) {
      displayEl.textContent = "INVALID DATE";
      displayEl.style.color = "var(--color-blocker)";
      displayEl.style.textShadow = "0 0 10px var(--color-blocker-glow)";
    }
    if (labelEl) labelEl.textContent = "Target: Invalid";
    return null;
  }

  if (labelEl) {
    labelEl.textContent = `Target: ${targetDateStr.replace("T", " ")}`;
  }

  function pad(num) {
    return String(num).padStart(2, '0');
  }

  function updateTimer() {
    const now = new Date().getTime();
    const diff = targetDate - now;

    if (displayEl) {
      const readiness = state.cachedIndex?.readiness || "At Risk";
      if (readiness === "No-Go") {
        displayEl.style.color = "var(--color-blocker)";
        displayEl.style.textShadow = "0 0 10px var(--color-blocker-glow)";
      } else if (readiness === "Go") {
        displayEl.style.color = "var(--color-go)";
        displayEl.style.textShadow = "0 0 10px var(--color-go-glow)";
      } else {
        displayEl.style.color = "var(--color-pending)";
        displayEl.style.textShadow = "0 0 10px var(--color-pending-glow)";
      }
    }

    if (diff <= 0) {
      if (displayEl) displayEl.textContent = "LAUNCH DAY REACHED";
      return;
    }

    const days = Math.floor(diff / (1000 * 60 * 60 * 24));
    const hours = Math.floor((diff % (1000 * 60 * 60 * 24)) / (1000 * 60 * 60));
    const minutes = Math.floor((diff % (1000 * 60 * 60)) / (1000 * 60));
    const seconds = Math.floor((diff % (1000 * 60)) / 1000);

    if (displayEl) {
      displayEl.textContent = `T-${pad(days)}d ${pad(hours)}h ${pad(minutes)}m ${pad(seconds)}s`;
    }
  }
  
  updateTimer();
  return setInterval(updateTimer, 1000);
}

export function startGantryLogSimulation(rcLogStream) {
  const logs = [
    { type: "success", msg: "gantry validate --ci passed (exit 0) · 4 gates · 0 blockers" },
    { type: "info",    msg: "Rebuilding index.json after vault change detected" },
    { type: "info",    msg: "Candidate identity: AAB sha256 verified against release 3.4.0" },
    { type: "warning", msg: "go_no_go.md: gate 'QA' is PENDING — candidate evidence required" },
    { type: "info",    msg: "gantry impact --base main: 3 files changed, 1 blocker affected" },
    { type: "success", msg: "gantry audit score: 82/100 — above launch threshold" },
  ];

  function addLog() {
    if (!rcLogStream) return;
    const log = logs[Math.floor(Math.random() * logs.length)];
    const timeStr = new Date().toLocaleTimeString();
    
    const entryEl = document.createElement("div");
    entryEl.className = "log-entry";
    
    const timeSpan = document.createElement("span");
    timeSpan.className = "log-time";
    timeSpan.textContent = `[${timeStr}] `;
    
    const tagSpan = document.createElement("span");
    tagSpan.className = `log-tag ${log.type}`;
    tagSpan.textContent = log.type;
    
    const msgSpan = document.createElement("span");
    msgSpan.className = "log-msg";
    msgSpan.textContent = ` ${log.msg}`;
    
    entryEl.appendChild(timeSpan);
    entryEl.appendChild(tagSpan);
    entryEl.appendChild(msgSpan);
    
    rcLogStream.appendChild(entryEl);
    rcLogStream.scrollTop = rcLogStream.scrollHeight;

    while (rcLogStream.children.length > 10) {
      rcLogStream.removeChild(rcLogStream.firstChild);
    }
  }

  for (let i = 0; i < 3; i++) addLog();
  return setInterval(addLog, 5000);
}
