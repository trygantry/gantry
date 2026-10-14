import { state } from '../state.js';
import { fetchRecentChanges, fetchCommitDetails } from '../api.js';
import { showCommitDetailsModal } from '../ui.js';

export async function renderTimeline() {
  const container = document.getElementById("timeline-flow");
  if (!container) return;
  container.textContent = "";

  let changes = [];
  try {
    changes = await fetchRecentChanges();
  } catch (e) {
    console.warn("Failed to fetch recent changes for timeline:", e);
  }

  if (!changes || changes.length === 0) {
    const empty = document.createElement("div");
    empty.className = "empty-state text-muted";
    empty.textContent = "No recent activity found. Commit history will appear here.";
    container.appendChild(empty);
    return;
  }

  // Group commits by date
  const groups = {};
  changes.forEach(c => {
    const date = (c.date || "").split("T")[0].split(" ")[0] || "Unknown";
    if (!groups[date]) groups[date] = [];
    groups[date].push(c);
  });

  Object.keys(groups)
    .sort((a, b) => b.localeCompare(a))
    .forEach(date => {
      const dateEl = document.createElement("div");
      dateEl.className = "timeline-date";
      dateEl.textContent = date;
      container.appendChild(dateEl);

      const eventsEl = document.createElement("div");
      eventsEl.className = "timeline-events";

      groups[date].forEach(c => {
        const allowedKinds = new Set(["feat", "fix", "docs", "refactor", "chore", "misc"]);
        const kind = allowedKinds.has(c.kind) ? c.kind : "misc";

        const card = document.createElement("div");
        card.className = `timeline-event-card kind-${kind}`;

        const titleRow = document.createElement("div");
        titleRow.className = "event-title";

        const msgSpan = document.createElement("span");
        msgSpan.textContent = c.message;

        const timeEl = document.createElement("span");
        timeEl.className = "event-time";
        const timePart = (c.date || "").split(" ")[1] || "";
        timeEl.textContent = timePart;

        titleRow.appendChild(msgSpan);
        titleRow.appendChild(timeEl);

        const descEl = document.createElement("div");
        descEl.className = "event-desc";

        const kindLabels = {
          feat: "Feature",
          fix: "Fix",
          docs: "Docs",
          refactor: "Refactor",
          chore: "Chore",
          misc: "Misc"
        };
        const label = kindLabels[kind] || "Misc";

        const kindTag = document.createElement("span");
        kindTag.className = `change-tag kind-${kind}`;
        kindTag.textContent = label;
        descEl.appendChild(kindTag);

        if (c.hash) {
          const hashCode = document.createElement("code");
          hashCode.className = "hash-pill";
          hashCode.textContent = c.hash;
          descEl.appendChild(hashCode);
        }

        const authorSpan = document.createElement("span");
        authorSpan.textContent = c.author || "";
        descEl.appendChild(authorSpan);

        // Add interactive click listeners
        if (c.message.startsWith("File modified: ")) {
          const filename = c.message.replace("File modified: ", "").trim();
          let matchPath = null;
          if (state.cachedIndex) {
            if (filename === "go_no_go.md") {
              matchPath = "go_no_go.md";
            } else if (filename === "tasks.md") {
              matchPath = "tasks.md";
            } else {
              const foundBlocker = state.cachedIndex.blockers?.find(b => b.path.endsWith(filename));
              if (foundBlocker) matchPath = foundBlocker.path;

              const foundQa = state.cachedIndex.qa_suites?.find(q => q.path.endsWith(filename));
              if (foundQa) matchPath = foundQa.path;
            }
          }
          if (matchPath) {
            card.addEventListener("click", () => {
              if (window.openFileInEditor) {
                window.openFileInEditor(matchPath);
              }
            });
          }
        } else if (c.hash) {
          card.addEventListener("click", async () => {
            try {
              const details = await fetchCommitDetails(c.hash);
              showCommitDetailsModal(details);
            } catch (err) {
              console.error("Failed to show commit details:", err);
            }
          });
        }

        card.appendChild(titleRow);
        card.appendChild(descEl);
        eventsEl.appendChild(card);
      });

      container.appendChild(eventsEl);
    });
}
