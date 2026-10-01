pub struct Position {
    line: u32,
    column: u16,
}

impl Position {
    pub fn new(line: u32, column: u16) -> Position {
        Position { line, column }
    }
}
