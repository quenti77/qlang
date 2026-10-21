//! Whether the cases of a `match` cover every value of the type matched.
//!
//! The patterns are lowered to constructors (the variants of an enum, `none` / a value for a
//! nullable, `true` / `false`, the one constructor of a tuple), then the classic "is a
//! wildcard still useful" matrix algorithm runs on them. Literals and ranges never cover a
//! type (they are neither a constructor nor a wildcard), so a `match` on numbers or text still
//! needs an `else`.

use super::*;
use crate::ast::Pattern;

/// A pattern reduced to what matters for coverage.
#[derive(Clone)]
enum Pat {
    /// A name or `_`: matches anything.
    Wild,
    /// A number, text or range: matches some values of a type with no finite list of values.
    Lit,
    /// A constructor (its index in the type's list) and the patterns of what it carries.
    Ctor(usize, Vec<Pat>),
}

impl Checker {
    /// Whether the unguarded `patterns` cover every value of type `ty`.
    pub(super) fn patterns_exhaustive(&mut self, ty: &Ty, patterns: &[&Pattern]) -> bool {
        let rows = patterns.iter().map(|p| vec![self.lower(p, ty)]).collect();
        self.rows_exhaustive(rows, std::slice::from_ref(ty))
    }

    fn lower(&mut self, p: &Pattern, ty: &Ty) -> Pat {
        let ty = self.resolve(ty);
        if let Ty::Error = ty {
            return Pat::Wild;
        }
        if let Ty::Nullable(inner) = &ty {
            return match p {
                Pattern::None => Pat::Ctor(0, Vec::new()),
                Pattern::Bind { id, .. } if !self.res.patterns.contains_key(id) => Pat::Wild,
                _ => Pat::Ctor(1, vec![self.lower(p, inner)]),
            };
        }
        match p {
            Pattern::Bind { id, .. } | Pattern::Path { id, .. } => {
                match self.res.patterns.get(id) {
                    Some(&(_, vi)) => Pat::Ctor(vi, Vec::new()),
                    None => Pat::Wild,
                }
            }
            Pattern::Variant { fields, id, .. } => {
                let Some(&(_, vi)) = self.res.patterns.get(id) else {
                    return Pat::Wild;
                };
                let Some(tys) = self.ctor_types(&ty).and_then(|c| c.get(vi).cloned()) else {
                    return Pat::Wild;
                };
                if tys.len() != fields.len() {
                    return Pat::Wild;
                }
                let subs = fields
                    .iter()
                    .zip(&tys)
                    .map(|(f, t)| self.lower(f, t))
                    .collect();
                Pat::Ctor(vi, subs)
            }
            Pattern::Tuple(items) => match &ty {
                Ty::Tuple(ts) if ts.len() == items.len() => Pat::Ctor(
                    0,
                    items
                        .iter()
                        .zip(ts)
                        .map(|(p, t)| self.lower(p, t))
                        .collect(),
                ),
                _ => Pat::Wild,
            },
            Pattern::Bool(b) => Pat::Ctor(usize::from(*b), Vec::new()),
            Pattern::None => Pat::Wild,
            Pattern::Int(_) | Pattern::Float(_) | Pattern::Str(_) | Pattern::Range { .. } => {
                Pat::Lit
            }
        }
    }

    /// The constructors of a type with a finite list of them, each with the types it carries.
    fn ctor_types(&mut self, ty: &Ty) -> Option<Vec<Vec<Ty>>> {
        match self.resolve(ty) {
            Ty::Enum(eid, args) => {
                let def = self.defs.enums[eid].clone();
                let map: HashMap<ParamId, Ty> = def
                    .tparams
                    .iter()
                    .copied()
                    .zip(args.iter().cloned())
                    .collect();
                Some(
                    def.variants
                        .iter()
                        .map(|v| v.fields.iter().map(|(_, t)| t.subst(&map)).collect())
                        .collect(),
                )
            }
            Ty::Tuple(ts) => Some(vec![ts]),
            Ty::Bool => Some(vec![Vec::new(), Vec::new()]),
            Ty::Nullable(inner) => Some(vec![Vec::new(), vec![*inner]]),
            _ => None,
        }
    }

    fn rows_exhaustive(&mut self, rows: Vec<Vec<Pat>>, tys: &[Ty]) -> bool {
        if tys.is_empty() {
            return !rows.is_empty();
        }
        if rows.is_empty() {
            return false;
        }
        let ctors = if rows.iter().any(|r| matches!(r[0], Pat::Ctor(..))) {
            self.ctor_types(&tys[0])
        } else {
            None
        };
        let Some(ctors) = ctors else {
            // nothing to split on: only the rows that match anything can cover the rest
            let rest = rows
                .into_iter()
                .filter(|r| matches!(r[0], Pat::Wild))
                .map(|r| r.into_iter().skip(1).collect())
                .collect();
            return self.rows_exhaustive(rest, &tys[1..]);
        };
        for (ci, carried) in ctors.iter().enumerate() {
            let mut special: Vec<Vec<Pat>> = Vec::new();
            for r in &rows {
                match &r[0] {
                    Pat::Ctor(c, subs) if *c == ci && subs.len() == carried.len() => {
                        special.push(subs.iter().chain(&r[1..]).cloned().collect());
                    }
                    Pat::Wild => {
                        special.push(
                            carried
                                .iter()
                                .map(|_| Pat::Wild)
                                .chain(r[1..].iter().cloned())
                                .collect(),
                        );
                    }
                    _ => {}
                }
            }
            let next: Vec<Ty> = carried.iter().chain(&tys[1..]).cloned().collect();
            if !self.rows_exhaustive(special, &next) {
                return false;
            }
        }
        true
    }
}
