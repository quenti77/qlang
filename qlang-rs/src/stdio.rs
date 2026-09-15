/// A minimal output sink: an ordered log of printed lines. Kept as plain
/// data (no I/O of its own) so the language core stays independent of
/// whatever the host does with it (print to a terminal, append to a GUI
/// pane, collect in a test, ...).
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Std {
    log: Vec<String>,
}

impl Std {
    pub fn new() -> Self {
        Self { log: Vec::new() }
    }

    pub fn log(&self) -> &[String] {
        &self.log
    }

    pub fn print(&mut self, value: impl Into<String>) {
        self.log.push(value.into());
    }

    pub fn clear(&mut self) {
        self.log.clear();
    }
}
