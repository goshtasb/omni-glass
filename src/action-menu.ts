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
import { listen, emit } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-shell";
import { WebviewWindow, getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { LogicalSize } from "@tauri-apps/api/dpi";

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

interface ActionResultBody {
  type: string;
  text?: string;
  filePath?: string;
  command?: string;
  clipboardContent?: string;
  mimeType?: string;
}

interface ActionResultMeta {
  tokensUsed?: number;
  processingNote?: string;
}

interface ActionResult {
  status: string;
  actionId: string;
  result: ActionResultBody;
  metadata?: ActionResultMeta;
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
      <div id="menu-actions">
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
    // Local actions — no LLM call needed
    if (actionId === "copy_text" || actionId === "copy_command" || actionId === "copy_traceback" || actionId === "copy_code") {
      const text = await invoke<string>("get_ocr_text");
      await invoke("copy_to_clipboard", { text });
      showFeedback(`Copied ${text.length} chars`);
      closeAfterDelay(800);
      return;
    }

    if (actionId === "search_web" || actionId === "search_error" || actionId === "search_command" || actionId === "search_online" || actionId === "search_docs") {
      const text = await invoke<string>("get_ocr_text");
      const query = text.slice(0, 200).trim();
      const url = `https://www.google.com/search?q=${encodeURIComponent(query)}`;
      await open(url);
      showFeedback("Opening search...");
      closeAfterDelay(800);
      return;
    }

    // LLM-backed actions — call execute_action Tauri command
    actionInProgress = true;
    showLoading(actionId);

    const result = await invoke<ActionResult>("execute_action", {
      actionId,
    });

    console.log(`[ACTION] Result: status=${result.status}, type=${result.result.type}`);

    if (result.status === "error") {
      console.error(`[ACTION] Execute error: ${result.result.text}`);
      showFeedback(result.result.text || "Action failed", true);
      // Don't auto-close — let user read the error
      return;
    }

    // Handle result by type
    switch (result.result.type) {
      case "text":
        showTextResult(result.result.text || "No content returned.");
        break;

      case "clipboard":
        if (result.result.clipboardContent) {
          await invoke("copy_to_clipboard", { text: result.result.clipboardContent });
          showFeedback("Copied to clipboard");
          closeAfterDelay(800);
        }
        break;

      case "file":
        await handleFileResult(result);
        break;

      case "command":
        await handleCommandResult(result);
        break;

      default:
        showFeedback(`Unknown result type: ${result.result.type}`, true);
    }
  } catch (err) {
    console.error(`[ACTION] Failed to execute ${actionId}:`, err);
    showFeedback(`Error: ${err}`, true);
  }
}

// ── Result handlers ─────────────────────────────────────────────────

async function showTextResult(text: string): Promise<void> {
  const container = document.getElementById("action-menu")!;
  const wrapper = container.querySelector("div")!;

  // Expand the window to fit text content
  wrapper.style.width = "380px";

  // Extract code block for "Copy Fix" (content inside ``` fences)
  const codeBlock = extractCodeBlock(text);
  const rendered = renderMarkdownLight(text);

  // Replace actions with text result
  const actionsEl = document.getElementById("menu-actions");
  if (actionsEl) {
    actionsEl.innerHTML = `
      <div style="
        padding: 12px 14px;
        font-size: 13px;
        color: rgba(255,255,255,0.9);
        line-height: 1.5;
        max-height: 300px;
        overflow-y: auto;
        word-wrap: break-word;
      ">${rendered}</div>
      <div style="
        padding: 6px 14px 8px;
        display: flex;
        gap: 8px;
        justify-content: flex-end;
        border-top: 1px solid rgba(255,255,255,0.1);
      ">
        ${codeBlock ? `<button id="btn-copy-fix" style="
          background: rgba(74,222,128,0.15);
          border: 1px solid rgba(74,222,128,0.4);
          color: #4ade80;
          padding: 4px 12px;
          border-radius: 4px;
          cursor: pointer;
          font-size: 12px;
        ">Copy Fix</button>` : ""}
        <button id="btn-copy-result" style="
          background: transparent;
          border: 1px solid rgba(255,255,255,0.2);
          color: rgba(255,255,255,0.8);
          padding: 4px 12px;
          border-radius: 4px;
          cursor: pointer;
          font-size: 12px;
        ">Copy All</button>
        <button id="btn-close-result" style="
          background: transparent;
          border: 1px solid rgba(255,255,255,0.2);
          color: rgba(255,255,255,0.8);
          padding: 4px 12px;
          border-radius: 4px;
          cursor: pointer;
          font-size: 12px;
        ">Close</button>
      </div>
    `;

    if (codeBlock) {
      document.getElementById("btn-copy-fix")?.addEventListener("click", async () => {
        await invoke("copy_to_clipboard", { text: codeBlock });
        showFeedback("Fix copied");
        closeAfterDelay(600);
      });
    }

    document.getElementById("btn-copy-result")?.addEventListener("click", async () => {
      await invoke("copy_to_clipboard", { text });
      showFeedback("Copied");
      closeAfterDelay(600);
    });

    document.getElementById("btn-close-result")?.addEventListener("click", async () => {
      try { await invoke("close_action_menu"); } catch { /* closing */ }
    });

    // Resize window to fit content after render
    requestAnimationFrame(async () => {
      const contentEl = actionsEl.querySelector("div");
      if (contentEl) {
        const contentHeight = contentEl.scrollHeight;
        // content + button bar (~40px) + summary area (~50px) + padding
        const totalHeight = Math.min(contentHeight + 110, 500);
        try {
          const win = getCurrentWebviewWindow();
          await win.setSize(new LogicalSize(400, totalHeight));
        } catch { /* resize not critical */ }
      }
    });
  }
}

/** Extract content from the first ``` code block, or null if none found. */
function extractCodeBlock(text: string): string | null {
  const match = text.match(/```[\w]*\n([\s\S]*?)```/);
  return match ? match[1].trim() : null;
}

/** Lightweight markdown → HTML: code blocks, inline code, bold, line breaks. */
function renderMarkdownLight(text: string): string {
  let html = escapeHtml(text);

  // Fenced code blocks: ```lang\n...\n``` → styled <pre>
  html = html.replace(
    /```(\w*)\n([\s\S]*?)```/g,
    (_match, _lang, code) => `<pre style="
      background: rgba(0,0,0,0.4);
      border: 1px solid rgba(255,255,255,0.1);
      border-radius: 4px;
      padding: 8px 10px;
      margin: 6px 0;
      font-family: 'SF Mono', Menlo, monospace;
      font-size: 12px;
      line-height: 1.4;
      overflow-x: auto;
      white-space: pre;
      color: #e2e8f0;
    ">${code.trim()}</pre>`
  );

  // Inline code: `...` → styled <code>
  html = html.replace(
    /`([^`]+)`/g,
    `<code style="
      background: rgba(0,0,0,0.3);
      padding: 1px 4px;
      border-radius: 3px;
      font-family: 'SF Mono', Menlo, monospace;
      font-size: 12px;
    ">$1</code>`
  );

  // Bold: **...** → <strong>
  html = html.replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>");

  // Line breaks (but not inside <pre>)
  html = html.replace(/\n/g, "<br>");
  // Fix: remove <br> inside <pre> blocks (restore newlines)
  html = html.replace(/<pre([^>]*)>([\s\S]*?)<\/pre>/g, (_m, attrs, content) => {
    return `<pre${attrs}>${content.replace(/<br>/g, "\n")}</pre>`;
  });

  return html;
}

async function handleFileResult(result: ActionResult): Promise<void> {
  const content = result.result.text || "";
  const filename = result.result.filePath || "export.csv";

  try {
    const fullPath = await invoke<string>("write_to_desktop", {
      filename,
      content,
    });
    showFeedback(`Saved: ${filename}`);
    console.log(`[ACTION] File written to: ${fullPath}`);
    closeAfterDelay(1500);
  } catch (err) {
    showFeedback(`File write failed: ${err}`, true);
  }
}

async function handleCommandResult(result: ActionResult): Promise<void> {
  const command = result.result.command || "";
  const explanation = result.result.text || "Run this command?";

  // Open confirmation dialog window
  try {
    const confirmWindow = new WebviewWindow("confirm-dialog", {
      url: "confirm-dialog.html",
      title: "Confirm Command",
      width: 480,
      height: 300,
      decorations: false,
      alwaysOnTop: true,
      resizable: false,
      skipTaskbar: true,
    });

    // Send the command data after the window loads
    confirmWindow.once("tauri://created", async () => {
      // Small delay to let JS initialize
      setTimeout(async () => {
        await emit("confirm-command", {
          command,
          explanation,
          actionId: result.actionId,
        });
      }, 200);
    });

    // Close the action menu
    try { await invoke("close_action_menu"); } catch { /* closing */ }
  } catch (err) {
    console.error("[ACTION] Failed to open confirmation dialog:", err);
    showFeedback(`Error: ${err}`, true);
  }
}

// ── UI helpers ──────────────────────────────────────────────────────

function showLoading(actionId: string): void {
  const actionsEl = document.getElementById("menu-actions");
  if (actionsEl) {
    // Find and disable the clicked action row, show spinner
    actionsEl.querySelectorAll(".action-row").forEach((row) => {
      const el = row as HTMLElement;
      if (el.dataset.actionId === actionId) {
        el.style.opacity = "0.6";
        el.style.pointerEvents = "none";
        const label = el.querySelector("span:last-child");
        if (label) label.textContent = "Working...";
      }
    });
  }
}

function showFeedback(message: string, isError = false): void {
  const container = document.getElementById("action-menu")!;
  const feedback = document.createElement("div");
  const color = isError ? "#fca5a5" : "#4ade80";
  feedback.style.cssText = `padding:8px 14px;text-align:center;color:${color};font-size:13px;background:#1a1a2e;border-top:1px solid rgba(255,255,255,0.1)`;
  feedback.textContent = message;
  container.querySelector("div")?.appendChild(feedback);
}

function closeAfterDelay(ms: number): void {
  setTimeout(async () => {
    try { await invoke("close_action_menu"); } catch { /* closing */ }
  }, ms);
}

// ── Init: skeleton + event listeners ───────────────────────────────

let menuRendered = false;
let actionInProgress = false;

async function init(): Promise<void> {
  // Render skeleton immediately — Copy Text is clickable right away
  renderSkeleton();

  // Listen for streaming events from Rust
  listen<ActionMenuSkeleton>("action-menu-skeleton", (event) => {
    console.log("[RENDER] Received skeleton event");
    updateSummary(event.payload);
  });

  listen<ActionMenu>("action-menu-complete", (event) => {
    console.log("[RENDER] Received complete event:", event.payload.contentType);
    menuRendered = true;
    renderMenu(event.payload);
  });

  // Robust polling: events can be missed if JS loads after the Rust event fires.
  // Poll the state every 500ms until we get real data (up to 15s).
  pollForMenu();
}

async function pollForMenu(): Promise<void> {
  const MAX_POLLS = 20;  // 20 x 500ms = 10 seconds
  let polls = 0;

  const timer = setInterval(async () => {
    polls++;

    // Stop polling if menu was already rendered by an event
    if (menuRendered) {
      clearInterval(timer);
      return;
    }

    try {
      const menu = (await invoke("get_action_menu")) as ActionMenu;
      // get_action_menu returns Err while classify is still running (state = None).
      // Once it returns Ok, classify has finished — render whatever we got.
      console.log(`[RENDER] Poll #${polls}: got menu (type=${menu.contentType})`);
      menuRendered = true;
      renderMenu(menu);
      clearInterval(timer);
    } catch {
      // State is None — classify hasn't finished yet, keep polling
      if (polls >= MAX_POLLS) {
        console.log(`[RENDER] Poll timeout after ${polls} attempts`);
        clearInterval(timer);
      }
    }
  }, 500);
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

// Click outside (window blur) closes the menu.
// Only active after the menu is fully rendered — prevents premature closure
// if the window briefly loses focus during creation or while streaming.
window.addEventListener("blur", async () => {
  if (!menuRendered || actionInProgress) return;
  try {
    await invoke("close_action_menu");
  } catch {
    // Window might already be closing
  }
});

init();
