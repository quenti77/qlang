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
const HELPERS: usize = 15;

/// The most variables of the module that the native code of a loop keeps.
const MAX_GLOBALS: usize = 8;

/// Where in the buffer given to a loop the table of objects and the numbers of the variables are.
const GTABLE: usize = MAX_INLINE * 2;

/// The most calls of function values in a loop, and the most variables each takes from the code
/// around it: where their addresses are in the buffer given to a loop.
const MAX_CSITES: usize = 8;
const MAX_FREES: usize = 8;
const CBASE: usize = GTABLE + 1 + MAX_GLOBALS;

/// The most parameters of a function that takes objects (their addresses are put in a buffer).
const MAX_ARGS: usize = 16;

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

/// The structure in the object register at `v`, and the field of `site` in it, as the bits of a
/// number: `Some` only for the type the checker gave the field (an `int`, a `float`, a `bool`).
///
/// # Safety
/// `v` points to an object register of the run that is going on, and `site` to a field site of
/// the code being run (both stay alive while the native code runs).
unsafe fn field_bits(v: *const Value, site: *const FieldSite) -> Option<i64> {
    let (Value::Struct(st), site) = (unsafe { &*v }, unsafe { &*site }) else {
        return None;
    };
    let f = st.fields.borrow();
    let mut found = f.get_at_key(site.slot.get() as usize, &site.key);
    if found.is_none()
        && let Some(i) = f.position(&site.name)
    {
        site.slot.set(i as u32);
        found = f.get_at(i, &site.name);
    }
    match (found, &site.ty) {
        (Some(Value::Int(i)), Ty::Int) => Some(*i),
        (Some(Value::Float(x)), Ty::Float) => Some(x.to_bits() as i64),
        (Some(Value::Bool(x)), Ty::Bool) => Some(i64::from(*x)),
        _ => None,
    }
}

extern "C" fn h_field(ctx: *mut Ctx, v: *const Value, site: *const FieldSite) -> i64 {
    // SAFETY: see `field_bits`.
    match unsafe { field_bits(v, site) } {
        Some(bits) => bits,
        None => fail(ctx),
    }
}

/// Replaces a number of a structure by another: anything else (a field that is not there yet,
/// another kind of value) is left to the machine.
extern "C" fn h_setfield(ctx: *mut Ctx, v: *const Value, site: *const FieldSite, bits: i64) -> i64 {
    // SAFETY: see `field_bits`.
    let (Value::Struct(st), site) = (unsafe { &*v }, unsafe { &*site }) else {
        return fail(ctx);
    };
    let mut f = st.fields.borrow_mut();
    match (
        f.get_mut_at_key(site.slot.get() as usize, &site.key),
        &site.ty,
    ) {
        (Some(Value::Int(x)), Ty::Int) => *x = bits,
        (Some(Value::Float(x)), Ty::Float) => *x = f64::from_bits(bits as u64),
        (Some(Value::Bool(x)), Ty::Bool) => *x = bits != 0,
        _ => return fail(ctx),
    }
    0
}

/// The address of the number of the field `site` in the structure at `v`, kept by the native
/// code for the whole call (nothing it runs can move the field). `0` and the flag when the
/// value is not a structure with that number.
extern "C" fn h_fieldptr(ctx: *mut Ctx, v: *const Value, site: *const FieldSite) -> i64 {
    // SAFETY: see `field_bits`.
    let (Value::Struct(st), site) = (unsafe { &*v }, unsafe { &*site }) else {
        return fail(ctx);
    };
    let Ok(mut f) = st.fields.try_borrow_mut() else {
        return fail(ctx);
    };
    let mut found = f
        .get_mut_at_key(site.slot.get() as usize, &site.key)
        .is_some();
    if !found && let Some(i) = f.position(&site.name) {
        site.slot.set(i as u32);
        found = f.get_mut_at(i, &site.name).is_some();
    }
    if !found {
        return fail(ctx);
    }
    match (
        f.get_mut_at_key(site.slot.get() as usize, &site.key),
        &site.ty,
    ) {
        (Some(Value::Int(x)), Ty::Int) => std::ptr::from_mut(x) as i64,
        (Some(Value::Float(x)), Ty::Float) => std::ptr::from_mut(x) as i64,
        (Some(Value::Bool(x)), Ty::Bool) => std::ptr::from_mut(x) as i64,
        _ => fail(ctx),
    }
}

/// `dst = table[k]` for an object register (a variable of the module that is an object).
extern "C" fn h_loadobj(dst: *mut Value, table: *const Value, k: i64) -> i64 {
    // SAFETY: `dst` is an object register of this run and `table` the values the machine
    // gave for this loop, with `k` inside.
    unsafe { *dst = (*table.offset(k as isize)).clone() };
    0
}

/// `dst = src` for two object registers.
extern "C" fn h_movo(dst: *mut Value, src: *const Value) -> i64 {
    // SAFETY: two object registers of this run.
    unsafe { *dst = (*src).clone() };
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
        jb.symbol("qlang_jit_field", h_field as *const u8);
        jb.symbol("qlang_jit_setfield", h_setfield as *const u8);
        jb.symbol("qlang_jit_fieldptr", h_fieldptr as *const u8);
        jb.symbol("qlang_jit_loadobj", h_loadobj as *const u8);
        jb.symbol("qlang_jit_movo", h_movo as *const u8);
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
        let (i1, i2, i3, i4, i5) = (
            ints(&module, 1),
            ints(&module, 2),
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
            decl("qlang_jit_field", &i3)?,
            decl("qlang_jit_setfield", &i4)?,
            decl("qlang_jit_fieldptr", &i3)?,
            decl("qlang_jit_loadobj", &i3)?,
            decl("qlang_jit_movo", &i2)?,
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
            let Some(callees) = scan(&codes[i as usize], false, &[]) else {
                return false;
            };
            // a function that writes fields keeps them until it returns, so that the machine
            // can run the call again from the start when the native code leaves: a caller
            // that could leave after it would run the writes twice
            if callees.iter().any(|c| writes_fields(&codes[*c as usize])) {
                return false;
            }
            set.push(i);
            todo.extend(callees);
        }
        let mut declared = Vec::new();
        for &i in &set {
            let c = &codes[i as usize];
            if c.param_tys.iter().any(is_obj) {
                if c.param_tys.len() > MAX_ARGS {
                    return false;
                }
                c.jit.objs.set(true);
            }
            let sig = arity_sig(&self.module, nargs(&codes[i as usize]));
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
            let Some(s) = self.shim(i, id, nargs(&codes[i as usize])) else {
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
    pub(in crate::interp) fn compile_osr(
        &mut self,
        root: u32,
        codes: &[Rc<Code>],
        closures: &[(usize, u32)],
    ) -> bool {
        let Some(callees) = scan(&codes[root as usize], true, closures) else {
            return false;
        };
        for c in callees {
            if !self.ids.contains_key(&c) && !self.compile(c, codes) {
                return false;
            }
        }
        *codes[root as usize].jit.osr_arrays.borrow_mut() = inline_arrays(&codes[root as usize]);
        let Some(globals) = osr_globals(&codes[root as usize]) else {
            return false;
        };
        // a variable that the loop keeps in memory must not also be one a function it calls
        // reads or writes through its address
        for (_, t) in closures {
            for fr in &codes[*t as usize].frees {
                let clash = globals.iter().any(|(g, ..)| {
                    let gr = &codes[root as usize].globals[*g as usize];
                    Some(gr.module) == fr.global && gr.name == fr.name
                });
                if clash {
                    return false;
                }
            }
        }
        *codes[root as usize].jit.osr_globals.borrow_mut() = globals;
        *codes[root as usize].jit.osr_closures.borrow_mut() = closures.to_vec();
        let sig = arity_sig(&self.module, 4);
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
        cx.func.signature = arity_sig(&self.module, if osr { 4 } else { nargs(code) });
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
            let is_f = float_registers(code, codes);
            let ty_of = |r: usize| if is_f[r] { types::F64 } else { types::I64 };
            let vars: Vec<Variable> = (0..code.nregs).map(|r| b.declare_var(ty_of(r))).collect();
            let em_vars = vars.clone();
            // the structures that are parameters: their addresses
            let ovars: Vec<Variable> = (0..code.nregs).map(|_| b.declare_var(types::I64)).collect();
            for (r, v) in ovars.iter().enumerate() {
                let init = if !osr && r < code.param_tys.len() && is_obj(&code.param_tys[r]) {
                    ps[r + 1]
                } else {
                    b.ins().iconst(types::I64, 0)
                };
                b.def_var(*v, init);
            }
            for (r, v) in vars.iter().enumerate() {
                let init = if osr {
                    b.ins()
                        .load(ty_of(r), MemFlagsData::trusted(), ps[1], (r * 8) as i32)
                } else if r < code.param_tys.len() {
                    if is_f[r] {
                        b.ins().bitcast(types::F64, MemFlagsData::new(), ps[r + 1])
                    } else {
                        ps[r + 1]
                    }
                } else if is_f[r] {
                    b.ins().f64const(0.0)
                } else {
                    b.ins().iconst(types::I64, 0)
                };
                b.def_var(*v, init);
            }
            // the variables taken from the code around: read from their addresses when the
            // function starts, written back when it returns
            let mut free_ptrs = Vec::new();
            if !osr {
                for (j, fr) in code.frees.iter().enumerate() {
                    let p = ps[code.param_tys.len() + 1 + j];
                    let x = if fr.ty == Ty::Bool {
                        b.ins().uload8(types::I64, MemFlagsData::trusted(), p, 0)
                    } else {
                        b.ins().load(types::I64, MemFlagsData::trusted(), p, 0)
                    };
                    let r = usize::from(fr.reg);
                    let x = if is_f[r] {
                        b.ins().bitcast(types::F64, MemFlagsData::new(), x)
                    } else {
                        x
                    };
                    b.def_var(em_vars[r], x);
                    if fr.assigned {
                        free_ptrs.push((fr.reg, fr.ty.clone(), p));
                    }
                }
            }
            let mut parts = HashMap::new();
            if osr {
                for (k, (reg, ety)) in code.jit.osr_arrays.borrow().iter().enumerate() {
                    let (p, n) = (b.declare_var(types::I64), b.declare_var(types::I64));
                    let at = (k * 16) as i32;
                    let x = b.ins().load(types::I64, MemFlagsData::trusted(), ps[4], at);
                    b.def_var(p, x);
                    let y = b
                        .ins()
                        .load(types::I64, MemFlagsData::trusted(), ps[4], at + 8);
                    b.def_var(n, y);
                    parts.insert(*reg, (p, n, *ety));
                }
            }
            // the variables of the module that a loop keeps in memory: their numbers (written
            // back where the machine reads them when the native code leaves) and the table
            // of the objects
            let gl = code.jit.osr_globals.borrow().clone();
            let mut gvars = Vec::new();
            let mut gtable = None;
            if osr {
                for (k, (_, obj, _)) in gl.iter().enumerate() {
                    let v = b.declare_var(types::I64);
                    let x = if *obj {
                        b.ins().iconst(types::I64, 0)
                    } else {
                        b.ins().load(
                            types::I64,
                            MemFlagsData::trusted(),
                            ps[4],
                            ((GTABLE + 1 + k) * 8) as i32,
                        )
                    };
                    b.def_var(v, x);
                    gvars.push(v);
                }
                gtable = Some(b.ins().load(
                    types::I64,
                    MemFlagsData::trusted(),
                    ps[4],
                    (GTABLE * 8) as i32,
                ));
            }
            let spill_to = gvars.clone();
            let bail = b.create_block();
            let at = b.append_block_param(bail, types::I64);
            let mut em = Em {
                b: &mut b,
                ctx: ps[0],
                bail,
                poll,
                helpers,
                vars,
                is_f,
                module: &mut self.module,
                ids: &self.ids,
                codes,
                osr: osr.then(|| (ps[1], ps[2], ps[3])),
                at: 0,
                parts,
                ovars,
                gvars,
                gl: gl.clone(),
                gtable,
                gbuf: osr.then(|| ps[4]),
                closures: code.jit.osr_closures.borrow().clone(),
                free_ptrs,
                fptr: HashMap::new(),
                fval: HashMap::new(),
                cached: false,
                written: Vec::new(),
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
                for (k, (_, obj, stored)) in gl.iter().enumerate() {
                    if *stored && !*obj {
                        let x = b.use_var(spill_to[k]);
                        b.ins().store(
                            MemFlagsData::trusted(),
                            x,
                            ps[4],
                            ((GTABLE + 1 + k) * 8) as i32,
                        );
                    }
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

/// Which registers hold only floats: they are kept as `f64` in native code, without a move
/// between the two kinds of registers at each instruction. A register used both ways (or one
/// nothing tells about) stays an integer; the access functions convert where the two meet, so
/// a wrong guess costs a move, never a wrong result.
fn float_registers(code: &Code, codes: &[Rc<Code>]) -> Vec<bool> {
    let n = code.nregs;
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(p: &mut [usize], x: usize) -> usize {
        let mut r = x;
        while p[r] != r {
            r = p[r];
        }
        let mut c = x;
        while p[c] != r {
            let next = p[c];
            p[c] = r;
            c = next;
        }
        r
    }
    let (mut fl, mut it) = (vec![false; n], vec![false; n]);
    for (k, t) in code.param_tys.iter().enumerate().take(n) {
        if *t == Ty::Float {
            fl[k] = true;
        } else {
            it[k] = true;
        }
    }
    let mut moves = Vec::new();
    {
        let mut f = |rs: &[Reg]| rs.iter().for_each(|r| fl[usize::from(*r)] = true);
        let mut i = |rs: &[Reg]| rs.iter().for_each(|r| it[usize::from(*r)] = true);
        for op in &code.ops {
            match *op {
                Op::ConstI(..) => {}
                Op::Mov(d, a) => moves.push((usize::from(d), usize::from(a))),
                Op::AddF(d, a, b)
                | Op::SubF(d, a, b)
                | Op::MulF(d, a, b)
                | Op::DivF(d, a, b)
                | Op::ModF(d, a, b)
                | Op::PowF(d, a, b)
                | Op::MinF(d, a, b)
                | Op::MaxF(d, a, b) => f(&[d, a, b]),
                Op::NegF(d, a) | Op::AbsF(d, a) | Op::SqrtF(d, a) => f(&[d, a]),
                Op::SqrtI(d, a) | Op::ItoF(d, a) => {
                    f(&[d]);
                    i(&[a]);
                }
                Op::FtoI(d, a) => {
                    i(&[d]);
                    f(&[a]);
                }
                Op::DivII(d, a, b) => {
                    f(&[d]);
                    i(&[a, b]);
                }
                Op::LtF(d, a, b)
                | Op::LeF(d, a, b)
                | Op::GtF(d, a, b)
                | Op::GeF(d, a, b)
                | Op::EqF(d, a, b)
                | Op::NeF(d, a, b) => {
                    i(&[d]);
                    f(&[a, b]);
                }
                Op::JnF { a, b, .. } => f(&[a, b]),
                Op::ALoad { dst, idx, ety, .. } => {
                    i(&[idx]);
                    if ety == Ety::F { f(&[dst]) } else { i(&[dst]) }
                }
                Op::ForArr { dst, idx, ety, .. } => {
                    i(&[idx]);
                    if ety == Ety::F { f(&[dst]) } else { i(&[dst]) }
                }
                Op::AStore { idx, src, ety, .. } => {
                    i(&[idx]);
                    if ety == Ety::F { f(&[src]) } else { i(&[src]) }
                }
                Op::APush { src, ety, .. } => {
                    if ety == Ety::F {
                        f(&[src])
                    } else {
                        i(&[src])
                    }
                }
                Op::ACheck { idx, .. } => i(&[idx]),
                Op::Field { dst, fld, .. } => {
                    if code.fields[fld as usize].ty == Ty::Float {
                        f(&[dst])
                    } else {
                        i(&[dst])
                    }
                }
                Op::SetField { src, fld, .. } => {
                    if code.fields[fld as usize].ty == Ty::Float {
                        f(&[src])
                    } else {
                        i(&[src])
                    }
                }
                Op::ALen(d, _) => i(&[d]),
                Op::Call { code: t, base, dst } => {
                    let callee = &codes[t as usize];
                    for (k, ty) in callee.param_tys.iter().enumerate() {
                        let r = base + k as Reg;
                        if *ty == Ty::Float { f(&[r]) } else { i(&[r]) }
                    }
                    if callee.ret == Ty::Float {
                        f(&[dst])
                    } else {
                        i(&[dst])
                    }
                }
                Op::Ret(a) => {
                    if code.ret == Ty::Float {
                        f(&[a])
                    } else {
                        i(&[a])
                    }
                }
                Op::AddI(d, a, b)
                | Op::SubI(d, a, b)
                | Op::MulI(d, a, b)
                | Op::IntDivI(d, a, b)
                | Op::ModI(d, a, b)
                | Op::PowI(d, a, b)
                | Op::MinI(d, a, b)
                | Op::MaxI(d, a, b)
                | Op::LtI(d, a, b)
                | Op::LeI(d, a, b)
                | Op::GtI(d, a, b)
                | Op::GeI(d, a, b)
                | Op::EqI(d, a, b)
                | Op::NeI(d, a, b) => i(&[d, a, b]),
                Op::NegI(d, a)
                | Op::AbsI(d, a)
                | Op::Not(d, a)
                | Op::AddImm(d, a, _)
                | Op::SubImm(d, a, _)
                | Op::DivK(d, a, _)
                | Op::ModK(d, a, _) => i(&[d, a]),
                Op::JnLtI(a, b, _)
                | Op::JnLeI(a, b, _)
                | Op::JnGtI(a, b, _)
                | Op::JnGeI(a, b, _)
                | Op::JnEqI(a, b, _)
                | Op::JnNeI(a, b, _) => i(&[a, b]),
                Op::JnLtImm(a, ..)
                | Op::JnLeImm(a, ..)
                | Op::JnGtImm(a, ..)
                | Op::JnGeImm(a, ..)
                | Op::JnEqImm(a, ..)
                | Op::JnNeImm(a, ..)
                | Op::JBackImm { a, .. }
                | Op::StepCheck(a)
                | Op::JmpF(a, _)
                | Op::JmpT(a, _)
                | Op::IncI(a) => i(&[a]),
                Op::JBack { a, b, .. } => i(&[a, b]),
                Op::ForLoop { cnt, lim, step, .. } | Op::ForCheck { cnt, lim, step, .. } => {
                    i(&[cnt, lim, step]);
                }
                _ => {}
            }
        }
    }
    for (d, a) in moves {
        let (x, y) = (find(&mut parent, d), find(&mut parent, a));
        parent[x] = y;
    }
    let (mut cf, mut ci) = (vec![false; n], vec![false; n]);
    for r in 0..n {
        let root = find(&mut parent, r);
        cf[root] |= fl[r];
        ci[root] |= it[r];
    }
    (0..n)
        .map(|r| {
            let root = find(&mut parent, r);
            cf[root] && !ci[root]
        })
        .collect()
}

/// The most arrays a loop reads in place.
const MAX_INLINE: usize = 16;

/// The arrays (object register, kind of the elements) that the native code of `code` reads and
/// writes in place, from a pointer and a length given when it starts. That is only right when
/// nothing in the code can make an array grow or move: no `push`.
fn inline_arrays(code: &Code) -> Vec<(Reg, Ety)> {
    let mut found: Vec<(Reg, Ety)> = Vec::new();
    for op in &code.ops {
        match *op {
            Op::APush { .. } => return Vec::new(),
            Op::ALoad { arr, ety, .. }
            | Op::AStore { arr, ety, .. }
            | Op::ForArr { arr, ety, .. } => match found.iter().find(|(r, _)| *r == arr) {
                Some((_, e)) if *e != ety => return Vec::new(),
                Some(_) => {}
                None => found.push((arr, ety)),
            },
            _ => {}
        }
    }
    if found.len() > MAX_INLINE {
        return Vec::new();
    }
    found
}

/// The functions `code` calls when it can be compiled to native code; `None` otherwise.
/// The place an instruction jumps to, for the ones that can jump back.
fn jump_target(op: &Op) -> Option<u32> {
    match *op {
        Op::Jmp(t)
        | Op::JnLtI(_, _, t)
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
        Op::ForLoop { body, .. } => Some(body),
        _ => None,
    }
}

/// Whether the instruction `at` is inside a loop: between a jump back and the place it goes to.
/// Any other jump that is not native keeps the code out as well (`false` only when sure).
fn in_loop(code: &Code, at: usize) -> bool {
    code.ops.iter().enumerate().any(|(j, op)| {
        jump_target(op).is_some_and(|t| (t as usize) <= j && (t as usize) <= at && at <= j)
    }) || matches!(code.ops[at], Op::JnEnum { .. })
}

fn scan(code: &Code, osr: bool, closures: &[(usize, u32)]) -> Option<Vec<u32>> {
    let scalar = |t: &Ty| matches!(t, Ty::Int | Ty::Float | Ty::Bool);
    // a function called from native code: numbers or structures in, a number out. The entry in
    // the middle of a run (a loop, or a function that is running) only needs the registers to
    // be numbers.
    if !osr
        && (code.is_loop
            || code.frees.iter().any(|f| !scalar(&f.ty))
            || code.frees.len() > MAX_FREES
            || code.param_tys.len() > MAX_ARGS
            || !code
                .param_tys
                .iter()
                .all(|t| scalar(t) || matches!(t, Ty::Struct(..)))
            || !scalar(&code.ret))
    {
        return None;
    }
    // a structure that is a parameter, and was not given another value
    let object = |r: Reg| matches!(code.param_tys.get(usize::from(r)), Some(Ty::Struct(..)));
    let mut callees = Vec::new();
    for (at, op) in code.ops.iter().enumerate() {
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
            // a number in a structure, read or written in place (the entry in the middle of a run only)
            Op::Field { obj, fld, .. } | Op::SetField { obj, fld, .. }
                if scalar(&code.fields[fld as usize].ty) && (osr || object(obj)) => {}
            // a structure handed on to a function called from here
            Op::MovO(_, a) if !osr && object(a) => {}
            // the entry in the middle of a run: a variable of the module is kept in memory
            // (its number) or given by the machine (its object), and objects are copied
            Op::LoadG { g, .. }
                if osr
                    && (scalar(&code.globals[g as usize].ty)
                        || is_obj(&code.globals[g as usize].ty)) => {}
            Op::StoreG { g, .. } if osr && scalar(&code.globals[g as usize].ty) => {}
            Op::MovO(..) if osr => {}
            Op::CallValue { f, .. }
                if osr
                    && closures.len() <= MAX_CSITES
                    && !writes_obj_reg(code, f)
                    && closures.iter().any(|c| c.0 == at) =>
            {
                callees.extend(closures.iter().filter(|c| c.0 == at).map(|c| c.1));
            }
            Op::Call { code: t, .. } => callees.push(t),
            // the entry in the middle of a run leaves to the machine at an instruction it cannot
            // translate, as long as none is in a loop (it would leave at each turn)
            _ if osr && !in_loop(code, at) => {}
            _ => return None,
        }
    }
    if !osr && !callees.is_empty() && writes_fields(code) {
        return None;
    }
    Some(callees)
}

/// The variables of the module that the code of a loop reads or writes, as `Code::jit` keeps
/// them; `None` when there are too many, or when one is written that holds an object.
fn osr_globals(code: &Code) -> Option<Vec<(u32, bool, bool)>> {
    let mut found: Vec<(u32, bool, bool)> = Vec::new();
    for op in &code.ops {
        let (g, store) = match *op {
            Op::LoadG { g, .. } => (g, false),
            Op::StoreG { g, .. } => (g, true),
            _ => continue,
        };
        let obj = is_obj(&code.globals[g as usize].ty);
        if obj && store {
            return None;
        }
        match found.iter_mut().find(|(h, ..)| *h == g) {
            Some(e) => e.2 |= store,
            None => found.push((g, obj, store)),
        }
    }
    (found.len() <= MAX_GLOBALS).then_some(found)
}

/// The values a native function is given: its parameters, then the addresses of the variables
/// it takes from the code around it.
fn nargs(code: &Code) -> usize {
    code.param_tys.len() + code.frees.len()
}

/// Whether `code` writes something that is not its own: a field of a structure, or a variable
/// of the code around it.
fn writes_fields(code: &Code) -> bool {
    code.frees.iter().any(|f| f.assigned)
        || code.ops.iter().any(|op| matches!(op, Op::SetField { .. }))
}

/// Whether an instruction of `code` gives a new value to the object register `r`.
fn writes_obj_reg(code: &Code, r: Reg) -> bool {
    code.ops
        .iter()
        .any(|op| matches!(*op, Op::MovO(d, _) | Op::LoadG { dst: d, .. } if d == r))
}

struct Em<'a, 'b> {
    b: &'a mut FunctionBuilder<'b>,
    ctx: ir::Value,
    bail: Block,
    poll: ir::FuncRef,
    helpers: [ir::FuncRef; HELPERS],
    vars: Vec<Variable>,
    /// The registers kept as `f64`.
    is_f: Vec<bool>,
    module: &'a mut JITModule,
    ids: &'a HashMap<u32, FuncId>,
    codes: &'a [Rc<Code>],
    /// For the entry in the middle of a run: the registers in memory and the instruction.
    osr: Option<(ir::Value, ir::Value, ir::Value)>,
    /// The instruction being translated: where the machine goes on when the native code leaves.
    at: usize,
    /// The arrays read in place: pointer and length, in variables, and the kind of the elements.
    parts: HashMap<Reg, (Variable, Variable, Ety)>,
    /// The structures held by the registers (their addresses), for a function.
    ovars: Vec<Variable>,
    /// For a loop: the variables of the module it uses (see `JitSlot::osr_globals`), the numbers
    /// of those in variables, the table of the objects, and the buffer the machine reads.
    gvars: Vec<Variable>,
    gl: Vec<(u32, bool, bool)>,
    gtable: Option<ir::Value>,
    gbuf: Option<ir::Value>,
    /// The calls of function values the loop was compiled for, and the variables a function
    /// writes back to the code around it (register, type, address).
    closures: Vec<(usize, u32)>,
    free_ptrs: Vec<(Reg, Ty, ir::Value)>,
    /// Where the number of each field used (structure register, name of the field) is, and
    /// its value held in a variable when the function writes fields: the writes go to memory
    /// only when it returns.
    fptr: HashMap<(Reg, String), Variable>,
    fval: HashMap<(Reg, String), Variable>,
    cached: bool,
    written: Vec<((Reg, String), Ty)>,
}

impl Em<'_, '_> {
    /// The register as an integer (the bits of a float).
    fn get(&mut self, r: Reg) -> ir::Value {
        let v = self.b.use_var(self.vars[r as usize]);
        if self.is_f[usize::from(r)] {
            self.b.ins().bitcast(types::I64, MemFlagsData::new(), v)
        } else {
            v
        }
    }

    fn set(&mut self, r: Reg, v: ir::Value) {
        let v = if self.is_f[usize::from(r)] {
            self.b.ins().bitcast(types::F64, MemFlagsData::new(), v)
        } else {
            v
        };
        self.b.def_var(self.vars[r as usize], v);
    }

    fn fget(&mut self, r: Reg) -> ir::Value {
        let v = self.b.use_var(self.vars[r as usize]);
        if self.is_f[usize::from(r)] {
            v
        } else {
            self.b.ins().bitcast(types::F64, MemFlagsData::new(), v)
        }
    }

    fn fset(&mut self, r: Reg, v: ir::Value) {
        let v = if self.is_f[usize::from(r)] {
            v
        } else {
            self.b.ins().bitcast(types::I64, MemFlagsData::new(), v)
        };
        self.b.def_var(self.vars[r as usize], v);
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

    /// The address of the element `i` (`ptr + i * size`).
    fn elem_addr(&mut self, ptr: ir::Value, i: ir::Value, ety: Ety) -> ir::Value {
        let off = if ety == Ety::B {
            i
        } else {
            self.b.ins().ishl_imm_s(i, 3)
        };
        self.b.ins().iadd(ptr, off)
    }

    /// The address of the element `idx` of the array in `arr`; leaves when it does not exist.
    fn element(&mut self, arr: Reg, idx: Reg, ety: Ety) -> Option<(ir::Value, Ety)> {
        let (p, n, e) = self.parts.get(&arr).copied()?;
        let _ = ety;
        let (ptr, len) = (self.b.use_var(p), self.b.use_var(n));
        let i = self.get(idx);
        let outside = self.b.ins().icmp(IntCC::UnsignedGreaterThanOrEqual, i, len);
        self.bail_if(outside);
        Some((self.elem_addr(ptr, i, e), e))
    }

    fn load_elem(&mut self, addr: ir::Value, ety: Ety) -> ir::Value {
        if ety == Ety::B {
            self.b
                .ins()
                .uload8(types::I64, MemFlagsData::trusted(), addr, 0)
        } else {
            self.b
                .ins()
                .load(types::I64, MemFlagsData::trusted(), addr, 0)
        }
    }

    fn store_elem(&mut self, addr: ir::Value, v: ir::Value, ety: Ety) {
        if ety == Ety::B {
            let nz = self.b.ins().icmp_imm_s(IntCC::NotEqual, v, 0);
            let one = self.b.ins().uextend(types::I64, nz);
            self.b.ins().istore8(MemFlagsData::trusted(), one, addr, 0);
        } else {
            self.b.ins().store(MemFlagsData::trusted(), v, addr, 0);
        }
    }

    /// The registers go back to memory and the native code returns the instruction `i`, where the
    /// machine goes on (the entry in the middle of a run only).
    fn leave_to_machine(&mut self, i: usize) -> Option<()> {
        let (regs, _, _) = self.osr?;
        for r in 0..self.vars.len() {
            let v = self.b.use_var(self.vars[r]);
            self.b
                .ins()
                .store(MemFlagsData::trusted(), v, regs, (r * 8) as i32);
        }
        let buf = self.gbuf?;
        for (k, (_, obj, stored)) in self.gl.clone().iter().enumerate() {
            if *stored && !*obj {
                let x = self.b.use_var(self.gvars[k]);
                self.b.ins().store(
                    MemFlagsData::trusted(),
                    x,
                    buf,
                    ((GTABLE + 1 + k) * 8) as i32,
                );
            }
        }
        let at = self.imm(i as i64);
        self.b.ins().return_(&[at]);
        Some(())
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

    /// A number of a field, from its address.
    fn load_field(&mut self, p: ir::Value, ty: &Ty) -> ir::Value {
        if *ty == Ty::Bool {
            self.b
                .ins()
                .uload8(types::I64, MemFlagsData::trusted(), p, 0)
        } else {
            self.b.ins().load(types::I64, MemFlagsData::trusted(), p, 0)
        }
    }

    /// A function that takes structures finds the field of each one it uses when it starts
    /// (the machine runs the call itself when one is missing, which tells what is wrong).
    fn hoist(&mut self, code: &Code) -> Option<()> {
        let mut keys: Vec<((Reg, String), usize)> = Vec::new();
        for op in &code.ops {
            let (Op::Field { obj, fld, .. } | Op::SetField { obj, fld, .. }) = *op else {
                continue;
            };
            let key = (obj, code.fields[fld as usize].name.clone());
            if !keys.iter().any(|(k, _)| *k == key) {
                keys.push((key.clone(), fld as usize));
            }
            if matches!(op, Op::SetField { .. }) && !self.written.iter().any(|(k, _)| *k == key) {
                self.written
                    .push((key, code.fields[fld as usize].ty.clone()));
            }
        }
        self.cached = !self.written.is_empty();
        let mut ptrs = Vec::new();
        for (key, fld) in keys {
            let site = &code.fields[fld];
            let sp = self.imm(site as *const FieldSite as i64);
            let o = self.b.use_var(self.ovars[usize::from(key.0)]);
            let p = self.helper_checked(12, &[self.ctx, o, sp]);
            let pv = self.b.declare_var(types::I64);
            self.b.def_var(pv, p);
            self.fptr.insert(key.clone(), pv);
            if self.cached {
                let x = self.load_field(p, &site.ty);
                let fv = self.b.declare_var(types::I64);
                self.b.def_var(fv, x);
                self.fval.insert(key.clone(), fv);
            }
            ptrs.push((key, p));
        }
        // the same number reached through two registers would be held twice
        if self.cached {
            for (i, (a, pa)) in ptrs.iter().enumerate() {
                for (b, pb) in &ptrs[i + 1..] {
                    if a.1 == b.1 {
                        let same = self.b.ins().icmp(IntCC::Equal, *pa, *pb);
                        self.bail_if(same);
                    }
                }
            }
        }
        Some(())
    }

    /// The numbers held for the fields written go to their places (a function returns).
    fn commit(&mut self) {
        for (r, ty, p) in self.free_ptrs.clone() {
            let x = self.get(r);
            if ty == Ty::Bool {
                self.b.ins().istore8(MemFlagsData::trusted(), x, p, 0);
            } else {
                self.b.ins().store(MemFlagsData::trusted(), x, p, 0);
            }
        }
        for (key, ty) in self.written.clone() {
            let p = self.b.use_var(self.fptr[&key]);
            let x = self.b.use_var(self.fval[&key]);
            if ty == Ty::Bool {
                self.b.ins().istore8(MemFlagsData::trusted(), x, p, 0);
            } else {
                self.b.ins().store(MemFlagsData::trusted(), x, p, 0);
            }
        }
    }

    fn body(&mut self, code: &Code) -> Option<()> {
        if self.osr.is_none() {
            self.hoist(code)?;
        }
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
                Op::ConstI(d, v) if self.is_f[usize::from(d)] => {
                    let x = self.b.ins().f64const(f64::from_bits(v as u64));
                    self.b.def_var(self.vars[usize::from(d)], x);
                }
                Op::ConstI(d, v) => {
                    let v = self.imm(v);
                    self.set(d, v);
                }
                Op::Mov(d, a) if self.is_f[usize::from(d)] && self.is_f[usize::from(a)] => {
                    let v = self.b.use_var(self.vars[usize::from(a)]);
                    self.b.def_var(self.vars[usize::from(d)], v);
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
                Op::ALen(d, a) if self.parts.contains_key(&a) => {
                    let n = self.b.use_var(self.parts[&a].1);
                    self.set(d, n);
                }
                Op::ALoad { dst, arr, idx, ety } if self.parts.contains_key(&arr) => {
                    let (addr, ety) = self.element(arr, idx, ety)?;
                    let v = self.load_elem(addr, ety);
                    self.set(dst, v);
                }
                Op::ACheck { arr, idx } if self.parts.contains_key(&arr) => {
                    self.element(arr, idx, Ety::I)?;
                }
                Op::AStore { arr, idx, src, ety } if self.parts.contains_key(&arr) => {
                    let (addr, ety) = self.element(arr, idx, ety)?;
                    let v = self.get(src);
                    self.store_elem(addr, v, ety);
                }
                Op::ForArr {
                    dst,
                    arr,
                    idx,
                    exit,
                    ety,
                } if self.parts.contains_key(&arr) => {
                    let (p, n, _) = self.parts[&arr];
                    let (ptr, len) = (self.b.use_var(p), self.b.use_var(n));
                    let i = self.get(idx);
                    let inside = self.b.ins().icmp(IntCC::UnsignedLessThan, i, len);
                    let load = self.b.create_block();
                    self.b.ins().brif(inside, load, &[], tgt(exit), &[]);
                    self.b.switch_to_block(load);
                    let addr = self.elem_addr(ptr, i, ety);
                    let v = self.load_elem(addr, ety);
                    self.set(dst, v);
                    self.b.ins().jump(next?, &[]);
                    dead = true;
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
                Op::Field { dst, obj, fld } if self.osr.is_none() => {
                    let site = &code.fields[fld as usize];
                    let key = (obj, site.name.clone());
                    let v = if self.cached {
                        self.b.use_var(self.fval[&key])
                    } else {
                        let p = self.b.use_var(self.fptr[&key]);
                        self.load_field(p, &site.ty)
                    };
                    self.set(dst, v);
                }
                Op::SetField { obj, src, fld } if self.osr.is_none() => {
                    let key = (obj, code.fields[fld as usize].name.clone());
                    let x = self.get(src);
                    self.b.def_var(self.fval[&key], x);
                }
                Op::MovO(d, a) if self.osr.is_none() => {
                    let v = self.b.use_var(self.ovars[usize::from(a)]);
                    self.b.def_var(self.ovars[usize::from(d)], v);
                }
                Op::MovO(d, a) => {
                    let (x, y) = (self.obj_addr(d)?, self.obj_addr(a)?);
                    self.b.ins().call(self.helpers[14], &[x, y]);
                }
                Op::LoadG { dst, g } => {
                    let k = self.gl.iter().position(|e| e.0 == g)?;
                    if self.gl[k].1 {
                        let (d, t) = (self.obj_addr(dst)?, self.gtable?);
                        let kk = self.imm(k as i64);
                        self.b.ins().call(self.helpers[13], &[d, t, kk]);
                    } else {
                        let x = self.b.use_var(self.gvars[k]);
                        self.set(dst, x);
                    }
                }
                Op::StoreG { src, g } => {
                    let k = self.gl.iter().position(|e| e.0 == g)?;
                    let x = self.get(src);
                    self.b.def_var(self.gvars[k], x);
                }
                Op::Field { dst, obj, fld } => {
                    let p = self.obj_addr(obj)?;
                    let site = self.imm(&code.fields[fld as usize] as *const FieldSite as i64);
                    let v = self.helper_checked(10, &[self.ctx, p, site]);
                    self.set(dst, v);
                }
                Op::SetField { obj, src, fld } => {
                    let p = self.obj_addr(obj)?;
                    let site = self.imm(&code.fields[fld as usize] as *const FieldSite as i64);
                    let x = self.get(src);
                    self.helper_checked(11, &[self.ctx, p, site, x]);
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
                } => self.call_native(target, base, dst, Vec::new())?,
                // a function value that the loop was compiled for (see `JitSlot::osr_closures`):
                // the variables it takes from the code around it are given as addresses
                Op::CallValue { dst, base, .. } => {
                    let j = self.closures.iter().position(|c| c.0 == i)?;
                    let target = self.closures[j].1;
                    let buf = self.gbuf?;
                    let n = self.codes[target as usize].frees.len();
                    let mut free_args = Vec::new();
                    for m in 0..n {
                        let at = ((CBASE + j * MAX_FREES + m) * 8) as i32;
                        free_args.push(self.b.ins().load(
                            types::I64,
                            MemFlagsData::trusted(),
                            buf,
                            at,
                        ));
                    }
                    self.call_native(target, base, dst, free_args)?;
                }
                Op::Ret(_) | Op::RetUnit | Op::RetO(_) if self.osr.is_some() => {
                    // the registers go back to memory, and the machine runs this return
                    self.leave_to_machine(i)?;
                    dead = true;
                }
                Op::Ret(a) => {
                    let v = self.get(a);
                    self.commit();
                    self.b.ins().return_(&[v]);
                    dead = true;
                }
                // an instruction that is not native, outside every loop (`scan` checked): the
                // machine goes on from here
                _ if self.osr.is_some() => {
                    self.leave_to_machine(i)?;
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

    fn call_native(
        &mut self,
        target: u32,
        base: Reg,
        dst: Reg,
        free_args: Vec<ir::Value>,
    ) -> Option<()> {
        let steps = self.next_step();
        let d = self
            .b
            .ins()
            .load(types::I64, MemFlagsData::trusted(), self.ctx, DEPTH);
        let max = self
            .b
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
        let codes = self.codes;
        for (k, ty) in codes[target as usize]
            .param_tys
            .iter()
            .enumerate()
            .take(arity)
        {
            let r = base + k as Reg;
            if is_obj(ty) {
                let p = if self.osr.is_some() {
                    self.obj_addr(r)?
                } else {
                    self.b.use_var(self.ovars[usize::from(r)])
                };
                args.push(p);
            } else {
                args.push(self.get(r));
            }
        }
        args.extend(free_args);
        let call = self.b.ins().call(f, &args);
        let r = self.b.inst_results(call)[0];
        self.b
            .ins()
            .store(MemFlagsData::trusted(), d, self.ctx, DEPTH);
        let flag = self
            .b
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

        Some(())
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
        target: u32,
        stack: &[i64],
        ostack: &[Value],
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
        // a function that takes structures is given their addresses in place of the numbers
        let c = self.code_at(target);
        let mut args = [0i64; MAX_ARGS];
        let given = if c.jit.objs.get() {
            for (k, t) in c.param_tys.iter().enumerate() {
                args[k] = if is_obj(t) {
                    ostack.get(at + k)? as *const Value as i64
                } else {
                    stack[at + k]
                };
            }
            args.as_ptr()
        } else {
            stack[at..].as_ptr()
        };
        let v = f(&mut ctx, given);
        if ctx.bail != 0 {
            return None;
        }
        self.steps = ctx.steps as u64;
        Some(v)
    }

    /// Counts a jump back in `cur`; when it is hot, compiles it for an entry in the middle of its
    /// run. The native entry point when there is one.
    #[inline]
    pub(super) fn jit_osr_entry(&mut self, cur: u32, ostack: &[Value], base: usize) -> usize {
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
        // the function values the loop calls: the code each one runs now
        let code = self.code_at(cur);
        let mut closures = Vec::new();
        for (at, op) in code.ops.iter().enumerate() {
            if let Op::CallValue { f, site, .. } = *op
                && let Some(Value::Func(fv)) = ostack.get(base + usize::from(f))
                && fv.tenv.is_none()
                && let Some(t) = self.vm_func_target(fv, &code.sites[site as usize])
            {
                closures.push((at, t));
            }
        }
        let ok = jit.compile_osr(cur, &self.vm_codes, &closures);
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
        cur: u32,
        entry: usize,
        stack: &mut [i64],
        ostack: &mut [Value],
        base: usize,
        pc: usize,
    ) -> usize {
        // where the arrays that the code reads in place are, and how long: nothing in it can
        // move them
        let mut parts = [0i64; CBASE + MAX_CSITES * MAX_FREES];
        for (k, (reg, ety)) in self.code_at(cur).jit.osr_arrays.borrow().iter().enumerate() {
            let Some(Value::Array(rc)) = ostack.get(base + usize::from(*reg)) else {
                return self.jit_osr_refuse(cur, pc);
            };
            let a = rc.raw();
            let (p, n) = match (&*a, ety) {
                (Arr::Ints(v), Ety::I) => (v.as_ptr() as i64, v.len()),
                (Arr::Floats(v), Ety::F) => (v.as_ptr() as i64, v.len()),
                (Arr::Bools(v), Ety::B) => (v.as_ptr() as i64, v.len()),
                _ => {
                    drop(a);
                    return self.jit_osr_refuse(cur, pc);
                }
            };
            parts[k * 2] = p;
            parts[k * 2 + 1] = n as i64;
        }
        // the variables of the module the loop uses: numbers in the buffer, objects in a table
        let code = self.code_at(cur);
        let globals = code.jit.osr_globals.borrow().clone();
        let mut table = vec![Value::Unit; globals.len().max(1)];
        for (k, (g, obj, _)) in globals.iter().enumerate() {
            let gr = &code.globals[*g as usize];
            let mut c = gr.slot.get();
            let got = self.envs[gr.module].with_cached(&gr.name, &mut c, |v| slot_of(v, &gr.ty));
            gr.slot.set(c);
            match got {
                Some(Some(Slot::N(b))) if !*obj => parts[GTABLE + 1 + k] = b,
                Some(Some(Slot::O(o))) if *obj => table[k] = o,
                _ => return self.jit_osr_refuse(cur, pc),
            }
        }
        parts[GTABLE] = table.as_ptr() as i64;
        // the function values the loop calls: still the same code, and where their variables are
        let sites = code.jit.osr_closures.borrow().clone();
        for (j, (at, target)) in sites.iter().enumerate() {
            let Op::CallValue { f, site, .. } = code.ops[*at] else {
                return self.jit_osr_refuse(cur, pc);
            };
            let Some(Value::Func(fv)) = ostack.get(base + usize::from(f)) else {
                return self.jit_osr_refuse(cur, pc);
            };
            if fv.tenv.is_some()
                || self.vm_func_target(fv, &code.sites[site as usize]) != Some(*target)
            {
                return self.jit_osr_refuse(cur, pc);
            }
            for (m, fr) in self.code_at(*target).frees.iter().enumerate() {
                let mut c = fr.cache.get();
                let look = |slot: &mut Value| match (slot, &fr.ty) {
                    (Value::Int(x), Ty::Int) => Some(std::ptr::from_mut(x) as i64),
                    (Value::Float(x), Ty::Float) => Some(std::ptr::from_mut(x) as i64),
                    (Value::Bool(x), Ty::Bool) => Some(std::ptr::from_mut(x) as i64),
                    _ => None,
                };
                let p = match fr.global {
                    Some(g) => self.envs[g].update_cached(&fr.name, &mut c, look),
                    None => fv.env.update_cached(&fr.name, &mut c, look),
                };
                fr.cache.set(c);
                let Some(Some(p)) = p else {
                    return self.jit_osr_refuse(cur, pc);
                };
                parts[CBASE + j * MAX_FREES + m] = p;
            }
        }
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
        let f: extern "C" fn(*mut Ctx, *mut i64, i64, *mut Value, *mut i64) -> i64 =
            unsafe { std::mem::transmute(entry) };
        let at = f(
            &mut ctx,
            stack[base..].as_mut_ptr(),
            pc as i64,
            ostack.as_mut_ptr().wrapping_add(base),
            parts.as_mut_ptr(),
        );
        // the registers are back in memory, with the steps as they were at that instruction
        self.steps = ctx.steps as u64;
        // and the variables of the module the loop wrote
        for (k, (g, obj, stored)) in globals.iter().enumerate() {
            if *stored && !*obj {
                let gr = &code.globals[*g as usize];
                let mut c = gr.slot.get();
                let bits = [parts[GTABLE + 1 + k]];
                self.envs[gr.module].update_cached(&gr.name, &mut c, |slot| {
                    store_reg(slot, &bits, &[], 0, &gr.ty);
                });
                gr.slot.set(c);
            }
        }
        at as usize
    }

    /// The arrays are not stored the way the native code reads them: the machine goes on, and
    /// the native code is given up for this code.
    fn jit_osr_refuse(&self, cur: u32, pc: usize) -> usize {
        self.code_at(cur).jit.osr_refused.set(true);
        pc
    }
}
