//! Positions in source files.


/// A byte range in one source file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Span {
    pub file: u32,
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub fn new(file: u32, start: usize, end: usize) -> Span {
        Span {
            file,
            start: start as u32,
            end: end as u32,
        }
    }

    /// Smallest span covering both.
    pub fn to(self, other: Span) -> Span {
        Span {
            file: self.file,
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

pub struct SourceFile {
    pub name: String,
    pub text: String,
    line_starts: Vec<u32>,
}

/// All source files of one compilation.
#[derive(Default)]
pub struct SourceMap {
    pub files: Vec<SourceFile>,
}

/// A resolved human-readable location (1-based lines and columns).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Location {
    pub file: String,
    pub line: u32,
    pub col: u32,
    pub end_line: u32,
    pub end_col: u32,
}

impl SourceMap {
    pub fn add(&mut self, name: &str, text: &str) -> u32 {
        let mut line_starts = vec![0u32];
        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i as u32 + 1);
            }
        }
        self.files.push(SourceFile {
            name: name.to_string(),
            text: text.to_string(),
            line_starts,
        });
        (self.files.len() - 1) as u32
    }

    pub fn file(&self, id: u32) -> Option<&SourceFile> {
        self.files.get(id as usize)
    }

    fn line_col(&self, file: &SourceFile, offset: u32) -> (u32, u32) {
        let line = match file.line_starts.binary_search(&offset) {
            Ok(i) => i,
            Err(i) => i - 1,
        };
        let line_start = file.line_starts[line] as usize;
        let upto = (offset as usize).min(file.text.len());
        // columns count characters, not bytes
        let col = file.text.get(line_start..upto).map_or(0, |s| s.chars().count());
        (line as u32 + 1, col as u32 + 1)
    }

    pub fn locate(&self, span: Span) -> Location {
        match self.file(span.file) {
            Some(f) => {
                let (line, col) = self.line_col(f, span.start);
                let (end_line, end_col) = self.line_col(f, span.end.max(span.start));
                Location {
                    file: f.name.clone(),
                    line,
                    col,
                    end_line,
                    end_col,
                }
            }
            None => Location {
                file: "<unknown>".to_string(),
                line: 1,
                col: 1,
                end_line: 1,
                end_col: 1,
            },
        }
    }

    /// The text of a 1-based line, without its newline.
    pub fn line_text(&self, file: u32, line: u32) -> Option<&str> {
        let f = self.file(file)?;
        let start = *f.line_starts.get(line as usize - 1)? as usize;
        let rest = &f.text[start..];
        Some(rest.split('\n').next().unwrap_or("").trim_end_matches('\r'))
    }
}

/// Split a path into its root (`""`, `"/"` or a drive like `"C:/"`) and the rest.
/// Backslashes must already have been turned into `/`.
fn split_root(path: &str) -> (&str, &str) {
    let b = path.as_bytes();
    if path.starts_with('/') {
        ("/", path.trim_start_matches('/'))
    } else if b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':' {
        let end = if b.get(2) == Some(&b'/') { 3 } else { 2 };
        (&path[..end], &path[end..])
    } else {
        ("", path)
    }
}

/// Resolve an `import` path relative to the importing file.
///
/// Import paths always use `/` (on every system) and are never absolute: `/x.q`, `C:/x.q`
/// and anything with a backslash are refused. The result is normalised (`.` and inner `..`
/// removed). A relative importer may reach outside its own directory: the result then starts
/// with `..` (`../lib/x.q`). Whether that file may be read is for the host to decide.
/// Nothing can go above the root of an absolute importer. The importer may be written with
/// `\` (a Windows path); the result then uses `/`.
pub fn resolve_module_path(importer: &str, import_path: &str) -> Option<String> {
    if import_path.is_empty() || import_path.contains('\\') || split_root(import_path).0 != "" {
        return None;
    }
    let importer = importer.replace('\\', "/");
    let (root, rest) = split_root(&importer);
    let dir = rest.rfind('/').map_or("", |i| &rest[..i]);
    let joined = if dir.is_empty() { import_path.to_string() } else { format!("{dir}/{import_path}") };
    let mut parts: Vec<&str> = Vec::new();
    for part in joined.split('/') {
        match part {
            "" | "." => {}
            ".." => match parts.last() {
                Some(&last) if last != ".." => {
                    parts.pop();
                }
                _ if !root.is_empty() => return None,
                _ => parts.push(".."),
            },
            p => parts.push(p),
        }
    }
    if parts.is_empty() || parts.last() == Some(&"..") {
        return None;
    }
    Some(format!("{root}{}", parts.join("/")))
}

#[cfg(test)]
mod tests {
    use super::resolve_module_path as r;

    #[test]
    fn imports_are_relative_to_the_importing_file() {
        assert_eq!(r("main.q", "math.q").as_deref(), Some("math.q"));
        assert_eq!(r("lib/a.q", "b.q").as_deref(), Some("lib/b.q"));
        assert_eq!(r("lib/a.q", "../c.q").as_deref(), Some("c.q"));
        assert_eq!(r("lib/a.q", "./sub/../b.q").as_deref(), Some("lib/b.q"));
        assert_eq!(r("app/main.q", "../lib/x.q").as_deref(), Some("lib/x.q"));
    }

    #[test]
    fn a_relative_importer_may_go_outside_its_directory() {
        assert_eq!(r("main.q", "../x.q").as_deref(), Some("../x.q"));
        assert_eq!(r("../x.q", "y.q").as_deref(), Some("../y.q"));
        assert_eq!(r("../x.q", "../z.q").as_deref(), Some("../../z.q"));
        assert_eq!(r("a/b/main.q", "../../../x.q").as_deref(), Some("../x.q"));
        // the same file written two ways has the same name
        assert_eq!(r("sub/main.q", "../sub/other.q"), r("sub/main.q", "other.q"));
    }

    #[test]
    fn absolute_paths_and_nonsense_are_refused() {
        assert_eq!(r("main.q", "/etc/x.q"), None);
        assert_eq!(r("main.q", ""), None);
        assert_eq!(r("main.q", ".."), None);
        assert_eq!(r("main.q", "dir/.."), None);
        assert_eq!(r("/home/me/main.q", "../../../../x.q"), None, "above the root of the file system");
        assert_eq!(r("/home/me/main.q", "../lib/x.q").as_deref(), Some("/home/lib/x.q"));
    }

    #[test]
    fn windows_paths_are_understood() {
        // an importer written the Windows way: the drive is the root, `..` stays under it
        assert_eq!(r("C:\\proj\\main.q", "lib/x.q").as_deref(), Some("C:/proj/lib/x.q"));
        assert_eq!(r("C:\\proj\\app\\main.q", "../lib/x.q").as_deref(), Some("C:/proj/lib/x.q"));
        assert_eq!(r("C:\\main.q", "../x.q"), None, "above the drive root");
        assert_eq!(r("examples\\tour.q", "tour_math.q").as_deref(), Some("examples/tour_math.q"));
        // import paths use `/` everywhere and are never absolute
        assert_eq!(r("main.q", "lib\\x.q"), None);
        assert_eq!(r("main.q", "C:/x.q"), None);
        assert_eq!(r("main.q", "C:x.q"), None);
        assert_eq!(r("main.q", "\\\\server\\share\\x.q"), None);
    }
}
