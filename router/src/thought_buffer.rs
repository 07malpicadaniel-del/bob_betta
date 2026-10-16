pub struct ParsedOutput {
    pub thinking_trace: Option<String>,
    pub clean_response: String,
    pub thought_lines: usize,
    pub thought_words: usize,
}

pub struct ThoughtBuffer {
    last_thought: Option<String>,
}

impl ThoughtBuffer {
    pub fn new() -> Self {
        Self { last_thought: None }
    }

    /// Separa el flujo interno <think>...</think> de la respuesta final al usuario
    pub fn process_output(&mut self, raw_output: &str) -> ParsedOutput {
        let trimmed = raw_output.trim();

        if let Some(end_idx) = trimmed.find("</think>") {
            let start_idx = trimmed
                .find("<think>")
                .map(|i| i + 7)
                .unwrap_or(0);

            let thought_content = trimmed[start_idx..end_idx].trim().to_string();
            let clean_response = trimmed[end_idx + 8..].trim().to_string();

            let thought_lines = thought_content.lines().count();
            let thought_words = thought_content.split_whitespace().count();

            self.last_thought = Some(thought_content.clone());

            ParsedOutput {
                thinking_trace: Some(thought_content),
                clean_response,
                thought_lines,
                thought_words,
            }
        } else {
            ParsedOutput {
                thinking_trace: None,
                clean_response: trimmed.to_string(),
                thought_lines: 0,
                thought_words: 0,
            }
        }
    }

    pub fn last_thought(&self) -> Option<&str> {
        self.last_thought.as_deref()
    }
}
