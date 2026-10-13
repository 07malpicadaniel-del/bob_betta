#[derive(Debug, Default)]
pub struct KvCacheMonitor {
    pub tokens_in_context: usize,
    pub max_tokens: usize,
}

impl KvCacheMonitor {
    pub fn new(max_tokens: usize) -> Self {
        Self {
            tokens_in_context: 0,
            max_tokens,
        }
    }

    pub fn usage_ratio(&self) -> f32 {
        if self.max_tokens == 0 {
            return 0.0;
        }
        self.tokens_in_context as f32 / self.max_tokens as f32
    }

    pub fn register_tokens(&mut self, count: usize) {
        self.tokens_in_context += count;
    }

    pub fn clear(&mut self) {
        self.tokens_in_context = 0;
    }
}
