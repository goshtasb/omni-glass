/**
 * Text launcher — type a command, get autonomous execution.
 *
 * Opened from tray menu "Type Command". Provides a text input
 * that routes typed commands through the LLM pipeline.
 * The LLM decides the result type and the launcher auto-executes:
 *   - command → show confirmation, then run via shell
 *   - clipboard → auto-copy to clipboard
 *   - file → save to Desktop
 *   - text → display inline
 *
 * Enter = submit, Escape = close.
 * Window auto-resizes to fit response content.
 */

import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-shell";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { LogicalSize, PhysicalPosition } from "@tauri-apps/api/dpi";

const appWindow = getCurrentWindow();
const WIDTH = 600;
const INPUT_HEIGHT = 72;
const MAX_HEIGHT = 500;

// Store the original user question for command output summarization
let lastUserQuestion = "";

// ── Window sizing ────────────────────────────────────────────────────

async function resizeToContent(): Promise<void> {
  await new Promise(r => setTimeout(r, 20));
  const outer = document.getElementById("launcher")!;
  const h = Math.min(Math.max(outer.scrollHeight + 16, INPUT_HEIGHT), MAX_HEIGHT);
  try {
    await appWindow.setSize(new LogicalSize(WIDTH, h));
  } catch { /* window closing */ }
}

async function resetSize(): Promise<void> {
  try {
    await appWindow.setSize(new LogicalSize(WIDTH, INPUT_HEIGHT));
  } catch { /* window closing */ }
}

// ── Render ───────────────────────────────────────────────────────────

function renderInput(): void {
  const container = document.getElementById("launcher")!;
  container.innerHTML = `
    <div style="
      background: #1a1a2e;
      border: 1px solid rgba(255,255,255,0.15);
      border-radius: 8px;
      box-shadow: 0 4px 16px rgba(0,0,0,0.4);
      overflow: hidden;
    ">
      <div id="drag-handle" style="
        height: 14px;
        cursor: grab;
        display: flex;
        align-items: center;
        justify-content: center;
        background: rgba(255,255,255,0.03);
      ">
        <div style="
          width: 32px;
          height: 4px;
          border-radius: 2px;
          background: rgba(255,255,255,0.2);
          pointer-events: none;
        "></div>
      </div>
      <input id="text-input" type="text" placeholder="Ask anything or type a command..." style="
        width: 100%;
        padding: 12px 14px;
        background: transparent;
        border: none;
        color: #e2e8f0;
        font-size: 15px;
        outline: none;
        font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
      " autofocus />
      <div id="result-area"></div>
    </div>
  `;

  const input = document.getElementById("text-input") as HTMLInputElement;
  input.addEventListener("keydown", async (e) => {
    if (e.key === "Enter" && input.value.trim()) {
      e.preventDefault();
      await submitCommand(input.value.trim());
    }
    if (e.key === "Escape") {
      await closeLauncher();
    }
  });

  const dragHandle = document.getElementById("drag-handle");
  if (dragHandle) {
    let startX = 0;
    let startY = 0;
    let winX = 0;
    let winY = 0;

    dragHandle.addEventListener("mousedown", async (e) => {
      e.preventDefault();
      const scale = window.devicePixelRatio;
      startX = e.screenX;
      startY = e.screenY;
      const pos = await appWindow.outerPosition();
      winX = pos.x;
      winY = pos.y;
      dragHandle.style.cursor = "grabbing";

      const onMove = (ev: MouseEvent) => {
        const dx = (ev.screenX - startX) * scale;
        const dy = (ev.screenY - startY) * scale;
        appWindow.setPosition(new PhysicalPosition(winX + dx, winY + dy));
      };

      const onUp = () => {
        dragHandle.style.cursor = "grab";
        document.removeEventListener("mousemove", onMove);
        document.removeEventListener("mouseup", onUp);
      };

      document.addEventListener("mousemove", onMove);
      document.addEventListener("mouseup", onUp);
    });
  }
}

// ── Submit ───────────────────────────────────────────────────────────

async function submitCommand(text: string): Promise<void> {
  lastUserQuestion = text;
  const input = document.getElementById("text-input") as HTMLInputElement;
  input.disabled = true;
  input.style.opacity = "0.5";

  showStatus("Thinking...");
  await resizeToContent();

  try {
    const result = await invoke<TextCommandResult>("execute_text_command", { text });

    if (result.status === "error") {
      showTextResult(result.text || "Something went wrong.", true);
    } else {
      await handleResult(result);
    }
  } catch (err) {
    showTextResult(`Error: ${err}`, true);
  }

  await resizeToContent();
}

interface TextCommandResult {
  status: string;
  text: string;
  actionId: string | null;
  resultType: string;
  command: string | null;
  filePath: string | null;
  fileContent: string | null;
  clipboardContent: string | null;
}

// ── Result routing ──────────────────────────────────────────────────

async function handleResult(result: TextCommandResult): Promise<void> {
  switch (result.resultType) {
    case "command":
      showCommandConfirmation(result);
      break;
    case "clipboard":
      await handleClipboard(result);
      break;
    case "file":
      await handleFile(result);
      break;
    default:
      // "text" or unknown — check for URLs in the response
      showTextResult(result.text || "Done.", false);
      break;
  }
}

// ── Command execution (with confirmation) ───────────────────────────

function showCommandConfirmation(result: TextCommandResult): void {
  const area = document.getElementById("result-area")!;
  const cmd = result.command || "";
  const explanation = result.text || "";

  area.innerHTML = `
    ${explanation ? `<div style="
      padding: 10px 14px;
      font-size: 13px;
      color: rgba(255,255,255,0.7);
      border-top: 1px solid rgba(255,255,255,0.08);
      line-height: 1.4;
    ">${renderLight(explanation)}</div>` : ""}
    <div style="
      padding: 8px 14px;
      border-top: 1px solid rgba(255,255,255,0.08);
    ">
      <div style="
        font-size: 11px;
        color: rgba(255,255,255,0.4);
        margin-bottom: 4px;
      ">Command to run:</div>
      <pre style="
        background: rgba(0,0,0,0.4);
        border: 1px solid rgba(255,255,255,0.1);
        border-radius: 4px;
        padding: 8px 10px;
        font-family: 'SF Mono', Menlo, monospace;
        font-size: 12px;
        color: #e2e8f0;
        white-space: pre-wrap;
        word-break: break-all;
        margin: 0;
      ">${escapeHtml(cmd)}</pre>
    </div>
    <div style="
      padding: 6px 14px 8px;
      display: flex;
      gap: 8px;
      justify-content: flex-end;
      border-top: 1px solid rgba(255,255,255,0.08);
    ">
      <button id="btn-cancel" style="
        background: transparent;
        border: 1px solid rgba(255,255,255,0.2);
        color: rgba(255,255,255,0.7);
        padding: 4px 12px;
        border-radius: 4px;
        cursor: pointer;
        font-size: 12px;
      ">Cancel</button>
      <button id="btn-run" style="
        background: #3b82f6;
        border: 1px solid #3b82f6;
        color: white;
        padding: 4px 14px;
        border-radius: 4px;
        cursor: pointer;
        font-size: 12px;
        font-weight: 500;
      ">Run</button>
    </div>
  `;

  document.getElementById("btn-cancel")?.addEventListener("click", () => closeLauncher());
  document.getElementById("btn-run")?.addEventListener("click", async () => {
    const btn = document.getElementById("btn-run") as HTMLButtonElement;
    btn.disabled = true;
    btn.textContent = "Running...";
    btn.style.opacity = "0.6";

    try {
      const rawOutput = await invoke<string>("run_confirmed_command", { command: cmd });
      if (!rawOutput) {
        showTextResult("Command completed successfully.", false);
      } else {
        // Summarize multi-line output through LLM for human-readable answer
        showStatus("Summarizing...");
        await resizeToContent();
        try {
          const summary = await invoke<string>("summarize_command_output", {
            userQuestion: lastUserQuestion,
            command: cmd,
            rawOutput,
          });
          showTextResult(summary, false);
        } catch {
          showTextResult(rawOutput, false); // Fallback to raw output
        }
      }
    } catch (err) {
      showTextResult(`Command failed: ${err}`, true);
    }
    await resizeToContent();
  });
}

// ── Clipboard auto-copy ─────────────────────────────────────────────

async function handleClipboard(result: TextCommandResult): Promise<void> {
  const content = result.clipboardContent || result.text || "";
  try {
    await invoke("copy_to_clipboard", { text: content });
    showTextResult("Copied to clipboard.", false);
    setTimeout(() => closeLauncher(), 1000);
  } catch (err) {
    showTextResult(`Failed to copy: ${err}`, true);
  }
}

// ── File save ───────────────────────────────────────────────────────

async function handleFile(result: TextCommandResult): Promise<void> {
  const filename = result.filePath || "output.txt";
  const content = result.fileContent || result.text || "";
  try {
    const path = await invoke<string>("write_to_desktop", { filename, content });
    showTextResult(`Saved to: ${path}`, false);
  } catch (err) {
    showTextResult(`Failed to save file: ${err}`, true);
  }
}

// ── Result display ───────────────────────────────────────────────────

function showStatus(message: string): void {
  const area = document.getElementById("result-area")!;
  area.innerHTML = `
    <div style="
      padding: 10px 14px;
      font-size: 13px;
      color: rgba(255,255,255,0.5);
      border-top: 1px solid rgba(255,255,255,0.08);
    ">${escapeHtml(message)}</div>
  `;
}

function showTextResult(text: string, isError: boolean): void {
  const area = document.getElementById("result-area")!;
  const color = isError ? "#fca5a5" : "rgba(255,255,255,0.85)";

  // Extract URLs from the text for clickable links
  const urls = extractUrls(text);

  area.innerHTML = `
    <div style="
      padding: 12px 14px;
      font-size: 13px;
      color: ${color};
      line-height: 1.5;
      max-height: 340px;
      overflow-y: auto;
      border-top: 1px solid rgba(255,255,255,0.08);
      word-wrap: break-word;
    ">${renderLight(text)}</div>
    <div style="
      padding: 6px 14px 8px;
      display: flex;
      gap: 8px;
      justify-content: flex-end;
      border-top: 1px solid rgba(255,255,255,0.08);
    ">
      ${urls.length > 0 ? `<button id="btn-open-url" style="
        background: #3b82f6;
        border: 1px solid #3b82f6;
        color: white;
        padding: 4px 12px;
        border-radius: 4px;
        cursor: pointer;
        font-size: 12px;
      ">Open Link</button>` : ""}
      <button id="btn-copy" style="
        background: transparent;
        border: 1px solid rgba(255,255,255,0.2);
        color: rgba(255,255,255,0.7);
        padding: 4px 12px;
        border-radius: 4px;
        cursor: pointer;
        font-size: 12px;
      ">Copy</button>
      <button id="btn-close" style="
        background: transparent;
        border: 1px solid rgba(255,255,255,0.2);
        color: rgba(255,255,255,0.7);
        padding: 4px 12px;
        border-radius: 4px;
        cursor: pointer;
        font-size: 12px;
      ">Close</button>
    </div>
  `;

  if (urls.length > 0) {
    document.getElementById("btn-open-url")?.addEventListener("click", async () => {
      try { await open(urls[0]); } catch { /* ignore */ }
      setTimeout(() => closeLauncher(), 500);
    });
  }

  document.getElementById("btn-copy")?.addEventListener("click", async () => {
    await invoke("copy_to_clipboard", { text });
    const btn = document.getElementById("btn-copy")!;
    btn.textContent = "Copied";
    setTimeout(() => closeLauncher(), 500);
  });

  document.getElementById("btn-close")?.addEventListener("click", () => closeLauncher());
}

// ── Helpers ──────────────────────────────────────────────────────────

function escapeHtml(text: string): string {
  const div = document.createElement("div");
  div.textContent = text;
  return div.innerHTML;
}

function extractUrls(text: string): string[] {
  const urlPattern = /https?:\/\/[^\s<>"')\]]+/g;
  return text.match(urlPattern) || [];
}

/** Minimal markdown: code blocks, inline code, bold, line breaks. */
function renderLight(text: string): string {
  let html = escapeHtml(text);
  html = html.replace(
    /```(\w*)\n([\s\S]*?)```/g,
    (_m, _lang, code) => `<pre style="
      background:rgba(0,0,0,0.4);border:1px solid rgba(255,255,255,0.1);
      border-radius:4px;padding:8px 10px;margin:6px 0;
      font-family:'SF Mono',Menlo,monospace;font-size:12px;
      line-height:1.4;overflow-x:auto;white-space:pre;color:#e2e8f0;
    ">${code.trim()}</pre>`
  );
  html = html.replace(/`([^`]+)`/g,
    `<code style="background:rgba(0,0,0,0.3);padding:1px 4px;border-radius:3px;font-family:'SF Mono',Menlo,monospace;font-size:12px;">$1</code>`
  );
  html = html.replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>");
  html = html.replace(/\n/g, "<br>");
  html = html.replace(/<pre([^>]*)>([\s\S]*?)<\/pre>/g, (_m, a, c) =>
    `<pre${a}>${c.replace(/<br>/g, "\n")}</pre>`
  );
  return html;
}

async function closeLauncher(): Promise<void> {
  await resetSize();
  try { await invoke("close_text_launcher"); } catch { /* closing */ }
}

// ── Init ─────────────────────────────────────────────────────────────

document.addEventListener("keydown", async (e: KeyboardEvent) => {
  if (e.key === "Escape") await closeLauncher();
});

renderInput();
