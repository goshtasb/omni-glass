/**
 * Permission prompt — shows pending plugin approvals with risk badges.
 *
 * Opened by the loader when plugins need user consent. Displays one
 * plugin at a time: name, version, permissions, risk level.
 *
 * Flow:
 * 1. Loader queues unapproved plugins in PendingApprovals state
 * 2. Loader opens this window after scanning
 * 3. This window calls get_pending_approvals to load the first plugin
 * 4. User clicks Allow/Deny → approve_plugin command
 * 5. Check for more pending; close when done
 */

import { invoke } from "@tauri-apps/api/core";

interface FsPerm {
  path: string;
  access: string;
}

interface ShellPerm {
  commands: string[];
}

interface Permissions {
  clipboard: boolean;
  network: string[] | null;
  filesystem: FsPerm[] | null;
  environment: string[] | null;
  shell: ShellPerm | null;
}

interface PendingPlugin {
  id: string;
  name: string;
  version: string;
  description: string;
  permissions: Permissions;
  riskLevel: string; // "Low" | "Medium" | "High"
  isUpdate: boolean;
}

function escapeHtml(text: string): string {
  const div = document.createElement("div");
  div.textContent = text;
  return div.innerHTML;
}

function riskColor(level: string): string {
  switch (level) {
    case "Low": return "#22c55e";
    case "Medium": return "#f59e0b";
    case "High": return "#ef4444";
    default: return "#94a3b8";
  }
}

function riskBg(level: string): string {
  switch (level) {
    case "Low": return "rgba(34,197,94,0.15)";
    case "Medium": return "rgba(245,158,11,0.15)";
    case "High": return "rgba(239,68,68,0.15)";
    default: return "rgba(148,163,184,0.1)";
  }
}

function renderPermissionList(perms: Permissions): string {
  const items: string[] = [];

  if (perms.clipboard) {
    items.push(`<li>Clipboard access</li>`);
  }

  if (perms.network && perms.network.length > 0) {
    const domains = perms.network.map(d => escapeHtml(d)).join(", ");
    items.push(`<li>Network: ${domains}</li>`);
  }

  if (perms.filesystem && perms.filesystem.length > 0) {
    for (const fs of perms.filesystem) {
      items.push(`<li>Filesystem (${escapeHtml(fs.access)}): ${escapeHtml(fs.path)}</li>`);
    }
  }

  if (perms.environment && perms.environment.length > 0) {
    const vars = perms.environment.map(v => escapeHtml(v)).join(", ");
    items.push(`<li>Environment vars: ${vars}</li>`);
  }

  if (perms.shell) {
    const cmds = perms.shell.commands.map(c => escapeHtml(c)).join(", ");
    items.push(`<li>Shell commands: ${cmds}</li>`);
  }

  if (items.length === 0) {
    items.push(`<li style="color:rgba(255,255,255,0.5)">No special permissions</li>`);
  }

  return items.join("\n");
}

function renderPlugin(plugin: PendingPlugin): void {
  const container = document.getElementById("permission-prompt")!;
  const color = riskColor(plugin.riskLevel);
  const bg = riskBg(plugin.riskLevel);
  const title = plugin.isUpdate ? "Updated Permissions" : "New Plugin";

  container.innerHTML = `
    <div style="
      background: #1a1a2e;
      border-radius: 8px;
      box-shadow: 0 4px 16px rgba(0,0,0,0.4);
      border: 1px solid rgba(255,255,255,0.1);
      padding: 16px;
      max-width: 440px;
    ">
      <div style="display:flex;align-items:center;justify-content:space-between;margin-bottom:12px;">
        <div style="font-size:15px;font-weight:600;color:#e2e8f0;">
          ${escapeHtml(title)}
        </div>
        <div style="
          background:${bg};
          color:${color};
          border:1px solid ${color};
          border-radius:12px;
          padding:2px 10px;
          font-size:11px;
          font-weight:600;
        ">${escapeHtml(plugin.riskLevel)} Risk</div>
      </div>

      <div style="margin-bottom:12px;">
        <div style="font-size:14px;font-weight:500;color:#f1f5f9;">
          ${escapeHtml(plugin.name)}
          <span style="color:rgba(255,255,255,0.4);font-weight:400;font-size:12px;">
            v${escapeHtml(plugin.version)}
          </span>
        </div>
        <div style="font-size:12px;color:rgba(255,255,255,0.5);margin-top:2px;">
          ${escapeHtml(plugin.id)}
        </div>
      </div>

      ${plugin.description ? `
        <div style="
          font-size:13px;
          color:rgba(255,255,255,0.7);
          margin-bottom:12px;
          line-height:1.4;
        ">${escapeHtml(plugin.description)}</div>
      ` : ""}

      <div style="
        background:#0d1117;
        border:1px solid rgba(255,255,255,0.1);
        border-radius:6px;
        padding:10px 12px;
        margin-bottom:16px;
      ">
        <div style="font-size:11px;font-weight:600;color:rgba(255,255,255,0.5);text-transform:uppercase;letter-spacing:0.5px;margin-bottom:8px;">
          Requested Permissions
        </div>
        <ul style="
          list-style:none;
          font-size:13px;
          color:#e2e8f0;
          line-height:1.8;
        ">
          ${renderPermissionList(plugin.permissions)}
        </ul>
      </div>

      <div style="display:flex;gap:8px;justify-content:flex-end;" id="button-row">
        <button id="btn-deny" style="
          background:transparent;
          border:1px solid rgba(255,255,255,0.2);
          color:rgba(255,255,255,0.8);
          padding:6px 16px;
          border-radius:6px;
          cursor:pointer;
          font-size:13px;
        ">Deny</button>
        <button id="btn-allow" style="
          background:#16a34a;
          border:none;
          color:white;
          padding:6px 16px;
          border-radius:6px;
          cursor:pointer;
          font-size:13px;
          font-weight:500;
        ">Allow</button>
      </div>
    </div>
  `;

  document.getElementById("btn-deny")!.addEventListener("click", () => {
    handleDecision(plugin.id, false);
  });

  document.getElementById("btn-allow")!.addEventListener("click", () => {
    handleDecision(plugin.id, true);
  });
}

async function handleDecision(pluginId: string, approved: boolean): Promise<void> {
  const allowBtn = document.getElementById("btn-allow") as HTMLButtonElement;
  const denyBtn = document.getElementById("btn-deny") as HTMLButtonElement;
  allowBtn.disabled = true;
  denyBtn.disabled = true;
  allowBtn.style.opacity = "0.6";
  denyBtn.style.opacity = "0.6";

  try {
    await invoke("approve_plugin", { pluginId, approved });
  } catch (err) {
    console.error("approve_plugin failed:", err);
  }

  // Check for more pending plugins
  await loadNext();
}

async function loadNext(): Promise<void> {
  try {
    const pending = await invoke<PendingPlugin[]>("get_pending_approvals");
    if (pending.length > 0) {
      renderPlugin(pending[0]);
    } else {
      window.close();
    }
  } catch (err) {
    console.error("get_pending_approvals failed:", err);
    window.close();
  }
}

// Escape key closes (denies remaining)
document.addEventListener("keydown", (e: KeyboardEvent) => {
  if (e.key === "Escape") {
    window.close();
  }
});

// Init: load the first pending plugin
loadNext();
