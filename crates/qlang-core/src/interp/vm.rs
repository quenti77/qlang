//! A typed register machine for the functions that only compute with numbers and booleans.
//!
//! The checker already knows every type, so for a function whose parameters, locals and
//! results are all `int`, `float` or `bool`, the tree walker's per-node work (matching the
//! shape of a value, looking up a name, building a scope for each call) is wasted. Such a
//! function is compiled, the first time it is called, to instructions on `i64` registers
//! (a `float` is its bits, a `bool` is 0 or 1) with the operation already chosen by type.
//! A function that does anything else (text, arrays, structures, closures, globals,
//! `print`...) is not compiled and runs as before; the two kinds call each other freely,
//! through [`Interp::call_fid`].

use super::*;

type Reg = u16;

#[derive(Clone, Copy, Debug)]
enum Op {
    ConstI(Reg, i64),
    Mov(Reg, Reg),
    AddI(Reg, Reg, Reg),
    SubI(Reg, Reg, Reg),
    MulI(Reg, Reg, Reg),
    IntDivI(Reg, Reg, Reg),
    ModI(Reg, Reg, Reg),
    /// `int / int` is a `float`
    DivII(Reg, Reg, Reg),
    AddImm(Reg, Reg, i64),
    NegI(Reg, Reg),
    AddF(Reg, Reg, Reg),
    SubF(Reg, Reg, Reg),
    MulF(Reg, Reg, Reg),
    DivF(Reg, Reg, Reg),
    ModF(Reg, Reg, Reg),
    NegF(Reg, Reg),
    Not(Reg, Reg),
    LtI(Reg, Reg, Reg),
    LeI(Reg, Reg, Reg),
    GtI(Reg, Reg, Reg),
    GeI(Reg, Reg, Reg),
    EqI(Reg, Reg, Reg),
    NeI(Reg, Reg, Reg),
    LtF(Reg, Reg, Reg),
    LeF(Reg, Reg, Reg),
    GtF(Reg, Reg, Reg),
    GeF(Reg, Reg, Reg),
    EqF(Reg, Reg, Reg),
    NeF(Reg, Reg, Reg),
    Jmp(u32),
    /// Jump when the register is false.
    JmpF(Reg, u32),
    /// Jump when the register is true.
    JmpT(Reg, u32),
    /// Call the compiled function `code` with its arguments in the registers from `base`;
    /// the result goes to `dst`.
    Call {
        code: u32,
        base: Reg,
        dst: Reg,
    },
    Ret(Reg),
    RetUnit,
}

/// A compiled function.
pub(super) struct Code {
    name: String,
    nregs: usize,
    ops: Vec<Op>,
    spans: Vec<Span>,
    param_tys: Vec<Ty>,
    ret: Ty,
}

#[derive(Clone)]
pub(super) enum VmState {
    Unknown,
    No,
    /// Index into `Interp::vm_codes`.
    Yes(u32),
}

fn scalar(t: &Ty) -> bool {
    matches!(t, Ty::Int | Ty::Float | Ty::Bool)
}

struct Gen<'a> {
    prog: &'a Program,
    ops: Vec<Op>,
    spans: Vec<Span>,
    scopes: Vec<Vec<(String, Reg, Ty)>>,
    top: usize,
    max: usize,
    /// For each enclosing loop: where `continue` goes (patched), and the `break` jumps.
    loops: Vec<LoopCtx>,
    ret: Ty,
    /// The functions called, by `FnId` (a call names its target by that number until all
    /// the functions of the group are compiled, then it is renumbered).
    callees: Vec<FnId>,
}

#[derive(Default)]
struct LoopCtx {
    breaks: Vec<usize>,
    continues: Vec<usize>,
}

impl Gen<'_> {
    fn emit(&mut self, op: Op, span: Span) -> usize {
        self.ops.push(op);
        self.spans.push(span);
        self.ops.len() - 1
    }

    fn alloc(&mut self) -> Option<Reg> {
        let r = self.top;
        self.top += 1;
        self.max = self.max.max(self.top);
        Reg::try_from(r).ok()
    }

    fn here(&self) -> u32 {
        self.ops.len() as u32
    }

    fn patch(&mut self, at: usize, target: u32) {
        match &mut self.ops[at] {
            Op::Jmp(t) | Op::JmpF(_, t) | Op::JmpT(_, t) => *t = target,
            _ => {}
        }
    }

    /// Whether the name at this node means a local variable (not a global or a function).
    fn is_local_use(&self, id: NodeId) -> bool {
        matches!(self.prog.res.paths.get(&id), None | Some(PathRes::Local))
    }

    fn lookup(&self, name: &str) -> Option<(Reg, Ty)> {
        self.scopes
            .iter()
            .rev()
            .flat_map(|s| s.iter().rev())
            .find(|(n, _, _)| n == name)
            .map(|(_, r, t)| (*r, t.clone()))
    }

    /// Where the value of `e` is, and its type: a local's own register, or a new temporary.
    fn value(&mut self, e: &Expr) -> Option<(Reg, Ty)> {
        if let ExprKind::Ident(name, _) = &e.kind
            && self.is_local_use(e.id)
            && let Some(found) = self.lookup(name)
        {
            return Some(found);
        }
        let dst = self.alloc()?;
        let ty = self.into(e, dst)?;
        Some((dst, ty))
    }

    /// Compiles `e` so that its value ends up in `dst`.
    fn into(&mut self, e: &Expr, dst: Reg) -> Option<Ty> {
        let span = e.span;
        match &e.kind {
            ExprKind::Int(v) => {
                self.emit(Op::ConstI(dst, *v), span);
                Some(Ty::Int)
            }
            ExprKind::Float(v) => {
                self.emit(Op::ConstI(dst, v.to_bits() as i64), span);
                Some(Ty::Float)
            }
            ExprKind::Bool(b) => {
                self.emit(Op::ConstI(dst, i64::from(*b)), span);
                Some(Ty::Bool)
            }
            ExprKind::Ident(name, targs) => {
                if !targs.is_empty() || !self.is_local_use(e.id) {
                    return None;
                }
                let (r, ty) = self.lookup(name)?;
                if r != dst {
                    self.emit(Op::Mov(dst, r), span);
                }
                Some(ty)
            }
            ExprKind::Unary(op, inner) => {
                let mark = self.top;
                let (r, ty) = self.value(inner)?;
                let out = match (op, &ty) {
                    (UnOp::Not, Ty::Bool) => {
                        self.emit(Op::Not(dst, r), span);
                        Ty::Bool
                    }
                    (UnOp::Neg, Ty::Int) => {
                        self.emit(Op::NegI(dst, r), span);
                        Ty::Int
                    }
                    (UnOp::Neg, Ty::Float) => {
                        self.emit(Op::NegF(dst, r), span);
                        Ty::Float
                    }
                    _ => return None,
                };
                self.top = mark;
                Some(out)
            }
            ExprKind::Binary(BinOp::And | BinOp::Or, l, r) => {
                let is_and = matches!(&e.kind, ExprKind::Binary(BinOp::And, ..));
                if self.into(l, dst)? != Ty::Bool {
                    return None;
                }
                let j = if is_and {
                    self.emit(Op::JmpF(dst, 0), span)
                } else {
                    self.emit(Op::JmpT(dst, 0), span)
                };
                let mark = self.top;
                if self.into(r, dst)? != Ty::Bool {
                    return None;
                }
                self.top = mark;
                let end = self.here();
                self.patch(j, end);
                Some(Ty::Bool)
            }
            ExprKind::Binary(op, l, r) => {
                let mark = self.top;
                let (a, ta) = self.value(l)?;
                let (b, tb) = self.value(r)?;
                if ta != tb {
                    return None;
                }
                let int = ta == Ty::Int;
                let float = ta == Ty::Float;
                let (opc, out): (Op, Ty) = match op {
                    BinOp::Add if int => (Op::AddI(dst, a, b), Ty::Int),
                    BinOp::Sub if int => (Op::SubI(dst, a, b), Ty::Int),
                    BinOp::Mul if int => (Op::MulI(dst, a, b), Ty::Int),
                    BinOp::IntDiv if int => (Op::IntDivI(dst, a, b), Ty::Int),
                    BinOp::Mod if int => (Op::ModI(dst, a, b), Ty::Int),
                    BinOp::Div if int => (Op::DivII(dst, a, b), Ty::Float),
                    BinOp::Add if float => (Op::AddF(dst, a, b), Ty::Float),
                    BinOp::Sub if float => (Op::SubF(dst, a, b), Ty::Float),
                    BinOp::Mul if float => (Op::MulF(dst, a, b), Ty::Float),
                    BinOp::Div if float => (Op::DivF(dst, a, b), Ty::Float),
                    BinOp::Mod if float => (Op::ModF(dst, a, b), Ty::Float),
                    BinOp::Lt if int => (Op::LtI(dst, a, b), Ty::Bool),
                    BinOp::LtEq if int => (Op::LeI(dst, a, b), Ty::Bool),
                    BinOp::Gt if int => (Op::GtI(dst, a, b), Ty::Bool),
                    BinOp::GtEq if int => (Op::GeI(dst, a, b), Ty::Bool),
                    BinOp::Eq if int || ta == Ty::Bool => (Op::EqI(dst, a, b), Ty::Bool),
                    BinOp::NotEq if int || ta == Ty::Bool => (Op::NeI(dst, a, b), Ty::Bool),
                    BinOp::Lt if float => (Op::LtF(dst, a, b), Ty::Bool),
                    BinOp::LtEq if float => (Op::LeF(dst, a, b), Ty::Bool),
                    BinOp::Gt if float => (Op::GtF(dst, a, b), Ty::Bool),
                    BinOp::GtEq if float => (Op::GeF(dst, a, b), Ty::Bool),
                    BinOp::Eq if float => (Op::EqF(dst, a, b), Ty::Bool),
                    BinOp::NotEq if float => (Op::NeF(dst, a, b), Ty::Bool),
                    _ => return None,
                };
                self.emit(opc, span);
                self.top = mark;
                Some(out)
            }
            ExprKind::If {
                branches,
                else_block,
            } => {
                let else_block = else_block.as_ref()?;
                let mut ends = Vec::new();
                let mut ty: Option<Ty> = None;
                for (cond, block) in branches {
                    let mark = self.top;
                    let (c, tc) = self.value(cond)?;
                    if tc != Ty::Bool {
                        return None;
                    }
                    let skip = self.emit(Op::JmpF(c, 0), cond.span);
                    self.top = mark;
                    let t = self.block(block, Some(dst))?;
                    if *ty.get_or_insert(t.clone()) != t {
                        return None;
                    }
                    ends.push(self.emit(Op::Jmp(0), span));
                    let next = self.here();
                    self.patch(skip, next);
                }
                let t = self.block(else_block, Some(dst))?;
                if *ty.get_or_insert(t.clone()) != t {
                    return None;
                }
                let end = self.here();
                for j in ends {
                    self.patch(j, end);
                }
                ty
            }
            ExprKind::Call { callee, args } => {
                if !matches!(callee.kind, ExprKind::Ident(..))
                    || self.prog.res.arg_order.contains_key(&e.id)
                    || args.iter().any(|a| a.name.is_some())
                {
                    return None;
                }
                let Some(PathRes::Fn(fid)) = self.prog.res.paths.get(&callee.id) else {
                    return None;
                };
                let fid = *fid;
                let def = &self.prog.defs.fns[fid];
                let decl = def.decl.as_ref()?;
                if decl.external
                    || decl.body.is_none()
                    || !decl.tparams.is_empty()
                    || def.has_self
                    || def.impl_id.is_some()
                    || def.params.len() != args.len()
                    || !def.params.iter().all(scalar)
                    || !(scalar(&def.ret) || def.ret == Ty::Unit)
                {
                    return None;
                }
                let mark = self.top;
                let base = self.top as Reg;
                for (a, want) in args.iter().zip(&def.params) {
                    let r = self.alloc()?;
                    if self.into(&a.value, r)? != *want {
                        return None;
                    }
                }
                let code = self.callee_index(fid)?;
                self.emit(Op::Call { code, base, dst }, span);
                self.top = mark;
                Some(def.ret.clone())
            }
            _ => None,
        }
    }

    fn callee_index(&mut self, fid: FnId) -> Option<u32> {
        if !self.callees.contains(&fid) {
            self.callees.push(fid);
        }
        u32::try_from(fid).ok()
    }

    /// A block in its own scope. With `dst`, the value of its last expression goes there.
    fn block(&mut self, b: &Block, dst: Option<Reg>) -> Option<Ty> {
        self.scopes.push(Vec::new());
        let mark = self.top;
        let mut ty = Ty::Unit;
        let n = b.stmts.len();
        for (i, s) in b.stmts.iter().enumerate() {
            let last = i + 1 == n;
            match (&s.kind, dst, last) {
                (StmtKind::Expr(e), Some(d), true) => {
                    ty = self.into(e, d)?;
                }
                _ => {
                    self.stmt(s)?;
                    if last && dst.is_some() {
                        // a block that must give a value ended with something that gives none
                        return None;
                    }
                }
            }
        }
        if n == 0 && dst.is_some() {
            return None;
        }
        self.top = mark;
        self.scopes.pop();
        Some(ty)
    }

    fn stmt(&mut self, s: &Stmt) -> Option<()> {
        let span = s.span;
        match &s.kind {
            StmtKind::Let { name, init, .. } => {
                let dst = self.alloc()?;
                let ty = self.into(init, dst)?;
                if !scalar(&ty) {
                    return None;
                }
                // the temporaries of the initializer are free again, the variable stays
                self.top = dst as usize + 1;
                self.scopes.last_mut()?.push((name.name.clone(), dst, ty));
                Some(())
            }
            StmtKind::Expr(e) => self.effect(e),
            StmtKind::Return(e) => {
                let mark = self.top;
                match e {
                    Some(e) => {
                        let (r, ty) = self.value(e)?;
                        if ty != self.ret {
                            return None;
                        }
                        self.emit(Op::Ret(r), span);
                    }
                    None => {
                        if self.ret != Ty::Unit {
                            return None;
                        }
                        self.emit(Op::RetUnit, span);
                    }
                }
                self.top = mark;
                Some(())
            }
            StmtKind::While { cond, body } => {
                let start = self.here();
                let mark = self.top;
                let (c, tc) = self.value(cond)?;
                if tc != Ty::Bool {
                    return None;
                }
                let exit = self.emit(Op::JmpF(c, 0), cond.span);
                self.top = mark;
                self.loops.push(LoopCtx::default());
                self.block(body, None)?;
                let ctx = self.loops.pop()?;
                for j in ctx.continues {
                    self.patch(j, start);
                }
                self.emit(Op::Jmp(start), span);
                let end = self.here();
                self.patch(exit, end);
                for j in ctx.breaks {
                    self.patch(j, end);
                }
                Some(())
            }
            StmtKind::For {
                var,
                var2: None,
                var_pattern: None,
                var2_pattern: None,
                iter,
                step: None,
                body,
            } => {
                let ExprKind::Range {
                    start,
                    end,
                    inclusive,
                } = &iter.kind
                else {
                    return None;
                };
                let mark = self.top;
                let cnt = self.alloc()?;
                if self.into(start, cnt)? != Ty::Int {
                    return None;
                }
                let lim = self.alloc()?;
                if self.into(end, lim)? != Ty::Int {
                    return None;
                }
                let test = self.alloc()?;
                let head = self.here();
                self.emit(
                    if *inclusive {
                        Op::LeI(test, cnt, lim)
                    } else {
                        Op::LtI(test, cnt, lim)
                    },
                    span,
                );
                let exit = self.emit(Op::JmpF(test, 0), span);
                self.scopes.push(vec![]);
                let v = self.alloc()?;
                self.scopes.last_mut()?.push((var.name.clone(), v, Ty::Int));
                self.emit(Op::Mov(v, cnt), span);
                self.loops.push(LoopCtx::default());
                self.block(body, None)?;
                let ctx = self.loops.pop()?;
                let next = self.here();
                for j in ctx.continues {
                    self.patch(j, next);
                }
                self.emit(Op::AddImm(cnt, cnt, 1), span);
                self.emit(Op::Jmp(head), span);
                let end = self.here();
                self.patch(exit, end);
                for j in ctx.breaks {
                    self.patch(j, end);
                }
                self.scopes.pop();
                self.top = mark;
                Some(())
            }
            StmtKind::Break => {
                let j = self.emit(Op::Jmp(0), span);
                self.loops.last_mut()?.breaks.push(j);
                Some(())
            }
            StmtKind::Continue => {
                let j = self.emit(Op::Jmp(0), span);
                self.loops.last_mut()?.continues.push(j);
                Some(())
            }
            _ => None,
        }
    }

    /// An expression used for what it does, not for its value: an assignment to a local, a
    /// call, or an `if` / `while`-like construct.
    fn effect(&mut self, e: &Expr) -> Option<()> {
        let mark = self.top;
        match &e.kind {
            ExprKind::Assign { op, target, value } => {
                let ExprKind::Ident(name, _) = &target.kind else {
                    return None;
                };
                if !self.is_local_use(target.id) {
                    return None;
                }
                let (reg, ty) = self.lookup(name)?;
                match op {
                    None => {
                        // evaluated into a temporary first: `x = x + 1` reads the old `x`
                        let (r, t) = self.value(value)?;
                        if t != ty {
                            return None;
                        }
                        if r != reg {
                            self.emit(Op::Mov(reg, r), e.span);
                        }
                    }
                    Some(bop) => {
                        let (r, t) = self.value(value)?;
                        if t != ty {
                            return None;
                        }
                        let int = ty == Ty::Int;
                        let float = ty == Ty::Float;
                        let opc = match bop {
                            BinOp::Add if int => Op::AddI(reg, reg, r),
                            BinOp::Sub if int => Op::SubI(reg, reg, r),
                            BinOp::Mul if int => Op::MulI(reg, reg, r),
                            BinOp::IntDiv if int => Op::IntDivI(reg, reg, r),
                            BinOp::Mod if int => Op::ModI(reg, reg, r),
                            BinOp::Add if float => Op::AddF(reg, reg, r),
                            BinOp::Sub if float => Op::SubF(reg, reg, r),
                            BinOp::Mul if float => Op::MulF(reg, reg, r),
                            BinOp::Div if float => Op::DivF(reg, reg, r),
                            _ => return None,
                        };
                        self.emit(opc, e.span);
                    }
                }
            }
            ExprKind::If {
                branches,
                else_block,
            } => {
                let mut ends = Vec::new();
                for (cond, block) in branches {
                    let m = self.top;
                    let (c, tc) = self.value(cond)?;
                    if tc != Ty::Bool {
                        return None;
                    }
                    let skip = self.emit(Op::JmpF(c, 0), cond.span);
                    self.top = m;
                    self.block(block, None)?;
                    ends.push(self.emit(Op::Jmp(0), e.span));
                    let next = self.here();
                    self.patch(skip, next);
                }
                if let Some(b) = else_block {
                    self.block(b, None)?;
                }
                let end = self.here();
                for j in ends {
                    self.patch(j, end);
                }
            }
            ExprKind::Call { .. } => {
                let dst = self.alloc()?;
                self.into(e, dst)?;
            }
            _ => return None,
        }
        self.top = mark;
        Some(())
    }
}

/// A frame of the machine's own call stack.
struct Frame {
    code: u32,
    pc: usize,
    base: usize,
    /// Register of the caller that receives the result.
    dst: Reg,
}

impl Interp<'_> {
    /// Calls `fid` on the machine when it is compiled (compiling it now if it is the first
    /// time). `None` when it cannot run there: the tree walker does it.
    pub(super) fn try_vm(
        &mut self,
        fid: FnId,
        this: &Option<Value>,
        args: &[Value],
        span: Span,
    ) -> Option<R<Value>> {
        if this.is_some() || !self.vm_enabled {
            return None;
        }
        if self.vm_state.len() <= fid {
            let n = self.prog.defs.fns.len().max(fid + 1);
            self.vm_state.resize(n, VmState::Unknown);
        }
        let idx = match &self.vm_state[fid] {
            VmState::No => return None,
            VmState::Yes(i) => *i,
            VmState::Unknown => self.vm_compile(fid)?,
        };
        let code = self.vm_codes[idx as usize].clone();
        if args.len() != code.param_tys.len() {
            return None;
        }
        let mut regs: Vec<i64> = Vec::with_capacity(code.nregs);
        for (a, t) in args.iter().zip(&code.param_tys) {
            regs.push(match (a, t) {
                (Value::Int(i), Ty::Int) => *i,
                (Value::Float(f), Ty::Float) => f.to_bits() as i64,
                (Value::Bool(b), Ty::Bool) => i64::from(*b),
                _ => return None,
            });
        }
        Some(self.vm_run(idx, regs, span).map(|r| match code.ret {
            Ty::Int => Value::Int(r),
            Ty::Float => Value::Float(f64::from_bits(r as u64)),
            Ty::Bool => Value::Bool(r != 0),
            _ => Value::Unit,
        }))
    }

    /// Compiles `fid` and every function it calls (a call to a function that is not plain
    /// numeric code makes the whole group fall back to the tree walker, and `fid` is marked so).
    fn vm_compile(&mut self, fid: FnId) -> Option<u32> {
        let prog = self.prog;
        let mut batch: Vec<(FnId, Code, Vec<FnId>)> = Vec::new();
        let mut pending = vec![fid];
        let mut seen: Vec<FnId> = vec![fid];
        while let Some(f) = pending.pop() {
            let def = &prog.defs.fns[f];
            let compiled = (|| {
                let decl = def.decl.as_ref()?;
                let body = decl.body.as_ref()?;
                if decl.external
                    || !decl.tparams.is_empty()
                    || def.has_self
                    || def.impl_id.is_some()
                    || !def.params.iter().all(scalar)
                    || !(scalar(&def.ret) || def.ret == Ty::Unit)
                {
                    return None;
                }
                let mut g = Gen {
                    prog,
                    ops: Vec::new(),
                    spans: Vec::new(),
                    scopes: vec![Vec::new()],
                    top: 0,
                    max: 0,
                    loops: Vec::new(),
                    ret: def.ret.clone(),
                    callees: Vec::new(),
                };
                for (p, t) in decl.params.iter().zip(&def.params) {
                    let r = g.alloc()?;
                    g.scopes[0].push((p.name.name.clone(), r, t.clone()));
                }
                if def.ret == Ty::Unit {
                    g.block(body, None)?;
                    g.emit(Op::RetUnit, decl.span);
                } else {
                    let res = g.alloc()?;
                    if g.block(body, Some(res))? != def.ret {
                        return None;
                    }
                    g.emit(Op::Ret(res), decl.span);
                }
                Some((
                    Code {
                        name: def.name.clone(),
                        nregs: g.max + 1,
                        ops: g.ops,
                        spans: g.spans,
                        param_tys: def.params.clone(),
                        ret: def.ret.clone(),
                    },
                    g.callees,
                ))
            })();
            let Some((code, callees)) = compiled else {
                self.vm_state[fid] = VmState::No;
                return None;
            };
            for c in &callees {
                if !seen.contains(c) && !matches!(self.vm_state.get(*c), Some(VmState::Yes(_))) {
                    seen.push(*c);
                    pending.push(*c);
                }
            }
            batch.push((f, code, callees));
        }
        // number the functions of the group, then point every call at its target
        let first = self.vm_codes.len() as u32;
        let mut index: HashMap<FnId, u32> = HashMap::new();
        for (k, (f, _, _)) in batch.iter().enumerate() {
            index.insert(*f, first + k as u32);
        }
        for (_, code, callees) in &mut batch {
            for op in &mut code.ops {
                if let Op::Call { code: target, .. } = op {
                    let f = *target as usize;
                    *target = match index.get(&f) {
                        Some(i) => *i,
                        None => match self.vm_state.get(f) {
                            Some(VmState::Yes(i)) => *i,
                            _ => return None,
                        },
                    };
                }
            }
            let _ = callees;
        }
        for (f, code, _) in batch {
            if self.vm_state.len() <= f {
                self.vm_state.resize(f + 1, VmState::Unknown);
            }
            self.vm_state[f] = VmState::Yes(self.vm_codes.len() as u32);
            self.vm_codes.push(Rc::new(code));
        }
        match self.vm_state[fid] {
            VmState::Yes(i) => Some(i),
            _ => None,
        }
    }

    fn vm_run(&mut self, root: u32, args: Vec<i64>, span: Span) -> R<i64> {
        let mut stack = std::mem::take(&mut self.vm_stack);
        stack.clear();
        let code = self.vm_codes[root as usize].clone();
        stack.resize(code.nregs.max(args.len()), 0);
        stack[..args.len()].copy_from_slice(&args);
        let mut frames: Vec<Frame> = Vec::new();
        if self.depth >= self.limits.max_depth {
            self.vm_stack = stack;
            return self.fail("R901", "too many nested calls (infinite recursion?)", span);
        }
        self.depth += 1;
        self.frames.push((Name::new(&code.name), span));
        let r = self.vm_loop(&mut stack, &mut frames, root);
        // leave the interpreter's own call bookkeeping as it was
        for _ in 0..=frames.len() {
            self.frames.pop();
            self.depth -= 1;
        }
        self.vm_stack = stack;
        r
    }

    fn vm_loop(&mut self, stack: &mut Vec<i64>, frames: &mut Vec<Frame>, root: u32) -> R<i64> {
        let mut cur = root;
        let mut code = self.vm_codes[cur as usize].clone();
        let mut pc = 0usize;
        let mut base = 0usize;
        macro_rules! r {
            ($x:expr) => {
                stack[base + $x as usize]
            };
        }
        macro_rules! fail {
            ($c:expr, $m:expr) => {
                return self.fail($c, $m, code.spans[pc - 1])
            };
        }
        macro_rules! fl {
            ($x:expr) => {
                f64::from_bits(r!($x) as u64)
            };
        }
        loop {
            let op = code.ops[pc];
            pc += 1;
            self.steps += 1;
            match op {
                Op::ConstI(d, v) => r!(d) = v,
                Op::Mov(d, a) => r!(d) = r!(a),
                Op::AddI(d, a, b) => match r!(a).checked_add(r!(b)) {
                    Some(v) => r!(d) = v,
                    None => fail!("R010", "integer overflow"),
                },
                Op::SubI(d, a, b) => match r!(a).checked_sub(r!(b)) {
                    Some(v) => r!(d) = v,
                    None => fail!("R010", "integer overflow"),
                },
                Op::MulI(d, a, b) => match r!(a).checked_mul(r!(b)) {
                    Some(v) => r!(d) = v,
                    None => fail!("R010", "integer overflow"),
                },
                Op::IntDivI(d, a, b) => {
                    if r!(b) == 0 {
                        fail!("R011", "division by zero");
                    }
                    match r!(a).checked_div(r!(b)) {
                        Some(v) => r!(d) = v,
                        None => fail!("R010", "integer overflow"),
                    }
                }
                Op::ModI(d, a, b) => {
                    if r!(b) == 0 {
                        fail!("R011", "division by zero");
                    }
                    match r!(a).checked_rem(r!(b)) {
                        Some(v) => r!(d) = v,
                        None => fail!("R010", "integer overflow"),
                    }
                }
                Op::DivII(d, a, b) => {
                    if r!(b) == 0 {
                        fail!("R011", "division by zero");
                    }
                    r!(d) = ((r!(a) as f64) / (r!(b) as f64)).to_bits() as i64;
                }
                Op::AddImm(d, a, k) => match r!(a).checked_add(k) {
                    Some(v) => r!(d) = v,
                    None => fail!("R010", "integer overflow"),
                },
                Op::NegI(d, a) => match r!(a).checked_neg() {
                    Some(v) => r!(d) = v,
                    None => fail!("R010", "integer overflow"),
                },
                Op::AddF(d, a, b) => r!(d) = (fl!(a) + fl!(b)).to_bits() as i64,
                Op::SubF(d, a, b) => r!(d) = (fl!(a) - fl!(b)).to_bits() as i64,
                Op::MulF(d, a, b) => r!(d) = (fl!(a) * fl!(b)).to_bits() as i64,
                Op::DivF(d, a, b) => {
                    if fl!(b) == 0.0 {
                        fail!("R011", "division by zero");
                    }
                    r!(d) = (fl!(a) / fl!(b)).to_bits() as i64;
                }
                Op::ModF(d, a, b) => {
                    if fl!(b) == 0.0 {
                        fail!("R011", "division by zero");
                    }
                    r!(d) = (fl!(a) % fl!(b)).to_bits() as i64;
                }
                Op::NegF(d, a) => r!(d) = (-fl!(a)).to_bits() as i64,
                Op::Not(d, a) => r!(d) = i64::from(r!(a) == 0),
                Op::LtI(d, a, b) => r!(d) = i64::from(r!(a) < r!(b)),
                Op::LeI(d, a, b) => r!(d) = i64::from(r!(a) <= r!(b)),
                Op::GtI(d, a, b) => r!(d) = i64::from(r!(a) > r!(b)),
                Op::GeI(d, a, b) => r!(d) = i64::from(r!(a) >= r!(b)),
                Op::EqI(d, a, b) => r!(d) = i64::from(r!(a) == r!(b)),
                Op::NeI(d, a, b) => r!(d) = i64::from(r!(a) != r!(b)),
                Op::LtF(d, a, b) => r!(d) = i64::from(fl!(a) < fl!(b)),
                Op::LeF(d, a, b) => r!(d) = i64::from(fl!(a) <= fl!(b)),
                Op::GtF(d, a, b) => r!(d) = i64::from(fl!(a) > fl!(b)),
                Op::GeF(d, a, b) => r!(d) = i64::from(fl!(a) >= fl!(b)),
                Op::EqF(d, a, b) => r!(d) = i64::from(fl!(a) == fl!(b)),
                Op::NeF(d, a, b) => r!(d) = i64::from(fl!(a) != fl!(b)),
                Op::Jmp(t) => {
                    if (t as usize) < pc && self.steps > self.limits.max_steps {
                        fail!(
                            "R900",
                            "the program ran for too long (step limit reached: infinite loop?)"
                        );
                    }
                    pc = t as usize;
                }
                Op::JmpF(c, t) => {
                    if r!(c) == 0 {
                        pc = t as usize;
                    }
                }
                Op::JmpT(c, t) => {
                    if r!(c) != 0 {
                        pc = t as usize;
                    }
                }
                Op::Call {
                    code: target,
                    base: b,
                    dst,
                } => {
                    if self.steps > self.limits.max_steps {
                        fail!(
                            "R900",
                            "the program ran for too long (step limit reached: infinite loop?)"
                        );
                    }
                    if self.depth >= self.limits.max_depth {
                        fail!("R901", "too many nested calls (infinite recursion?)");
                    }
                    let span = code.spans[pc - 1];
                    let callee = self.vm_codes[target as usize].clone();
                    frames.push(Frame {
                        code: cur,
                        pc,
                        base,
                        dst,
                    });
                    base += b as usize;
                    if stack.len() < base + callee.nregs {
                        stack.resize(base + callee.nregs + 64, 0);
                    }
                    self.depth += 1;
                    self.frames.push((Name::new(&callee.name), span));
                    cur = target;
                    code = callee;
                    pc = 0;
                }
                Op::Ret(_) | Op::RetUnit => {
                    let v = match op {
                        Op::Ret(a) => r!(a),
                        _ => 0,
                    };
                    match frames.pop() {
                        None => return Ok(v),
                        Some(f) => {
                            self.frames.pop();
                            self.depth -= 1;
                            cur = f.code;
                            code = self.vm_codes[cur as usize].clone();
                            pc = f.pc;
                            base = f.base;
                            r!(f.dst) = v;
                        }
                    }
                }
            }
        }
    }
}
