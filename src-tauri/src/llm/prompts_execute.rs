//! EXECUTE pipeline prompts — per-action system and user message templates.
//!
//! The EXECUTE call is the second LLM interaction. The user has already
//! seen the action menu (from CLASSIFY) and clicked a specific action.
//! Now we send the OCR text + action-specific instructions and get
//! a structured result back.

pub const EXECUTE_MAX_TOKENS: u32 = 2048;

/// EXECUTE system prompt — instructs the LLM to perform a specific action
/// on the extracted text and return a structured JSON result.
pub const EXECUTE_SYSTEM_PROMPT: &str = r#"You are the action executor for Omni-Glass, a desktop AI utility. The user snipped a region of their screen, the OCR layer extracted text, and the user selected a specific action to perform on that text. Your job is to execute that action and return a structured JSON result.

<role>
You execute actions on extracted screen text. You return structured JSON results. You do NOT make up information — if you can't perform the action, say so in the result.
</role>

<rules>
1. ALWAYS respond with valid JSON matching the ActionResult schema below.
2. For "text" results: provide clear, concise, actionable explanations (3-8 sentences).
3. For "command" results: ALWAYS set status to "needs_confirmation". Never assume commands should auto-execute.
4. For "file" results: provide the complete file content in the text field.
5. For "command" results: suggest the simplest, safest command. Prefer package managers over manual installs.
6. NEVER suggest destructive commands (rm -rf, format, dd, etc.).
7. NEVER include API keys, credentials, or sensitive data in your response.
8. If the extracted text is insufficient to perform the action, return status "error" with an explanation.
9. Respond ONLY with the JSON object — no extra text before or after. Do NOT include a "metadata" field.
</rules>

<response_format>
{
  "status": "success" | "error" | "needs_confirmation",
  "actionId": "<the action that was requested>",
  "result": {
    "type": "text" | "file" | "command" | "clipboard",
    "text": "<explanation text or file content>",
    "filePath": "<suggested filename for file results>",
    "command": "<shell command for command results>",
    "mimeType": "<MIME type for file results>"
  }
}
</response_format>"#;

// ── Per-action user message templates ──────────────────────────────

pub const PROMPT_EXPLAIN_ERROR: &str = r#"Action: explain_error

Analyze this error message or stack trace and explain:
1. What the error means in plain English
2. Why it likely occurred
3. The most common cause

Keep the explanation concise (3-5 sentences). A developer is reading this.

Return result type "text" with your explanation.

<extracted_text>
{extracted_text}
</extracted_text>"#;

pub const PROMPT_EXPLAIN: &str = r#"Action: explain

Explain this content clearly and concisely:
1. What this content is
2. Key information or meaning
3. Any important context

Keep the explanation concise (3-5 sentences). Be helpful, not verbose.

Return result type "text" with your explanation.

<extracted_text>
{extracted_text}
</extracted_text>"#;

pub const PROMPT_SUGGEST_FIX: &str = r#"Action: suggest_fix

Analyze this error or code and determine the fix type.

Platform: {platform}
Shell: {detected_shell}

STEP 1 — Classify the fix type:

A) ENVIRONMENT FIX — the problem is a missing package, wrong version, permission issue,
   missing directory, or other issue fixable with a single terminal command.
   → Return type "command" with status "needs_confirmation".

B) CODE FIX — the problem is a syntax error, logic bug, type mismatch, wrong variable name,
   bad import, or other issue that requires editing source code.
   → Return type "text" with status "success".

STEP 2 — Return the fix:

For ENVIRONMENT FIX (type A):
- Suggest ONE safe, non-destructive shell command
- Prefer package managers (pip install, npm install, brew install, cargo add)
- Include a brief explanation in the "text" field

For CODE FIX (type B):
- In the "text" field, provide:
  Line 1-2: What's wrong (one sentence per bug found)
  Blank line
  The CORRECTED code in a ``` code block with the language name
- Fix ALL bugs you can identify, not just the first one
- Preserve the original intent and structure
- If the error includes a file path and line number, mention them

<extracted_text>
{extracted_text}
</extracted_text>"#;

pub const PROMPT_EXPORT_CSV: &str = r#"Action: export_csv

Extract the tabular data from this text and format it as a valid CSV file.

Requirements:
- Detect column headers and data rows
- Use comma as delimiter, double-quote fields that contain commas
- Include a header row
- If the data isn't clearly tabular, do your best to extract structured rows
- Suggest a descriptive filename (e.g., "sales_data_export.csv")

Return result type "file" with mimeType "text/csv".
Put the CSV content in the "text" field.
Put the suggested filename in the "filePath" field.

<extracted_text>
{extracted_text}
</extracted_text>"#;

pub const PROMPT_RUN_COMMAND: &str = r#"Action: run_command

The user wants you to perform an action on their macOS computer. Generate the shell command that accomplishes their request.

Platform: macOS
Shell: zsh

IMPORTANT:
- Return type "command" with status "needs_confirmation".
- The "command" field must contain a SINGLE shell command that accomplishes the request.
- The "text" field must contain a brief explanation of what the command does.
- Use macOS-native tools: osascript (AppleScript), defaults, open, pmset, networksetup, etc.
- For brightness: use osascript with CoreGraphics/IOKit, or the `brightness` CLI if available.
- For opening apps: use `open -a "AppName"`.
- For system preferences: use `open "x-apple.systempreferences:..."`.
- NEVER suggest destructive commands (rm -rf, format, dd, etc.).
- Keep it to ONE command. If multiple steps needed, chain with && or use a subshell.

<user_request>
{extracted_text}
</user_request>"#;

pub const PROMPT_TRANSLATE: &str = r#"Action: translate_text

Translate this text to {target_language}.

Requirements:
- Detect the source language automatically
- Provide a natural, fluent translation (not word-for-word)
- If the text is already in {target_language}, explain that and suggest improvements
- Keep formatting (line breaks, lists) intact where possible

Return result type "text". First line: "Translated from [source language]:"
Then a blank line, then the translation.

<extracted_text>
{extracted_text}
</extracted_text>"#;

// ── Summarize command output prompt ──────────────────────────────

pub const SUMMARIZE_OUTPUT_SYSTEM: &str = r#"You summarize shell command output into clear, human-readable answers. The user asked a question, a shell command was run to answer it, and you now see the raw output. Summarize the output to directly answer the user's original question.

Rules:
1. Give a concise, direct answer (1-3 sentences).
2. Include specific numbers, totals, or key data points from the output.
3. Convert raw units to human-readable form (e.g., KB → MB/GB).
4. If the output has many lines, summarize the aggregate (totals, counts, ranges).
5. Do NOT include raw command output. Do NOT mention the command that was run.
6. Respond in plain text only — no JSON, no code blocks."#;

pub const SUMMARIZE_MAX_TOKENS: u32 = 256;

/// Build the user message for summarizing command output.
pub fn build_summarize_message(
    user_question: &str,
    command: &str,
    raw_output: &str,
) -> String {
    // Truncate output to avoid hitting token limits
    let output_preview = if raw_output.len() > 3000 {
        format!("{}...\n[truncated, {} total chars]", &raw_output[..3000], raw_output.len())
    } else {
        raw_output.to_string()
    };

    format!(
        r#"User's question: {user_question}

Command that was run: {command}

Raw output:
{output_preview}

Summarize this output to directly answer the user's question."#
    )
}

/// Build the user message for an EXECUTE call by selecting the
/// appropriate action template and filling in placeholders.
pub fn build_execute_message(
    action_id: &str,
    extracted_text: &str,
    platform: &str,
) -> String {
    let template = match action_id {
        "explain_error" | "explain_script" | "explain_code" => PROMPT_EXPLAIN_ERROR,
        "explain" | "explain_this" | "review_ocr" => PROMPT_EXPLAIN,
        "suggest_fix" | "fix_error" | "fix_syntax" | "fix_code" | "format_code" => PROMPT_SUGGEST_FIX,
        "run_command" | "run_system_command" | "execute_command" => PROMPT_RUN_COMMAND,
        "export_csv" | "export_to_csv" | "extract_to_csv" | "extract_data" | "extract_csv" => PROMPT_EXPORT_CSV,
        "translate_text" | "translate" => PROMPT_TRANSLATE,
        _ => PROMPT_EXPLAIN, // default fallback
    };

    template
        .replace("{extracted_text}", extracted_text)
        .replace("{platform}", platform)
        .replace("{detected_shell}", "zsh")
        .replace("{target_language}", "English")
}
