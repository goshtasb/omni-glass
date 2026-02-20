/**
 * Action menu UI — two-state streaming renderer.
 *
 * State 1 (Skeleton): Shows immediately when the window opens.
 *   - Shimmer placeholder for summary
 *   - Copy Text button (always available — OCR text is already stored)
 *   - 3 shimmer placeholders for loading actions
 *
 * State 2 (Complete): Fills in when the streaming LLM response finishes.
 *   - Real summary text (replaces shimmer)
 *   - All action buttons with icons and labels
 *
 * Events from Rust:
 *   - "action-menu-skeleton": { contentType, summary } — updates summary text
 *   - "action-menu-complete": full ActionMenu JSON — renders all actions
 */

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-shell";

interface Action {
  id: string;
  label: string;
  icon: string;
  priority: number;
  description: string;
  requiresExecution: boolean;
}

interface ActionMenu {
  contentType: string;
  confidence: number;
  summary: string;
  detectedLanguage: string | null;
  actions: Action[];
}

interface ActionMenuSkeleton {
  contentType: string;
  summary: string;
}

const ICON_MAP: Record<string, string> = {
  clipboard: "\u{1F4CB}",
  table: "\u{1F4CA}",
  code: "\u{1F4BB}",
  lightbulb: "\u{1F4A1}",
  wrench: "\u{1F527}",
  language: "\u{1F310}",
  search: "\u{1F50D}",
  file: "\u{1F4C4}",
  terminal: "\u{2B1B}",
  mail: "\u{1F4E7}",
  calculator: "\u{1F522}",
  link: "\u{1F517}",
  download: "\u{2B07}\u{FE0F}",
  eye: "\u{1F441}\u{FE0F}",
  edit: "\u{270F}\u{FE0F}",
  sparkles: "\u{2728}",
};

function getIcon(name: string): string {
  return ICON_MAP[name] || "\u{2022}";
}

function escapeHtml(text: string): string {
  const div = document.createElement("div");
  div.textContent = text;
  return div.innerHTML;
}

// ── State 1: Skeleton ──────────────────────────────────────────────

function renderSkeleton(): void {
  const container = document.getElementById("action-menu")!;

  container.innerHTML = `
    <div style="
      background: #1a1a2e;
      border-radius: 8px;
      box-shadow: 0 4px 12px rgba(0,0,0,0.3);
      width: 280px;
      overflow: hidden;
      border: 1px solid rgba(255,255,255,0.1);
    ">
      <div id="menu-summary" style="
        padding: 10px 14px;
        font-size: 13px;
        color: rgba(255,255,255,0.7);
        border-bottom: 1px solid rgba(255,255,255,0.1);
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
        min-height: 35px;
        display: flex;
        align-items: center;
      ">
        <div class="shimmer" style="height:14px;width:70%;"></div>
      </div>
      <div id="menu-actions">
        <div class="action-row" data-action-id="copy_text" style="
          padding: 0 14px;
          height: 36px;
          display: flex;
          align-items: center;
          gap: 10px;
          cursor: pointer;
          transition: background 0.1s;
        " title="Copy the extracted text to clipboard">
          <span style="font-size: 16px; width: 20px; text-align: center;">\u{1F4CB}</span>
          <span style="flex: 1; font-size: 14px;">Copy Text</span>
        </div>
        <div style="padding:8px 14px;display:flex;align-items:center;gap:10px;">
          <div class="shimmer" style="width:20px;height:20px;"></div>
          <div class="shimmer" style="flex:1;height:14px;"></div>
        </div>
        <div style="padding:8px 14px;display:flex;align-items:center;gap:10px;">
          <div class="shimmer" style="width:20px;height:20px;"></div>
          <div class="shimmer" style="flex:1;height:14px;"></div>
        </div>
        <div style="padding:8px 14px;display:flex;align-items:center;gap:10px;">
          <div class="shimmer" style="width:20px;height:20px;"></div>
          <div class="shimmer" style="flex:1;height:14px;"></div>
        </div>
      </div>
    </div>
  `;

  // Copy Text is clickable immediately in skeleton state
  attachActionHandlers();
}

// ── Skeleton update: summary arrives from streaming ────────────────

function updateSummary(skeleton: ActionMenuSkeleton): void {
  const summaryEl = document.getElementById("menu-summary");
  if (summaryEl) {
    summaryEl.innerHTML = "";
    summaryEl.textContent = skeleton.summary;
    console.log(
      `[RENDER] Skeleton updated: type=${skeleton.contentType}, summary="${skeleton.summary}"`
    );
  }
}

// ── State 2: Complete menu ─────────────────────────────────────────

function renderMenu(menu: ActionMenu): void {
  const container = document.getElementById("action-menu")!;

  // Sort actions by priority
  const sorted = [...menu.actions].sort((a, b) => a.priority - b.priority);

  container.innerHTML = `
    <div style="
      background: #1a1a2e;
      border-radius: 8px;
      box-shadow: 0 4px 12px rgba(0,0,0,0.3);
      width: 280px;
      overflow: hidden;
      border: 1px solid rgba(255,255,255,0.1);
    ">
      <div style="
        padding: 10px 14px;
        font-size: 13px;
        color: rgba(255,255,255,0.7);
        border-bottom: 1px solid rgba(255,255,255,0.1);
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
      ">
        ${escapeHtml(menu.summary)}
      </div>
      ${sorted
        .map(
          (action) => `
        <div class="action-row" data-action-id="${action.id}" style="
          padding: 0 14px;
          height: 36px;
          display: flex;
          align-items: center;
          gap: 10px;
          cursor: pointer;
          transition: background 0.1s;
        " title="${escapeHtml(action.description)}">
          <span style="font-size: 16px; width: 20px; text-align: center;">${getIcon(action.icon)}</span>
          <span style="flex: 1; font-size: 14px;">${escapeHtml(action.label)}</span>
        </div>
      `
        )
        .join("")}
    </div>
  `;

  attachActionHandlers();
  console.log(
    `[RENDER] Complete menu: ${menu.actions.length} actions, type=${menu.contentType}`
  );
}

// ── Shared: attach click/hover handlers to action rows ─────────────

function attachActionHandlers(): void {
  const container = document.getElementById("action-menu")!;
  container.querySelectorAll(".action-row").forEach((row) => {
    const el = row as HTMLElement;

    el.addEventListener("mouseenter", () => {
      el.style.background = "#0f3460";
    });
    el.addEventListener("mouseleave", () => {
      el.style.background = "transparent";
    });

    el.addEventListener("click", async () => {
      const actionId = el.dataset.actionId;
      console.log(`[ACTION] User clicked: ${actionId}`);
      await executeAction(actionId || "");
    });
  });
}

// ── Action execution ───────────────────────────────────────────────

async function executeAction(actionId: string): Promise<void> {
  try {
    if (actionId === "copy_text") {
      const text = await invoke<string>("get_ocr_text");
      await invoke("copy_to_clipboard", { text });
      showFeedback(`Copied ${text.length} chars`);
    } else if (actionId === "search_web") {
      const text = await invoke<string>("get_ocr_text");
      const query = text.slice(0, 200).trim();
      const url = `https://www.google.com/search?q=${encodeURIComponent(query)}`;
      await open(url);
      showFeedback("Opening search...");
    } else {
      showFeedback("Requires API credits");
    }
  } catch (err) {
    console.error(`[ACTION] Failed to execute ${actionId}:`, err);
    showFeedback(`Error: ${err}`);
  }

  // Close menu after brief feedback delay
  setTimeout(async () => {
    try {
      await invoke("close_action_menu");
    } catch {
      /* closing */
    }
  }, 800);
}

function showFeedback(message: string): void {
  const container = document.getElementById("action-menu")!;
  const feedback = document.createElement("div");
  feedback.style.cssText =
    "padding:8px 14px;text-align:center;color:#4ade80;font-size:13px;background:#1a1a2e;border-top:1px solid rgba(255,255,255,0.1)";
  feedback.textContent = message;
  container.querySelector("div")?.appendChild(feedback);
}

// ── Init: skeleton + event listeners ───────────────────────────────

async function init(): Promise<void> {
  // Render skeleton immediately — Copy Text is clickable right away
  renderSkeleton();

  // Listen for streaming events from Rust
  listen<ActionMenuSkeleton>("action-menu-skeleton", (event) => {
    updateSummary(event.payload);
  });

  listen<ActionMenu>("action-menu-complete", (event) => {
    renderMenu(event.payload);
  });

  // Fallback: if streaming events were missed (e.g. JS loaded late),
  // poll the state after a delay.
  setTimeout(async () => {
    const actionsEl = document.getElementById("menu-actions");
    if (actionsEl && actionsEl.querySelector(".shimmer")) {
      try {
        const menu = (await invoke("get_action_menu")) as ActionMenu;
        console.log("[RENDER] Fallback poll: menu available in state");
        renderMenu(menu);
      } catch {
        // Menu not ready yet — streaming events will handle it
      }
    }
  }, 3000);
}

// Escape key closes the menu
document.addEventListener("keydown", async (e: KeyboardEvent) => {
  if (e.key === "Escape") {
    try {
      await invoke("close_action_menu");
    } catch {
      // Window might already be closing
    }
  }
});

init();
