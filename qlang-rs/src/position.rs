#[derive(Debug, Clone, PartialEq)]
pub struct Position {
    pub index: usize,
    pub line: usize,
    pub col: usize,
    pub content: String,
}

impl Position {
    pub fn new(index: usize, line: usize, col: usize, content: impl Into<String>) -> Self {
        Self {
            index,
            line,
            col,
            content: content.into(),
        }
    }

    pub fn finish_col(&self) -> usize {
        self.col + self.content.chars().count()
    }

    /// Mirrors the TS `Position.advance`: moves the cursor past `content`
    /// (or a single character when `content` is empty) and optionally
    /// starts a new line.
    pub fn advance(&mut self, new_line: bool, content: &str) -> &mut Self {
        self.content = content.to_string();
        let step = if content.is_empty() {
            1
        } else {
            content.chars().count()
        };
        self.index += step;
        self.col += step;

        if new_line {
            self.line += 1;
            self.col = 1;
        }

        self
    }
}
