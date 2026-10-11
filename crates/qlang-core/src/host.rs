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
    /// `time.now()` is available (the host has a clock).
    pub time: bool,
    /// `random` and the generators of `uuid` and `ulid` are available.
    pub random: bool,
}

pub trait Host {
    /// Output text, exactly as given (newlines included).
    fn print(&mut self, text: &str);

    /// Read one line of input (without its newline). `Ok(None)` is end of input.
    fn read_line(&mut self) -> Result<Option<String>, HostError> {
        Err(HostError::Unsupported("read"))
    }

    /// The current time: milliseconds since 1970-01-01 00:00:00 UTC.
    fn now_ms(&mut self) -> Result<i64, HostError> {
        Err(HostError::Unsupported("time"))
    }

    /// 64 random bits. Hosts that offer a seed return the same sequence for the same seed.
    fn random_u64(&mut self) -> Result<u64, HostError> {
        Err(HostError::Unsupported("random"))
    }

    /// Source text of a module, for `import`. `path` is already normalised.
    fn load_module(&mut self, _path: &str) -> Result<String, String> {
        Err("modules are not available in this environment".to_string())
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }

    /// A name that is the same for the same module however its path was written
    /// (`lib/a.q`, `lib/../lib/a.q`, a symbolic link...). Used to load a module once
    /// and to detect circular imports. By default the normalised path itself.
    fn canonical_path(&mut self, path: &str) -> String {
        path.to_string()
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
    /// The clock, in milliseconds since 1970. `None`: there is no clock.
    pub clock: Option<i64>,
    /// State of the random generator. `None`: there is no randomness.
    pub seed: Option<u64>,
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
        Capabilities { read: self.allow_read, time: self.clock.is_some(), random: self.seed.is_some() }
    }

    fn now_ms(&mut self) -> Result<i64, HostError> {
        self.clock.ok_or(HostError::Unsupported("time"))
    }

    fn random_u64(&mut self) -> Result<u64, HostError> {
        match &mut self.seed {
            Some(state) => Ok(splitmix64(state)),
            None => Err(HostError::Unsupported("random")),
        }
    }
}

/// A small deterministic generator (SplitMix64), for hosts that replay a seed.
pub fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}
