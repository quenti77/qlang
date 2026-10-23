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
use std::cell::Cell;

type Reg = u16;

/// A comparison.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Cmp {
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
}

/// The kind of the elements of an array of numbers.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Ety {
    I,
    F,
    B,
    /// An object (a text, a structure, an array): the element is an object register.
    O,
}

fn ety_of(t: &Ty) -> Option<Ety> {
    match t {
        Ty::Int => Some(Ety::I),
        Ty::Float => Some(Ety::F),
        Ty::Bool => Some(Ety::B),
        _ if vty(t) => Some(Ety::O),
        _ => None,
    }
}

/// The element of an array as a register value, if it has the kind expected.
fn elem_bits(v: &Value, ety: Ety) -> Option<i64> {
    match (v, ety) {
        (Value::Int(i), Ety::I) => Some(*i),
        (Value::Float(f), Ety::F) => Some(f.to_bits() as i64),
        (Value::Bool(b), Ety::B) => Some(i64::from(*b)),
        _ => None,
    }
}

fn elem_value(bits: i64, ety: Ety) -> Value {
    match ety {
        Ety::I => Value::Int(bits),
        Ety::F => Value::Float(f64::from_bits(bits as u64)),
        Ety::B => Value::Bool(bits != 0),
        Ety::O => Value::Unit,
    }
}

#[derive(Clone, Copy, Debug)]
enum Op {
    ConstI(Reg, i64),
    Mov(Reg, Reg),
    /// The same for an object register (an array: the reference is copied).
    MovO(Reg, Reg),
    AddI(Reg, Reg, Reg),
    SubI(Reg, Reg, Reg),
    MulI(Reg, Reg, Reg),
    IntDivI(Reg, Reg, Reg),
    ModI(Reg, Reg, Reg),
    /// `int / int` is a `float`
    DivII(Reg, Reg, Reg),
    NegI(Reg, Reg),
    PowI(Reg, Reg, Reg),
    ItoF(Reg, Reg),
    FtoI(Reg, Reg),
    AbsI(Reg, Reg),
    AbsF(Reg, Reg),
    SqrtI(Reg, Reg),
    SqrtF(Reg, Reg),
    MinI(Reg, Reg, Reg),
    MaxI(Reg, Reg, Reg),
    MinF(Reg, Reg, Reg),
    MaxF(Reg, Reg, Reg),
    PowF(Reg, Reg, Reg),
    /// A built-in method of a number, run by the tree walker's own implementation (so that
    /// its results and errors are the same): receiver and arguments from `base`.
    Method {
        dst: Reg,
        base: Reg,
        site: u32,
    },
    /// `cnt += step`, or jump to the end of the loop when that overflows.
    ForNext {
        cnt: Reg,
        step: Reg,
        exit: u32,
    },
    /// The `step` of a `for` must not be 0.
    StepCheck(Reg),
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
    /// Jump when the comparison is false (a test and its jump in one instruction).
    JnLtI(Reg, Reg, u32),
    JnLeI(Reg, Reg, u32),
    JnGtI(Reg, Reg, u32),
    JnGeI(Reg, Reg, u32),
    JnEqI(Reg, Reg, u32),
    JnNeI(Reg, Reg, u32),
    JnF {
        a: Reg,
        b: Reg,
        t: u32,
        cmp: Cmp,
    },
    /// `for`: leave the loop when `cnt` is out of the range.
    ForCheck {
        cnt: Reg,
        lim: Reg,
        step: Reg,
        incl: bool,
        exit: u32,
    },
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
    /// Read / write a variable of the module (a global), by its entry in `Code::globals`.
    LoadG {
        dst: Reg,
        g: u32,
    },
    StoreG {
        src: Reg,
        g: u32,
    },
    /// Arrays live in the object registers (`vm_ostack`).
    ALen(Reg, Reg),
    ALoad {
        dst: Reg,
        arr: Reg,
        idx: Reg,
        ety: Ety,
    },
    /// The index of an element assigned must exist (checked before the right side is computed).
    ACheck {
        arr: Reg,
        idx: Reg,
    },
    AStore {
        arr: Reg,
        idx: Reg,
        src: Reg,
        ety: Ety,
    },
    APush {
        arr: Reg,
        src: Reg,
        ety: Ety,
    },
    /// The same four for arrays of objects.
    ALoadO {
        dst: Reg,
        arr: Reg,
        idx: Reg,
    },
    AStoreO {
        arr: Reg,
        idx: Reg,
        src: Reg,
    },
    APushO {
        arr: Reg,
        src: Reg,
    },
    ForArrO {
        dst: Reg,
        arr: Reg,
        idx: Reg,
        exit: u32,
    },
    /// A new empty array.
    NewArr(Reg),
    /// A new empty map.
    NewMap(Reg),
    /// The keys and the values of a map, as two arrays (`for k, v in map`).
    MapSnap {
        keys: Reg,
        vals: Reg,
        map: Reg,
    },
    /// `none`
    ConstNone(Reg),
    /// A number into the object register of a `T?` / a number out of it (when it is not `none`).
    Box {
        dst: Reg,
        src: Reg,
        ety: Ety,
    },
    Unbox {
        dst: Reg,
        src: Reg,
        ety: Ety,
    },
    /// Whether the `T?` in the second register is `none`.
    IsNone(Reg, Reg),
    /// `base[idx]` for a map or a text, as the interpreter does it (`Code::sites[site]` holds
    /// the types), and the same for a key that must exist.
    IndexG {
        dst: Reg,
        base: Reg,
        idx: Reg,
        site: u32,
    },
    MapReq {
        dst: Reg,
        map: Reg,
        key: Reg,
        site: u32,
    },
    /// `map[key] = val`
    MapSet {
        map: Reg,
        key: Reg,
        val: Reg,
        site: u32,
    },
    /// Strings: a constant of `Code::strs`, concatenation, comparison.
    ConstS(Reg, u32),
    AddS(Reg, Reg, Reg),
    CmpS {
        dst: Reg,
        a: Reg,
        b: Reg,
        cmp: Cmp,
    },
    /// An interpolated string: the values of its `{}` parts are in the registers from `base`
    /// (`Code::fmts` says how to put them together).
    Format {
        dst: Reg,
        base: Reg,
        fmt: u32,
    },
    /// `obj.field`: the field `Code::fields[fld]` of the structure in `obj`.
    Field {
        dst: Reg,
        obj: Reg,
        fld: u32,
    },
    /// `obj.field = src`
    SetField {
        obj: Reg,
        src: Reg,
        fld: u32,
    },
    /// A structure literal: the values of the fields written are in the registers from `base`.
    NewStruct {
        dst: Reg,
        base: Reg,
        lit: u32,
    },
    /// A call of a function the machine cannot run, through the interpreter: arguments from
    /// `base`.
    CallFn {
        dst: Reg,
        base: Reg,
        site: u32,
    },
    /// A call of a method of a structure, dispatched by the interpreter as usual: receiver
    /// and arguments from `base`.
    CallMethod {
        dst: Reg,
        base: Reg,
        site: u32,
    },
    /// A built-in function (`print`): its argument is in `base`.
    Builtin {
        base: Reg,
        site: u32,
    },
    /// `for x in array`: the element at `idx`, or leave the loop.
    ForArr {
        dst: Reg,
        arr: Reg,
        idx: Reg,
        exit: u32,
        ety: Ety,
    },
    IncI(Reg),
    Ret(Reg),
    /// Return the object in the register.
    RetO(Reg),
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
    sites: Vec<Site>,
    globals: Vec<GlobalRef>,
    /// The text constants and the shapes of the interpolated strings.
    strs: Vec<Value>,
    fmts: Vec<Vec<FmtPart>>,
    fields: Vec<FieldSite>,
    lits: Vec<LitSite>,
    /// For a compiled loop: the variables it shares with the code around it.
    frees: Vec<Free>,
    /// Whether this is a compiled loop (it shows no frame of its own in a trace).
    is_loop: bool,
    /// Whether it uses object registers (their frame must be emptied when it returns).
    uses_objs: bool,
}

/// A field read or written by name, and its type.
struct FieldSite {
    name: String,
    ty: Ty,
}

/// A structure literal: the structure and the names of the fields written, in order.
struct LitSite {
    sid: StructId,
    names: Vec<(String, Ty)>,
}

/// A piece of an interpolated string: literal text, or the next value (of this type).
enum FmtPart {
    Lit(String),
    Arg(Ty),
}

/// A variable of a module, read or written by a compiled function.
struct GlobalRef {
    module: ModId,
    name: String,
    ty: Ty,
    /// Where the variable was found last time (see `Scope::get_cached`).
    slot: Cell<u32>,
}

/// A variable a compiled loop shares with the code around it: it is loaded into a register
/// when the loop starts and written back when it ends (when the loop assigned it).
#[derive(Clone)]
struct Free {
    name: String,
    reg: Reg,
    ty: Ty,
    /// The module whose variable it is; `None` for a local variable of the enclosing code.
    global: Option<ModId>,
    assigned: bool,
}

/// A method call made through [`Op::Method`]: its name, the types of the receiver and the
/// arguments, and of the result.
struct Site {
    name: String,
    tys: Vec<Ty>,
    ret: Ty,
    /// The call expression (`sum` of an empty array of floats depends on it).
    node: NodeId,
    /// The method name as the interpreter's dispatch wants it (user methods).
    ident: Option<Ident>,
    /// For `CallFn`: the function.
    fid: Option<FnId>,
}

#[derive(Clone)]
pub(super) enum VmState {
    Unknown,
    No,
    /// Index into `Interp::vm_codes`.
    Yes(u32),
}

fn scalar_ety(t: &Ty) -> Option<Ety> {
    match t {
        Ty::Int => Some(Ety::I),
        Ty::Float => Some(Ety::F),
        Ty::Bool => Some(Ety::B),
        _ => None,
    }
}

fn scalar(t: &Ty) -> bool {
    matches!(t, Ty::Int | Ty::Float | Ty::Bool)
}

/// What a register can hold: a number, a boolean, a string, or an array of numbers.
fn vty(t: &Ty) -> bool {
    scalar(t)
        || *t == Ty::Str
        || matches!(t, Ty::Array(e) | Ty::Nullable(e) if vty(e))
        || matches!(t, Ty::Struct(_, args) if args.is_empty())
        || matches!(t, Ty::Map(k, v) if matches!(**k, Ty::Int | Ty::Str | Ty::Bool) && vty(v))
}

/// Whether a value of this type lives in the object registers.
fn is_obj(t: &Ty) -> bool {
    matches!(
        t,
        Ty::Array(_) | Ty::Str | Ty::Struct(..) | Ty::Nullable(_) | Ty::Map(..)
    )
}

/// Whether code needs the object registers: it has an array operation, or an array among its
/// parameters (or the variables a loop shares), the variables of the module it reads, or the
/// arguments of a method it calls.
fn uses_objects(ops: &[Op], tys: &[Ty], sites: &[Site], globals: &[GlobalRef]) -> bool {
    let array = is_obj;
    tys.iter().any(array)
        || globals.iter().any(|g| array(&g.ty))
        || sites
            .iter()
            .any(|s| s.tys.iter().any(array) || array(&s.ret))
        || ops.iter().any(|op| {
            matches!(
                op,
                Op::MovO(..)
                    | Op::RetO(_)
                    | Op::NewArr(_)
                    | Op::NewMap(_)
                    | Op::MapSnap { .. }
                    | Op::ConstNone(_)
                    | Op::Box { .. }
                    | Op::Unbox { .. }
                    | Op::IsNone(..)
                    | Op::IndexG { .. }
                    | Op::MapReq { .. }
                    | Op::MapSet { .. }
                    | Op::ConstS(..)
                    | Op::AddS(..)
                    | Op::CmpS { .. }
                    | Op::Format { .. }
                    | Op::Builtin { .. }
                    | Op::Field { .. }
                    | Op::SetField { .. }
                    | Op::NewStruct { .. }
                    | Op::CallMethod { .. }
                    | Op::CallFn { .. }
                    | Op::ALen(..)
                    | Op::ALoad { .. }
                    | Op::ACheck { .. }
                    | Op::AStore { .. }
                    | Op::APush { .. }
                    | Op::ForArr { .. }
                    | Op::ALoadO { .. }
                    | Op::AStoreO { .. }
                    | Op::APushO { .. }
                    | Op::ForArrO { .. }
            )
        })
}

struct Gen<'a> {
    prog: &'a Program,
    /// Functions that cannot be compiled: they are called through the interpreter.
    deny: &'a [FnId],
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
    sites: Vec<Site>,
    globals: Vec<GlobalRef>,
    strs: Vec<Value>,
    fmts: Vec<Vec<FmtPart>>,
    fields: Vec<FieldSite>,
    lits: Vec<LitSite>,
    /// Compiling a loop (not a function): its free variables live in registers.
    loop_mode: bool,
    /// The free variables are known (second pass): no new one may appear.
    preset: bool,
    frees: Vec<Free>,
    /// A call to a function was compiled (it could change a global a loop keeps in a register).
    has_call: bool,
    /// In a loop: module variables are read and written where they live (a call in the loop
    /// could change them), not kept in registers.
    direct_globals: bool,
    /// Tuples that were only taken apart (`(a, b) = (b, a + b)`): the registers of their elements.
    tuples: Vec<(String, Vec<(Reg, Ty)>)>,
    /// Constants used as operands: kept in registers loaded once, at the start. First pass: the
    /// ones met; second pass: `consts_ready`, each with its register.
    consts: Vec<(i64, Reg)>,
    consts_ready: bool,
}

enum Access {
    Reg(Reg, Ty),
    Global(u32, Ty),
}

#[derive(Default)]
struct LoopCtx {
    breaks: Vec<usize>,
    continues: Vec<usize>,
}

impl Gen<'_> {
    fn emit(&mut self, op: Op, span: Span) -> usize {
        // the arrays of objects have instructions of their own (the others stay fast)
        let op = match op {
            Op::ALoad {
                dst,
                arr,
                idx,
                ety: Ety::O,
            } => Op::ALoadO { dst, arr, idx },
            Op::AStore {
                arr,
                idx,
                src,
                ety: Ety::O,
            } => Op::AStoreO { arr, idx, src },
            Op::APush {
                arr,
                src,
                ety: Ety::O,
            } => Op::APushO { arr, src },
            Op::ForArr {
                dst,
                arr,
                idx,
                exit,
                ety: Ety::O,
            } => Op::ForArrO {
                dst,
                arr,
                idx,
                exit,
            },
            other => other,
        };
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

    /// Copies `src` to `dst`, in the bank of the type `ty`.
    fn mov(&mut self, dst: Reg, src: Reg, ty: &Ty, span: Span) {
        if dst != src {
            let op = if is_obj(ty) {
                Op::MovO(dst, src)
            } else {
                Op::Mov(dst, src)
            };
            self.emit(op, span);
        }
    }

    /// The type of the field `name` of the structure `sid` (or of a parent), when a register
    /// can hold it.
    fn field_ty(&self, sid: StructId, name: &str) -> Option<Ty> {
        for (asid, _) in self.prog.defs.ancestry(sid, &[]) {
            let sd = &self.prog.defs.structs[asid];
            if let Some(f) = sd.fields.iter().find(|f| f.name == name) {
                return (!f.is_static && vty(&f.ty)).then(|| f.ty.clone());
            }
        }
        None
    }

    fn field_site(&mut self, name: &str, ty: &Ty) -> Option<u32> {
        let i = match self
            .fields
            .iter()
            .position(|f| f.name == name && f.ty == *ty)
        {
            Some(i) => i,
            None => {
                self.fields.push(FieldSite {
                    name: name.to_string(),
                    ty: ty.clone(),
                });
                self.fields.len() - 1
            }
        };
        u32::try_from(i).ok()
    }

    /// The instruction for `d = a <op> b` on values of the type `ty` (compound assignment).
    fn compound(bop: BinOp, ty: &Ty, d: Reg, a: Reg, b: Reg) -> Option<Op> {
        let int = *ty == Ty::Int;
        let float = *ty == Ty::Float;
        Some(match bop {
            BinOp::Add if *ty == Ty::Str => Op::AddS(d, a, b),
            BinOp::Add if int => Op::AddI(d, a, b),
            BinOp::Sub if int => Op::SubI(d, a, b),
            BinOp::Mul if int => Op::MulI(d, a, b),
            BinOp::IntDiv if int => Op::IntDivI(d, a, b),
            BinOp::Mod if int => Op::ModI(d, a, b),
            BinOp::Add if float => Op::AddF(d, a, b),
            BinOp::Sub if float => Op::SubF(d, a, b),
            BinOp::Mul if float => Op::MulF(d, a, b),
            BinOp::Div if float => Op::DivF(d, a, b),
            _ => return None,
        })
    }

    /// `obj.field = v`, `obj.field += v`.
    fn assign_field(
        &mut self,
        e: &Expr,
        op: Option<BinOp>,
        target: &Expr,
        value: &Expr,
    ) -> Option<()> {
        let ExprKind::Field(obj, name, targs) = &target.kind else {
            return None;
        };
        if !targs.is_empty() {
            return None;
        }
        let (o, ot) = self.value(obj)?;
        let Ty::Struct(sid, _) = &ot else {
            return None;
        };
        let ty = self.field_ty(*sid, &name.name)?;
        let fld = self.field_site(&name.name, &ty)?;
        match op {
            None => {
                let r = self.value_as(value, &ty)?;
                self.emit(
                    Op::SetField {
                        obj: o,
                        src: r,
                        fld,
                    },
                    e.span,
                );
            }
            Some(bop) => {
                let cur = self.alloc()?;
                self.emit(
                    Op::Field {
                        dst: cur,
                        obj: o,
                        fld,
                    },
                    target.span,
                );
                let (r, t) = self.value(value)?;
                if t != ty {
                    return None;
                }
                let opc = Self::compound(bop, &ty, cur, cur, r)?;
                self.emit(opc, e.span);
                self.emit(
                    Op::SetField {
                        obj: o,
                        src: cur,
                        fld,
                    },
                    e.span,
                );
            }
        }
        Some(())
    }

    /// The method `name` as the structure `x` (or a parent) defines it: the structure and the
    /// function; `None` when there is none, or several.
    fn resolve_method(&self, x: StructId, name: &str) -> Option<(StructId, FnId)> {
        let defs = &self.prog.defs;
        for (a, _) in defs.ancestry(x, &[]) {
            let mut found: Option<FnId> = None;
            for imp in &defs.impls {
                if !matches!(&imp.target, Ty::Struct(s, args) if *s == a && args.is_empty()) {
                    continue;
                }
                for (n, fid) in &imp.methods {
                    if n == name {
                        if found.is_some_and(|f| f != *fid) {
                            return None;
                        }
                        found = Some(*fid);
                    }
                }
            }
            if let Some(fid) = found {
                return Some((a, fid));
            }
        }
        None
    }

    /// The function a call of the method `name` on a `sid` always reaches: every structure
    /// that is a `sid` or extends it must resolve the name to the same function (no override).
    fn static_method(&self, sid: StructId, name: &str) -> Option<FnId> {
        let (_, fid) = self.resolve_method(sid, name)?;
        let defs = &self.prog.defs;
        for x in 0..defs.structs.len() {
            if defs.ancestry(x, &[]).iter().any(|(a, _)| *a == sid)
                && self.resolve_method(x, name).map(|r| r.1) != Some(fid)
            {
                return None;
            }
        }
        Some(fid)
    }

    /// The method `name` of the structure `sid` (or of a parent) with a `self`: parameter
    /// types and result type, if all the impls that define it agree and a register can hold them.
    fn user_method(&self, sid: StructId, name: &str) -> Option<(Vec<Ty>, Ty)> {
        let defs = &self.prog.defs;
        // a field holding a function is called through the field
        if defs
            .ancestry(sid, &[])
            .iter()
            .any(|(a, _)| defs.structs[*a].fields.iter().any(|f| f.name == name))
        {
            return None;
        }
        let mut found: Option<(Vec<Ty>, Ty)> = None;
        for (asid, _) in defs.ancestry(sid, &[]) {
            for imp in &defs.impls {
                if !matches!(&imp.target, Ty::Struct(s, a) if *s == asid && a.is_empty())
                    || !imp.tparams.is_empty()
                {
                    continue;
                }
                for (n, fid) in &imp.methods {
                    if n != name {
                        continue;
                    }
                    let f = &defs.fns[*fid];
                    let decl = f.decl.as_ref()?;
                    if !f.has_self
                        || !decl.tparams.is_empty()
                        || !f.params.iter().all(vty)
                        || !(vty(&f.ret) || f.ret == Ty::Unit)
                    {
                        return None;
                    }
                    let sig = (f.params.clone(), f.ret.clone());
                    match &found {
                        None => found = Some(sig),
                        Some(prev) if *prev == sig => {}
                        Some(_) => return None,
                    }
                }
            }
            if found.is_some() {
                return found;
            }
        }
        found
    }

    fn here(&self) -> u32 {
        self.ops.len() as u32
    }

    fn patch(&mut self, at: usize, target: u32) {
        match &mut self.ops[at] {
            Op::Jmp(t)
            | Op::JmpF(_, t)
            | Op::JmpT(_, t)
            | Op::JnLtI(_, _, t)
            | Op::JnLeI(_, _, t)
            | Op::JnGtI(_, _, t)
            | Op::JnGeI(_, _, t)
            | Op::JnEqI(_, _, t)
            | Op::JnNeI(_, _, t)
            | Op::JnF { t, .. }
            | Op::ForCheck { exit: t, .. } => *t = target,
            _ => {}
        }
    }

    /// A jump taken when `cond` is false: the comparison and the jump in one instruction
    /// when `cond` is a comparison of two numbers. Returns the jump, to patch.
    fn jump_if_false(&mut self, cond: &Expr) -> Option<usize> {
        let mark = self.top;
        let (len, slen) = (self.ops.len(), self.spans.len());
        if let ExprKind::Binary(
            op @ (BinOp::Lt | BinOp::LtEq | BinOp::Gt | BinOp::GtEq | BinOp::Eq | BinOp::NotEq),
            l,
            r,
        ) = &cond.kind
        {
            let (a, ta) = self.value(l)?;
            let (b, tb) = self.value(r)?;
            let cmp = match op {
                BinOp::Lt => Cmp::Lt,
                BinOp::LtEq => Cmp::Le,
                BinOp::Gt => Cmp::Gt,
                BinOp::GtEq => Cmp::Ge,
                BinOp::Eq => Cmp::Eq,
                _ => Cmp::Ne,
            };
            let fused = match (&ta, &tb) {
                (Ty::Int, Ty::Int) => Some(match cmp {
                    Cmp::Lt => Op::JnLtI(a, b, 0),
                    Cmp::Le => Op::JnLeI(a, b, 0),
                    Cmp::Gt => Op::JnGtI(a, b, 0),
                    Cmp::Ge => Op::JnGeI(a, b, 0),
                    Cmp::Eq => Op::JnEqI(a, b, 0),
                    Cmp::Ne => Op::JnNeI(a, b, 0),
                }),
                (Ty::Bool, Ty::Bool) if matches!(cmp, Cmp::Eq | Cmp::Ne) => {
                    Some(if cmp == Cmp::Eq {
                        Op::JnEqI(a, b, 0)
                    } else {
                        Op::JnNeI(a, b, 0)
                    })
                }
                (Ty::Float, Ty::Float) => Some(Op::JnF { a, b, t: 0, cmp }),
                _ => None,
            };
            if let Some(op) = fused {
                let at = self.emit(op, cond.span);
                self.top = mark;
                return Some(at);
            }
            // mixed numbers: the general way (the operands are compiled again)
            self.ops.truncate(len);
            self.spans.truncate(slen);
            self.top = mark;
        }
        let (c, tc) = self.value(cond)?;
        if tc != Ty::Bool {
            return None;
        }
        let at = self.emit(Op::JmpF(c, 0), cond.span);
        self.top = mark;
        Some(at)
    }

    /// How the variable named at this node is reached: a register, or a module variable.
    fn access(&mut self, e: &Expr, write: bool) -> Option<Access> {
        let ExprKind::Ident(name, targs) = &e.kind else {
            return None;
        };
        if !targs.is_empty() {
            return None;
        }
        match self.prog.res.paths.get(&e.id) {
            Some(PathRes::Global(m, n)) => {
                let ty = self.prog.res.ident_tys.get(&e.id)?.clone();
                if self.loop_mode && !self.direct_globals {
                    let r = self.free(n, Some(*m), &ty, write)?;
                    Some(Access::Reg(r, ty))
                } else {
                    let g = self.global(*m, n, &ty);
                    Some(Access::Global(g, ty))
                }
            }
            None | Some(PathRes::Local) => {
                if let Some((r, t)) = self.lookup(name) {
                    return Some(Access::Reg(r, t));
                }
                if !self.loop_mode {
                    return None;
                }
                let ty = self.prog.res.ident_tys.get(&e.id)?.clone();
                let r = self.free(name, None, &ty, write)?;
                Some(Access::Reg(r, ty))
            }
            _ => None,
        }
    }

    /// A use of a `T?` variable that a test narrowed to `T`: the value as a `T`.
    fn narrow_access(&mut self, e: &Expr, acc: Access) -> Option<Access> {
        let Some(n) = self.prog.res.ident_narrow.get(&e.id).cloned() else {
            return Some(acc);
        };
        let declared = match &acc {
            Access::Reg(_, t) | Access::Global(_, t) => t.clone(),
        };
        if !matches!(&declared, Ty::Nullable(inner) if **inner == n) {
            return Some(acc);
        }
        let src = match acc {
            Access::Reg(r, _) => r,
            Access::Global(g, _) => {
                let tmp = self.alloc()?;
                self.emit(Op::LoadG { dst: tmp, g }, e.span);
                tmp
            }
        };
        match scalar_ety(&n) {
            Some(ety) => {
                let tmp = self.alloc()?;
                self.emit(Op::Unbox { dst: tmp, src, ety }, e.span);
                Some(Access::Reg(tmp, n))
            }
            None => Some(Access::Reg(src, n)),
        }
    }

    /// Compiles `e` into `dst` as a value of the type `want`: for a `T?`, `none` and a plain
    /// `T` are accepted too.
    fn compile_as(&mut self, e: &Expr, dst: Reg, want: &Ty) -> Option<()> {
        let Ty::Nullable(inner) = want else {
            return (self.into(e, dst)? == *want).then_some(());
        };
        if matches!(e.kind, ExprKind::None) {
            self.emit(Op::ConstNone(dst), e.span);
            return Some(());
        }
        if let ExprKind::If {
            branches,
            else_block,
        } = &e.kind
        {
            return self
                .if_expr(e, branches, else_block.as_ref(), dst, Some(want))
                .map(|_| ());
        }
        let mark = self.top;
        let tmp = self.alloc()?;
        let t = self.into(e, tmp)?;
        if t == *want {
            self.mov(dst, tmp, want, e.span);
        } else if t == **inner {
            match scalar_ety(&t) {
                Some(ety) => {
                    self.emit(Op::Box { dst, src: tmp, ety }, e.span);
                }
                None => {
                    self.emit(Op::MovO(dst, tmp), e.span);
                }
            }
        } else if t == Ty::NoneT {
            self.emit(Op::ConstNone(dst), e.span);
        } else {
            return None;
        }
        self.top = mark;
        Some(())
    }

    /// A register holding the value of `e` as a `want`.
    fn value_as(&mut self, e: &Expr, want: &Ty) -> Option<Reg> {
        if matches!(want, Ty::Nullable(_)) {
            let tmp = self.alloc()?;
            self.compile_as(e, tmp, want)?;
            Some(tmp)
        } else {
            let (r, t) = self.value(e)?;
            (t == *want).then_some(r)
        }
    }

    fn new_site(&mut self, tys: Vec<Ty>, ret: Ty, node: NodeId) -> u32 {
        self.sites.push(Site {
            name: String::new(),
            tys,
            ret,
            node,
            ident: None,
            fid: None,
        });
        (self.sites.len() - 1) as u32
    }

    fn global(&mut self, module: ModId, name: &str, ty: &Ty) -> u32 {
        if let Some(i) = self
            .globals
            .iter()
            .position(|g| g.module == module && g.name == name)
        {
            return i as u32;
        }
        self.globals.push(GlobalRef {
            module,
            name: name.to_string(),
            ty: ty.clone(),
            slot: Cell::new(NO_SLOT),
        });
        (self.globals.len() - 1) as u32
    }

    /// The register of a variable shared with the code around the loop.
    fn free(&mut self, name: &str, global: Option<ModId>, ty: &Ty, write: bool) -> Option<Reg> {
        if let Some(f) = self
            .frees
            .iter_mut()
            .find(|f| f.name == name && f.global == global)
        {
            if f.ty != *ty {
                return None;
            }
            f.assigned |= write;
            return Some(f.reg);
        }
        if self.preset {
            return None;
        }
        let reg = self.alloc()?;
        self.frees.push(Free {
            name: name.to_string(),
            reg,
            ty: ty.clone(),
            global,
            assigned: write,
        });
        Some(reg)
    }

    fn lookup(&self, name: &str) -> Option<(Reg, Ty)> {
        self.scopes
            .iter()
            .rev()
            .flat_map(|s| s.iter().rev())
            .find(|(n, _, _)| n == name)
            .map(|(_, r, t)| (*r, t.clone()))
    }

    /// Where the value of `e` is, and its type: a variable's own register, or a new temporary.
    fn value(&mut self, e: &Expr) -> Option<(Reg, Ty)> {
        if matches!(e.kind, ExprKind::SelfVal) {
            return self.lookup("self");
        }
        if matches!(e.kind, ExprKind::Ident(..))
            && let Some(acc) = self.access(e, false)
            && let Some(Access::Reg(r, t)) = self.narrow_access(e, acc)
        {
            return Some((r, t));
        }
        // a literal operand lives in a register of its own, loaded once
        let literal = match &e.kind {
            ExprKind::Int(v) => Some((*v, Ty::Int)),
            ExprKind::Float(v) => Some((v.to_bits() as i64, Ty::Float)),
            ExprKind::Bool(b) => Some((i64::from(*b), Ty::Bool)),
            _ => None,
        };
        if let Some((bits, ty)) = literal {
            if self.consts_ready {
                if let Some((_, r)) = self.consts.iter().find(|(b, _)| *b == bits) {
                    return Some((*r, ty));
                }
            } else if !self.consts.iter().any(|(b, _)| *b == bits) {
                self.consts.push((bits, 0));
            }
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
            ExprKind::Ident(..) => {
                let acc = self.access(e, false)?;
                match self.narrow_access(e, acc)? {
                    Access::Reg(r, ty) => {
                        self.mov(dst, r, &ty, span);
                        Some(ty)
                    }
                    Access::Global(g, ty) => {
                        self.emit(Op::LoadG { dst, g }, span);
                        Some(ty)
                    }
                }
            }
            ExprKind::Array(items) => {
                let ty = self.prog.res.array_lits.get(&e.id)?.clone();
                let Ty::Array(el) = &ty else { return None };
                let ety = ety_of(el)?;
                let tmp = self.alloc()?;
                self.emit(Op::NewArr(tmp), span);
                for it in items {
                    let mark = self.top;
                    let r = self.value_as(it, el)?;
                    self.emit(
                        Op::APush {
                            arr: tmp,
                            src: r,
                            ety,
                        },
                        span,
                    );
                    self.top = mark;
                }
                self.emit(Op::MovO(dst, tmp), span);
                self.top = tmp as usize;
                Some(ty)
            }
            ExprKind::Map(items) => {
                let ty = self.prog.res.array_lits.get(&e.id)?.clone();
                let Ty::Map(k, v) = &ty else { return None };
                let tmp = self.alloc()?;
                self.emit(Op::NewMap(tmp), span);
                let site = self.new_site(
                    vec![ty.clone(), (**k).clone(), (**v).clone()],
                    Ty::Unit,
                    e.id,
                );
                for (kx, vx) in items {
                    let mark = self.top;
                    let kr = self.value_as(kx, k)?;
                    let vr = self.value_as(vx, v)?;
                    self.emit(
                        Op::MapSet {
                            map: tmp,
                            key: kr,
                            val: vr,
                            site,
                        },
                        span,
                    );
                    self.top = mark;
                }
                self.emit(Op::MovO(dst, tmp), span);
                self.top = tmp as usize;
                Some(ty)
            }
            ExprKind::None => {
                self.emit(Op::ConstNone(dst), span);
                Some(Ty::NoneT)
            }
            ExprKind::SelfVal => {
                let (r, t) = self.lookup("self")?;
                self.mov(dst, r, &t, span);
                Some(t)
            }
            ExprKind::Field(obj, name, targs)
                if targs.is_empty() && self.prog.res.paths.get(&e.id).is_none() =>
            {
                let mark = self.top;
                let (o, ot) = self.value(obj)?;
                let Ty::Struct(sid, _) = &ot else {
                    return None;
                };
                let ty = self.field_ty(*sid, &name.name)?;
                let fld = self.field_site(&name.name, &ty)?;
                self.emit(Op::Field { dst, obj: o, fld }, span);
                self.top = mark;
                Some(ty)
            }
            ExprKind::StructLit(lit) => {
                if lit.base.is_some() {
                    return None;
                }
                let sid = *self.prog.res.struct_lits.get(&e.id)?;
                if !self.prog.defs.structs[sid].tparams.is_empty() {
                    return None;
                }
                let mark = self.top;
                let base = self.top as Reg;
                let mut names = Vec::new();
                for item in &lit.fields {
                    let want = self.field_ty(sid, &item.name.name)?;
                    let r = self.alloc()?;
                    self.compile_as(&item.value, r, &want)?;
                    names.push((item.name.name.clone(), want));
                }
                self.lits.push(LitSite { sid, names });
                let lit = u32::try_from(self.lits.len() - 1).ok()?;
                self.emit(Op::NewStruct { dst, base, lit }, span);
                self.top = mark;
                Some(Ty::Struct(sid, Vec::new()))
            }
            ExprKind::Str(parts) => {
                if parts.iter().all(|p| matches!(p, StrPart::Lit(_))) {
                    let text: String = parts
                        .iter()
                        .map(|p| match p {
                            StrPart::Lit(t) => t.as_str(),
                            StrPart::Expr(_) => "",
                        })
                        .collect();
                    let idx = match self
                        .strs
                        .iter()
                        .position(|v| matches!(v, Value::Str(x) if **x == *text))
                    {
                        Some(i) => i,
                        None => {
                            self.strs.push(Value::str(&text));
                            self.strs.len() - 1
                        }
                    };
                    self.emit(Op::ConstS(dst, u32::try_from(idx).ok()?), span);
                } else {
                    let mark = self.top;
                    let base = self.top as Reg;
                    let mut fmt = Vec::new();
                    for p in parts {
                        match p {
                            StrPart::Lit(t) => fmt.push(FmtPart::Lit(t.clone())),
                            StrPart::Expr(x) => {
                                let r = self.alloc()?;
                                let t = self.into(x, r)?;
                                if !vty(&t) {
                                    return None;
                                }
                                fmt.push(FmtPart::Arg(t));
                            }
                        }
                    }
                    self.fmts.push(fmt);
                    let fmt = u32::try_from(self.fmts.len() - 1).ok()?;
                    self.emit(Op::Format { dst, base, fmt }, span);
                    self.top = mark;
                }
                Some(Ty::Str)
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
            ExprKind::Binary(op @ (BinOp::Eq | BinOp::NotEq), l, r)
                if matches!(l.kind, ExprKind::None) != matches!(r.kind, ExprKind::None) =>
            {
                // `x == none`, `x != none`
                let other = if matches!(l.kind, ExprKind::None) {
                    r
                } else {
                    l
                };
                let mark = self.top;
                let (a, ta) = self.value(other)?;
                if !matches!(ta, Ty::Nullable(_)) {
                    return None;
                }
                self.emit(Op::IsNone(dst, a), span);
                if matches!(op, BinOp::NotEq) {
                    self.emit(Op::Not(dst, dst), span);
                }
                self.top = mark;
                Some(Ty::Bool)
            }
            ExprKind::Binary(op, l, r) => {
                let mark = self.top;
                let (mut a, mut ta) = self.value(l)?;
                let (mut b, mut tb) = self.value(r)?;
                // an `int` next to a `float` is turned into a float first (as the tree walker does)
                if !matches!(op, BinOp::IntDiv) && matches!((&ta, &tb), (Ty::Int, Ty::Float)) {
                    let t = self.alloc()?;
                    self.emit(Op::ItoF(t, a), span);
                    (a, ta) = (t, Ty::Float);
                } else if !matches!(op, BinOp::IntDiv) && matches!((&ta, &tb), (Ty::Float, Ty::Int))
                {
                    let t = self.alloc()?;
                    self.emit(Op::ItoF(t, b), span);
                    (b, tb) = (t, Ty::Float);
                }
                if ta == Ty::Str && tb == Ty::Str {
                    let cmp = |c| Op::CmpS { dst, a, b, cmp: c };
                    let opc = match op {
                        BinOp::Add => Op::AddS(dst, a, b),
                        BinOp::Eq => cmp(Cmp::Eq),
                        BinOp::NotEq => cmp(Cmp::Ne),
                        BinOp::Lt => cmp(Cmp::Lt),
                        BinOp::LtEq => cmp(Cmp::Le),
                        BinOp::Gt => cmp(Cmp::Gt),
                        BinOp::GtEq => cmp(Cmp::Ge),
                        _ => return None,
                    };
                    self.emit(opc, span);
                    self.top = mark;
                    return Some(if matches!(op, BinOp::Add) {
                        Ty::Str
                    } else {
                        Ty::Bool
                    });
                }
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
                    BinOp::Pow if int => (Op::PowI(dst, a, b), Ty::Int),
                    BinOp::Div if int => (Op::DivII(dst, a, b), Ty::Float),
                    BinOp::Add if float => (Op::AddF(dst, a, b), Ty::Float),
                    BinOp::Sub if float => (Op::SubF(dst, a, b), Ty::Float),
                    BinOp::Mul if float => (Op::MulF(dst, a, b), Ty::Float),
                    BinOp::Div if float => (Op::DivF(dst, a, b), Ty::Float),
                    BinOp::Mod if float => (Op::ModF(dst, a, b), Ty::Float),
                    BinOp::Pow if float => (Op::PowF(dst, a, b), Ty::Float),
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
            ExprKind::Index(base, idx) => {
                let mark = self.top;
                let (arr, at) = self.value(base)?;
                let (el, key): (Ty, Ty) = match &at {
                    Ty::Array(el) => ((**el).clone(), Ty::Int),
                    // `map[key]` is `V?`; `text[i]` is a text
                    Ty::Map(k, v) => (Ty::nullable((**v).clone()), (**k).clone()),
                    Ty::Str => (Ty::Str, Ty::Int),
                    _ => return None,
                };
                let (i, ti) = self.value(idx)?;
                if ti != key {
                    return None;
                }
                if !matches!(at, Ty::Array(_)) {
                    let site = self.new_site(vec![at.clone(), ti], el.clone(), e.id);
                    self.emit(
                        Op::IndexG {
                            dst,
                            base: arr,
                            idx: i,
                            site,
                        },
                        span,
                    );
                    self.top = mark;
                    return Some(el);
                }
                let ety = ety_of(&el)?;
                self.emit(
                    Op::ALoad {
                        dst,
                        arr,
                        idx: i,
                        ety,
                    },
                    span,
                );
                self.top = mark;
                Some(el)
            }
            ExprKind::TupleField(inner, idx) => {
                let ExprKind::Ident(name, _) = &inner.kind else {
                    return None;
                };
                let (_, regs) = self.tuples.iter().rev().find(|(n, _)| n == name)?;
                let (r, t) = regs.get(*idx)?.clone();
                self.emit(Op::Mov(dst, r), span);
                Some(t)
            }
            ExprKind::Cast(inner, _) => {
                let target = self.prog.res.casts.get(&e.id)?.clone();
                let mark = self.top;
                let (r, ty) = self.value(inner)?;
                match (&ty, &target) {
                    (Ty::Int, Ty::Float) => {
                        self.emit(Op::ItoF(dst, r), span);
                    }
                    (Ty::Float, Ty::Int) => {
                        self.emit(Op::FtoI(dst, r), span);
                    }
                    (a, b) if a == b && scalar(a) => {
                        if r != dst {
                            self.emit(Op::Mov(dst, r), span);
                        }
                    }
                    _ => return None,
                }
                self.top = mark;
                Some(target)
            }
            ExprKind::If {
                branches,
                else_block,
            } => self.if_expr(e, branches, else_block.as_ref(), dst, None),
            ExprKind::Call { callee, args } if matches!(callee.kind, ExprKind::Field(..)) => {
                self.method_call(e, callee, args, dst)
            }
            ExprKind::Call { callee, args } => {
                if !matches!(callee.kind, ExprKind::Ident(..))
                    || self.prog.res.arg_order.contains_key(&e.id)
                    || args.iter().any(|a| a.name.is_some())
                {
                    return None;
                }
                if let Some(PathRes::BuiltinFn(name @ ("print" | "write"))) =
                    self.prog.res.paths.get(&callee.id)
                {
                    if args.len() != 1 {
                        return None;
                    }
                    let mark = self.top;
                    let base = self.top as Reg;
                    let r = self.alloc()?;
                    let t = self.into(&args[0].value, r)?;
                    if !vty(&t) {
                        return None;
                    }
                    let site = self.sites.len() as u32;
                    self.sites.push(Site {
                        name: (*name).to_string(),
                        tys: vec![t],
                        ret: Ty::Unit,
                        node: e.id,
                        ident: None,
                        fid: None,
                    });
                    self.emit(Op::Builtin { base, site }, span);
                    self.top = mark;
                    return Some(Ty::Unit);
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
                    || !def.params.iter().all(vty)
                    || !(vty(&def.ret) || def.ret == Ty::Unit)
                {
                    return None;
                }
                let mark = self.top;
                let base = self.top as Reg;
                for (a, want) in args.iter().zip(&def.params) {
                    let r = self.alloc()?;
                    self.compile_as(&a.value, r, want)?;
                }
                self.has_call = true;
                if self.deny.contains(&fid) {
                    // not compilable: the interpreter runs it
                    let site = self.sites.len() as u32;
                    self.sites.push(Site {
                        name: def.name.clone(),
                        tys: def.params.clone(),
                        ret: def.ret.clone(),
                        node: e.id,
                        ident: None,
                        fid: Some(fid),
                    });
                    self.emit(Op::CallFn { dst, base, site }, span);
                } else {
                    let code = self.callee_index(fid)?;
                    self.emit(Op::Call { code, base, dst }, span);
                }
                self.top = mark;
                Some(def.ret.clone())
            }
            _ => None,
        }
    }

    /// `x.abs()`, `n.sqrt()`, `a.max(b)`...: the methods of numbers.
    fn method_call(&mut self, e: &Expr, callee: &Expr, args: &[CallArg], dst: Reg) -> Option<Ty> {
        let ExprKind::Field(obj, name, targs) = &callee.kind else {
            return None;
        };
        if !targs.is_empty()
            || args.iter().any(|a| a.name.is_some())
            || self.prog.res.arg_order.contains_key(&e.id)
            || self.prog.res.trait_calls.contains(&e.id)
            || self.prog.res.class_calls.contains(&e.id)
            || self.prog.res.super_calls.contains_key(&callee.id)
            || self.prog.res.paths.get(&callee.id).is_some()
        {
            return None;
        }
        let mark = self.top;
        let base = self.top as Reg;
        let recv = self.alloc()?;
        let rt = self.into(obj, recv)?;
        let user = matches!(&rt, Ty::Struct(..));
        let (want, ret) = match &rt {
            Ty::Struct(sid, _) => self.user_method(*sid, &name.name)?,
            // a built-in method: the signature the checker found for this call
            _ => {
                let (srt, params, ret) = self.prog.res.builtin_sigs.get(&e.id)?;
                if *srt != rt {
                    return None;
                }
                (params.clone(), ret.clone())
            }
        };
        if want.len() != args.len() {
            return None;
        }
        let mut tys = vec![rt.clone()];
        for (a, w) in args.iter().zip(&want) {
            let r = self.alloc()?;
            self.compile_as(&a.value, r, w)?;
            tys.push(w.clone());
        }
        let span = e.span;
        let fast = match (rt.clone(), name.name.as_str()) {
            (Ty::Int, "abs") => Some(Op::AbsI(dst, recv)),
            (Ty::Float, "abs") => Some(Op::AbsF(dst, recv)),
            (Ty::Int, "sqrt") => Some(Op::SqrtI(dst, recv)),
            (Ty::Float, "sqrt") => Some(Op::SqrtF(dst, recv)),
            (Ty::Int, "min") => Some(Op::MinI(dst, recv, recv + 1)),
            (Ty::Int, "max") => Some(Op::MaxI(dst, recv, recv + 1)),
            (Ty::Float, "min") => Some(Op::MinF(dst, recv, recv + 1)),
            (Ty::Float, "max") => Some(Op::MaxF(dst, recv, recv + 1)),
            (Ty::Array(_), "len") => Some(Op::ALen(dst, recv)),
            (Ty::Array(el), "push") => Some(Op::APush {
                arr: recv,
                src: recv + 1,
                ety: ety_of(&el)?,
            }),
            _ => None,
        };
        match fast {
            Some(op) => {
                self.emit(op, span);
            }
            None => {
                // a method nobody overrides is called directly
                let direct = match &rt {
                    Ty::Struct(sid, _) => self
                        .static_method(*sid, &name.name)
                        .filter(|f| !self.deny.contains(f)),
                    _ => None,
                };
                if let Some(fid) = direct {
                    self.has_call = true;
                    let code = self.callee_index(fid)?;
                    self.emit(Op::Call { code, base, dst }, span);
                } else {
                    let site = self.sites.len() as u32;
                    self.sites.push(Site {
                        name: name.name.clone(),
                        tys,
                        ret: ret.clone(),
                        node: e.id,
                        ident: user.then(|| name.clone()),
                        fid: None,
                    });
                    if user {
                        // the method could change a variable of the module
                        self.has_call = true;
                        self.emit(Op::CallMethod { dst, base, site }, span);
                    } else {
                        self.emit(Op::Method { dst, base, site }, span);
                    }
                }
            }
        }
        self.top = mark;
        Some(ret)
    }

    /// `xs[i] = v`, `xs[i] += v`, `xs[] = v`.
    fn assign_element(
        &mut self,
        e: &Expr,
        op: Option<BinOp>,
        target: &Expr,
        value: &Expr,
    ) -> Option<()> {
        match &target.kind {
            ExprKind::Append(base) => {
                if op.is_some() {
                    return None;
                }
                let (arr, at) = self.value(base)?;
                let Ty::Array(el) = &at else {
                    return None;
                };
                let ety = ety_of(el)?;
                let r = self.value_as(value, el)?;
                self.emit(Op::APush { arr, src: r, ety }, e.span);
                Some(())
            }
            ExprKind::Index(base, idx) => {
                let (arr, at) = self.value(base)?;
                if let Ty::Map(k, v) = &at {
                    // `map[key] = v`, `map[key] += v`
                    let (kr, kt) = self.value(idx)?;
                    if kt != **k {
                        return None;
                    }
                    let site =
                        self.new_site(vec![at.clone(), kt, (**v).clone()], (**v).clone(), e.id);
                    match op {
                        None => {
                            let r = self.value_as(value, v)?;
                            self.emit(
                                Op::MapSet {
                                    map: arr,
                                    key: kr,
                                    val: r,
                                    site,
                                },
                                e.span,
                            );
                        }
                        Some(bop) => {
                            let cur = self.alloc()?;
                            self.emit(
                                Op::MapReq {
                                    dst: cur,
                                    map: arr,
                                    key: kr,
                                    site,
                                },
                                target.span,
                            );
                            let r = self.value_as(value, v)?;
                            let opc = Self::compound(bop, v, cur, cur, r)?;
                            self.emit(opc, e.span);
                            self.emit(
                                Op::MapSet {
                                    map: arr,
                                    key: kr,
                                    val: cur,
                                    site,
                                },
                                e.span,
                            );
                        }
                    }
                    return Some(());
                }
                let Ty::Array(el) = &at else {
                    return None;
                };
                let el = (**el).clone();
                let ety = ety_of(&el)?;
                let (i, ti) = self.value(idx)?;
                if ti != Ty::Int {
                    return None;
                }
                // the index must exist before the right side is computed
                self.emit(Op::ACheck { arr, idx: i }, target.span);
                match op {
                    None => {
                        let r = self.value_as(value, &el)?;
                        self.emit(
                            Op::AStore {
                                arr,
                                idx: i,
                                src: r,
                                ety,
                            },
                            e.span,
                        );
                    }
                    Some(bop) => {
                        let cur = self.alloc()?;
                        self.emit(
                            Op::ALoad {
                                dst: cur,
                                arr,
                                idx: i,
                                ety,
                            },
                            target.span,
                        );
                        let r = self.value_as(value, &el)?;
                        let opc = Self::compound(bop, &el, cur, cur, r)?;
                        self.emit(opc, e.span);
                        self.emit(
                            Op::AStore {
                                arr,
                                idx: i,
                                src: cur,
                                ety,
                            },
                            e.span,
                        );
                    }
                }
                Some(())
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

    /// `if ... then a else b end` as a value in `dst`; with `want`, each branch is a `want`.
    fn if_expr(
        &mut self,
        e: &Expr,
        branches: &[(Expr, Block)],
        else_block: Option<&Block>,
        dst: Reg,
        want: Option<&Ty>,
    ) -> Option<Ty> {
        let span = e.span;
        let else_block = else_block?;
        let mut ends = Vec::new();
        let mut ty: Option<Ty> = None;
        for (cond, block) in branches {
            let skip = self.jump_if_false(cond)?;
            let t = self.block_as(block, Some(dst), want)?;
            if *ty.get_or_insert(t.clone()) != t {
                return None;
            }
            ends.push(self.emit(Op::Jmp(0), span));
            let next = self.here();
            self.patch(skip, next);
        }
        let t = self.block_as(else_block, Some(dst), want)?;
        if *ty.get_or_insert(t.clone()) != t {
            return None;
        }
        let end = self.here();
        for j in ends {
            self.patch(j, end);
        }
        ty
    }

    /// A block in its own scope. With `dst`, the value of its last expression goes there.
    fn block(&mut self, b: &Block, dst: Option<Reg>) -> Option<Ty> {
        self.block_as(b, dst, None)
    }

    /// The same, the last expression being converted to the type `want` when there is one.
    fn block_as(&mut self, b: &Block, dst: Option<Reg>, want: Option<&Ty>) -> Option<Ty> {
        self.scopes.push(Vec::new());
        let mark = self.top;
        let mut ty = Ty::Unit;
        let n = b.stmts.len();
        for (i, s) in b.stmts.iter().enumerate() {
            let last = i + 1 == n;
            match (&s.kind, dst, last) {
                (StmtKind::Expr(e), Some(d), true) => match want {
                    Some(w) => {
                        self.compile_as(e, d, w)?;
                        ty = w.clone();
                    }
                    None => ty = self.into(e, d)?,
                },
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
            StmtKind::Seq { stmts, .. } => {
                for st in stmts {
                    self.stmt(st)?;
                }
                Some(())
            }
            StmtKind::Let {
                name,
                init:
                    Expr {
                        kind: ExprKind::Tuple(items),
                        ..
                    },
                ..
            } if name.name.starts_with('$') => {
                // the hidden variable of `(a, b) = (b, a + b)`: every element is computed first
                let mut regs = Vec::new();
                for it in items {
                    let r = self.alloc()?;
                    let t = self.into(it, r)?;
                    if !scalar(&t) {
                        return None;
                    }
                    regs.push((r, t));
                }
                self.tuples.push((name.name.clone(), regs));
                Some(())
            }
            StmtKind::LetTuple {
                pattern: TuplePattern::Tuple(names, _),
                init:
                    Expr {
                        kind: ExprKind::Tuple(items),
                        ..
                    },
                ..
            } if names.len() == items.len()
                && names
                    .iter()
                    .all(|n| matches!(n, TuplePattern::Name(_) | TuplePattern::Wild(_))) =>
            {
                // `let (a, b) = (x, y)`: the values are computed first, then named
                let mut regs = Vec::new();
                for it in items {
                    let r = self.alloc()?;
                    let t = self.into(it, r)?;
                    if !scalar(&t) {
                        return None;
                    }
                    regs.push((r, t));
                }
                for (n, (r, t)) in names.iter().zip(regs) {
                    if let TuplePattern::Name(id) = n {
                        self.scopes.last_mut()?.push((id.name.clone(), r, t));
                    }
                }
                Some(())
            }
            StmtKind::Let { name, init, .. } => {
                let dst = self.alloc()?;
                let ty = match self.prog.res.let_tys.get(&init.id).cloned() {
                    Some(declared) => {
                        self.compile_as(init, dst, &declared)?;
                        declared
                    }
                    None => self.into(init, dst)?,
                };
                if !vty(&ty) {
                    return None;
                }
                // the temporaries of the initializer are free again, the variable stays
                self.top = dst as usize + 1;
                self.scopes.last_mut()?.push((name.name.clone(), dst, ty));
                Some(())
            }
            StmtKind::Expr(e) => self.effect(e),
            StmtKind::Return(e) => {
                if self.loop_mode {
                    // a `return` inside a loop leaves a function the machine knows nothing of
                    return None;
                }
                let mark = self.top;
                match e {
                    Some(e) => {
                        let ret = self.ret.clone();
                        let r = self.value_as(e, &ret)?;
                        let op = if is_obj(&self.ret) {
                            Op::RetO(r)
                        } else {
                            Op::Ret(r)
                        };
                        self.emit(op, span);
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
                let exit = self.jump_if_false(cond)?;
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
                var2,
                var_pattern: None,
                var2_pattern: None,
                iter,
                step: None,
                body,
            } if !matches!(iter.kind, ExprKind::Range { .. }) => {
                let mark = self.top;
                let (arr, at) = self.value(iter)?;
                // a map is looped over through snapshots of its keys and values
                let (arr, el, map_vals) = match &at {
                    Ty::Array(el) => (arr, (**el).clone(), None),
                    Ty::Map(k, v) => {
                        let keys = self.alloc()?;
                        let vals = self.alloc()?;
                        self.emit(
                            Op::MapSnap {
                                keys,
                                vals,
                                map: arr,
                            },
                            span,
                        );
                        (keys, (**k).clone(), Some((vals, (**v).clone())))
                    }
                    _ => return None,
                };
                let ety = ety_of(&el)?;
                let cnt = self.alloc()?;
                self.emit(Op::ConstI(cnt, 0), span);
                self.scopes.push(vec![]);
                // `for x in xs` / `for i, x in xs` / `for k, v in map`
                let (idx_name, item_name) = match (var2, &map_vals) {
                    (Some(v2), None) => (Some(var.name.clone()), v2.name.clone()),
                    _ => (None, var.name.clone()),
                };
                let idx_reg = match &idx_name {
                    Some(n) => {
                        let r = self.alloc()?;
                        self.scopes.last_mut()?.push((n.clone(), r, Ty::Int));
                        Some(r)
                    }
                    None => None,
                };
                let item = self.alloc()?;
                self.scopes.last_mut()?.push((item_name, item, el));
                let head = self.here();
                let get = self.emit(
                    Op::ForArr {
                        dst: item,
                        arr,
                        idx: cnt,
                        exit: 0,
                        ety,
                    },
                    span,
                );
                if let Some(r) = idx_reg {
                    self.emit(Op::Mov(r, cnt), span);
                }
                if let (Some((vals, vt)), Some(v2)) = (&map_vals, var2) {
                    let vr = self.alloc()?;
                    self.emit(
                        Op::ALoad {
                            dst: vr,
                            arr: *vals,
                            idx: cnt,
                            ety: ety_of(vt)?,
                        },
                        span,
                    );
                    self.scopes
                        .last_mut()?
                        .push((v2.name.clone(), vr, vt.clone()));
                }
                self.loops.push(LoopCtx::default());
                self.block(body, None)?;
                let ctx = self.loops.pop()?;
                let next = self.here();
                for j in ctx.continues {
                    self.patch(j, next);
                }
                self.emit(Op::IncI(cnt), span);
                self.emit(Op::Jmp(head), span);
                let end = self.here();
                if let Op::ForArr { exit, .. } | Op::ForArrO { exit, .. } = &mut self.ops[get] {
                    *exit = end;
                }
                for j in ctx.breaks {
                    self.patch(j, end);
                }
                self.scopes.pop();
                self.top = mark;
                Some(())
            }
            StmtKind::For {
                var,
                var2: None,
                var_pattern: None,
                var2_pattern: None,
                iter,
                step,
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
                let stp = self.alloc()?;
                match step {
                    Some(st) => {
                        if self.into(st, stp)? != Ty::Int {
                            return None;
                        }
                    }
                    None => {
                        self.emit(Op::ConstI(stp, 1), span);
                    }
                }
                self.emit(Op::StepCheck(stp), span);
                let head = self.here();
                let exit = self.emit(
                    Op::ForCheck {
                        cnt,
                        lim,
                        step: stp,
                        incl: *inclusive,
                        exit: 0,
                    },
                    span,
                );
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
                let step_at = self.emit(
                    Op::ForNext {
                        cnt,
                        step: stp,
                        exit: 0,
                    },
                    span,
                );
                self.emit(Op::Jmp(head), span);
                let end = self.here();
                self.patch(exit, end);
                if let Op::ForNext { exit, .. } = &mut self.ops[step_at] {
                    *exit = end;
                }
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
            ExprKind::Assign { op, target, value }
                if matches!(target.kind, ExprKind::Index(..) | ExprKind::Append(_)) =>
            {
                self.assign_element(e, *op, target, value)?;
            }
            ExprKind::Assign { op, target, value }
                if matches!(target.kind, ExprKind::Field(..))
                    && self.prog.res.paths.get(&target.id).is_none() =>
            {
                self.assign_field(e, *op, target, value)?;
            }
            ExprKind::Assign { op, target, value } => {
                let (reg, ty, global) = match self.access(target, true)? {
                    Access::Reg(r, t) => (r, t, None),
                    Access::Global(g, t) => (self.alloc()?, t, Some(g)),
                };
                if let (Some(g), Some(_)) = (global, op) {
                    // read the variable first, as the tree walker does, then the right side
                    self.emit(Op::LoadG { dst: reg, g }, e.span);
                }
                let r = self.value_as(value, &ty)?;
                match op {
                    None => {
                        if global.is_none() {
                            self.mov(reg, r, &ty, e.span);
                        }
                        if let Some(g) = global {
                            self.emit(Op::StoreG { src: r, g }, e.span);
                        }
                    }
                    Some(bop) => {
                        let int = ty == Ty::Int;
                        let float = ty == Ty::Float;
                        let opc = match bop {
                            BinOp::Add if ty == Ty::Str => Op::AddS(reg, reg, r),
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
                        if let Some(g) = global {
                            self.emit(Op::StoreG { src: reg, g }, e.span);
                        }
                    }
                }
            }
            ExprKind::If {
                branches,
                else_block,
            } => {
                let mut ends = Vec::new();
                for (cond, block) in branches {
                    let skip = self.jump_if_false(cond)?;
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

/// What is compiled: a function, or a loop statement.
#[derive(Clone, Copy)]
pub(super) enum Unit<'a> {
    Fn(FnId),
    Loop(&'a Stmt),
}

/// Compiles one unit; also the functions it calls.
fn gen_unit(prog: &Program, unit: Unit, deny: &[FnId]) -> Option<(Code, Vec<FnId>)> {
    let new_gen = |ret: Ty, loop_mode: bool| Gen {
        prog,
        deny,
        ops: Vec::new(),
        spans: Vec::new(),
        scopes: vec![Vec::new()],
        top: 0,
        max: 0,
        loops: Vec::new(),
        ret,
        callees: Vec::new(),
        sites: Vec::new(),
        globals: Vec::new(),
        strs: Vec::new(),
        fmts: Vec::new(),
        fields: Vec::new(),
        lits: Vec::new(),
        loop_mode,
        preset: false,
        frees: Vec::new(),
        has_call: false,
        direct_globals: false,
        tuples: Vec::new(),
        consts: Vec::new(),
        consts_ready: false,
    };
    match unit {
        Unit::Fn(f) => {
            let def = &prog.defs.fns[f];
            let decl = def.decl.as_ref()?;
            let body = decl.body.as_ref()?;
            // a method of a non-generic impl: `self` is its first parameter
            let self_ty: Option<Ty> = match def.impl_id {
                Some(i) => {
                    let imp = &prog.defs.impls[i];
                    if !imp.tparams.is_empty() || !vty(&imp.target) {
                        return None;
                    }
                    def.has_self.then(|| imp.target.clone())
                }
                None => None,
            };
            if decl.external
                || !decl.tparams.is_empty()
                || (def.has_self && self_ty.is_none())
                || !def.params.iter().all(vty)
                || !(vty(&def.ret) || def.ret == Ty::Unit)
            {
                return None;
            }
            // `consts`: the constants the first pass met, kept in registers in the second
            let attempt = |consts: Option<&[i64]>| -> Option<Gen> {
                let mut g = new_gen(def.ret.clone(), false);
                if let Some(t) = &self_ty {
                    let r = g.alloc()?;
                    g.scopes[0].push(("self".to_string(), r, t.clone()));
                }
                for (p, t) in decl.params.iter().zip(&def.params) {
                    let r = g.alloc()?;
                    g.scopes[0].push((p.name.name.clone(), r, t.clone()));
                }
                if let Some(list) = consts {
                    for &bits in list {
                        let r = g.alloc()?;
                        g.consts.push((bits, r));
                        g.emit(Op::ConstI(r, bits), decl.span);
                    }
                    g.consts_ready = true;
                }
                if def.ret == Ty::Unit {
                    g.block(body, None)?;
                    g.emit(Op::RetUnit, decl.span);
                } else {
                    let res = g.alloc()?;
                    if g.block_as(body, Some(res), Some(&def.ret))? != def.ret {
                        return None;
                    }
                    let op = if is_obj(&def.ret) {
                        Op::RetO(res)
                    } else {
                        Op::Ret(res)
                    };
                    g.emit(op, decl.span);
                }
                Some(g)
            };
            let first = attempt(None)?;
            let g = if first.consts.is_empty() {
                first
            } else {
                let list: Vec<i64> = first.consts.iter().map(|c| c.0).collect();
                attempt(Some(&list))?
            };
            let param_tys: Vec<Ty> = self_ty.iter().chain(def.params.iter()).cloned().collect();
            let uses_objs = uses_objects(&g.ops, &param_tys, &g.sites, &g.globals);
            Some((
                Code {
                    name: def.name.clone(),
                    nregs: g.max + 1,
                    ops: g.ops,
                    spans: g.spans,
                    param_tys,
                    ret: def.ret.clone(),
                    sites: g.sites,
                    globals: g.globals,
                    strs: g.strs,
                    fmts: g.fmts,
                    fields: g.fields,
                    lits: g.lits,
                    frees: Vec::new(),
                    is_loop: false,
                    uses_objs,
                },
                g.callees,
            ))
        }
        Unit::Loop(s) => {
            if !matches!(s.kind, StmtKind::While { .. } | StmtKind::For { .. }) {
                return None;
            }
            // first pass: which variables the loop shares with the code around it
            let mut probe = new_gen(Ty::Unit, true);
            probe.stmt(s)?;
            if probe.frees.is_empty() {
                // nothing to compute with: not worth it
                return None;
            }
            // second pass: those variables are the first registers
            let mut g = new_gen(Ty::Unit, true);
            g.preset = true;
            // a call could change a module variable the loop keeps in a register
            g.direct_globals = probe.has_call && probe.frees.iter().any(|f| f.global.is_some());
            let shared: Vec<&Free> = probe
                .frees
                .iter()
                .filter(|f| !g.direct_globals || f.global.is_none())
                .collect();
            for (k, f) in shared.into_iter().enumerate() {
                g.frees.push(Free {
                    reg: k as Reg,
                    assigned: false,
                    ..f.clone()
                });
            }
            g.top = g.frees.len();
            g.max = g.top;
            if !probe.consts.is_empty() {
                for &(bits, _) in &probe.consts {
                    let r = g.alloc()?;
                    g.consts.push((bits, r));
                    g.emit(Op::ConstI(r, bits), s.span);
                }
                g.consts_ready = true;
            }
            g.stmt(s)?;
            g.emit(Op::RetUnit, s.span);
            let frees_tys: Vec<Ty> = g.frees.iter().map(|f| f.ty.clone()).collect();
            let uses_objs = uses_objects(&g.ops, &frees_tys, &g.sites, &g.globals);
            Some((
                Code {
                    name: "<loop>".to_string(),
                    nregs: g.max + 1,
                    ops: g.ops,
                    spans: g.spans,
                    param_tys: Vec::new(),
                    ret: Ty::Unit,
                    sites: g.sites,
                    globals: g.globals,
                    strs: g.strs,
                    fmts: g.fmts,
                    fields: g.fields,
                    lits: g.lits,
                    frees: g.frees,
                    is_loop: true,
                    uses_objs,
                },
                g.callees,
            ))
        }
    }
}

/// What goes into a register: a number (bits), or an object for the object registers.
enum Slot {
    N(i64),
    O(Value),
}

/// The slot for `v`, a value of type `t`.
fn slot_of(v: &Value, t: &Ty) -> Option<Slot> {
    match (v, t) {
        (Value::Int(i), Ty::Int) => Some(Slot::N(*i)),
        (Value::Float(f), Ty::Float) => Some(Slot::N(f.to_bits() as i64)),
        (Value::Bool(b), Ty::Bool) => Some(Slot::N(i64::from(*b))),
        (Value::Array(_), Ty::Array(_))
        | (Value::Str(_), Ty::Str)
        | (Value::Struct(_), Ty::Struct(..))
        | (Value::Map(_), Ty::Map(..)) => Some(Slot::O(v.clone())),
        // `T?`: `none`, or a `T` (always in an object register, a number too)
        (Value::None, Ty::Nullable(_)) => Some(Slot::O(Value::None)),
        (_, Ty::Nullable(inner)) => slot_of(v, inner).map(|_| Slot::O(v.clone())),
        _ => None,
    }
}

/// The value register `i` holds, of type `t`.
fn reg_value(stack: &[i64], ostack: &[Value], i: usize, t: &Ty) -> Value {
    match t {
        Ty::Int => Value::Int(stack[i]),
        Ty::Float => Value::Float(f64::from_bits(stack[i] as u64)),
        Ty::Bool => Value::Bool(stack[i] != 0),
        Ty::Array(_) | Ty::Str | Ty::Struct(..) | Ty::Nullable(_) | Ty::Map(..) => {
            ostack[i].clone()
        }
        _ => Value::Unit,
    }
}

/// Puts `v`, of type `t`, in register `i`; `false` when it has another shape.
fn set_reg(stack: &mut [i64], ostack: &mut [Value], i: usize, v: &Value, t: &Ty) -> bool {
    match slot_of(v, t) {
        Some(Slot::N(b)) => stack[i] = b,
        Some(Slot::O(o)) => ostack[i] = o,
        None => return false,
    }
    true
}

impl Interp<'_> {
    fn vm_release(&mut self) {
        self.vm_ostack.clear();
        self.vm_result = Value::Unit;
    }

    /// Calls `fid` on the machine when it is compiled (compiling it now if it is the first
    /// time). `None` when it cannot run there: the tree walker does it.
    #[inline(always)]
    pub(super) fn try_vm(
        &mut self,
        fid: FnId,
        this: &Option<Value>,
        args: &[Value],
        span: Span,
    ) -> Option<R<Value>> {
        // the common refusal, without a call: a function known not to compile
        if !self.vm_enabled || matches!(self.vm_state.get(fid), Some(VmState::No)) {
            return None;
        }
        self.try_vm_slow(fid, this, args, span)
    }

    #[inline(never)]
    fn try_vm_slow(
        &mut self,
        fid: FnId,
        this: &Option<Value>,
        args: &[Value],
        span: Span,
    ) -> Option<R<Value>> {
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
        // a method takes its `self` first
        let all: Vec<&Value> = this.iter().chain(args.iter()).collect();
        if all.len() != code.param_tys.len() || this.is_some() != self.prog.defs.fns[fid].has_self {
            return None;
        }
        self.vm_release();
        let mut init: Vec<(Reg, Slot)> = Vec::with_capacity(all.len());
        for (k, (a, t)) in all.iter().zip(&code.param_tys).enumerate() {
            init.push((k as Reg, slot_of(a, t)?));
        }
        let r = self.vm_run(idx, init, span);
        let out = r.map(|bits| match &code.ret {
            Ty::Int => Value::Int(bits),
            Ty::Float => Value::Float(f64::from_bits(bits as u64)),
            Ty::Bool => Value::Bool(bits != 0),
            Ty::Array(_) | Ty::Str | Ty::Struct(..) | Ty::Nullable(_) | Ty::Map(..) => {
                std::mem::replace(&mut self.vm_result, Value::Unit)
            }
            _ => Value::Unit,
        });
        self.vm_release();
        Some(out)
    }

    /// Compiles the function `fid` and everything it calls.
    fn vm_compile(&mut self, fid: FnId) -> Option<u32> {
        let r = self.vm_compile_unit(Unit::Fn(fid));
        if r.is_none() {
            self.vm_state[fid] = VmState::No;
        }
        r
    }

    /// Compiles a function or a loop, and every function it calls (a call to a function that
    /// is not plain numeric code makes the whole group fall back to the tree walker).
    fn vm_compile_unit(&mut self, root: Unit) -> Option<u32> {
        let prog = self.prog;
        // functions known not to compile are called through the interpreter
        let mut deny: Vec<FnId> = self
            .vm_state
            .iter()
            .enumerate()
            .filter(|(_, st)| matches!(st, VmState::No))
            .map(|(f, _)| f)
            .collect();
        let mut batch: Vec<(Option<FnId>, Code)>;
        'retry: loop {
            batch = Vec::new();
            let mut pending: Vec<Unit> = vec![root];
            let mut seen: Vec<FnId> = Vec::new();
            if let Unit::Fn(f) = root {
                seen.push(f);
            }
            while let Some(unit) = pending.pop() {
                let Some((code, callees)) = gen_unit(prog, unit, &deny) else {
                    match unit {
                        // a function the root calls cannot be compiled: the root calls it
                        // through the interpreter instead
                        Unit::Fn(f) if !matches!(root, Unit::Fn(r) if r == f) => {
                            if self.vm_state.len() <= f {
                                self.vm_state.resize(f + 1, VmState::Unknown);
                            }
                            self.vm_state[f] = VmState::No;
                            deny.push(f);
                            continue 'retry;
                        }
                        _ => return None,
                    }
                };
                for c in &callees {
                    if !seen.contains(c) && !matches!(self.vm_state.get(*c), Some(VmState::Yes(_)))
                    {
                        seen.push(*c);
                        pending.push(Unit::Fn(*c));
                    }
                }
                batch.push((
                    match unit {
                        Unit::Fn(f) => Some(f),
                        Unit::Loop(_) => None,
                    },
                    code,
                ));
            }
            break;
        }
        if std::env::var("QLANG_VM_DUMP").is_ok() {
            for (f, c) in &batch {
                eprintln!("== {:?} {} frees={}", f, c.name, c.frees.len());
                for (i, op) in c.ops.iter().enumerate() {
                    eprintln!("{i:3} {op:?}");
                }
            }
        }
        // number the group, then point every call at its target
        let first = self.vm_codes.len() as u32;
        let mut index: HashMap<FnId, u32> = HashMap::new();
        for (k, (f, _)) in batch.iter().enumerate() {
            if let Some(f) = f {
                index.insert(*f, first + k as u32);
            }
        }
        for (_, code) in &mut batch {
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
        }
        let mut root_index = first;
        for (k, (f, code)) in batch.into_iter().enumerate() {
            let i = self.vm_codes.len() as u32;
            if k == 0 {
                root_index = i;
            }
            if let Some(f) = f {
                if self.vm_state.len() <= f {
                    self.vm_state.resize(f + 1, VmState::Unknown);
                }
                self.vm_state[f] = VmState::Yes(i);
            }
            self.vm_codes.push(Rc::new(code));
        }
        Some(root_index)
    }

    /// Runs a `while` or `for` statement on the machine when it is a numeric loop: the
    /// variables it shares with the code around it are loaded into registers, and the ones it
    /// assigns are written back at the end. `None` when it cannot (the tree walker runs it).
    pub(super) fn try_vm_loop(&mut self, s: &Stmt, env: &Env) -> Option<R<Value>> {
        if !self.vm_enabled {
            return None;
        }
        let key = s as *const Stmt as usize;
        let idx = match self.vm_loops.get(&key) {
            Some(None) => return None,
            Some(Some(i)) => *i,
            None => {
                let r = self.vm_compile_unit(Unit::Loop(s));
                self.vm_loops.insert(key, r);
                r?
            }
        };
        let code = self.vm_codes[idx as usize].clone();
        self.vm_release();
        let mut init: Vec<(Reg, Slot)> = Vec::with_capacity(code.frees.len());
        for f in &code.frees {
            let v = match f.global {
                Some(m) => self.envs[m].get(&f.name),
                None => env.get(&f.name),
            };
            match v.and_then(|v| slot_of(&v, &f.ty)) {
                Some(slot) => init.push((f.reg, slot)),
                None => {
                    self.vm_release();
                    return None;
                }
            }
        }
        if let Err(e) = self.vm_run(idx, init, s.span) {
            self.vm_release();
            return Some(Err(e));
        }
        for f in code.frees.iter().filter(|f| f.assigned) {
            let v = reg_value(&self.vm_stack, &self.vm_ostack, f.reg as usize, &f.ty);
            match f.global {
                Some(m) => self.envs[m].assign(&f.name, v),
                None => env.assign(&f.name, v),
            };
        }
        self.vm_release();
        Some(Ok(Value::Unit))
    }

    fn vm_run(&mut self, root: u32, init: Vec<(Reg, Slot)>, span: Span) -> R<i64> {
        let mut stack = std::mem::take(&mut self.vm_stack);
        let mut ostack = std::mem::take(&mut self.vm_ostack);
        stack.clear();
        ostack.clear();
        let code = self.vm_codes[root as usize].clone();
        let n = code.nregs.max(init.len());
        stack.resize(n, 0);
        if code.uses_objs {
            ostack.resize(n, Value::Unit);
        }
        for (r, slot) in init {
            match slot {
                Slot::N(b) => stack[r as usize] = b,
                Slot::O(o) => ostack[r as usize] = o,
            }
        }
        let mut frames: Vec<Frame> = Vec::new();
        if self.depth >= self.limits.max_depth {
            self.vm_stack = stack;
            self.vm_ostack = ostack;
            return self.fail("R901", "too many nested calls (infinite recursion?)", span);
        }
        // a loop is not a call: it shows no frame of its own in the trace of an error
        let own = usize::from(code.is_loop);
        if own == 0 {
            self.depth += 1;
            self.frames.push((Name::new(&code.name), span));
        }
        let r = self.vm_loop(&mut stack, &mut ostack, &mut frames, root);
        // leave the interpreter's own call bookkeeping as it was
        for _ in 0..frames.len() + usize::from(own == 0) {
            self.frames.pop();
            self.depth -= 1;
        }
        self.vm_stack = stack;
        self.vm_ostack = ostack;
        r
    }

    fn vm_loop(
        &mut self,
        stack: &mut Vec<i64>,
        ostack: &mut Vec<Value>,
        frames: &mut Vec<Frame>,
        root: u32,
    ) -> R<i64> {
        let mut cur = root;
        let mut code = self.vm_codes[cur as usize].clone();
        let mut pc = 0usize;
        let mut base = 0usize;
        macro_rules! r {
            ($x:expr) => {
                stack[base + $x as usize]
            };
        }
        macro_rules! o {
            ($x:expr) => {
                ostack[base + $x as usize]
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
            match op {
                Op::ConstI(d, v) => r!(d) = v,
                Op::Mov(d, a) => r!(d) = r!(a),
                Op::MovO(d, a) => o!(d) = o!(a).clone(),
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
                    let (x, y) = (r!(a), r!(b));
                    if y == 0 {
                        fail!("R011", "division by zero");
                    }
                    // two small non-negative numbers: the 32-bit division is much faster
                    if (x | y) as u64 >> 31 == 0 {
                        r!(d) = i64::from(x as u32 / y as u32);
                    } else {
                        match x.checked_div(y) {
                            Some(v) => r!(d) = v,
                            None => fail!("R010", "integer overflow"),
                        }
                    }
                }
                Op::ModI(d, a, b) => {
                    let (x, y) = (r!(a), r!(b));
                    if y == 0 {
                        fail!("R011", "division by zero");
                    }
                    if (x | y) as u64 >> 31 == 0 {
                        r!(d) = i64::from(x as u32 % y as u32);
                    } else {
                        match x.checked_rem(y) {
                            Some(v) => r!(d) = v,
                            None => fail!("R010", "integer overflow"),
                        }
                    }
                }
                Op::DivII(d, a, b) => {
                    if r!(b) == 0 {
                        fail!("R011", "division by zero");
                    }
                    r!(d) = ((r!(a) as f64) / (r!(b) as f64)).to_bits() as i64;
                }
                Op::PowI(d, a, b) => {
                    if r!(b) < 0 {
                        fail!(
                            "R012",
                            "negative exponent: use decimals (`2.0 ** -1`) for fractions"
                        );
                    }
                    match u32::try_from(r!(b)).ok().and_then(|e| r!(a).checked_pow(e)) {
                        Some(v) => r!(d) = v,
                        None => fail!("R010", "integer overflow"),
                    }
                }
                Op::PowF(d, a, b) => r!(d) = fl!(a).powf(fl!(b)).to_bits() as i64,
                Op::ItoF(d, a) => r!(d) = (r!(a) as f64).to_bits() as i64,
                Op::FtoI(d, a) => {
                    let f = fl!(a);
                    if !f.is_finite() || f >= 9.3e18 || f <= -9.3e18 {
                        fail!("R013", format!("cannot convert {f} to an integer"));
                    }
                    r!(d) = f.trunc() as i64;
                }
                Op::AbsI(d, a) => match r!(a).checked_abs() {
                    Some(v) => r!(d) = v,
                    None => fail!("R010", "integer overflow"),
                },
                Op::AbsF(d, a) => r!(d) = fl!(a).abs().to_bits() as i64,
                Op::SqrtI(d, a) => {
                    if r!(a) < 0 {
                        fail!("R014", "square root of a negative number");
                    }
                    r!(d) = (r!(a) as f64).sqrt().to_bits() as i64;
                }
                Op::SqrtF(d, a) => {
                    if fl!(a) < 0.0 {
                        fail!("R014", "square root of a negative number");
                    }
                    r!(d) = fl!(a).sqrt().to_bits() as i64;
                }
                Op::MinI(d, a, b) => r!(d) = r!(a).min(r!(b)),
                Op::MaxI(d, a, b) => r!(d) = r!(a).max(r!(b)),
                Op::MinF(d, a, b) => r!(d) = fl!(a).min(fl!(b)).to_bits() as i64,
                Op::MaxF(d, a, b) => r!(d) = fl!(a).max(fl!(b)).to_bits() as i64,
                Op::LoadG { dst, g } => {
                    let gr = &code.globals[g as usize];
                    let mut c = gr.slot.get();
                    let v = self.envs[gr.module].get_cached(&gr.name, &mut c);
                    gr.slot.set(c);
                    let Some(v) = v else {
                        fail!(
                            "R002",
                            format!("`{}` is used before it has a value", gr.name)
                        );
                    };
                    if !set_reg(stack, ostack, base + dst as usize, &v, &gr.ty) {
                        fail!("R001", "unexpected type of a variable");
                    }
                }
                Op::StoreG { src, g } => {
                    let gr = &code.globals[g as usize];
                    let value = reg_value(stack, ostack, base + src as usize, &gr.ty);
                    let mut c = gr.slot.get();
                    let ok = self.envs[gr.module].assign_cached(&gr.name, value, &mut c);
                    gr.slot.set(c);
                    if !ok {
                        fail!("R002", format!("`{}` has no value yet", gr.name));
                    }
                }
                Op::Method { .. }
                | Op::NewStruct { .. }
                | Op::CallFn { .. }
                | Op::CallMethod { .. }
                | Op::AddS(..)
                | Op::CmpS { .. }
                | Op::Format { .. }
                | Op::Builtin { .. }
                | Op::IndexG { .. }
                | Op::MapReq { .. }
                | Op::MapSet { .. }
                | Op::MapSnap { .. } => self.vm_rare(op, &code, pc, base, stack, ostack)?,
                Op::NewMap(dst) => o!(dst) = Value::Map(Rc::new(RefCell::new(MapObj::default()))),
                Op::ConstNone(dst) => o!(dst) = Value::None,
                Op::Box { dst, src, ety } => o!(dst) = elem_value(r!(src), ety),
                Op::Unbox { dst, src, ety } => match (&o!(src), ety) {
                    (Value::Int(i), Ety::I) => r!(dst) = *i,
                    (Value::Float(f), Ety::F) => r!(dst) = f.to_bits() as i64,
                    (Value::Bool(b), Ety::B) => r!(dst) = i64::from(*b),
                    (Value::None, _) => fail!("R006", "the value is `none`"),
                    _ => fail!("R001", "a value of an unexpected type"),
                },
                Op::IsNone(d, a) => r!(d) = i64::from(matches!(o!(a), Value::None)),
                Op::ALen(d, a) => {
                    let Value::Array(rc) = &ostack[base + a as usize] else {
                        fail!("R001", "an array was expected");
                    };
                    r!(d) = rc.borrow().len() as i64;
                }
                Op::ALoad { dst, arr, idx, ety } => {
                    let i = r!(idx);
                    let Value::Array(rc) = &ostack[base + arr as usize] else {
                        fail!("R001", "an array was expected");
                    };
                    let a = rc.borrow();
                    match usize::try_from(i).ok().and_then(|u| a.get(u)) {
                        Some(v) => match elem_bits(v, ety) {
                            Some(b) => {
                                drop(a);
                                r!(dst) = b;
                            }
                            None => {
                                drop(a);
                                fail!("R001", "an element of an unexpected type");
                            }
                        },
                        None => {
                            let len = a.len();
                            drop(a);
                            fail!("R020", format!("index {i} is out of bounds (length {len})"));
                        }
                    }
                }
                Op::ACheck { arr, idx } => {
                    let i = r!(idx);
                    let Value::Array(rc) = &ostack[base + arr as usize] else {
                        fail!("R001", "an array was expected");
                    };
                    let len = rc.borrow().len();
                    if usize::try_from(i).ok().filter(|u| *u < len).is_none() {
                        fail!("R020", format!("index {i} is out of bounds (length {len})"));
                    }
                }
                Op::AStore { arr, idx, src, ety } => {
                    let (i, v) = (r!(idx), elem_value(r!(src), ety));
                    let Value::Array(rc) = &ostack[base + arr as usize] else {
                        fail!("R001", "an array was expected");
                    };
                    let mut a = rc.borrow_mut();
                    let len = a.len();
                    match usize::try_from(i).ok().filter(|u| *u < len) {
                        Some(u) => a[u] = v,
                        None => {
                            drop(a);
                            fail!("R020", format!("index {i} is out of bounds (length {len})"));
                        }
                    }
                }
                Op::ConstS(dst, i) => o!(dst) = code.strs[i as usize].clone(),
                Op::Field { dst, obj, fld } => {
                    let site = &code.fields[fld as usize];
                    let v = match &o!(obj) {
                        Value::Struct(st) => st.fields.borrow().get(&site.name).cloned(),
                        _ => None,
                    };
                    let Some(v) = v else {
                        fail!("R005", format!("this value has no field `{}`", site.name));
                    };
                    if !set_reg(stack, ostack, base + dst as usize, &v, &site.ty) {
                        fail!("R001", "a field of an unexpected type");
                    }
                }
                Op::SetField { obj, src, fld } => {
                    let site = &code.fields[fld as usize];
                    let v = reg_value(stack, ostack, base + src as usize, &site.ty);
                    match &o!(obj) {
                        Value::Struct(st) => {
                            st.fields.borrow_mut().insert(&site.name, v);
                        }
                        _ => fail!("R005", format!("this value has no field `{}`", site.name)),
                    }
                }
                Op::NewArr(dst) => {
                    o!(dst) = Value::Array(Rc::new(RefCell::new(Vec::new())));
                }
                Op::ALoadO { dst, arr, idx } => {
                    let i = r!(idx);
                    let got = {
                        let Value::Array(rc) = &o!(arr) else {
                            fail!("R001", "an array was expected");
                        };
                        let a = rc.borrow();
                        match usize::try_from(i).ok().and_then(|u| a.get(u)) {
                            Some(v) => Ok(v.clone()),
                            None => Err(a.len()),
                        }
                    };
                    match got {
                        Ok(v) => o!(dst) = v,
                        Err(len) => {
                            fail!("R020", format!("index {i} is out of bounds (length {len})"))
                        }
                    }
                }
                Op::AStoreO { arr, idx, src } => {
                    let (i, v) = (r!(idx), o!(src).clone());
                    let Value::Array(rc) = &ostack[base + arr as usize] else {
                        fail!("R001", "an array was expected");
                    };
                    let mut a = rc.borrow_mut();
                    let len = a.len();
                    match usize::try_from(i).ok().filter(|u| *u < len) {
                        Some(u) => a[u] = v,
                        None => {
                            drop(a);
                            fail!("R020", format!("index {i} is out of bounds (length {len})"));
                        }
                    }
                }
                Op::APushO { arr, src } => {
                    let v = o!(src).clone();
                    let Value::Array(rc) = &ostack[base + arr as usize] else {
                        fail!("R001", "an array was expected");
                    };
                    let rc = rc.clone();
                    let len = rc.borrow().len();
                    if len + 1 > self.limits.max_alloc {
                        fail!("R903", "a value became too large (memory limit reached)");
                    }
                    rc.borrow_mut().push(v);
                }
                Op::ForArrO {
                    dst,
                    arr,
                    idx,
                    exit,
                } => {
                    let i = r!(idx);
                    let got = {
                        let Value::Array(rc) = &o!(arr) else {
                            fail!("R001", "an array was expected");
                        };
                        rc.borrow().get(i as usize).cloned()
                    };
                    match got {
                        Some(v) => o!(dst) = v,
                        None => pc = exit as usize,
                    }
                }
                Op::APush { arr, src, ety } => {
                    let v = elem_value(r!(src), ety);
                    let Value::Array(rc) = &ostack[base + arr as usize] else {
                        fail!("R001", "an array was expected");
                    };
                    let rc = rc.clone();
                    let len = rc.borrow().len();
                    if len + 1 > self.limits.max_alloc {
                        fail!("R903", "a value became too large (memory limit reached)");
                    }
                    rc.borrow_mut().push(v);
                }
                Op::ForArr {
                    dst,
                    arr,
                    idx,
                    exit,
                    ety,
                } => {
                    let i = r!(idx);
                    let Value::Array(rc) = &ostack[base + arr as usize] else {
                        fail!("R001", "an array was expected");
                    };
                    let a = rc.borrow();
                    match a.get(i as usize) {
                        Some(v) => match elem_bits(v, ety) {
                            Some(b) => {
                                drop(a);
                                r!(dst) = b;
                            }
                            None => {
                                drop(a);
                                fail!("R001", "an element of an unexpected type");
                            }
                        },
                        None => {
                            drop(a);
                            pc = exit as usize;
                        }
                    }
                }
                Op::IncI(a) => r!(a) += 1,
                Op::ForNext { cnt, step, exit } => match r!(cnt).checked_add(r!(step)) {
                    Some(v) => r!(cnt) = v,
                    None => pc = exit as usize,
                },
                Op::StepCheck(st) => {
                    if r!(st) == 0 {
                        fail!("R031", "`step` cannot be 0");
                    }
                }
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
                    if (t as usize) < pc {
                        // a step is counted for each turn of a loop and each call
                        self.steps += 4;
                        if self.steps > self.limits.max_steps {
                            fail!(
                                "R900",
                                "the program ran for too long (step limit reached: infinite loop?)"
                            );
                        }
                        // the machine allocates (arrays grow): look at the memory now and then
                        if self.steps & 0x3FFF < 64 {
                            self.check_memory(code.spans[pc - 1])?;
                        }
                    }
                    pc = t as usize;
                }
                Op::JnLtI(a, b, t) => {
                    if r!(a) >= r!(b) {
                        pc = t as usize;
                    }
                }
                Op::JnLeI(a, b, t) => {
                    if r!(a) > r!(b) {
                        pc = t as usize;
                    }
                }
                Op::JnGtI(a, b, t) => {
                    if r!(a) <= r!(b) {
                        pc = t as usize;
                    }
                }
                Op::JnGeI(a, b, t) => {
                    if r!(a) < r!(b) {
                        pc = t as usize;
                    }
                }
                Op::JnEqI(a, b, t) => {
                    if r!(a) != r!(b) {
                        pc = t as usize;
                    }
                }
                Op::JnNeI(a, b, t) => {
                    if r!(a) == r!(b) {
                        pc = t as usize;
                    }
                }
                Op::JnF { a, b, t, cmp } => {
                    let (x, y) = (fl!(a), fl!(b));
                    let holds = match cmp {
                        Cmp::Lt => x < y,
                        Cmp::Le => x <= y,
                        Cmp::Gt => x > y,
                        Cmp::Ge => x >= y,
                        Cmp::Eq => x == y,
                        Cmp::Ne => x != y,
                    };
                    if !holds {
                        pc = t as usize;
                    }
                }
                Op::ForCheck {
                    cnt,
                    lim,
                    step,
                    incl,
                    exit,
                } => {
                    let (i, end, st) = (r!(cnt), r!(lim), r!(step));
                    let inside = if st > 0 {
                        if incl { i <= end } else { i < end }
                    } else if incl {
                        i >= end
                    } else {
                        i > end
                    };
                    if !inside {
                        pc = exit as usize;
                    }
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
                    self.steps += 4;
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
                    if callee.uses_objs && ostack.len() < base + callee.nregs {
                        ostack.resize(base + callee.nregs + 64, Value::Unit);
                    }
                    self.depth += 1;
                    self.frames.push((Name::new(&callee.name), span));
                    cur = target;
                    code = callee;
                    pc = 0;
                }
                Op::Ret(_) | Op::RetO(_) | Op::RetUnit => {
                    let v = match op {
                        Op::Ret(a) => r!(a),
                        _ => 0,
                    };
                    let obj = match op {
                        Op::RetO(a) => Some(std::mem::replace(&mut o!(a), Value::Unit)),
                        _ => None,
                    };
                    match frames.pop() {
                        None => {
                            if let Some(o) = obj {
                                self.vm_result = o;
                            }
                            return Ok(v);
                        }
                        Some(f) => {
                            // the objects of the frame that ends are released
                            if code.uses_objs {
                                for x in &mut ostack[base..base + code.nregs] {
                                    *x = Value::Unit;
                                }
                            }
                            self.frames.pop();
                            self.depth -= 1;
                            cur = f.code;
                            code = self.vm_codes[cur as usize].clone();
                            pc = f.pc;
                            base = f.base;
                            match obj {
                                Some(o) => o!(f.dst) = o,
                                None => r!(f.dst) = v,
                            }
                        }
                    }
                }
            }
        }
    }
}

impl Interp<'_> {
    /// The instructions that are rare or heavy, out of the dispatch loop so that it stays small.
    #[inline(never)]
    fn vm_rare(
        &mut self,
        op: Op,
        code: &Code,
        pc: usize,
        base: usize,
        stack: &mut [i64],
        ostack: &mut [Value],
    ) -> R<()> {
        macro_rules! r {
            ($x:expr) => {
                stack[base + $x as usize]
            };
        }
        macro_rules! o {
            ($x:expr) => {
                ostack[base + $x as usize]
            };
        }
        macro_rules! fail {
            ($c:expr, $m:expr) => {
                return self.fail($c, $m, code.spans[pc - 1])
            };
        }
        match op {
            Op::Method { dst, base: b, site } => {
                let span = code.spans[pc - 1];
                let st = &code.sites[site as usize];
                let at = base + b as usize;
                let recv = reg_value(stack, ostack, at, &st.tys[0]);
                let argv: Vec<Value> = st.tys[1..]
                    .iter()
                    .enumerate()
                    .map(|(i, t)| reg_value(stack, ostack, at + 1 + i, t))
                    .collect();
                let name = st.name.clone();
                let ret = st.ret.clone();
                self.cur_call = st.node;
                let out = match self.builtin_method(&recv, &name, &argv, span)? {
                    Some(v) => v,
                    None => return self.fail("R008", "this value has no such method", span),
                };
                if ret != Ty::Unit && !set_reg(stack, ostack, base + dst as usize, &out, &ret) {
                    return self.fail("R001", "unexpected result of a method", span);
                }
            }
            Op::NewStruct { dst, base: b, lit } => {
                let site = &code.lits[lit as usize];
                let prog = self.prog;
                let mut fields = SmallMap::default();
                for (k, (name, t)) in site.names.iter().enumerate() {
                    fields.insert(name, reg_value(stack, ostack, base + b as usize + k, t));
                }
                for (asid, _) in prog.defs.ancestry(site.sid, &[]) {
                    let sd = &prog.defs.structs[asid];
                    for f in &sd.fields {
                        if f.is_static || fields.contains_key(&f.name) {
                            continue;
                        }
                        if let Some(d) = &f.default {
                            let scope = Scope::new(Some(self.envs[sd.module].clone()));
                            let v = self.eval(d, &scope)?;
                            fields.insert(&f.name, v);
                        }
                    }
                }
                o!(dst) = Value::Struct(Rc::new(StructObj {
                    def: site.sid,
                    fields: RefCell::new(fields),
                }));
            }
            Op::CallFn { dst, base: b, site } => {
                let span = code.spans[pc - 1];
                let st = &code.sites[site as usize];
                let at = base + b as usize;
                let argv: Vec<Value> = st
                    .tys
                    .iter()
                    .enumerate()
                    .map(|(i, t)| reg_value(stack, ostack, at + i, t))
                    .collect();
                let Some(fid) = st.fid else {
                    fail!("R001", "a call without a function");
                };
                let out = self.call_fid(fid, None, argv, span)?;
                if st.ret != Ty::Unit && !set_reg(stack, ostack, base + dst as usize, &out, &st.ret)
                {
                    fail!("R001", "unexpected result of a function");
                }
            }
            Op::CallMethod { dst, base: b, site } => {
                let span = code.spans[pc - 1];
                let st = &code.sites[site as usize];
                let at = base + b as usize;
                let recv = reg_value(stack, ostack, at, &st.tys[0]);
                let argv: Vec<Value> = st.tys[1..]
                    .iter()
                    .enumerate()
                    .map(|(i, t)| reg_value(stack, ostack, at + 1 + i, t))
                    .collect();
                self.cur_call = st.node;
                let Some(ident) = st.ident.as_ref() else {
                    fail!("R001", "a method call without a name");
                };
                let out = self.call_method(recv, ident, argv, span)?;
                if st.ret != Ty::Unit && !set_reg(stack, ostack, base + dst as usize, &out, &st.ret)
                {
                    fail!("R001", "unexpected result of a method");
                }
            }
            Op::AddS(d, a, b) => {
                let (Value::Str(x), Value::Str(y)) = (&o!(a), &o!(b)) else {
                    fail!("R001", "a string was expected");
                };
                let n = x.len() + y.len();
                let span = code.spans[pc - 1];
                self.guard_alloc(n, span)?;
                self.charge_bytes(n, span)?;
                let mut text = String::with_capacity(n);
                text.push_str(x);
                text.push_str(y);
                o!(d) = Value::str(&text);
            }
            Op::CmpS { dst, a, b, cmp } => {
                let (Value::Str(x), Value::Str(y)) = (&o!(a), &o!(b)) else {
                    fail!("R001", "a string was expected");
                };
                let ord = x.as_ref().cmp(y.as_ref());
                r!(dst) = i64::from(match cmp {
                    Cmp::Eq => ord.is_eq(),
                    Cmp::Ne => ord.is_ne(),
                    Cmp::Lt => ord.is_lt(),
                    Cmp::Le => ord.is_le(),
                    Cmp::Gt => ord.is_gt(),
                    Cmp::Ge => ord.is_ge(),
                });
            }
            Op::Format { dst, base: b, fmt } => {
                let span = code.spans[pc - 1];
                let mut text = String::new();
                let mut at = base + b as usize;
                for part in &code.fmts[fmt as usize] {
                    match part {
                        FmtPart::Lit(t) => text.push_str(t),
                        FmtPart::Arg(t) => {
                            let v = reg_value(stack, ostack, at, t);
                            at += 1;
                            text.push_str(&self.stringify(&v, span)?);
                        }
                    }
                }
                self.guard_alloc(text.len(), span)?;
                self.charge_bytes(text.len(), span)?;
                o!(dst) = Value::str(&text);
            }
            Op::Builtin { base: b, site } => {
                let span = code.spans[pc - 1];
                let st = &code.sites[site as usize];
                let v = reg_value(stack, ostack, base + b as usize, &st.tys[0]);
                let name = st.name.clone();
                self.builtin_fn(&name, vec![v], span)?;
            }
            Op::IndexG {
                dst,
                base: b,
                idx,
                site,
            } => {
                let span = code.spans[pc - 1];
                let st = &code.sites[site as usize];
                let at = base;
                let bv = reg_value(stack, ostack, at + b as usize, &st.tys[0]);
                let iv = reg_value(stack, ostack, at + idx as usize, &st.tys[1]);
                let out = self.index_get(&bv, &iv, span)?;
                if !set_reg(stack, ostack, base + dst as usize, &out, &st.ret) {
                    fail!("R001", "unexpected result of an index");
                }
            }
            Op::MapReq {
                dst,
                map,
                key,
                site,
            } => {
                let st = &code.sites[site as usize];
                let k = reg_value(stack, ostack, base + key as usize, &st.tys[1]);
                let found = match &o!(map) {
                    Value::Map(m) => m.borrow().get(&k).cloned(),
                    _ => None,
                };
                let Some(v) = found else {
                    fail!(
                        "R022",
                        "this key is not in the map (use `get(key, default)` or check `has(key)` first)"
                    );
                };
                if !set_reg(stack, ostack, base + dst as usize, &v, &st.ret) {
                    fail!("R001", "unexpected value in a map");
                }
            }
            Op::MapSnap { keys, vals, map } => {
                let span = code.spans[pc - 1];
                let entries: Vec<(Value, Value)> = match &o!(map) {
                    Value::Map(m) => {
                        self.charge(m.borrow().len(), span)?;
                        m.borrow().entries.clone()
                    }
                    _ => fail!("R001", "a map was expected"),
                };
                let (ks, vs): (Vec<Value>, Vec<Value>) = entries.into_iter().unzip();
                o!(keys) = Value::array(ks);
                o!(vals) = Value::array(vs);
            }
            Op::MapSet {
                map,
                key,
                val,
                site,
            } => {
                let span = code.spans[pc - 1];
                let st = &code.sites[site as usize];
                let k = reg_value(stack, ostack, base + key as usize, &st.tys[1]);
                let v = reg_value(stack, ostack, base + val as usize, &st.tys[2]);
                let m = match &o!(map) {
                    Value::Map(m) => m.clone(),
                    _ => fail!("R001", "a map was expected"),
                };
                let len = m.borrow().len();
                self.guard_alloc(len + 1, span)?;
                if !m.borrow_mut().insert(k, v) {
                    fail!("R023", "this value cannot be used as a map key");
                }
            }
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn an_instruction_stays_small() {
        // the dispatch loop reads one per step: it must fit in a quarter of a cache line
        assert!(
            std::mem::size_of::<super::Op>() <= 16,
            "{}",
            std::mem::size_of::<super::Op>()
        );
    }
}
