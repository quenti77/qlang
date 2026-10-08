//! The boundary between the language and the outside world.
//!
//! The core never prints, reads or touches the disk: everything goes through
//! a [`Host`] supplied by the caller (terminal, web server, tests...).

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostError {
    /// This environment does not offer the requested capability.
    Unsupported(&'static str),
    Other(String),
}

/// What an environment can do. Checked at compile time so that, for example,
/// a call to `read()` is reported before running when no input is available.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Capabilities {
    /// `read()` is available.
    pub read: bool,
}

pub trait Host {
    /// Output text, exactly as given (newlines included).
    fn print(&mut self, text: &str);

    /// Read one line of input (without its newline). `Ok(None)` is end of input.
    fn read_line(&mut self) -> Result<Option<String>, HostError> {
        Err(HostError::Unsupported("read"))
    }

    /// Source text of a module, for `import`. `path` is already normalised.
    fn load_module(&mut self, _path: &str) -> Result<String, String> {
        Err("modules are not available in this environment".to_string())
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }
}

/// Resource limits for one run.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Maximum number of evaluation steps.
    pub max_steps: u64,
    /// Maximum call depth.
    pub max_depth: usize,
    /// Maximum number of output bytes.
    pub max_output: usize,
    /// Maximum size of one array (elements) or string (bytes).
    pub max_alloc: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_steps: 100_000_000,
            max_depth: 1_000,
            max_output: 10_000_000,
            max_alloc: 10_000_000,
        }
    }
}

/// A host that keeps everything in memory. Used by tests and by web servers.
#[derive(Default)]
pub struct MemoryHost {
    pub output: String,
    pub input: std::collections::VecDeque<String>,
    pub files: std::collections::HashMap<String, String>,
    pub allow_read: bool,
}

impl MemoryHost {
    pub fn new() -> MemoryHost {
        MemoryHost::default()
    }

    pub fn with_input(lines: &[&str]) -> MemoryHost {
        MemoryHost {
            input: lines.iter().map(|s| s.to_string()).collect(),
            allow_read: true,
            ..MemoryHost::default()
        }
    }
}

impl Host for MemoryHost {
    fn print(&mut self, text: &str) {
        self.output.push_str(text);
    }

    fn read_line(&mut self) -> Result<Option<String>, HostError> {
        if !self.allow_read {
            return Err(HostError::Unsupported("read"));
        }
        Ok(self.input.pop_front())
    }

    fn load_module(&mut self, path: &str) -> Result<String, String> {
        self.files
            .get(path)
            .cloned()
            .ok_or_else(|| format!("module \"{path}\" not found"))
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities { read: self.allow_read }
    }
}
