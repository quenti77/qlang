//! Native code for the hot functions that only compute with numbers (feature `jit`).
//!
//! A compiled function of the register machine whose instructions are all plain arithmetic,
//! comparisons, jumps and calls of other such functions is translated to machine code with
//! Cranelift. It has no effect on the outside (no global, no object, no print), so when the
//! native code meets anything that is not an ordinary result (an overflow, a division by zero,
//! a step or depth limit, a cancel) it only says so, and the machine runs the whole call again
//! from the start: the error, its line and its call stack are the machine's own.

use super::*;
use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::{self, AbiParam, Block, InstBuilder, MemFlagsData, types};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{FuncId, Linkage, Module, default_libcall_names};

/// How many calls make a function hot (`QLANG_JIT_HOT` changes it, for the tests).
fn hot() -> u32 {
    static HOT: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    *HOT.get_or_init(|| {
        std::env::var("QLANG_JIT_HOT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(300)
    })
}

/// What the native code shares with the machine (the fields are read and written by the
/// generated code, at these offsets).
#[repr(C)]
pub(in crate::interp) struct Ctx {
    pub steps: i64,
    pub max_steps: i64,
    pub depth: i64,
    pub max_depth: i64,
    pub bail: i64,
    pub deadline: Option<std::time::Instant>,
    pub max_alloc: usize,
}

/// How many helpers (see `Jit::helpers`).
const HELPERS: usize = 10;

const STEPS: i32 = 0;
const MAX_STEPS: i32 = 8;
const DEPTH: i32 = 16;
const MAX_DEPTH: i32 = 24;
const BAIL: i32 = 32;

/// A look at what stops a program from outside (the memory, a cancel, the time): `1` to leave.
extern "C" fn poll(ctx: *mut Ctx) -> i64 {
    // SAFETY: the pointer is the `Ctx` that the machine made for this run.
    let ctx = unsafe { &*ctx };
    let late = ctx.deadline.is_some_and(|d| std::time::Instant::now() > d);
    i64::from(crate::host::memory_exceeded() || crate::host::cancelled() || late)
}

/// `a ** b` for integers: `0` and the flag of the context raised when it overflows or `b < 0`.
extern "C" fn pow_i(ctx: *mut Ctx, a: i64, b: i64) -> i64 {
    match u32::try_from(b).ok().and_then(|e| a.checked_pow(e)) {
        Some(v) => v,
        None => {
            // SAFETY: the pointer is the `Ctx` that the machine made for this run.
            unsafe { (*ctx).bail = 1 };
            0
        }
    }
}

extern "C" fn pow_f(a: f64, b: f64) -> f64 {
    a.powf(b)
}

extern "C" fn rem_f(a: f64, b: f64) -> f64 {
    a % b
}

// Rust's `min` and `max` ignore a NaN, Cranelift's do not: the same functions as the machine's.
extern "C" fn min_f(a: f64, b: f64) -> f64 {
    a.min(b)
}

extern "C" fn max_f(a: f64, b: f64) -> f64 {
    a.max(b)
}

fn ety_from(n: i64) -> Ety {
    match n {
        0 => Ety::I,
        1 => Ety::F,
        _ => Ety::B,
    }
}

fn ety_num(e: Ety) -> i64 {
    match e {
        Ety::I => 0,
        Ety::F => 1,
        _ => 2,
    }
}

/// The array in the object register at `v`.
///
/// # Safety
/// `v` points to an object register of the run that is going on.
unsafe fn array_at<'a>(v: *const Value) -> Option<&'a ArrObj> {
    match unsafe { &*v } {
        Value::Array(rc) => Some(&**rc),
        _ => None,
    }
}

/// What is wrong is told through the flag of the context: the machine runs the instruction
/// again and says what it is.
fn fail(ctx: *mut Ctx) -> i64 {
    // SAFETY: the pointer is the `Ctx` that the machine made for this run.
    unsafe { (*ctx).bail = 1 };
    0
}

extern "C" fn h_alen(v: *const Value) -> i64 {
    // SAFETY: see `array_at`.
    match unsafe { array_at(v) } {
        Some(a) => a.raw().len() as i64,
        None => -1,
    }
}

extern "C" fn h_aload(ctx: *mut Ctx, v: *const Value, i: i64, ety: i64) -> i64 {
    // SAFETY: see `array_at`.
    let Some(a) = (unsafe { array_at(v) }) else {
        return fail(ctx);
    };
    match arr_get(&a.raw(), i, ety_from(ety)) {
        Elem::Ok(b) => b,
        _ => fail(ctx),
    }
}

extern "C" fn h_astore(ctx: *mut Ctx, v: *const Value, i: i64, bits: i64, ety: i64) -> i64 {
    // SAFETY: see `array_at`.
    let Some(a) = (unsafe { array_at(v) }) else {
        return fail(ctx);
    };
    match arr_set(&mut a.raw_mut(), i, bits, ety_from(ety)) {
        Elem::Ok(_) => 0,
        _ => fail(ctx),
    }
}

extern "C" fn h_acheck(ctx: *mut Ctx, v: *const Value, i: i64) -> i64 {
    // SAFETY: see `array_at`.
    let Some(a) = (unsafe { array_at(v) }) else {
        return fail(ctx);
    };
    if usize::try_from(i)
        .ok()
        .filter(|u| *u < a.raw().len())
        .is_none()
    {
        return fail(ctx);
    }
    0
}

extern "C" fn h_apush(ctx: *mut Ctx, v: *const Value, bits: i64, ety: i64) -> i64 {
    // SAFETY: see `array_at`; the context is the one of this run.
    let (Some(a), max) = (unsafe { array_at(v) }, unsafe { (*ctx).max_alloc }) else {
        return fail(ctx);
    };
    let mut arr = a.raw_mut();
    if arr.len() + 1 > max || !arr_push(&mut arr, bits, ety_from(ety)) {
        return fail(ctx);
    }
    0
}

pub(in crate::interp) struct Jit {
    module: JITModule,
    fb: FunctionBuilderContext,
    poll: FuncId,
    /// `pow_i`, `pow_f`, `rem_f`, `min_f`, `max_f`, then the five of the arrays
    helpers: [FuncId; HELPERS],
    ids: HashMap<u32, FuncId>,
}

fn arity_sig(module: &JITModule, n: usize) -> ir::Signature {
    let mut sig = module.make_signature();
    for _ in 0..=n {
        sig.params.push(AbiParam::new(types::I64));
    }
    sig.returns.push(AbiParam::new(types::I64));
    sig
}

impl Jit {
    /// `None` when the JIT is off (`QLANG_NO_JIT`) or the machine has no native target.
    pub(in crate::interp) fn new() -> Option<Jit> {
        if std::env::var_os("QLANG_NO_JIT").is_some() {
            return None;
        }
        let mut flags = settings::builder();
        flags.set("use_colocated_libcalls", "false").ok()?;
        flags.set("is_pic", "false").ok()?;
        flags.set("opt_level", "speed").ok()?;
        let isa = cranelift_native::builder()
            .ok()?
            .finish(settings::Flags::new(flags))
            .ok()?;
        let mut jb = JITBuilder::with_isa(isa, default_libcall_names());
        jb.symbol("qlang_jit_poll", poll as *const u8);
        jb.symbol("qlang_jit_pow_i", pow_i as *const u8);
        jb.symbol("qlang_jit_pow_f", pow_f as *const u8);
        jb.symbol("qlang_jit_rem_f", rem_f as *const u8);
        jb.symbol("qlang_jit_min_f", min_f as *const u8);
        jb.symbol("qlang_jit_max_f", max_f as *const u8);
        jb.symbol("qlang_jit_alen", h_alen as *const u8);
        jb.symbol("qlang_jit_aload", h_aload as *const u8);
        jb.symbol("qlang_jit_astore", h_astore as *const u8);
        jb.symbol("qlang_jit_acheck", h_acheck as *const u8);
        jb.symbol("qlang_jit_apush", h_apush as *const u8);
        let mut module = JITModule::new(jb);
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));
        let poll = module
            .declare_function("qlang_jit_poll", Linkage::Import, &sig)
            .ok()?;
        let mut ff = module.make_signature();
        for _ in 0..2 {
            ff.params.push(AbiParam::new(types::F64));
        }
        ff.returns.push(AbiParam::new(types::F64));
        let ints = |module: &JITModule, n: usize| {
            let mut sig = module.make_signature();
            for _ in 0..n {
                sig.params.push(AbiParam::new(types::I64));
            }
            sig.returns.push(AbiParam::new(types::I64));
            sig
        };
        let (i1, i3, i4, i5) = (
            ints(&module, 1),
            ints(&module, 3),
            ints(&module, 4),
            ints(&module, 5),
        );
        let mut decl = |name: &str, sig: &ir::Signature| {
            module.declare_function(name, Linkage::Import, sig).ok()
        };
        let helpers = [
            decl("qlang_jit_pow_i", &i3)?,
            decl("qlang_jit_pow_f", &ff)?,
            decl("qlang_jit_rem_f", &ff)?,
            decl("qlang_jit_min_f", &ff)?,
            decl("qlang_jit_max_f", &ff)?,
            decl("qlang_jit_alen", &i1)?,
            decl("qlang_jit_aload", &i4)?,
            decl("qlang_jit_astore", &i5)?,
            decl("qlang_jit_acheck", &i3)?,
            decl("qlang_jit_apush", &i4)?,
        ];
        Some(Jit {
            module,
            fb: FunctionBuilderContext::new(),
            poll,
            helpers,
            ids: HashMap::new(),
        })
    }

    /// Compiles `root` and the functions it calls. On success each of them has its native
    /// entry in `Code::jit`.
    pub(in crate::interp) fn compile(&mut self, root: u32, codes: &[Rc<Code>]) -> bool {
        let mut todo = vec![root];
        let mut set: Vec<u32> = Vec::new();
        while let Some(i) = todo.pop() {
            if self.ids.contains_key(&i) || set.contains(&i) {
                continue;
            }
            let Some(callees) = scan(&codes[i as usize], false) else {
                return false;
            };
            set.push(i);
            todo.extend(callees);
        }
        let mut declared = Vec::new();
        for &i in &set {
            let sig = arity_sig(&self.module, codes[i as usize].param_tys.len());
            let Ok(id) = self
                .module
                .declare_function(&format!("f{i}"), Linkage::Local, &sig)
            else {
                return false;
            };
            self.ids.insert(i, id);
            declared.push((i, id));
        }
        for &(i, id) in &declared {
            if self.define(id, &codes[i as usize], codes, false).is_none() {
                for (j, _) in &declared {
                    self.ids.remove(j);
                }
                return false;
            }
        }
        let mut shims = Vec::new();
        for &(i, id) in &declared {
            let Some(s) = self.shim(i, id, codes[i as usize].param_tys.len()) else {
                return false;
            };
            shims.push((i, s));
        }
        if self.module.finalize_definitions().is_err() {
            return false;
        }
        for (i, s) in shims {
            let p = self.module.get_finalized_function(s);
            codes[i as usize].jit.entry.set(p as usize);
        }
        true
    }

    /// Compiles `root` for an entry in the middle of its run (a loop that became hot), and the
    /// functions it calls. `false` when it cannot.
    pub(in crate::interp) fn compile_osr(&mut self, root: u32, codes: &[Rc<Code>]) -> bool {
        let Some(callees) = scan(&codes[root as usize], true) else {
            return false;
        };
        for c in callees {
            if !self.ids.contains_key(&c) && !self.compile(c, codes) {
                return false;
            }
        }
        let sig = arity_sig(&self.module, 3);
        let Ok(id) = self
            .module
            .declare_function(&format!("o{root}"), Linkage::Local, &sig)
        else {
            return false;
        };
        if self
            .define(id, &codes[root as usize], codes, true)
            .is_none()
            || self.module.finalize_definitions().is_err()
        {
            return false;
        }
        let p = self.module.get_finalized_function(id);
        codes[root as usize].jit.osr.set(p as usize);
        true
    }

    /// The function the machine calls: `(ctx, pointer to the arguments)`.
    fn shim(&mut self, i: u32, real: FuncId, n: usize) -> Option<FuncId> {
        let mut sig = self.module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));
        let id = self
            .module
            .declare_function(&format!("s{i}"), Linkage::Local, &sig)
            .ok()?;
        let mut cx = self.module.make_context();
        cx.func.signature = sig;
        {
            let mut b = FunctionBuilder::new(&mut cx.func, &mut self.fb);
            let e = b.create_block();
            b.append_block_params_for_function_params(e);
            b.switch_to_block(e);
            let ps = b.block_params(e).to_vec();
            let mut args = vec![ps[0]];
            for k in 0..n {
                args.push(
                    b.ins()
                        .load(types::I64, MemFlagsData::trusted(), ps[1], (k * 8) as i32),
                );
            }
            let f = self.module.declare_func_in_func(real, b.func);
            let call = b.ins().call(f, &args);
            let r = b.inst_results(call)[0];
            b.ins().return_(&[r]);
            b.seal_all_blocks();
            b.finalize(self.module.target_config());
        }
        self.module.define_function(id, &mut cx).ok()?;
        self.module.clear_context(&mut cx);
        Some(id)
    }

    fn define(&mut self, id: FuncId, code: &Code, codes: &[Rc<Code>], osr: bool) -> Option<()> {
        let mut cx = self.module.make_context();
        // a function takes its arguments; the entry in the middle of a run takes the registers
        // and the instruction to go on at
        cx.func.signature = arity_sig(&self.module, if osr { 3 } else { code.param_tys.len() });
        {
            let mut b = FunctionBuilder::new(&mut cx.func, &mut self.fb);
            let poll = self.module.declare_func_in_func(self.poll, b.func);
            let helpers = self
                .helpers
                .map(|h| self.module.declare_func_in_func(h, b.func));
            let entry = b.create_block();
            b.append_block_params_for_function_params(entry);
            b.switch_to_block(entry);
            let ps = b.block_params(entry).to_vec();
            let vars: Vec<Variable> = (0..code.nregs).map(|_| b.declare_var(types::I64)).collect();
            let em_vars = vars.clone();
            let zero = b.ins().iconst(types::I64, 0);
            for (r, v) in vars.iter().enumerate() {
                let init = if osr {
                    b.ins()
                        .load(types::I64, MemFlagsData::trusted(), ps[1], (r * 8) as i32)
                } else if r < code.param_tys.len() {
                    ps[r + 1]
                } else {
                    zero
                };
                b.def_var(*v, init);
            }
            let bail = b.create_block();
            let at = b.append_block_param(bail, types::I64);
            let mut em = Em {
                b: &mut b,
                ctx: ps[0],
                bail,
                poll,
                helpers,
                vars,
                module: &mut self.module,
                ids: &self.ids,
                codes,
                osr: osr.then(|| (ps[1], ps[2], ps[3])),
                at: 0,
            };
            em.body(code)?;
            let bail = em.bail;
            b.switch_to_block(bail);
            if osr {
                // the registers go back to memory, and the machine runs the instruction that
                // the native code could not (it gives the same error, at the same place)
                for (r, v) in em_vars.iter().enumerate() {
                    let x = b.use_var(*v);
                    b.ins()
                        .store(MemFlagsData::trusted(), x, ps[1], (r * 8) as i32);
                }
                b.ins().return_(&[at]);
            } else {
                let one = b.ins().iconst(types::I64, 1);
                b.ins().store(MemFlagsData::trusted(), one, ps[0], BAIL);
                let z = b.ins().iconst(types::I64, 0);
                b.ins().return_(&[z]);
            }
            b.seal_all_blocks();
            b.finalize(self.module.target_config());
        }
        self.module.define_function(id, &mut cx).ok()?;
        self.module.clear_context(&mut cx);
        Some(())
    }
}

/// The functions `code` calls when it can be compiled to native code; `None` otherwise.
fn scan(code: &Code, osr: bool) -> Option<Vec<u32>> {
    let scalar = |t: &Ty| matches!(t, Ty::Int | Ty::Float | Ty::Bool);
    if code.uses_objs && !osr {
        return None;
    }
    // a function called from native code: numbers in, a number out. The entry in the middle of
    // a run (a loop, or a function that is running) only needs the registers to be numbers.
    if !osr
        && (code.is_loop
            || !code.frees.is_empty()
            || !code.param_tys.iter().all(scalar)
            || !scalar(&code.ret))
    {
        return None;
    }
    let mut callees = Vec::new();
    for op in &code.ops {
        match *op {
            Op::ConstI(..)
            | Op::Mov(..)
            | Op::AddI(..)
            | Op::SubI(..)
            | Op::AddImm(..)
            | Op::SubImm(..)
            | Op::DivK(..)
            | Op::ModK(..)
            | Op::MulI(..)
            | Op::IntDivI(..)
            | Op::ModI(..)
            | Op::DivII(..)
            | Op::NegI(..)
            | Op::ItoF(..)
            | Op::FtoI(..)
            | Op::AbsI(..)
            | Op::AbsF(..)
            | Op::MinI(..)
            | Op::MaxI(..)
            | Op::MinF(..)
            | Op::MaxF(..)
            | Op::PowI(..)
            | Op::PowF(..)
            | Op::ModF(..)
            | Op::SqrtI(..)
            | Op::SqrtF(..)
            | Op::ForLoop { .. }
            | Op::ForCheck { .. }
            | Op::StepCheck(_)
            | Op::AddF(..)
            | Op::SubF(..)
            | Op::MulF(..)
            | Op::DivF(..)
            | Op::NegF(..)
            | Op::Not(..)
            | Op::LtI(..)
            | Op::LeI(..)
            | Op::GtI(..)
            | Op::GeI(..)
            | Op::EqI(..)
            | Op::NeI(..)
            | Op::LtF(..)
            | Op::LeF(..)
            | Op::GtF(..)
            | Op::GeF(..)
            | Op::EqF(..)
            | Op::NeF(..)
            | Op::Jmp(_)
            | Op::JnLtI(..)
            | Op::JnLeI(..)
            | Op::JnGtI(..)
            | Op::JnGeI(..)
            | Op::JnEqI(..)
            | Op::JnNeI(..)
            | Op::JnLtImm(..)
            | Op::JnLeImm(..)
            | Op::JnGtImm(..)
            | Op::JnGeImm(..)
            | Op::JnEqImm(..)
            | Op::JnNeImm(..)
            | Op::JBack { .. }
            | Op::JBackImm { .. }
            | Op::JnF { .. }
            | Op::JmpF(..)
            | Op::JmpT(..)
            | Op::IncI(_)
            | Op::Ret(_) => {}
            Op::RetUnit | Op::RetO(_) if osr => {}
            // arrays of numbers: through the machine's own functions (the entry in the middle
            // of a run only: the objects stay in their registers)
            Op::ALen(..) | Op::ACheck { .. } if osr => {}
            Op::ALoad { ety, .. }
            | Op::AStore { ety, .. }
            | Op::APush { ety, .. }
            | Op::ForArr { ety, .. }
                if osr && ety != Ety::O => {}
            Op::Call { code: t, .. } => callees.push(t),
            _ => return None,
        }
    }
    Some(callees)
}

struct Em<'a, 'b> {
    b: &'a mut FunctionBuilder<'b>,
    ctx: ir::Value,
    bail: Block,
    poll: ir::FuncRef,
    helpers: [ir::FuncRef; HELPERS],
    vars: Vec<Variable>,
    module: &'a mut JITModule,
    ids: &'a HashMap<u32, FuncId>,
    codes: &'a [Rc<Code>],
    /// For the entry in the middle of a run: the registers in memory and the instruction.
    osr: Option<(ir::Value, ir::Value, ir::Value)>,
    /// The instruction being translated: where the machine goes on when the native code leaves.
    at: usize,
}

impl Em<'_, '_> {
    fn get(&mut self, r: Reg) -> ir::Value {
        self.b.use_var(self.vars[r as usize])
    }

    fn set(&mut self, r: Reg, v: ir::Value) {
        self.b.def_var(self.vars[r as usize], v);
    }

    fn fget(&mut self, r: Reg) -> ir::Value {
        let v = self.get(r);
        self.b.ins().bitcast(types::F64, MemFlagsData::new(), v)
    }

    fn fset(&mut self, r: Reg, v: ir::Value) {
        let v = self.b.ins().bitcast(types::I64, MemFlagsData::new(), v);
        self.set(r, v);
    }

    fn bset(&mut self, r: Reg, c: ir::Value) {
        let v = self.b.ins().uextend(types::I64, c);
        self.set(r, v);
    }

    fn imm(&mut self, k: i64) -> ir::Value {
        self.b.ins().iconst(types::I64, k)
    }

    /// Leave the native code (to be run again by the machine) when `c` holds.
    fn bail_if(&mut self, c: ir::Value) {
        let go = self.b.create_block();
        let at = self.imm(self.at as i64);
        self.b.ins().brif(c, self.bail, &[at.into()], go, &[]);
        self.b.switch_to_block(go);
    }

    /// The steps after one more is counted; leaves when the limit is reached (the count is
    /// stored by `store_steps`, once nothing can stop the instruction any more).
    fn next_step(&mut self) -> ir::Value {
        let s = self
            .b
            .ins()
            .load(types::I64, MemFlagsData::trusted(), self.ctx, STEPS);
        let s = self.b.ins().iadd_imm_s(s, 4);
        let max = self
            .b
            .ins()
            .load(types::I64, MemFlagsData::trusted(), self.ctx, MAX_STEPS);
        let over = self.b.ins().icmp(IntCC::SignedGreaterThan, s, max);
        self.bail_if(over);
        s
    }

    fn store_steps(&mut self, s: ir::Value) {
        self.b
            .ins()
            .store(MemFlagsData::trusted(), s, self.ctx, STEPS);
    }

    /// A jump back: a step, and now and then a look at the outside.
    fn back_edge(&mut self) {
        let s = self.next_step();
        let low = self.b.ins().band_imm_s(s, 0x3FFF);
        let look = self.b.ins().icmp_imm_s(IntCC::UnsignedLessThan, low, 64);
        let poll = self.b.create_block();
        let go = self.b.create_block();
        self.b.ins().brif(look, poll, &[], go, &[]);
        self.b.switch_to_block(poll);
        let call = self.b.ins().call(self.poll, &[self.ctx]);
        let stop = self.b.inst_results(call)[0];
        let at = self.imm(self.at as i64);
        self.b.ins().brif(stop, self.bail, &[at.into()], go, &[]);
        self.b.switch_to_block(go);
        self.store_steps(s);
    }

    fn cmp_i(&mut self, cc: IntCC, a: Reg, b: Reg) -> ir::Value {
        let (x, y) = (self.get(a), self.get(b));
        self.b.ins().icmp(cc, x, y)
    }

    fn cmp_f(&mut self, cc: FloatCC, a: Reg, b: Reg) -> ir::Value {
        let (x, y) = (self.fget(a), self.fget(b));
        self.b.ins().fcmp(cc, x, y)
    }

    fn cmp_imm(&mut self, cc: IntCC, a: Reg, k: i32) -> ir::Value {
        let x = self.get(a);
        self.b.ins().icmp_imm_s(cc, x, i64::from(k))
    }

    /// Jump to `t` when `c` holds, else go on with the next instruction (`next`).
    fn branch(&mut self, c: ir::Value, t: Block, next: Block) {
        self.b.ins().brif(c, t, &[], next, &[]);
    }

    /// The address of the object register `r`.
    fn obj_addr(&mut self, r: Reg) -> Option<ir::Value> {
        let (_, _, objs) = self.osr?;
        Some(
            self.b
                .ins()
                .iadd_imm_s(objs, (usize::from(r) * std::mem::size_of::<Value>()) as i64),
        )
    }

    /// Calls a helper that tells it went wrong through the flag of the context, then leaves
    /// when it did.
    fn helper_checked(&mut self, h: usize, args: &[ir::Value]) -> ir::Value {
        let call = self.b.ins().call(self.helpers[h], args);
        let v = self.b.inst_results(call)[0];
        let flag = self
            .b
            .ins()
            .load(types::I64, MemFlagsData::trusted(), self.ctx, BAIL);
        self.bail_if(flag);
        v
    }

    /// An access to the array in the object register `arr`: `(ctx, array, indexes..., values...,
    /// kind of the elements)`.
    fn array_op(
        &mut self,
        h: usize,
        arr: Reg,
        idx: &[Reg],
        vals: &[Reg],
        ety: Ety,
    ) -> Option<ir::Value> {
        let p = self.obj_addr(arr)?;
        let mut args = vec![self.ctx, p];
        for r in idx.iter().chain(vals) {
            args.push(self.get(*r));
        }
        args.push(self.imm(ety_num(ety)));
        Some(self.helper_checked(h, &args))
    }

    /// `i` is inside the range of a `for` (the same test as the machine's).
    fn inside(&mut self, cnt: ir::Value, end: ir::Value, st: ir::Value, incl: bool) -> ir::Value {
        let pos = self.b.ins().icmp_imm_s(IntCC::SignedGreaterThan, st, 0);
        let (up, down) = if incl {
            (
                IntCC::SignedLessThanOrEqual,
                IntCC::SignedGreaterThanOrEqual,
            )
        } else {
            (IntCC::SignedLessThan, IntCC::SignedGreaterThan)
        };
        let u = self.b.ins().icmp(up, cnt, end);
        let d = self.b.ins().icmp(down, cnt, end);
        self.b.ins().select(pos, u, d)
    }

    fn body(&mut self, code: &Code) -> Option<()> {
        let n = code.ops.len();
        let mut starts = vec![false; n + 1];
        starts[0] = true;
        for (i, op) in code.ops.iter().enumerate() {
            let target = match *op {
                Op::Jmp(t) => Some(t),
                Op::JnLtI(_, _, t)
                | Op::JnLeI(_, _, t)
                | Op::JnGtI(_, _, t)
                | Op::JnGeI(_, _, t)
                | Op::JnEqI(_, _, t)
                | Op::JnNeI(_, _, t)
                | Op::JnLtImm(_, _, t)
                | Op::JnLeImm(_, _, t)
                | Op::JnGtImm(_, _, t)
                | Op::JnGeImm(_, _, t)
                | Op::JnEqImm(_, _, t)
                | Op::JnNeImm(_, _, t)
                | Op::JnF { t, .. }
                | Op::JmpF(_, t)
                | Op::JmpT(_, t)
                | Op::JBack { t, .. }
                | Op::JBackImm { t, .. } => Some(t),
                Op::ForCheck { exit, .. } | Op::ForArr { exit, .. } => Some(exit),
                Op::ForLoop { body, .. } => Some(body),
                Op::Ret(_) | Op::RetUnit | Op::RetO(_) => None,
                _ => continue,
            };
            if let Some(t) = target {
                starts[t as usize] = true;
            }
            starts[i + 1] = true;
        }
        let blocks: Vec<Option<Block>> = starts
            .iter()
            .map(|&s| s.then(|| self.b.create_block()))
            .collect();
        let mut dead = false;
        if let Some((_, pc, _)) = self.osr {
            // go on where the machine was: at the start of a loop body
            let mut entries = std::collections::BTreeSet::new();
            for (i, op) in code.ops.iter().enumerate() {
                match *op {
                    Op::Jmp(t) if (t as usize) <= i => {
                        entries.insert(t);
                    }
                    Op::JBack { t, .. } | Op::JBackImm { t, .. } => {
                        entries.insert(t);
                    }
                    Op::ForLoop { body, .. } => {
                        entries.insert(body);
                    }
                    _ => {}
                }
            }
            let mut sw = cranelift_frontend::Switch::new();
            for t in entries {
                sw.set_entry(u128::from(t), blocks[t as usize]?);
            }
            let stray = self.b.create_block();
            sw.emit(self.b, pc, stray);
            self.b.switch_to_block(stray);
            self.b.ins().jump(self.bail, &[pc.into()]);
            dead = true;
        }
        for (i, op) in code.ops.iter().enumerate() {
            self.at = i;
            if let Some(blk) = blocks[i] {
                if !dead {
                    self.b.ins().jump(blk, &[]);
                }
                self.b.switch_to_block(blk);
                dead = false;
            }
            if dead {
                continue;
            }
            let next = blocks[i + 1];
            let tgt = |t: u32| blocks[t as usize].unwrap();
            match *op {
                Op::ConstI(d, v) => {
                    let v = self.imm(v);
                    self.set(d, v);
                }
                Op::Mov(d, a) => {
                    let v = self.get(a);
                    self.set(d, v);
                }
                Op::AddI(d, a, b) => {
                    let (x, y) = (self.get(a), self.get(b));
                    let (v, o) = self.b.ins().sadd_overflow(x, y);
                    self.bail_if(o);
                    self.set(d, v);
                }
                Op::SubI(d, a, b) => {
                    let (x, y) = (self.get(a), self.get(b));
                    let (v, o) = self.b.ins().ssub_overflow(x, y);
                    self.bail_if(o);
                    self.set(d, v);
                }
                Op::MulI(d, a, b) => {
                    let (x, y) = (self.get(a), self.get(b));
                    let (v, o) = self.b.ins().smul_overflow(x, y);
                    self.bail_if(o);
                    self.set(d, v);
                }
                Op::AddImm(d, a, k) => {
                    let x = self.get(a);
                    let y = self.imm(i64::from(k));
                    let (v, o) = self.b.ins().sadd_overflow(x, y);
                    self.bail_if(o);
                    self.set(d, v);
                }
                Op::SubImm(d, a, k) => {
                    let x = self.get(a);
                    let y = self.imm(i64::from(k));
                    let (v, o) = self.b.ins().ssub_overflow(x, y);
                    self.bail_if(o);
                    self.set(d, v);
                }
                Op::IntDivI(d, a, b) | Op::ModI(d, a, b) => {
                    let (x, y) = (self.get(a), self.get(b));
                    let zero = self.b.ins().icmp_imm_s(IntCC::Equal, y, 0);
                    let m1 = self.b.ins().icmp_imm_s(IntCC::Equal, y, -1);
                    let min = self.b.ins().icmp_imm_s(IntCC::Equal, x, i64::MIN);
                    let ovf = self.b.ins().band(m1, min);
                    let bad = self.b.ins().bor(zero, ovf);
                    self.bail_if(bad);
                    let v = if matches!(op, Op::IntDivI(..)) {
                        self.b.ins().sdiv(x, y)
                    } else {
                        self.b.ins().srem(x, y)
                    };
                    self.set(d, v);
                }
                Op::DivK(d, a, i) => {
                    let x = self.get(a);
                    let k = self.imm(i64::from(code.divs[i as usize].0));
                    let v = self.b.ins().sdiv(x, k);
                    self.set(d, v);
                }
                Op::ModK(d, a, i) => {
                    let x = self.get(a);
                    let k = self.imm(i64::from(code.divs[i as usize].0));
                    let v = self.b.ins().srem(x, k);
                    self.set(d, v);
                }
                Op::DivII(d, a, b) => {
                    let (x, y) = (self.get(a), self.get(b));
                    let zero = self.b.ins().icmp_imm_s(IntCC::Equal, y, 0);
                    self.bail_if(zero);
                    let fx = self.b.ins().fcvt_from_sint(types::F64, x);
                    let fy = self.b.ins().fcvt_from_sint(types::F64, y);
                    let v = self.b.ins().fdiv(fx, fy);
                    self.fset(d, v);
                }
                Op::NegI(d, a) | Op::AbsI(d, a) => {
                    let x = self.get(a);
                    let min = self.b.ins().icmp_imm_s(IntCC::Equal, x, i64::MIN);
                    self.bail_if(min);
                    let neg = self.b.ins().ineg(x);
                    let v = if matches!(op, Op::NegI(..)) {
                        neg
                    } else {
                        self.b.ins().iabs(x)
                    };
                    self.set(d, v);
                }
                Op::ItoF(d, a) => {
                    let x = self.get(a);
                    let v = self.b.ins().fcvt_from_sint(types::F64, x);
                    self.fset(d, v);
                }
                Op::FtoI(d, a) => {
                    let f = self.fget(a);
                    let abs = self.b.ins().fabs(f);
                    let lim = self.b.ins().f64const(9.3e18);
                    let ok = self.b.ins().fcmp(FloatCC::LessThan, abs, lim);
                    let bad = self.b.ins().bxor_imm_s(ok, 1);
                    self.bail_if(bad);
                    let v = self.b.ins().fcvt_to_sint_sat(types::I64, f);
                    self.set(d, v);
                }
                Op::PowI(d, a, b) => {
                    let (x, y) = (self.get(a), self.get(b));
                    let call = self.b.ins().call(self.helpers[0], &[self.ctx, x, y]);
                    let v = self.b.inst_results(call)[0];
                    let flag =
                        self.b
                            .ins()
                            .load(types::I64, MemFlagsData::trusted(), self.ctx, BAIL);
                    self.bail_if(flag);
                    self.set(d, v);
                }
                Op::PowF(d, a, b) | Op::MinF(d, a, b) | Op::MaxF(d, a, b) => {
                    let h = match op {
                        Op::PowF(..) => 1,
                        Op::MinF(..) => 3,
                        _ => 4,
                    };
                    let (x, y) = (self.fget(a), self.fget(b));
                    let call = self.b.ins().call(self.helpers[h], &[x, y]);
                    let v = self.b.inst_results(call)[0];
                    self.fset(d, v);
                }
                Op::ModF(d, a, b) => {
                    let (x, y) = (self.fget(a), self.fget(b));
                    let z = self.b.ins().f64const(0.0);
                    let zero = self.b.ins().fcmp(FloatCC::Equal, y, z);
                    self.bail_if(zero);
                    let call = self.b.ins().call(self.helpers[2], &[x, y]);
                    let v = self.b.inst_results(call)[0];
                    self.fset(d, v);
                }
                Op::SqrtI(d, a) => {
                    let x = self.get(a);
                    let neg = self.b.ins().icmp_imm_s(IntCC::SignedLessThan, x, 0);
                    self.bail_if(neg);
                    let f = self.b.ins().fcvt_from_sint(types::F64, x);
                    let v = self.b.ins().sqrt(f);
                    self.fset(d, v);
                }
                Op::SqrtF(d, a) => {
                    let x = self.fget(a);
                    let z = self.b.ins().f64const(0.0);
                    let neg = self.b.ins().fcmp(FloatCC::LessThan, x, z);
                    self.bail_if(neg);
                    let v = self.b.ins().sqrt(x);
                    self.fset(d, v);
                }
                Op::ALen(d, a) => {
                    let p = self.obj_addr(a)?;
                    let call = self.b.ins().call(self.helpers[5], &[p]);
                    let n = self.b.inst_results(call)[0];
                    let bad = self.b.ins().icmp_imm_s(IntCC::SignedLessThan, n, 0);
                    self.bail_if(bad);
                    self.set(d, n);
                }
                Op::ALoad { dst, arr, idx, ety } => {
                    let v = self.array_op(6, arr, &[idx], &[], ety)?;
                    self.set(dst, v);
                }
                Op::ACheck { arr, idx } => {
                    let p = self.obj_addr(arr)?;
                    let i = self.get(idx);
                    self.helper_checked(8, &[self.ctx, p, i]);
                }
                Op::AStore { arr, idx, src, ety } => {
                    let v = self.array_op(7, arr, &[idx], &[src], ety)?;
                    let _ = v;
                }
                Op::APush { arr, src, ety } => {
                    let p = self.obj_addr(arr)?;
                    let x = self.get(src);
                    let e = self.imm(ety_num(ety));
                    self.helper_checked(9, &[self.ctx, p, x, e]);
                }
                Op::ForArr {
                    dst,
                    arr,
                    idx,
                    exit,
                    ety,
                } => {
                    let p = self.obj_addr(arr)?;
                    let call = self.b.ins().call(self.helpers[5], &[p]);
                    let n = self.b.inst_results(call)[0];
                    let bad = self.b.ins().icmp_imm_s(IntCC::SignedLessThan, n, 0);
                    self.bail_if(bad);
                    let i = self.get(idx);
                    let inside = self.b.ins().icmp(IntCC::UnsignedLessThan, i, n);
                    let load = self.b.create_block();
                    self.b.ins().brif(inside, load, &[], tgt(exit), &[]);
                    self.b.switch_to_block(load);
                    let v = self.array_op(6, arr, &[idx], &[], ety)?;
                    self.set(dst, v);
                    self.b.ins().jump(next?, &[]);
                    dead = true;
                }
                Op::AbsF(d, a) => {
                    let f = self.fget(a);
                    let v = self.b.ins().fabs(f);
                    self.fset(d, v);
                }
                Op::MinI(d, a, b) => {
                    let (x, y) = (self.get(a), self.get(b));
                    let v = self.b.ins().smin(x, y);
                    self.set(d, v);
                }
                Op::MaxI(d, a, b) => {
                    let (x, y) = (self.get(a), self.get(b));
                    let v = self.b.ins().smax(x, y);
                    self.set(d, v);
                }
                Op::AddF(d, a, b) => {
                    let (x, y) = (self.fget(a), self.fget(b));
                    let v = self.b.ins().fadd(x, y);
                    self.fset(d, v);
                }
                Op::SubF(d, a, b) => {
                    let (x, y) = (self.fget(a), self.fget(b));
                    let v = self.b.ins().fsub(x, y);
                    self.fset(d, v);
                }
                Op::MulF(d, a, b) => {
                    let (x, y) = (self.fget(a), self.fget(b));
                    let v = self.b.ins().fmul(x, y);
                    self.fset(d, v);
                }
                Op::DivF(d, a, b) => {
                    let (x, y) = (self.fget(a), self.fget(b));
                    let z = self.b.ins().f64const(0.0);
                    let zero = self.b.ins().fcmp(FloatCC::Equal, y, z);
                    self.bail_if(zero);
                    let v = self.b.ins().fdiv(x, y);
                    self.fset(d, v);
                }
                Op::NegF(d, a) => {
                    let x = self.fget(a);
                    let v = self.b.ins().fneg(x);
                    self.fset(d, v);
                }
                Op::Not(d, a) => {
                    let x = self.get(a);
                    let c = self.b.ins().icmp_imm_s(IntCC::Equal, x, 0);
                    self.bset(d, c);
                }
                Op::LtI(d, a, b) => {
                    let c = self.cmp_i(IntCC::SignedLessThan, a, b);
                    self.bset(d, c);
                }
                Op::LeI(d, a, b) => {
                    let c = self.cmp_i(IntCC::SignedLessThanOrEqual, a, b);
                    self.bset(d, c);
                }
                Op::GtI(d, a, b) => {
                    let c = self.cmp_i(IntCC::SignedGreaterThan, a, b);
                    self.bset(d, c);
                }
                Op::GeI(d, a, b) => {
                    let c = self.cmp_i(IntCC::SignedGreaterThanOrEqual, a, b);
                    self.bset(d, c);
                }
                Op::EqI(d, a, b) => {
                    let c = self.cmp_i(IntCC::Equal, a, b);
                    self.bset(d, c);
                }
                Op::NeI(d, a, b) => {
                    let c = self.cmp_i(IntCC::NotEqual, a, b);
                    self.bset(d, c);
                }
                Op::LtF(d, a, b) => {
                    let c = self.cmp_f(FloatCC::LessThan, a, b);
                    self.bset(d, c);
                }
                Op::LeF(d, a, b) => {
                    let c = self.cmp_f(FloatCC::LessThanOrEqual, a, b);
                    self.bset(d, c);
                }
                Op::GtF(d, a, b) => {
                    let c = self.cmp_f(FloatCC::GreaterThan, a, b);
                    self.bset(d, c);
                }
                Op::GeF(d, a, b) => {
                    let c = self.cmp_f(FloatCC::GreaterThanOrEqual, a, b);
                    self.bset(d, c);
                }
                Op::EqF(d, a, b) => {
                    let c = self.cmp_f(FloatCC::Equal, a, b);
                    self.bset(d, c);
                }
                Op::NeF(d, a, b) => {
                    let c = self.cmp_f(FloatCC::NotEqual, a, b);
                    self.bset(d, c);
                }
                Op::Jmp(t) => {
                    if (t as usize) <= i {
                        self.back_edge();
                    }
                    self.b.ins().jump(tgt(t), &[]);
                    dead = true;
                }
                Op::JnLtI(a, b, t) => {
                    let c = self.cmp_i(IntCC::SignedGreaterThanOrEqual, a, b);
                    self.branch(c, tgt(t), next?);
                    dead = true;
                }
                Op::JnLeI(a, b, t) => {
                    let c = self.cmp_i(IntCC::SignedGreaterThan, a, b);
                    self.branch(c, tgt(t), next?);
                    dead = true;
                }
                Op::JnGtI(a, b, t) => {
                    let c = self.cmp_i(IntCC::SignedLessThanOrEqual, a, b);
                    self.branch(c, tgt(t), next?);
                    dead = true;
                }
                Op::JnGeI(a, b, t) => {
                    let c = self.cmp_i(IntCC::SignedLessThan, a, b);
                    self.branch(c, tgt(t), next?);
                    dead = true;
                }
                Op::JnEqI(a, b, t) => {
                    let c = self.cmp_i(IntCC::NotEqual, a, b);
                    self.branch(c, tgt(t), next?);
                    dead = true;
                }
                Op::JnNeI(a, b, t) => {
                    let c = self.cmp_i(IntCC::Equal, a, b);
                    self.branch(c, tgt(t), next?);
                    dead = true;
                }
                Op::JnLtImm(a, k, t) => {
                    let c = self.cmp_imm(IntCC::SignedGreaterThanOrEqual, a, k);
                    self.branch(c, tgt(t), next?);
                    dead = true;
                }
                Op::JnLeImm(a, k, t) => {
                    let c = self.cmp_imm(IntCC::SignedGreaterThan, a, k);
                    self.branch(c, tgt(t), next?);
                    dead = true;
                }
                Op::JnGtImm(a, k, t) => {
                    let c = self.cmp_imm(IntCC::SignedLessThanOrEqual, a, k);
                    self.branch(c, tgt(t), next?);
                    dead = true;
                }
                Op::JnGeImm(a, k, t) => {
                    let c = self.cmp_imm(IntCC::SignedLessThan, a, k);
                    self.branch(c, tgt(t), next?);
                    dead = true;
                }
                Op::JnEqImm(a, k, t) => {
                    let c = self.cmp_imm(IntCC::NotEqual, a, k);
                    self.branch(c, tgt(t), next?);
                    dead = true;
                }
                Op::JnNeImm(a, k, t) => {
                    let c = self.cmp_imm(IntCC::Equal, a, k);
                    self.branch(c, tgt(t), next?);
                    dead = true;
                }
                Op::JnF { a, b, t, cmp } => {
                    // the jump is taken when the comparison is false (a NaN makes it so)
                    let cc = match cmp {
                        Cmp::Lt => FloatCC::UnorderedOrGreaterThanOrEqual,
                        Cmp::Le => FloatCC::UnorderedOrGreaterThan,
                        Cmp::Gt => FloatCC::UnorderedOrLessThanOrEqual,
                        Cmp::Ge => FloatCC::UnorderedOrLessThan,
                        Cmp::Eq => FloatCC::NotEqual,
                        Cmp::Ne => FloatCC::Equal,
                    };
                    let c = self.cmp_f(cc, a, b);
                    self.branch(c, tgt(t), next?);
                    dead = true;
                }
                Op::JmpF(r, t) => {
                    let x = self.get(r);
                    let c = self.b.ins().icmp_imm_s(IntCC::Equal, x, 0);
                    self.branch(c, tgt(t), next?);
                    dead = true;
                }
                Op::JmpT(r, t) => {
                    let x = self.get(r);
                    let c = self.b.ins().icmp_imm_s(IntCC::NotEqual, x, 0);
                    self.branch(c, tgt(t), next?);
                    dead = true;
                }
                Op::JBack { cmp, a, b, t } => {
                    let (x, y) = (self.get(a), self.get(b));
                    let c = self.outcomes(cmp, x, y);
                    self.back_branch(c, tgt(t), next?);
                    dead = true;
                }
                Op::JBackImm { cmp, a, k, t } => {
                    let x = self.get(a);
                    let y = self.imm(i64::from(k));
                    let c = self.outcomes(cmp, x, y);
                    self.back_branch(c, tgt(t), next?);
                    dead = true;
                }
                Op::ForCheck {
                    cnt,
                    lim,
                    step,
                    incl,
                    exit,
                } => {
                    let (i, end, st) = (self.get(cnt), self.get(lim), self.get(step));
                    let inside = self.inside(i, end, st, incl);
                    // jump to `exit` when it is not
                    self.b.ins().brif(inside, next?, &[], tgt(exit), &[]);
                    dead = true;
                }
                Op::ForLoop {
                    cnt,
                    lim,
                    step,
                    incl,
                    body,
                } => {
                    let (c, st) = (self.get(cnt), self.get(step));
                    let (v, o) = self.b.ins().sadd_overflow(c, st);
                    let go = self.b.create_block();
                    // an overflow ends the loop, without a step
                    self.b.ins().brif(o, next?, &[], go, &[]);
                    self.b.switch_to_block(go);
                    self.back_edge();
                    self.set(cnt, v);
                    let end = self.get(lim);
                    let inside = self.inside(v, end, st, incl);
                    self.b.ins().brif(inside, tgt(body), &[], next?, &[]);
                    dead = true;
                }
                Op::StepCheck(r) => {
                    let x = self.get(r);
                    let z = self.b.ins().icmp_imm_s(IntCC::Equal, x, 0);
                    self.bail_if(z);
                }
                Op::IncI(a) => {
                    let x = self.get(a);
                    let v = self.b.ins().iadd_imm_s(x, 1);
                    self.set(a, v);
                }
                Op::Call {
                    code: target,
                    base,
                    dst,
                } => {
                    let steps = self.next_step();
                    let d = self
                        .b
                        .ins()
                        .load(types::I64, MemFlagsData::trusted(), self.ctx, DEPTH);
                    let max =
                        self.b
                            .ins()
                            .load(types::I64, MemFlagsData::trusted(), self.ctx, MAX_DEPTH);
                    let deep = self.b.ins().icmp(IntCC::SignedGreaterThanOrEqual, d, max);
                    self.bail_if(deep);
                    let d1 = self.b.ins().iadd_imm_s(d, 1);
                    self.b
                        .ins()
                        .store(MemFlagsData::trusted(), d1, self.ctx, DEPTH);
                    self.store_steps(steps);
                    let callee = self.ids.get(&target).copied()?;
                    let f = self.module.declare_func_in_func(callee, &mut *self.b.func);
                    let arity = self.arity(target);
                    let mut args = vec![self.ctx];
                    for k in 0..arity {
                        args.push(self.get(base + k as Reg));
                    }
                    let call = self.b.ins().call(f, &args);
                    let r = self.b.inst_results(call)[0];
                    self.b
                        .ins()
                        .store(MemFlagsData::trusted(), d, self.ctx, DEPTH);
                    let flag =
                        self.b
                            .ins()
                            .load(types::I64, MemFlagsData::trusted(), self.ctx, BAIL);
                    // a callee that left: the machine makes this call itself, from the steps as
                    // they were before it
                    let undo = self.b.create_block();
                    let go = self.b.create_block();
                    self.b.ins().brif(flag, undo, &[], go, &[]);
                    self.b.switch_to_block(undo);
                    let before = self.b.ins().iadd_imm_s(steps, -4);
                    self.store_steps(before);
                    let at = self.imm(self.at as i64);
                    self.b.ins().jump(self.bail, &[at.into()]);
                    self.b.switch_to_block(go);
                    self.set(dst, r);
                }
                Op::Ret(_) | Op::RetUnit | Op::RetO(_) if self.osr.is_some() => {
                    // the registers go back to memory, and the machine runs this return
                    let (regs, _, _) = self.osr?;
                    for r in 0..self.vars.len() {
                        let v = self.b.use_var(self.vars[r]);
                        self.b
                            .ins()
                            .store(MemFlagsData::trusted(), v, regs, (r * 8) as i32);
                    }
                    let at = self.imm(i as i64);
                    self.b.ins().return_(&[at]);
                    dead = true;
                }
                Op::Ret(a) => {
                    let v = self.get(a);
                    self.b.ins().return_(&[v]);
                    dead = true;
                }
                _ => return None,
            }
        }
        // the last block (after the final instruction) is never reached
        if let Some(blk) = blocks[n] {
            if !dead {
                self.b.ins().jump(blk, &[]);
            }
            self.b.switch_to_block(blk);
            let at = self.imm(n as i64);
            self.b.ins().jump(self.bail, &[at.into()]);
        }
        Some(())
    }

    /// The comparison of two integers of a loop (`holds`): `outcomes` has a bit for `<`, `==`, `>`.
    fn outcomes(&mut self, mask: u8, x: ir::Value, y: ir::Value) -> ir::Value {
        let mut acc: Option<ir::Value> = None;
        for (bit, cc) in [
            (1u8, IntCC::SignedLessThan),
            (2, IntCC::Equal),
            (4, IntCC::SignedGreaterThan),
        ] {
            if mask & bit != 0 {
                let c = self.b.ins().icmp(cc, x, y);
                acc = Some(match acc {
                    Some(a) => self.b.ins().bor(a, c),
                    None => c,
                });
            }
        }
        match acc {
            Some(c) => c,
            None => self.b.ins().iconst(types::I8, 0),
        }
    }

    /// The end of a loop: a step and the jump back when `c` holds, else the next instruction.
    fn back_branch(&mut self, c: ir::Value, t: Block, next: Block) {
        let again = self.b.create_block();
        self.b.ins().brif(c, again, &[], next, &[]);
        self.b.switch_to_block(again);
        self.back_edge();
        self.b.ins().jump(t, &[]);
    }

    fn arity(&self, target: u32) -> usize {
        self.codes[target as usize].param_tys.len()
    }
}

impl Interp<'_> {
    /// Counts a call of `target`; at the hot-th, compiles it to native code. The entry point
    /// of the native code when there is one.
    #[inline]
    pub(in crate::interp) fn jit_entry(&mut self, target: u32) -> usize {
        let c = self.code_at(target);
        let e = c.jit.entry.get();
        if e != 0 || c.jit.refused.get() {
            return e;
        }
        let n = c.jit.calls.get() + 1;
        c.jit.calls.set(n);
        if n < hot() {
            return 0;
        }
        c.jit.refused.set(true);
        if self.vm_jit.is_none() && !self.vm_jit_tried {
            self.vm_jit_tried = true;
            self.vm_jit = Jit::new().map(Box::new);
        }
        let Some(mut jit) = self.vm_jit.take() else {
            return 0;
        };
        let ok = jit.compile(target, &self.vm_codes);
        self.vm_jit = Some(jit);
        if ok { c.jit.entry.get() } else { 0 }
    }

    /// Runs the call of `entry` with the arguments at `stack[at..]`. `None` when the native code
    /// left (the machine then runs the call itself, from the start, and finds out why).
    #[inline]
    pub(in crate::interp) fn jit_call(
        &mut self,
        entry: usize,
        stack: &[i64],
        at: usize,
    ) -> Option<i64> {
        let steps = self.steps as i64 + 4;
        let max_steps = self.limits.max_steps as i64;
        if steps > max_steps || self.depth >= self.limits.max_depth {
            return None;
        }
        let mut ctx = Ctx {
            steps,
            max_steps,
            depth: self.depth as i64 + 1,
            max_depth: self.limits.max_depth as i64,
            bail: 0,
            deadline: self.deadline,
            max_alloc: self.limits.max_alloc,
        };
        // SAFETY: `entry` is the shim made by `Jit::shim` for this function, which reads as many
        // arguments from the pointer as the function has parameters (all inside `stack`).
        let f: extern "C" fn(*mut Ctx, *const i64) -> i64 = unsafe { std::mem::transmute(entry) };
        let v = f(&mut ctx, stack[at..].as_ptr());
        if ctx.bail != 0 {
            return None;
        }
        self.steps = ctx.steps as u64;
        Some(v)
    }

    /// Counts a jump back in `cur`; when it is hot, compiles it for an entry in the middle of its
    /// run. The native entry point when there is one.
    #[inline]
    pub(super) fn jit_osr_entry(&mut self, cur: u32) -> usize {
        let c = self.code_at(cur);
        if c.jit.osr_refused.get() {
            return 0;
        }
        let e = c.jit.osr.get();
        if e != 0 {
            return e;
        }
        let n = c.jit.calls.get() + 1;
        c.jit.calls.set(n);
        if n < hot() {
            return 0;
        }
        c.jit.osr_refused.set(true);
        if self.vm_jit.is_none() && !self.vm_jit_tried {
            self.vm_jit_tried = true;
            self.vm_jit = Jit::new().map(Box::new);
        }
        let Some(mut jit) = self.vm_jit.take() else {
            return 0;
        };
        let ok = jit.compile_osr(cur, &self.vm_codes);
        self.vm_jit = Some(jit);
        if ok {
            c.jit.osr_refused.set(false);
            c.jit.osr.get()
        } else {
            0
        }
    }

    /// Runs the rest of the code at `pc` natively, on the registers `stack[base..]`. The
    /// instruction to go on at: a return, or the one the native code could not run (an error,
    /// a limit: the machine runs it and says what is wrong).
    #[inline]
    pub(super) fn jit_osr(
        &mut self,
        entry: usize,
        stack: &mut [i64],
        ostack: &mut [Value],
        base: usize,
        pc: usize,
    ) -> usize {
        let mut ctx = Ctx {
            steps: self.steps as i64,
            max_steps: self.limits.max_steps as i64,
            depth: self.depth as i64,
            max_depth: self.limits.max_depth as i64,
            bail: 0,
            deadline: self.deadline,
            max_alloc: self.limits.max_alloc,
        };
        // SAFETY: `entry` is the function made by `Jit::compile_osr` for this code, which reads
        // and writes the `nregs` registers of the code from the pointer (all inside `stack`).
        let f: extern "C" fn(*mut Ctx, *mut i64, i64, *mut Value) -> i64 =
            unsafe { std::mem::transmute(entry) };
        let at = f(
            &mut ctx,
            stack[base..].as_mut_ptr(),
            pc as i64,
            ostack.as_mut_ptr().wrapping_add(base),
        );
        // the registers are back in memory, with the steps as they were at that instruction
        self.steps = ctx.steps as u64;
        at as usize
    }
}
