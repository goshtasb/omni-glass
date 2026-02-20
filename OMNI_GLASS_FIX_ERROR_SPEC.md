# Fix Error: What It Actually Does vs. What It Should Do

**Saved:** February 20, 2026  
**Context:** Post-Phase 1 analysis of the "Fix Error" action's real capabilities and the gap between user expectation and product reality  
**Status:** Reference document — execute Path A after current bugs are resolved

---

## What "Fix Error" Does Right Now

The LLM sees the error text and returns a single shell command. For a `ModuleNotFoundError: No module named 'pandas'`, it returns `pip install pandas`. The user sees the command in the confirmation dialog, clicks Run, and the command executes in a child process. Output appears in the dialog.

That's it. It runs one shell command.

## What It Does NOT Do

- It doesn't open your code editor
- It doesn't edit the file that caused the error
- It doesn't know which file the error came from (it only has the OCR text of the traceback, not access to your filesystem)
- It can't navigate to line 42 of `analysis.py` and fix the bad import
- It can't refactor your code
- It can't create a new file

## The Honest Scope of "Fix Error" Today

It works for a narrow category of errors that are fixable with a single terminal command — missing packages (`pip install X`), permission issues (`chmod`), missing directories (`mkdir -p`), version mismatches (`nvm use 18`). These are real, common problems. But they're maybe 20% of the errors a developer encounters.

The other 80% — syntax errors, logic bugs, type mismatches, wrong function arguments, misconfigured YAML — require *editing a file*. And Omni-Glass cannot do that today.

---

## Two Paths Forward

### Path A: Generate a Patch and Hand It to the User (Recommended — Build First)

**Effort:** ~1 day (prompt change + UI addition)  
**Risk:** Low  
**Phase:** Can ship as a Phase 1 enhancement

The LLM reads the error, infers the likely fix, and generates a diff or code snippet. Omni-Glass displays it in the action menu with a "Copy Fix" button. The user manually pastes it into their editor. 

**How it works:**

1. User snips a traceback that includes a file path and line number (e.g., `File "/Users/dev/analysis.py", line 42, in <module>`)
2. The EXECUTE prompt is enhanced to detect two error categories:
   - **Environment errors** (missing packages, wrong versions, permissions) → return a shell command (already working)
   - **Code errors** (syntax, logic, type, import) → return a code snippet with the fix
3. For code errors, the LLM returns:
   ```json
   {
     "status": "success",
     "actionId": "suggest_fix",
     "result": {
       "type": "text",
       "text": "**File:** `analysis.py`, line 42\n\n**Current:**\n```python\nimport panda as pd\n```\n\n**Fix:**\n```python\nimport pandas as pd\n```\n\n**Explanation:** The module name is `pandas`, not `panda`."
     }
   }
   ```
4. The action menu displays the fix with:
   - The file path and line number highlighted
   - The code snippet in a monospace block
   - A "Copy Fix" button that copies just the corrected code
   - A "Copy All" button that copies the full explanation

**Prompt changes needed:**

Update `PROMPT_SUGGEST_FIX` in `prompts_execute.rs` to include:

```
Analyze this error and determine the fix type:

1. If the fix is a terminal command (install package, change permissions, create directory):
   Return status "needs_confirmation" with type "command" and the shell command.

2. If the fix requires editing source code:
   Return status "success" with type "text" containing:
   - The file path and line number from the stack trace
   - The current broken code (if visible in the error)
   - The corrected code
   - A one-sentence explanation of the fix
   
   Format the code blocks with triple backticks and the language name.

Do NOT suggest editing files you cannot see. If the error doesn't include enough context 
to determine the code fix, explain what the error means and what the user should look for 
in their code.
```

**UI changes needed:**

The action menu's text display area needs basic markdown rendering — at minimum, code blocks with monospace font and a distinct background. The "Copy Fix" button should extract just the content of the "Fix" code block, not the entire explanation.

**What this gives the user:**

Snip a `SyntaxError: unexpected indent` traceback → click "Fix Error" → see the corrected code with the indentation fixed → click "Copy Fix" → paste into their editor. Not magic, but genuinely useful. Covers maybe 50% of errors instead of 20%.

---

### Path B: Write Directly Into the User's Environment (Future — Phase 3+)

**Effort:** Weeks of work. Requires MCP plugin system.  
**Risk:** High (filesystem access, trust model, correctness)  
**Phase:** Phase 3+ via MCP plugin

Omni-Glass reads the traceback, identifies the file path and line number from the stack trace, opens the file, applies the fix, and saves it. This is the "magic" experience — but it's a fundamentally different level of system access.

**What it requires:**

1. **MCP plugin system** (Phase 2, HANDS-01 through HANDS-05) — the plugin runtime must exist before an "Editor Bridge" plugin can be loaded
2. **Filesystem access** — the plugin needs read/write to the user's project directory, gated by the sandbox permission prompt
3. **File context** — the error traceback shows a file path and line number, but the LLM needs to see the surrounding code to make a correct fix. The plugin would need to:
   - Parse the file path from the stack trace
   - Read the file from disk
   - Send the relevant code section + error to the LLM
   - Generate the fix
   - Write the fix back to the file
4. **Editor integration** (ideal but optional) — connect to VS Code's extension API or the Language Server Protocol (LSP) to apply edits through the editor rather than directly writing files. This gives the user undo support and shows the change in their editor immediately.
5. **Trust model** — the user must explicitly grant the plugin permission to read and write files in specific directories. This is the sandbox permission prompt from the PRD.

**What this looks like as a plugin:**

```
Plugin: "Code Fix Bridge"
Permissions requested:
  📁 Files: ~/projects (read-write)
  🌐 Network: none
  🔑 Secrets: none

Workflow:
  1. User snips error → CLASSIFY identifies it as a code error
  2. Action menu shows "Fix in Editor" (from the plugin)
  3. User clicks → plugin reads the file from the stack trace
  4. Plugin sends file context + error to the LLM
  5. LLM returns the fix as a diff
  6. Plugin shows the diff in a preview pane (like a PR review)
  7. User clicks "Apply Fix" → plugin writes the file
  8. If VS Code bridge is active: file opens to the changed line
```

**This is what Claude Code and Cursor do** — and they have the full project context, a conversation history, and explicit user trust to modify files. Omni-Glass would achieve something similar but through a plugin model rather than a monolithic application.

**Do not attempt this until:**
- MCP client is implemented (Phase 2, HANDS-01)
- Plugin sandbox is implemented (Phase 2, HANDS-02/03)
- Permission prompt UI exists (Phase 2, HANDS-04)
- At least one community plugin has been successfully loaded and executed

---

## Recommendation

**Build Path A now** (after current bugs are resolved). It's a 1-day enhancement that meaningfully improves the "Fix Error" experience from "works for 20% of errors" to "works for ~50% of errors." The prompt change and UI addition are minimal.

**Build Path B in Phase 3** as a flagship MCP plugin. It's the kind of plugin that demonstrates the full power of the platform and justifies the entire plugin ecosystem investment. It could be the "killer app" that drives plugin adoption.

**Never conflate the two in marketing.** Path A is "we show you the fix." Path B is "we apply the fix." Those are very different trust levels and very different products.

---

## Implementation Checklist (Path A)

- [ ] Update `PROMPT_SUGGEST_FIX` in `prompts_execute.rs` with dual-mode detection (command vs. code fix)
- [ ] Add basic markdown rendering to the action menu text display (code blocks with monospace + background)
- [ ] Add "Copy Fix" button that extracts only the corrected code block
- [ ] Test with 5 error types:
  1. Missing package (`ModuleNotFoundError`) → should return shell command (existing behavior)
  2. Syntax error (`SyntaxError: unexpected indent`) → should return code fix
  3. Type error (`TypeError: cannot unpack non-iterable int`) → should return code fix
  4. Import error with typo (`ImportError: cannot import name 'Flsk'`) → should return code fix
  5. File not found (`FileNotFoundError`) → should return shell command or explanation
- [ ] Verify that code fixes include file path + line number when available in the traceback
- [ ] Verify that "Copy Fix" copies only the corrected code, not the explanation
