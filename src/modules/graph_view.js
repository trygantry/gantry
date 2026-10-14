import { getImpactGraph } from './api.js';

function renderInspectorDetails(inspectorContent, node) {
  inspectorContent.textContent = "";

  const statusLine = document.createElement("p");
  const statusLabel = document.createElement("strong");
  statusLabel.textContent = node.status || "N/A";
  statusLine.textContent = "Status: ";
  statusLine.appendChild(statusLabel);
  inspectorContent.appendChild(statusLine);

  const detailsLine = document.createElement("p");
  if (node.kind === "blocker") {
    detailsLine.textContent = node.status === "resolved" ? "Resolved blocker." : "Active launch blocker.";
  } else if (node.kind === "file") {
    detailsLine.textContent = "Modified file in recent changes.";
  } else if (node.kind === "qa_suite") {
    detailsLine.textContent = "Launch QA checklist suite coverage.";
  } else if (node.kind === "gate") {
    detailsLine.textContent = "Launch gate criteria checklist item.";
  } else if (node.kind === "signoff") {
    detailsLine.textContent = "Required stakeholder sign-off status.";
  } else {
    detailsLine.textContent = "Evidence node in impact graph.";
  }
  inspectorContent.appendChild(detailsLine);
}

export async function initGraphView() {
  const btnTrace = document.getElementById("btn-impact-view-trace");
  const btnGraph = document.getElementById("btn-impact-view-graph");
  const listContainer = document.getElementById("audit-impact-list");
  const graphContainer = document.getElementById("audit-impact-graph-container");
  const btnInspectorClose = document.getElementById("btn-inspector-close");
  const inspector = document.getElementById("audit-impact-inspector");

  if (!btnTrace || !btnGraph || !listContainer || !graphContainer) return;

  // Ev button listeners
  btnTrace.addEventListener("click", () => {
    btnTrace.classList.add("active");
    btnGraph.classList.remove("active");
    listContainer.style.display = "flex";
    graphContainer.style.display = "none";
  });

  btnGraph.addEventListener("click", async () => {
    btnGraph.classList.add("active");
    btnTrace.classList.remove("active");
    listContainer.style.display = "none";
    graphContainer.style.display = "block";
    await drawGraph();
  });

  if (btnInspectorClose && inspector) {
    btnInspectorClose.addEventListener("click", () => {
      inspector.style.display = "none";
    });
  }
}

export async function drawGraph(baseRef = null) {
  const svg = document.getElementById("audit-impact-svg");
  const inspector = document.getElementById("audit-impact-inspector");
  const inspectorTitle = document.getElementById("inspector-title");
  const inspectorContent = document.getElementById("inspector-content");

  if (!svg) return;

  // Clear svg
  svg.innerHTML = "";

  let graphData;
  try {
    graphData = await getImpactGraph(baseRef);
  } catch (err) {
    console.error("Failed to load impact graph:", err);
    svg.innerHTML = `<text x="50%" y="50%" dominant-baseline="middle" text-anchor="middle" fill="var(--color-blocker)">Failed to load graph data</text>`;
    return;
  }

  if (!graphData || !graphData.nodes || graphData.nodes.length === 0) {
    svg.innerHTML = `<text x="50%" y="50%" dominant-baseline="middle" text-anchor="middle" fill="var(--text-muted)">No changes detected.</text>`;
    return;
  }

  const width = svg.clientWidth || 600;
  const height = svg.clientHeight || 350;

  // Define columns/layers
  const cols = {
    commit: [],
    file: [],
    blocker_qa: [], // blockers & qa_suites
    gate_signoff: [] // gates & signoffs
  };

  // Group nodes by columns
  graphData.nodes.forEach(node => {
    if (node.kind === "commit") {
      cols.commit.push(node);
    } else if (node.kind === "file") {
      cols.file.push(node);
    } else if (node.kind === "blocker" || node.kind === "qa_suite") {
      cols.blocker_qa.push(node);
    } else if (node.kind === "gate" || node.kind === "signoff") {
      cols.gate_signoff.push(node);
    }
  });

  // Map nodes to coordinates
  const positions = {};
  const padding = 40;
  const colX = [
    padding + 20, // Commit column
    width * 0.33,  // Files column
    width * 0.66,  // Blockers/QA column
    width - padding - 40 // Gates/Signoffs column
  ];

  const colKeys = ["commit", "file", "blocker_qa", "gate_signoff"];
  colKeys.forEach((key, colIndex) => {
    const list = cols[key];
    const x = colX[colIndex];
    const n = list.length;
    list.forEach((node, i) => {
      let y;
      if (n === 1) {
        y = height / 2;
      } else {
        y = padding + i * ((height - 2 * padding) / (n - 1));
      }
      positions[node.id] = { x, y, node };
    });
  });

  // Draw Edges (Links)
  const edgesG = document.createElementNS("http://www.w3.org/2000/svg", "g");
  edgesG.setAttribute("class", "graph-edges");
  svg.appendChild(edgesG);

  graphData.edges.forEach(edge => {
    const sourcePos = positions[edge.source];
    const targetPos = positions[edge.target];

    // Some files in edges might not be in modified files list (thus not in nodes)
    if (!sourcePos || !targetPos) return;

    const line = document.createElementNS("http://www.w3.org/2000/svg", "line");
    line.setAttribute("x1", sourcePos.x);
    line.setAttribute("y1", sourcePos.y);
    line.setAttribute("x2", targetPos.x);
    line.setAttribute("y2", targetPos.y);
    line.setAttribute("class", "graph-edge-line");
    line.setAttribute("stroke", edge.kind === "blocks" ? "var(--color-blocker)" : "var(--color-primary)");

    if (edge.kind === "covers") {
      line.setAttribute("stroke-dasharray", "4,4");
    }

    // Interactive highlights
    line.setAttribute("data-source", edge.source);
    line.setAttribute("data-target", edge.target);

    edgesG.appendChild(line);
  });

  // Draw Nodes (Circles)
  const nodesG = document.createElementNS("http://www.w3.org/2000/svg", "g");
  nodesG.setAttribute("class", "graph-nodes");
  svg.appendChild(nodesG);

  Object.values(positions).forEach(({ x, y, node }) => {
    const nodeG = document.createElementNS("http://www.w3.org/2000/svg", "g");
    nodeG.setAttribute("transform", `translate(${x}, ${y})`);
    nodeG.setAttribute("tabindex", "0");
    nodeG.setAttribute("role", "button");
    nodeG.setAttribute("aria-label", `${node.kind} ${node.label}`);
    nodeG.style.cursor = "pointer";

    const circle = document.createElementNS("http://www.w3.org/2000/svg", "circle");
    circle.setAttribute("r", "10");
    circle.setAttribute("class", "graph-node-circle");

    // Color code nodes based on status/type
    let colorClass = "node-file";
    if (node.kind === "commit") {
      colorClass = "node-commit";
    } else if (node.kind === "blocker") {
      colorClass = node.status?.toLowerCase() === "resolved" ? "node-blocker-resolved" : "node-blocker-open";
    } else if (node.kind === "qa_suite") {
      colorClass = node.status === "complete" || node.status === "100%" ? "node-qa-complete" : "node-qa-incomplete";
    } else if (node.kind === "gate") {
      colorClass = node.status === "checked" ? "node-gate-checked" : "node-gate-unchecked";
    } else if (node.kind === "signoff") {
      colorClass = node.status === "Approved" ? "node-signoff-approved" : "node-signoff-pending";
    }
    circle.classList.add(colorClass);

    // Label Text
    const label = document.createElementNS("http://www.w3.org/2000/svg", "text");
    label.setAttribute("y", "-15");
    label.setAttribute("class", "graph-node-text");
    label.setAttribute("font-size", "9px");
    
    // Truncate labels for files to avoid clutter
    let dispLabel = node.label;
    if (node.kind === "file" && dispLabel.includes("/")) {
      dispLabel = dispLabel.split("/").pop();
    }
    label.textContent = dispLabel;

    nodeG.appendChild(circle);
    nodeG.appendChild(label);

    // Interactivity
    nodeG.addEventListener("mouseenter", () => {
      // Highlight incoming/outgoing edges
      const lines = svg.querySelectorAll(".graph-edge-line");
      lines.forEach(line => {
        const src = line.getAttribute("data-source");
        const tgt = line.getAttribute("data-target");
        if (src === node.id || tgt === node.id) {
          line.style.strokeWidth = "3px";
          line.style.opacity = "1";
        } else {
          line.style.opacity = "0.2";
        }
      });
    });

    nodeG.addEventListener("mouseleave", () => {
      const lines = svg.querySelectorAll(".graph-edge-line");
      lines.forEach(line => {
        line.style.strokeWidth = "";
        line.style.opacity = "";
      });
    });

    nodeG.addEventListener("click", (e) => {
      e.stopPropagation();
      
      // Select node visually
      svg.querySelectorAll(".graph-node-circle").forEach(c => c.classList.remove("active"));
      circle.classList.add("active");

      // Populate inspector
      if (inspector && inspectorTitle && inspectorContent) {
        inspector.style.display = "block";
        inspectorTitle.textContent = `${node.kind.toUpperCase()}: ${node.label}`;
        renderInspectorDetails(inspectorContent, node);
      }
    });

    nodeG.addEventListener("keydown", (e) => {
      if (e.key === "Enter" || e.key === " ") {
        e.preventDefault();
        nodeG.dispatchEvent(new MouseEvent("click", { bubbles: true }));
      }
    });

    nodesG.appendChild(nodeG);
  });
}
