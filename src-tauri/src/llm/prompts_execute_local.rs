//! ChatML-format EXECUTE prompts for local Qwen-2.5 models.
//!
//! Compact versions of the Anthropic EXECUTE prompts, optimized for
//! smaller context windows and GBNF grammar enforcement.

/// EXECUTE system prompt — compact version for Qwen-2.5.
/// Includes JSON example since grammar enforcement is disabled.
pub const LOCAL_EXECUTE_SYSTEM: &str = r#"You execute actions on screen text and return a JSON result.

Rules:
1. Respond with ONLY the JSON object. No other text before or after.
2. For "text" results: concise explanation (3-8 sentences).
3. For "command" results: ALWAYS set status to "needs_confirmation".
4. NEVER suggest destructive commands (rm -rf, format, dd).

You MUST use this exact JSON format:
{"status":"success","actionId":"explain_error","result":{"type":"text","text":"Your explanation here."}}"#;

/// Max tokens for EXECUTE generation.
pub const LOCAL_EXECUTE_MAX_TOKENS: u32 = 1024;

/// Build a ChatML-formatted EXECUTE prompt for the local model.
pub fn build_local_execute_prompt(
    action_id: &str,
    extracted_text: &str,
    platform: &str,
) -> String {
    let action_instruction = match action_id {
        "explain_error" | "explain_script" | "explain_code" => {
            "Explain this error: what it means, why it occurred, and the most common cause. Result type: text."
        }
        "explain" | "explain_this" | "review_ocr" => {
            "Explain this content clearly and concisely. Result type: text."
        }
        "suggest_fix" | "fix_error" | "fix_syntax" | "fix_code" | "format_code" => {
            "Analyze and fix this error or code. If it's an environment issue, return type command with status needs_confirmation. If it's a code bug, return type text with corrected code in a ``` block."
        }
        "run_command" | "run_system_command" | "execute_command" => {
            "Generate a shell command for this request. Return type command with status needs_confirmation. Use safe, non-destructive commands."
        }
        "export_csv" | "export_to_csv" | "extract_data" => {
            "Extract tabular data as CSV. Return type file with mimeType text/csv. Put CSV in text field, filename in filePath."
        }
        "translate_text" | "translate" => {
            "Translate this text to English. Return type text. First line: Translated from [language]:"
        }
        _ => {
            "Analyze this content and provide a helpful response. Result type: text."
        }
    };

    let user_content = format!(
        "Action: {action_id}\nPlatform: {platform}\n\n{action_instruction}\n\nExtracted text:\n{extracted_text}"
    );

    format!(
        "<|im_start|>system\n{}<|im_end|>\n<|im_start|>user\n{}<|im_end|>\n<|im_start|>assistant\n",
        LOCAL_EXECUTE_SYSTEM,
        user_content,
    )
}

/// GBNF grammar for ActionResult JSON.
///
/// Constrains the local model to output valid ActionResult structure.
/// Note: Uses permissive ws placement so model-inserted whitespace
/// doesn't crash the grammar parser.
pub const ACTION_RESULT_GRAMMAR: &str = r#"root ::= "{" ws "\"status\"" ws ":" ws status ws "," ws "\"actionId\"" ws ":" ws string ws "," ws "\"result\"" ws ":" ws result ws "}"
status ::= "\"success\"" | "\"error\"" | "\"needs_confirmation\""
result ::= "{" ws "\"type\"" ws ":" ws resulttype ws "," ws "\"text\"" ws ":" ws string (ws "," ws optionalfield)* ws "}"
resulttype ::= "\"text\"" | "\"file\"" | "\"command\"" | "\"clipboard\""
optionalfield ::= "\"filePath\"" ws ":" ws string | "\"command\"" ws ":" ws string | "\"mimeType\"" ws ":" ws string
string ::= "\"" ([^"\\] | "\\" (["\\/bfnrt] | "u" [0-9a-fA-F] [0-9a-fA-F] [0-9a-fA-F] [0-9a-fA-F]))* "\""
ws ::= [ \t\n]*
"#;

/// GBNF grammar for text launcher route decision.
pub const ROUTE_DECISION_GRAMMAR: &str = r#"root ::= "{" ws "\"type\"" ws ":" ws routetype ws "," ws (directfields | toolfields) ws "}"
routetype ::= "\"direct\"" | "\"tool\""
directfields ::= "\"text\"" ws ":" ws string
toolfields ::= "\"tool_id\"" ws ":" ws string ws "," ws "\"input_text\"" ws ":" ws string
string ::= "\"" ([^"\\] | "\\" (["\\/bfnrt] | "u" [0-9a-fA-F] [0-9a-fA-F] [0-9a-fA-F] [0-9a-fA-F]))* "\""
ws ::= [ \t\n]*
"#;

/// Build a ChatML-formatted prompt for the args bridge (plugin tool args generation).
pub fn build_local_args_prompt(
    tool_name: &str,
    tool_description: &str,
    input_schema: &str,
    extracted_text: &str,
) -> String {
    let system = "You generate JSON arguments for a tool call. Given the tool's input schema and user text, produce a JSON object matching the schema exactly. Output ONLY valid JSON.";

    let user_content = format!(
        "Tool: {tool_name}\nDescription: {tool_description}\n\nInput schema:\n{input_schema}\n\nUser text:\n{extracted_text}"
    );

    format!(
        "<|im_start|>system\n{}<|im_end|>\n<|im_start|>user\n{}<|im_end|>\n<|im_start|>assistant\n",
        system,
        user_content,
    )
}

/// Build a ChatML-formatted prompt for text launcher routing.
pub fn build_local_text_command_prompt(
    user_text: &str,
    tools_prompt: &str,
) -> String {
    let system = r#"You are a command router. Given user text and available tools, decide: respond directly or route to a tool.

If responding directly: {"type": "direct", "text": "your response"}
If routing to a tool: {"type": "tool", "tool_id": "tool_name", "input_text": "text for the tool"}

Respond with ONLY valid JSON."#;

    let user_content = format!(
        "Available tools:\n{tools_prompt}\n\nUser request:\n{user_text}"
    );

    format!(
        "<|im_start|>system\n{}<|im_end|>\n<|im_start|>user\n{}<|im_end|>\n<|im_start|>assistant\n",
        system,
        user_content,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execute_prompt_uses_chatml() {
        let prompt = build_local_execute_prompt("explain_error", "NameError: foo", "macos");
        assert!(prompt.starts_with("<|im_start|>system\n"));
        assert!(prompt.contains("NameError: foo"));
        assert!(prompt.ends_with("<|im_start|>assistant\n"));
    }

    #[test]
    fn action_result_grammar_is_valid() {
        assert!(ACTION_RESULT_GRAMMAR.contains("root"));
        assert!(ACTION_RESULT_GRAMMAR.contains("status"));
        assert!(ACTION_RESULT_GRAMMAR.contains("needs_confirmation"));
    }

    #[test]
    fn args_prompt_includes_schema() {
        let prompt = build_local_args_prompt("create_issue", "Creates a GitHub issue", "{}", "error text");
        assert!(prompt.contains("create_issue"));
        assert!(prompt.contains("error text"));
    }

    #[test]
    fn text_command_prompt_includes_tools() {
        let prompt = build_local_text_command_prompt("what is 2+2", "- calculator: does math");
        assert!(prompt.contains("calculator"));
        assert!(prompt.contains("what is 2+2"));
    }
}
