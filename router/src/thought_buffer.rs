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

    /// Separa los bloques <think>...</think> garantizando que clean_response nunca quede vacío
    pub fn process_output(&mut self, raw_output: &str) -> ParsedOutput {
        let mut remaining = raw_output
            .replace("<|im_end|>", "")
            .replace("<|endoftext|>", "")
            .trim()
            .to_string();
        let mut collected_thoughts = Vec::new();

        while let Some(end_idx) = remaining.find("</think>") {
            let prefix = &remaining[..end_idx];
            if let Some(open_pos) = prefix.find("<think>") {
                let start_idx = open_pos + 7;
                let thought_chunk = remaining[start_idx..end_idx].trim().to_string();
                if !thought_chunk.is_empty() {
                    collected_thoughts.push(thought_chunk);
                }
                let before = &remaining[..open_pos];
                let after = &remaining[end_idx + 8..];
                remaining = format!("{}\n{}", before.trim(), after.trim()).trim().to_string();
            } else {
                let thought_chunk = prefix.trim().to_string();
                if !thought_chunk.is_empty() {
                    collected_thoughts.push(thought_chunk);
                }
                remaining = remaining[end_idx + 8..].trim().to_string();
            }
        }

        if let Some(dangling_idx) = remaining.find("<think>") {
            let dangling_thought = remaining[dangling_idx + 7..].trim().to_string();
            let before = remaining[..dangling_idx].trim().to_string();
            if !before.is_empty() {
                if !dangling_thought.is_empty() {
                    collected_thoughts.push(dangling_thought);
                }
                remaining = before;
            } else {
                // Si no hay texto fuera de <think>, conservamos el contenido para no devolver vacío
                remaining = dangling_thought;
            }
        }

        if !collected_thoughts.is_empty() {
            let full_thought = collected_thoughts.join("\n---\n");
            let thought_lines = full_thought.lines().count();
            let thought_words = full_thought.split_whitespace().count();

            // Rescate automático si el modelo escribió todo dentro de <think>
            if remaining.trim().is_empty() {
                remaining = full_thought.clone();
            }

            self.last_thought = Some(full_thought.clone());

            ParsedOutput {
                thinking_trace: Some(full_thought),
                clean_response: remaining,
                thought_lines,
                thought_words,
            }
        } else {
            ParsedOutput {
                thinking_trace: None,
                clean_response: remaining,
                thought_lines: 0,
                thought_words: 0,
            }
        }
    }

    pub fn last_thought(&self) -> Option<&str> {
        self.last_thought.as_deref()
    }
}
