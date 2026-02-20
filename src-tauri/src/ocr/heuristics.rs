//! Content structure heuristics for OCR output.
//!
//! Detects whether extracted text contains tabular data or source code.
//! These signals improve LLM classification accuracy.

/// Returns true if the text appears to contain tabular data.
///
/// Checks for consistent delimiters (tabs, pipes) or aligned whitespace columns.
pub fn detect_table_structure(text: &str) -> bool {
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.len() < 2 {
        return false;
    }

    let total = lines.len();

    // Check for consistent tab delimiters (>50% of lines have tabs)
    let tab_lines = lines.iter().filter(|l| l.contains('\t')).count();
    if tab_lines > total / 2 {
        return true;
    }

    // Check for consistent pipe delimiters (>50% of lines have pipes)
    let pipe_lines = lines.iter().filter(|l| l.contains('|')).count();
    if pipe_lines > total / 2 {
        return true;
    }

    // Check for aligned whitespace columns: multiple spaces at similar positions
    let space_positions: Vec<Vec<usize>> = lines
        .iter()
        .map(|line| {
            let mut positions = Vec::new();
            let mut in_spaces = false;
            for (i, ch) in line.char_indices() {
                if ch == ' ' {
                    if !in_spaces && i > 0 {
                        positions.push(i);
                    }
                    in_spaces = true;
                } else {
                    in_spaces = false;
                }
            }
            positions
        })
        .collect();

    if space_positions.len() >= 2 {
        if let Some(first_positions) = space_positions.first() {
            let aligned_count = space_positions
                .iter()
                .filter(|positions| {
                    positions
                        .iter()
                        .any(|p| first_positions.iter().any(|fp| (*p as i64 - *fp as i64).unsigned_abs() <= 2))
                })
                .count();
            if aligned_count > total / 2 {
                return true;
            }
        }
    }

    false
}

/// Returns true if the text appears to contain source code or a stack trace.
///
/// Looks for programming language keywords, bracket patterns, shell prompts,
/// and error/traceback markers. Requires at least 2 indicators to match.
pub fn detect_code_structure(text: &str) -> bool {
    let indicators: Vec<&dyn Fn(&str) -> bool> = vec![
        // Language keywords at line start
        &|t: &str| {
            t.lines().any(|l| {
                let trimmed = l.trim();
                ["import ", "from ", "const ", "let ", "var ", "function ",
                 "def ", "class ", "if ", "for ", "while ", "return "]
                    .iter()
                    .any(|kw| trimmed.starts_with(kw))
            })
        },
        // Lines ending with brackets/semicolons
        &|t: &str| {
            t.lines().any(|l| {
                let trimmed = l.trim();
                ['{', '}', ')', ';'].iter().any(|c| trimmed.ends_with(*c))
            })
        },
        // Indented control flow
        &|t: &str| {
            t.lines().any(|l| {
                l.starts_with("  ") || l.starts_with('\t')
            }) && t.lines().filter(|l| l.starts_with("  ") || l.starts_with('\t')).count() > 1
        },
        // Error/traceback markers
        &|t: &str| {
            t.lines().any(|l| {
                let trimmed = l.trim();
                trimmed.starts_with("Error") || trimmed.starts_with("Traceback")
                    || trimmed.starts_with("Exception") || trimmed.starts_with("at ")
                    || trimmed.contains("error[E") || trimmed.contains("panic!")
            })
        },
        // Comments
        &|t: &str| {
            t.lines().any(|l| {
                let trimmed = l.trim();
                trimmed.starts_with("//") || trimmed.starts_with('#')
                    || trimmed.starts_with("/*") || trimmed.starts_with('*')
            })
        },
    ];

    let match_count = indicators.iter().filter(|check| check(text)).count();
    match_count >= 2
}
