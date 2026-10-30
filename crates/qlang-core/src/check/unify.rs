//! Type relations: assignability, equality with inference variables,
//! joins, trait implementation lookup.

use super::*;

/// How a language trait (operator, `as`, index...) is implemented for a type.
#[derive(Clone, Debug)]
pub(crate) enum LangImpl {
    /// The language provides it; the result type is given.
    Builtin(Ty),
    /// A generic parameter with a bound; the result type is given.
    Bound(Ty),
    /// A user `impl`.
    #[allow(dead_code)]
    User { fid: FnId, ret: Ty },
}

impl LangImpl {
    pub fn ret(&self) -> Ty {
        match self {
            LangImpl::Builtin(t) | LangImpl::Bound(t) => t.clone(),
            LangImpl::User { ret, .. } => ret.clone(),
        }
    }
}

pub(crate) struct UserMethod {
    pub fid: FnId,
    pub impl_id: ImplId,
    /// Substitution of the impl's generic parameters (to inference variables).
    pub map: HashMap<ParamId, Ty>,
}

impl Checker {
    pub fn fresh(&mut self) -> Ty {
        self.subst.push(None);
        Ty::Infer(self.subst.len() as u32 - 1)
    }

    /// Replace solved inference variables, deeply.
    pub fn resolve(&self, t: &Ty) -> Ty {
        match t {
            Ty::Infer(n) => match &self.subst[*n as usize] {
                Some(inner) => self.resolve(inner),
                None => t.clone(),
            },
            Ty::Nullable(x) => Ty::nullable(self.resolve(x)),
            Ty::Array(x) => Ty::Array(Box::new(self.resolve(x))),
            Ty::Map(k, v) => Ty::Map(Box::new(self.resolve(k)), Box::new(self.resolve(v))),
            Ty::Fn(ps, r) => Ty::Fn(
                ps.iter().map(|p| self.resolve(p)).collect(),
                Box::new(self.resolve(r)),
            ),
            Ty::Struct(id, a) => Ty::Struct(*id, a.iter().map(|x| self.resolve(x)).collect()),
            Ty::Enum(id, a) => Ty::Enum(*id, a.iter().map(|x| self.resolve(x)).collect()),
            Ty::Tuple(ts) => Ty::Tuple(ts.iter().map(|x| self.resolve(x)).collect()),
            Ty::Trait(id, a) => Ty::Trait(*id, a.iter().map(|x| self.resolve(x)).collect()),
            other => other.clone(),
        }
    }

    fn occurs(&self, n: u32, t: &Ty) -> bool {
        match self.resolve(t) {
            Ty::Infer(m) => m == n,
            Ty::Nullable(x) | Ty::Array(x) => self.occurs(n, &x),
            Ty::Map(k, v) => self.occurs(n, &k) || self.occurs(n, &v),
            Ty::Fn(ps, r) => ps.iter().any(|p| self.occurs(n, p)) || self.occurs(n, &r),
            Ty::Struct(_, a) | Ty::Enum(_, a) | Ty::Trait(_, a) | Ty::Tuple(a) => {
                a.iter().any(|x| self.occurs(n, x))
            }
            _ => false,
        }
    }

    fn bind(&mut self, n: u32, t: &Ty) -> bool {
        if self.occurs(n, t) {
            return false;
        }
        self.subst[n as usize] = Some(t.clone());
        true
    }

    /// Replace the struct's own generic parameters by `args`.
    pub fn ancestor_args(&self, sid: StructId, args: &[Ty], target: StructId) -> Option<Vec<Ty>> {
        self.defs
            .ancestry(sid, args)
            .into_iter()
            .find(|(s, _)| *s == target)
            .map(|(_, a)| a)
    }

    /// Can a value of type `from` be used where `to` is expected?
    pub fn assignable(&mut self, from: &Ty, to: &Ty) -> bool {
        let f = self.resolve(from);
        let t = self.resolve(to);
        match (&f, &t) {
            (Ty::Error, _) | (_, Ty::Error) | (Ty::Never, _) => true,
            (Ty::Infer(a), Ty::Infer(b)) if a == b => true,
            (_, Ty::Infer(b)) => {
                if matches!(f, Ty::NoneT) {
                    true
                } else {
                    self.bind(*b, &f)
                }
            }
            (Ty::Infer(a), _) => self.bind(*a, &t),
            (Ty::NoneT, Ty::Nullable(_)) | (Ty::NoneT, Ty::NoneT) => true,
            (Ty::Nullable(a), Ty::Nullable(b)) => self.assignable(a, b),
            (_, Ty::Nullable(b)) => self.assignable(&f, b),
            (Ty::Array(a), Ty::Array(b)) => self.same(a, b),
            (Ty::Map(ka, va), Ty::Map(kb, vb)) => self.same(ka, kb) && self.same(va, vb),
            // tuples never change, so each element may be used where a wider one is expected
            (Ty::Tuple(a), Ty::Tuple(b)) => {
                a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| self.assignable(x, y))
            }
            (Ty::Fn(pa, ra), Ty::Fn(pb, rb)) => {
                pa.len() == pb.len()
                    && pa.iter().zip(pb.iter()).all(|(a, b)| self.assignable(b, a))
                    && (**rb == Ty::Unit || self.assignable(ra, rb))
            }
            (Ty::Struct(a, aa), Ty::Struct(b, ba)) => match self.ancestor_args(*a, aa, *b) {
                Some(args) => {
                    args.len() == ba.len()
                        && args.iter().zip(ba.iter()).all(|(x, y)| self.same(x, y))
                }
                None => false,
            },
            // a trait value is also a value of every trait it extends
            (_, Ty::Trait(tid, targs)) => self.implements(&f, *tid, targs),
            (Ty::Param(a), Ty::Param(b)) => a == b,
            (Ty::Enum(a, aa), Ty::Enum(b, ba)) => {
                a == b
                    && aa.len() == ba.len()
                    && aa.iter().zip(ba.iter()).all(|(x, y)| self.same(x, y))
            }
            (a, b) => a == b && !matches!(a, Ty::Param(_)),
        }
    }

    /// Equality, binding inference variables as needed. `never` matches anything
    /// (it is the element type of `[]`).
    pub fn same(&mut self, a: &Ty, b: &Ty) -> bool {
        let a = self.resolve(a);
        let b = self.resolve(b);
        match (&a, &b) {
            (Ty::Error, _) | (_, Ty::Error) | (Ty::Never, _) | (_, Ty::Never) => true,
            (Ty::Infer(x), Ty::Infer(y)) if x == y => true,
            (Ty::Infer(x), _) => self.bind(*x, &b),
            (_, Ty::Infer(y)) => self.bind(*y, &a),
            (Ty::Nullable(x), Ty::Nullable(y)) => self.same(x, y),
            (Ty::Array(x), Ty::Array(y)) => self.same(x, y),
            (Ty::Map(ka, va), Ty::Map(kb, vb)) => self.same(ka, kb) && self.same(va, vb),
            (Ty::Tuple(x), Ty::Tuple(y)) => {
                x.len() == y.len() && x.iter().zip(y.iter()).all(|(p, q)| self.same(p, q))
            }
            (Ty::Fn(pa, ra), Ty::Fn(pb, rb)) => {
                pa.len() == pb.len()
                    && pa.iter().zip(pb.iter()).all(|(x, y)| self.same(x, y))
                    && self.same(ra, rb)
            }
            (Ty::Struct(x, xa), Ty::Struct(y, ya))
            | (Ty::Enum(x, xa), Ty::Enum(y, ya))
            | (Ty::Trait(x, xa), Ty::Trait(y, ya)) => {
                x == y
                    && xa.len() == ya.len()
                    && xa.iter().zip(ya.iter()).all(|(p, q)| self.same(p, q))
            }
            _ => a == b,
        }
    }

    /// The common type of two branches, if any.
    pub fn join(&mut self, a: &Ty, b: &Ty) -> Option<Ty> {
        let a = self.resolve(a);
        let b = self.resolve(b);
        match (&a, &b) {
            (Ty::Never, _) | (Ty::Error, _) => return Some(b),
            (_, Ty::Never) | (_, Ty::Error) => return Some(a),
            (Ty::NoneT, Ty::NoneT) => return Some(Ty::NoneT),
            (Ty::NoneT, x) | (x, Ty::NoneT) => return Some(Ty::nullable(x.clone())),
            _ => {}
        }
        if self.assignable(&a, &b) {
            return Some(self.resolve(&b));
        }
        if self.assignable(&b, &a) {
            return Some(self.resolve(&a));
        }
        if let (Ty::Struct(sa, aa), Ty::Struct(..)) = (&a, &b) {
            for (anc, args) in self.defs.ancestry(*sa, aa) {
                let cand = Ty::Struct(anc, args);
                if self.assignable(&b, &cand) {
                    return Some(cand);
                }
            }
        }
        match (&a, &b) {
            (Ty::Nullable(x), y) | (y, Ty::Nullable(x)) => {
                let inner = self.join(x, y)?;
                Some(Ty::nullable(inner))
            }
            _ => None,
        }
    }

    // ------------------------------------------------------------ traits

    /// A trait followed by every trait it extends, directly or not, with their arguments.
    pub fn trait_closure(&self, tid: TraitId, targs: &[Ty]) -> Vec<(TraitId, Vec<Ty>)> {
        let mut out: Vec<(TraitId, Vec<Ty>)> = vec![(tid, targs.to_vec())];
        let mut next = 0;
        while next < out.len() && out.len() < 64 {
            let (t, a) = out[next].clone();
            next += 1;
            let def = &self.defs.traits[t];
            let map: HashMap<ParamId, Ty> = def.tparams.iter().cloned().zip(a).collect();
            for (st, sa) in &def.supers {
                if !out.iter().any(|(x, _)| x == st) {
                    out.push((*st, sa.iter().map(|x| x.subst(&map)).collect()));
                }
            }
        }
        out
    }

    pub fn implements(&mut self, ty: &Ty, tid: TraitId, targs: &[Ty]) -> bool {
        let ty = self.resolve(ty);
        match &ty {
            Ty::Error => return true,
            Ty::Trait(t, a) => {
                for (st, sa) in self.trait_closure(*t, a) {
                    if st == tid
                        && sa.len() == targs.len()
                        && sa.iter().zip(targs.iter()).all(|(x, y)| self.same(x, y))
                    {
                        return true;
                    }
                }
                return false;
            }
            Ty::Param(p) => {
                let bounds = self.defs.params[*p].bounds.clone();
                for (t, a) in bounds {
                    for (st, sa) in self.trait_closure(t, &a) {
                        if st == tid
                            && sa.len() == targs.len()
                            && sa.iter().zip(targs.iter()).all(|(x, y)| self.same(x, y))
                        {
                            return true;
                        }
                    }
                }
                return false;
            }
            _ => {}
        }
        if let Some(lang) = self.defs.lang_of(tid) {
            let args: Vec<Ty> = match lang {
                Lang::Eq | Lang::Ord => vec![ty.clone()],
                _ => targs.to_vec(),
            };
            if self.builtin_lang(&ty, lang, &args).is_some() {
                return true;
            }
        }
        let cands = self.candidate_types(&ty);
        for iid in 0..self.defs.impls.len() {
            let Some((t, a)) = self.defs.impls[iid].trait_.clone() else {
                continue;
            };
            if t != tid {
                continue;
            }
            for cand in &cands {
                if let Some(map) = self.impl_matches(iid, cand) {
                    let ia: Vec<Ty> = a.iter().map(|x| x.subst(&map)).collect();
                    if ia.len() == targs.len()
                        && ia.iter().zip(targs.iter()).all(|(x, y)| self.same(x, y))
                    {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// A type and its ancestors: where to look for impls and methods.
    pub fn candidate_types(&self, ty: &Ty) -> Vec<Ty> {
        match ty {
            Ty::Struct(s, a) => self
                .defs
                .ancestry(*s, a)
                .into_iter()
                .map(|(s, a)| Ty::Struct(s, a))
                .collect(),
            other => vec![other.clone()],
        }
    }

    /// If the impl's target matches `cand` exactly, the substitution of its generics.
    pub fn impl_matches(&mut self, iid: ImplId, cand: &Ty) -> Option<HashMap<ParamId, Ty>> {
        let imp = self.defs.impls[iid].clone();
        let mut map = HashMap::new();
        for (_, p) in &imp.tparams {
            let v = self.fresh();
            map.insert(*p, v);
        }
        let target = imp.target.subst(&map);
        if self.same(&target, cand) {
            Some(map)
        } else {
            None
        }
    }

    /// The impl method `name` for `ty`, looking in the type and its ancestors.
    pub fn find_user_method(&mut self, ty: &Ty, name: &str) -> Option<UserMethod> {
        let ty = self.resolve(ty);
        for cand in self.candidate_types(&ty) {
            for pass in 0..2 {
                for iid in 0..self.defs.impls.len() {
                    let imp = &self.defs.impls[iid];
                    let inherent = imp.trait_.is_none();
                    if (pass == 0) != inherent {
                        continue;
                    }
                    let Some((_, fid)) = imp.methods.iter().find(|(n, _)| n == name).cloned()
                    else {
                        continue;
                    };
                    if let Some(map) = self.impl_matches(iid, &cand) {
                        return Some(UserMethod {
                            fid,
                            impl_id: iid,
                            map,
                        });
                    }
                }
            }
        }
        None
    }

    fn numeric_pair(a: &Ty, b: &Ty) -> Option<(bool, bool)> {
        let n = |t: &Ty| match t {
            Ty::Int => Some(true),
            Ty::Float => Some(false),
            _ => None,
        };
        Some((n(a)?, n(b)?))
    }

    /// What the language itself provides for `ty` and a language trait.
    /// `args` are the trait's arguments (the right operand for operators).
    pub fn builtin_lang(&mut self, ty: &Ty, lang: Lang, args: &[Ty]) -> Option<Ty> {
        let ty = self.resolve(ty);
        let arg = args.first().map(|a| self.resolve(a));
        match lang {
            Lang::Add
            | Lang::Sub
            | Lang::Mul
            | Lang::Div
            | Lang::IntDiv
            | Lang::Mod
            | Lang::Pow => {
                let arg = arg?;
                if lang == Lang::Add && ty == Ty::Str && arg == Ty::Str {
                    return Some(Ty::Str);
                }
                // the time between two instants, in milliseconds
                if lang == Lang::Sub && ty == Ty::DateTime && arg == Ty::DateTime {
                    return Some(Ty::Int);
                }
                let (l_int, r_int) = Self::numeric_pair(&ty, &arg)?;
                let both_int = l_int && r_int;
                match lang {
                    Lang::Add | Lang::Sub | Lang::Mul => {
                        Some(if both_int { Ty::Int } else { Ty::Float })
                    }
                    Lang::Div => Some(Ty::Float),
                    Lang::IntDiv => both_int.then_some(Ty::Int),
                    Lang::Mod | Lang::Pow => Some(if both_int { Ty::Int } else { Ty::Float }),
                    _ => None,
                }
            }
            Lang::Neg => match ty {
                Ty::Int => Some(Ty::Int),
                Ty::Float => Some(Ty::Float),
                _ => None,
            },
            Lang::Eq => {
                let arg = arg?;
                self.eq_ok(&ty, &arg).then_some(Ty::Bool)
            }
            Lang::Ord => {
                let arg = arg?;
                if Self::numeric_pair(&ty, &arg).is_some()
                    || (matches!(ty, Ty::Str | Ty::Uuid | Ty::Ulid | Ty::DateTime) && ty == arg)
                {
                    return Some(Ty::Bool);
                }
                // tuples compare element by element, as long as their elements can be ordered
                match (&ty, &arg) {
                    (Ty::Tuple(ts), Ty::Tuple(us)) if ts.len() == us.len() => ts
                        .iter()
                        .zip(us)
                        .all(|(t, u)| {
                            self.find_lang_impl(t, Lang::Ord, std::slice::from_ref(u))
                                .is_some()
                        })
                        .then_some(Ty::Bool),
                    _ => None,
                }
            }
            Lang::Index => {
                let arg = arg?;
                match &ty {
                    Ty::Array(t) if arg == Ty::Int => Some((**t).clone()),
                    // a missing key gives `none`
                    Ty::Map(k, v) if self.assignable(&arg, k) => Some(Ty::nullable((**v).clone())),
                    Ty::Str if arg == Ty::Int => Some(Ty::Str),
                    _ => None,
                }
            }
            Lang::IndexSet => {
                let (i, v) = (arg?, self.resolve(args.get(1)?));
                match &ty {
                    Ty::Array(t) if i == Ty::Int && self.assignable(&v, t) => Some(Ty::Unit),
                    Ty::Map(k, mv) if self.assignable(&i, k) && self.assignable(&v, mv) => {
                        Some(Ty::Unit)
                    }
                    _ => None,
                }
            }
            Lang::Push => {
                let v = arg?;
                match &ty {
                    Ty::Array(t) if self.assignable(&v, t) => Some(Ty::Unit),
                    _ => None,
                }
            }
            Lang::As => None,
        }
    }

    /// Can `==` compare these two types?
    pub fn eq_ok(&mut self, a: &Ty, b: &Ty) -> bool {
        let a = self.resolve(a);
        let b = self.resolve(b);
        match (&a, &b) {
            (Ty::Error, _) | (_, Ty::Error) => true,
            // `x == none` is always allowed, even where flow analysis already knows x is present
            (Ty::NoneT, _) | (_, Ty::NoneT) => true,
            (Ty::Nullable(x), Ty::Nullable(y)) => self.eq_ok(x, y),
            (Ty::Nullable(x), y) | (y, Ty::Nullable(x)) => self.eq_ok(x, y),
            (Ty::Int | Ty::Float, Ty::Int | Ty::Float) => true,
            (Ty::Bool, Ty::Bool)
            | (Ty::Str, Ty::Str)
            | (Ty::Uuid, Ty::Uuid)
            | (Ty::Ulid, Ty::Ulid)
            | (Ty::DateTime, Ty::DateTime)
            | (Ty::Bytes, Ty::Bytes) => true,
            (Ty::Enum(x, xa), Ty::Enum(y, ya)) => {
                // variants with data are compared field by field, so those must be comparable
                x == y
                    && xa.len() == ya.len()
                    && xa.iter().zip(ya.iter()).all(|(p, q)| self.same(p, q))
            }
            (Ty::Array(x), Ty::Array(y)) => self.eq_ok(x, y),
            (Ty::Tuple(x), Ty::Tuple(y)) => {
                x.len() == y.len() && x.iter().zip(y.iter()).all(|(p, q)| self.eq_ok(p, q))
            }
            (Ty::Map(k1, v1), Ty::Map(k2, v2)) => self.same(k1, k2) && self.eq_ok(v1, v2),
            (Ty::Struct(..) | Ty::Param(_) | Ty::Trait(..), _) => self
                .find_lang_impl(&a, Lang::Eq, std::slice::from_ref(&b))
                .is_some(),
            _ => false,
        }
    }

    /// Find how `ty` supports a language trait, for the given trait arguments.
    pub fn find_lang_impl(&mut self, ty: &Ty, lang: Lang, args: &[Ty]) -> Option<LangImpl> {
        let ty = self.resolve(ty);
        if matches!(ty, Ty::Error) {
            return Some(LangImpl::Builtin(Ty::Error));
        }
        if !matches!(ty, Ty::Struct(..) | Ty::Param(_) | Ty::Trait(..))
            && let Some(out) = self.builtin_lang(&ty, lang, args)
        {
            return Some(LangImpl::Builtin(out));
        }
        let tid = self.defs.lang(lang);
        if let Ty::Param(p) = &ty {
            let bounds = self.defs.params[*p].bounds.clone();
            for (t, a) in bounds {
                if t != tid {
                    continue;
                }
                let ok = match lang {
                    Lang::Eq | Lang::Ord => true,
                    _ => a
                        .iter()
                        .zip(args.iter())
                        .all(|(x, y)| self.assignable(y, x)),
                };
                if ok {
                    let out = match lang {
                        Lang::Add
                        | Lang::Sub
                        | Lang::Mul
                        | Lang::Div
                        | Lang::IntDiv
                        | Lang::Mod
                        | Lang::Pow
                        | Lang::Neg => ty.clone(),
                        Lang::Eq | Lang::Ord => Ty::Bool,
                        Lang::IndexSet | Lang::Push => Ty::Unit,
                        _ => Ty::Error,
                    };
                    return Some(LangImpl::Bound(out));
                }
            }
            return None;
        }
        let cands = self.candidate_types(&ty);
        for iid in 0..self.defs.impls.len() {
            let Some((t, a)) = self.defs.impls[iid].trait_.clone() else {
                continue;
            };
            if t != tid {
                continue;
            }
            for cand in &cands {
                let Some(map) = self.impl_matches(iid, cand) else {
                    continue;
                };
                let ok = match lang {
                    Lang::Eq | Lang::Ord => args.first().is_some_and(|r| {
                        let r = r.clone();
                        self.assignable(&r, cand)
                    }),
                    Lang::As => a.first().is_some_and(|x| {
                        let x = x.subst(&map);
                        args.first().is_some_and(|y| {
                            let y = y.clone();
                            self.same(&x, &y)
                        })
                    }),
                    _ => {
                        let want: Vec<Ty> = a.iter().map(|x| x.subst(&map)).collect();
                        want.len() == args.len()
                            && want
                                .iter()
                                .zip(args.iter())
                                .all(|(x, y)| self.assignable(y, x))
                    }
                };
                if !ok {
                    continue;
                }
                let imp = &self.defs.impls[iid];
                let Some((_, fid)) = imp
                    .methods
                    .iter()
                    .find(|(n, _)| n == lang.method_name())
                    .cloned()
                else {
                    continue;
                };
                let ret = self.defs.fns[fid].ret.subst(&map);
                return Some(LangImpl::User { fid, ret });
            }
        }
        None
    }

    /// Can `from` be converted to `to` with `as`?
    pub fn cast_ok(&mut self, from: &Ty, to: &Ty) -> bool {
        let f = self.resolve(from);
        let t = self.resolve(to);
        if f.is_error() || t.is_error() {
            return true;
        }
        match (&f, &t) {
            (Ty::Int, Ty::Float) | (Ty::Float, Ty::Int) => return true,
            (_, Ty::Str) => return !matches!(f, Ty::Unit | Ty::Never),
            _ => {}
        }
        if self.assignable(&f, &t) {
            return true;
        }
        self.find_lang_impl(&f, Lang::As, std::slice::from_ref(&t))
            .is_some()
    }
}
