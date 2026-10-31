//! Versions (`1.2.3`, `v1.2.3`, `1.2.3-rc.1`) and the constraints a dependency may ask for
//! (`^1.2`, `~1.2.3`, `>=1.2, <2`, `=1.2.3`, `*`).

use std::cmp::Ordering;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    /// `rc.1` of `1.2.3-rc.1`: a pre-release comes before its release.
    pub pre: Option<String>,
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.major, self.minor, self.patch)
            .cmp(&(other.major, other.minor, other.patch))
            .then_with(|| match (&self.pre, &other.pre) {
                (None, None) => Ordering::Equal,
                (None, Some(_)) => Ordering::Greater,
                (Some(_), None) => Ordering::Less,
                (Some(a), Some(b)) => a.cmp(b),
            })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Version {
    /// A version tag: `v1.2.3` or `1.2.3`, with an optional `-pre-release`. Anything else is not one.
    pub fn parse(text: &str) -> Option<Version> {
        let t = text.strip_prefix('v').unwrap_or(text);
        let (core, pre) = match t.split_once('-') {
            Some((c, p)) if !p.is_empty() => (c, Some(p.to_string())),
            Some(_) => return None,
            None => (t, None),
        };
        let mut it = core.split('.');
        let (a, b, c) = (it.next()?, it.next()?, it.next()?);
        if it.next().is_some() {
            return None;
        }
        Some(Version {
            major: number(a)?,
            minor: number(b)?,
            patch: number(c)?,
            pre,
        })
    }
}

fn number(s: &str) -> Option<u64> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) || (s.len() > 1 && s.starts_with('0'))
    {
        return None;
    }
    s.parse().ok()
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Op {
    Exact,
    Greater,
    GreaterEq,
    Less,
    LessEq,
    Caret,
    Tilde,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Comparator {
    op: Op,
    major: u64,
    minor: Option<u64>,
    patch: Option<u64>,
    pre: Option<String>,
}

/// All the comparators must hold. A pre-release version only matches a constraint that names a
/// pre-release itself.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Constraint {
    parts: Vec<Comparator>,
    text: String,
}

impl Constraint {
    pub fn parse(text: &str) -> Result<Constraint, String> {
        let mut parts = Vec::new();
        for raw in text.split(',') {
            let raw = raw.trim();
            if raw.is_empty() {
                return Err(format!("`{text}` is not a version constraint"));
            }
            if raw == "*" {
                continue;
            }
            let (op, rest) = if let Some(r) = raw.strip_prefix(">=") {
                (Op::GreaterEq, r)
            } else if let Some(r) = raw.strip_prefix("<=") {
                (Op::LessEq, r)
            } else if let Some(r) = raw.strip_prefix('>') {
                (Op::Greater, r)
            } else if let Some(r) = raw.strip_prefix('<') {
                (Op::Less, r)
            } else if let Some(r) = raw.strip_prefix('=') {
                (Op::Exact, r)
            } else if let Some(r) = raw.strip_prefix('^') {
                (Op::Caret, r)
            } else if let Some(r) = raw.strip_prefix('~') {
                (Op::Tilde, r)
            } else {
                // `1.2` alone means `^1.2`
                (Op::Caret, raw)
            };
            let rest = rest.trim().strip_prefix('v').unwrap_or(rest.trim());
            let (core, pre) = match rest.split_once('-') {
                Some((c, p)) if !p.is_empty() => (c, Some(p.to_string())),
                Some(_) => return Err(format!("`{raw}` is not a version constraint")),
                None => (rest, None),
            };
            let mut nums = core.split('.').map(number);
            let bad = || format!("`{raw}` is not a version constraint");
            let major = nums.next().flatten().ok_or_else(bad)?;
            let minor = match nums.next() {
                None => None,
                Some(n) => Some(n.ok_or_else(bad)?),
            };
            let patch = match nums.next() {
                None => None,
                Some(n) => Some(n.ok_or_else(bad)?),
            };
            if nums.next().is_some() {
                return Err(bad());
            }
            parts.push(Comparator {
                op,
                major,
                minor,
                patch,
                pre,
            });
        }
        Ok(Constraint {
            parts,
            text: text.trim().to_string(),
        })
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// Both constraints at once.
    pub fn and(&self, other: &Constraint) -> Constraint {
        let mut parts = self.parts.clone();
        parts.extend(other.parts.iter().cloned());
        Constraint {
            parts,
            text: format!("{}, {}", self.text, other.text),
        }
    }

    pub fn matches(&self, v: &Version) -> bool {
        // 1.3.0-rc.1 is only for those who ask for a pre-release
        if v.pre.is_some() && !self.parts.iter().any(|c| c.pre.is_some()) {
            return false;
        }
        self.parts.iter().all(|c| c.matches(v))
    }

    /// The highest of `versions` that satisfies the constraint.
    pub fn best<'a, T>(&self, versions: impl IntoIterator<Item = (&'a Version, T)>) -> Option<T> {
        versions
            .into_iter()
            .filter(|(v, _)| self.matches(v))
            .max_by(|a, b| a.0.cmp(b.0))
            .map(|(_, t)| t)
    }
}

impl Comparator {
    fn lower(&self) -> Version {
        Version {
            major: self.major,
            minor: self.minor.unwrap_or(0),
            patch: self.patch.unwrap_or(0),
            pre: self.pre.clone(),
        }
    }

    fn matches(&self, v: &Version) -> bool {
        let base = self.lower();
        match self.op {
            Op::Exact => {
                v.major == self.major
                    && self.minor.is_none_or(|m| v.minor == m)
                    && self.patch.is_none_or(|p| v.patch == p)
                    && (self.pre.is_none() || v.pre == self.pre)
            }
            Op::Greater => match (self.minor, self.patch) {
                (Some(_), Some(_)) => *v > base,
                (Some(m), None) => (v.major, v.minor) > (self.major, m),
                _ => v.major > self.major,
            },
            Op::GreaterEq => *v >= base,
            Op::Less => *v < base,
            Op::LessEq => match (self.minor, self.patch) {
                (Some(_), Some(_)) => *v <= base,
                (Some(m), None) => (v.major, v.minor) <= (self.major, m),
                _ => v.major <= self.major,
            },
            // ^1.2.3 := >=1.2.3, <2.0.0 ; ^0.2.3 := >=0.2.3, <0.3.0 ; ^0.0.3 := =0.0.3
            Op::Caret => {
                if *v < base {
                    return false;
                }
                if self.major > 0 || self.minor.is_none() {
                    v.major == self.major
                } else if self.minor.unwrap_or(0) > 0 || self.patch.is_none() {
                    v.major == 0 && v.minor == self.minor.unwrap_or(0)
                } else {
                    v.major == 0 && v.minor == 0 && v.patch == self.patch.unwrap_or(0)
                }
            }
            // ~1.2.3 := >=1.2.3, <1.3.0 ; ~1 := >=1.0.0, <2.0.0
            Op::Tilde => {
                if *v < base {
                    return false;
                }
                match self.minor {
                    Some(m) => v.major == self.major && v.minor == m,
                    None => v.major == self.major,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap()
    }

    fn picks(constraint: &str, tags: &[&str]) -> Option<String> {
        let c = Constraint::parse(constraint).unwrap();
        let parsed: Vec<(Version, &str)> = tags
            .iter()
            .filter_map(|t| Version::parse(t).map(|x| (x, *t)))
            .collect();
        c.best(parsed.iter().map(|(x, t)| (x, *t)))
            .map(String::from)
    }

    #[test]
    fn versions_parse_and_order() {
        assert_eq!(v("v1.2.3"), v("1.2.3"));
        assert!(v("0.10.0") > v("0.9.0"));
        assert!(v("1.0.0-rc.1") < v("1.0.0"));
        for bad in ["1.2", "v1.2.3.4", "latest", "01.2.3", "1.2.3-", "a.b.c", ""] {
            assert!(Version::parse(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn the_caret_keeps_the_leftmost_number_that_is_not_zero() {
        let tags = [
            "v0.0.3", "v0.0.4", "v0.2.3", "v0.2.9", "v0.3.0", "v1.2.3", "v1.9.0", "v2.0.0",
        ];
        assert_eq!(picks("^1.2", &tags).as_deref(), Some("v1.9.0"));
        assert_eq!(picks("1.2", &tags).as_deref(), Some("v1.9.0"));
        assert_eq!(picks("^0.2.3", &tags).as_deref(), Some("v0.2.9"));
        assert_eq!(picks("^0.0.3", &tags).as_deref(), Some("v0.0.3"));
        assert_eq!(picks("^2", &tags).as_deref(), Some("v2.0.0"));
        assert_eq!(picks("^3", &tags), None);
    }

    #[test]
    fn tilde_ranges_exact_and_any() {
        let tags = ["v1.2.0", "v1.2.5", "v1.3.0", "v2.0.0"];
        assert_eq!(picks("~1.2.0", &tags).as_deref(), Some("v1.2.5"));
        assert_eq!(picks("~1", &tags).as_deref(), Some("v1.3.0"));
        assert_eq!(picks(">=1.2.1, <1.3", &tags).as_deref(), Some("v1.2.5"));
        assert_eq!(picks(">1.2.5", &tags).as_deref(), Some("v2.0.0"));
        assert_eq!(picks("<=1.2", &tags).as_deref(), Some("v1.2.5"));
        assert_eq!(picks("=1.3.0", &tags).as_deref(), Some("v1.3.0"));
        assert_eq!(picks("*", &tags).as_deref(), Some("v2.0.0"));
    }

    #[test]
    fn a_pre_release_needs_to_be_asked_for() {
        let tags = ["v1.0.0", "v1.1.0-rc.1"];
        assert_eq!(picks("^1", &tags).as_deref(), Some("v1.0.0"));
        assert_eq!(picks(">=1.1.0-rc.1", &tags).as_deref(), Some("v1.1.0-rc.1"));
    }

    #[test]
    fn two_constraints_are_one() {
        let c = Constraint::parse("^1.2")
            .unwrap()
            .and(&Constraint::parse("<1.5").unwrap());
        assert!(c.matches(&v("1.4.9")) && !c.matches(&v("1.5.0")) && !c.matches(&v("1.1.0")));
        assert!(Constraint::parse("^x").is_err());
        assert!(Constraint::parse("1.2.3.4").is_err());
        assert!(Constraint::parse("").is_err());
    }
}
