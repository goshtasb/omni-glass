//! ChatML-format CLASSIFY prompts for local Qwen-2.5 models.
//!
//! These are shorter and more explicit than the Anthropic prompts because:
//! - 3B models have smaller context windows (2048 tokens budget)
//! - Smaller models need more explicit formatting guidance
//! - GBNF grammar handles structure enforcement (prompts focus on content)

/// CLASSIFY system prompt — compact version for Qwen-2.5.
///
/// Includes explicit JSON schema example since grammar enforcement is
/// disabled (llama-cpp-2 v0.1.135 crash bug). The 3B model needs a
/// concrete example to produce the correct field names.
pub const LOCAL_CLASSIFY_SYSTEM: &str = r#"You are the action engine for Omni-Glass. Analyze screen text and return a JSON action menu.

Rules:
1. Respond with ONLY the JSON object. No other text before or after.
2. Suggest 3-5 actions ranked by usefulness.
3. For errors: include "Explain Error" and "Suggest Fix".
4. For code: include "Copy Text" and "Suggest Fix".
5. For non-English text: include "Translate".

You MUST use this exact JSON format:
{"contentType":"error","confidence":0.9,"summary":"Short description","detectedLanguage":null,"actions":[{"id":"explain_error","label":"Explain Error","icon":"lightbulb","priority":1,"description":"Explain what this error means","requiresExecution":true}]}

Content types: error, code, table, prose, list, kv_pairs, math, url, mixed, unknown
Allowed icons: clipboard, table, code, lightbulb, wrench, language, search, file, terminal, mail, calculator, link, download, eye, edit, sparkles"#;

/// Max tokens for CLASSIFY generation.
pub const LOCAL_CLASSIFY_MAX_TOKENS: u32 = 400;

/// Build a ChatML-formatted CLASSIFY prompt for the local model.
///
/// Format: `<|im_start|>system\n...<|im_end|>\n<|im_start|>user\n...<|im_end|>\n<|im_start|>assistant\n`
pub fn build_local_classify_prompt(
    text: &str,
    confidence: f64,
    has_table: bool,
    has_code: bool,
    plugin_tools: &str,
) -> String {
    let user_content = build_classify_user_content(text, confidence, has_table, has_code, plugin_tools);

    format!(
        "<|im_start|>system\n{}<|im_end|>\n<|im_start|>user\n{}<|im_end|>\n<|im_start|>assistant\n",
        LOCAL_CLASSIFY_SYSTEM,
        user_content,
    )
}

/// Build the user content portion of the CLASSIFY prompt.
fn build_classify_user_content(
    text: &str,
    confidence: f64,
    has_table: bool,
    has_code: bool,
    plugin_tools: &str,
) -> String {
    let mut content = format!(
        "OCR confidence: {confidence:.2}\nHas table structure: {has_table}\nHas code structure: {has_code}\n\nExtracted text:\n{text}"
    );

    if !plugin_tools.is_empty() {
        content.push_str(&format!(
            "\n\nAvailable plugin actions (include at least one with requiresExecution: true):\n{}",
            plugin_tools
        ));
    }

    content
}

/// GBNF grammar for ActionMenu JSON.
///
/// Constrains the local model to output valid ActionMenu structure.
/// Fields: contentType, confidence, summary, detectedLanguage, actions[].
/// Uses `ws` before every comma, brace, and bracket to prevent grammar
/// rejection when the model inserts whitespace (which is valid JSON).
pub const ACTION_MENU_GRAMMAR: &str = r#"root ::= "{" ws "\"contentType\"" ws ":" ws string ws "," ws "\"confidence\"" ws ":" ws number ws "," ws "\"summary\"" ws ":" ws string ws "," ws "\"detectedLanguage\"" ws ":" ws nullablestring ws "," ws "\"actions\"" ws ":" ws "[" ws action (ws "," ws action)* ws "]" ws "}"
action ::= "{" ws "\"id\"" ws ":" ws string ws "," ws "\"label\"" ws ":" ws string ws "," ws "\"icon\"" ws ":" ws string ws "," ws "\"priority\"" ws ":" ws integer ws "," ws "\"description\"" ws ":" ws string ws "," ws "\"requiresExecution\"" ws ":" ws boolean ws "}"
nullablestring ::= string | "null"
string ::= "\"" ([^"\\] | "\\" (["\\/bfnrt] | "u" [0-9a-fA-F] [0-9a-fA-F] [0-9a-fA-F] [0-9a-fA-F]))* "\""
number ::= "-"? ("0" | [1-9] [0-9]*) ("." [0-9]+)?
integer ::= "0" | [1-9] [0-9]*
boolean ::= "true" | "false"
ws ::= [ \t\n]*
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_prompt_uses_chatml_format() {
        let prompt = build_local_classify_prompt("hello world", 0.95, false, false, "");
        assert!(prompt.starts_with("<|im_start|>system\n"));
        assert!(prompt.contains("<|im_end|>"));
        assert!(prompt.ends_with("<|im_start|>assistant\n"));
    }

    #[test]
    fn classify_prompt_includes_text() {
        let prompt = build_local_classify_prompt("some error text", 0.9, false, true, "");
        assert!(prompt.contains("some error text"));
        assert!(prompt.contains("Has code structure: true"));
    }

    #[test]
    fn classify_prompt_includes_plugin_tools() {
        let prompt = build_local_classify_prompt("text", 0.9, false, false, "- Create Issue: files bugs");
        assert!(prompt.contains("Create Issue"));
        assert!(prompt.contains("plugin actions"));
    }

    #[test]
    fn grammar_string_is_nonempty() {
        assert!(!ACTION_MENU_GRAMMAR.trim().is_empty());
        assert!(ACTION_MENU_GRAMMAR.contains("root"));
        assert!(ACTION_MENU_GRAMMAR.contains("action"));
    }
}
