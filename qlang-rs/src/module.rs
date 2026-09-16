use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::error::QError;

/// Host-provided way to satisfy `lire` (read) expressions. Kept behind a
/// trait so the language core never assumes a specific I/O runtime (a CLI's
/// stdin, a GUI dialog, a canned script for tests, ...).
pub trait InputSource {
    fn read(&mut self, message: &str) -> Option<String>;
}

/// An input source that never provides a value: useful for headless
/// evaluation and tests where `lire` should simply yield `rien`.
#[derive(Default)]
pub struct NoopInput;

impl InputSource for NoopInput {
    fn read(&mut self, _message: &str) -> Option<String> {
        None
    }
}

/// Resolves the source behind a module referenced by `inclure "chemin"`.
/// Kept behind a trait so embedders can supply virtual filesystems, bundled
/// assets, or in-memory sources instead of assuming real disk access.
pub trait ModuleResolver {
    fn resolve(&self, path: &str) -> Result<String, QError>;
}

/// Default resolver: reads modules from disk, relative to `base_dir`
/// (typically the directory holding the entry script), the same way
/// Node's `require` resolves relative paths.
pub struct FsModuleResolver {
    base_dir: PathBuf,
}

impl FsModuleResolver {
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
        }
    }
}

impl Default for FsModuleResolver {
    fn default() -> Self {
        Self::new(".")
    }
}

impl ModuleResolver for FsModuleResolver {
    fn resolve(&self, path: &str) -> Result<String, QError> {
        let full_path = self.base_dir.join(path);
        fs::read_to_string(&full_path).map_err(|err| {
            QError::module(format!(
                "Impossible de lire le module '{}': {err}",
                full_path.display()
            ))
        })
    }
}

/// In-memory resolver, primarily meant for tests and embedders that ship
/// their modules as static strings rather than files on disk.
#[derive(Default)]
pub struct MapModuleResolver {
    modules: HashMap<String, String>,
}

impl MapModuleResolver {
    pub fn new() -> Self {
        Self {
            modules: HashMap::new(),
        }
    }

    pub fn with_module(mut self, path: impl Into<String>, source: impl Into<String>) -> Self {
        self.modules.insert(path.into(), source.into());
        self
    }
}

impl ModuleResolver for MapModuleResolver {
    fn resolve(&self, path: &str) -> Result<String, QError> {
        self.modules
            .get(path)
            .cloned()
            .ok_or_else(|| QError::module(format!("Module '{path}' introuvable")))
    }
}
