use std::collections::HashMap;
use std::fs;

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

/// Default resolver: reads modules straight from disk.
#[derive(Default)]
pub struct FsModuleResolver;

impl ModuleResolver for FsModuleResolver {
    fn resolve(&self, path: &str) -> Result<String, QError> {
        fs::read_to_string(path)
            .map_err(|err| QError::module(format!("Impossible de lire le module '{path}': {err}")))
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
        Self { modules: HashMap::new() }
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
