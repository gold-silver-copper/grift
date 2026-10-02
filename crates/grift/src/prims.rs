//! The Rust primitives.
//!
//! This is the whole of the language that is not written in grift. `$vau`
//! is the only primitive operative; every other primitive is bound in the
//! primitive environment as an applicative.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::error::{Fault, GResult};
use crate::heap::{FrameKind, Object, Operative, SubCont};
use crate::machine::{Ctl, Machine};
use crate::value::{ObjId, PrimId, SymId, Value};

type PrimFn = fn(&mut Machine, &[Value], ObjId, Value) -> GResult<Ctl>;

/// A primitive: its name (empty for unnamed primitives that only exist
/// closed over data) and its implementation.
struct Prim {
    name: &'static str,
    f: PrimFn,
}

macro_rules! prims {
    ($($id:ident = $name:literal => $f:expr,)*) => {
        const PRIMS: &[Prim] = &[$(Prim { name: $name, f: $f },)*];
        prims!(@ids 0u16; $($id,)*);
    };
    (@ids $n:expr; ) => {};
    (@ids $n:expr; $id:ident, $($rest:ident,)*) => {
        #[allow(dead_code)]
        pub(crate) const $id: PrimId = PrimId($n);
        prims!(@ids $n + 1u16; $($rest,)*);
    };
}

prims! {
    VAU = "$vau" => |_, _, _, _| unreachable!("$vau is dispatched by the machine"),
    WRAP = "wrap" => p_wrap,
    UNWRAP = "unwrap" => p_unwrap,
    EVAL = "eval" => p_eval,
    MAKE_ENVIRONMENT = "make-environment" => p_make_environment,
    ENVIRONMENT_BIND = "environment-bind" => p_environment_bind,
    ENVIRONMENT_BIND_RECURSIVE = "environment-bind-recursive" => p_environment_bind_recursive,
    MAKE_DECLARATION = "make-declaration" => p_make_declaration,
    APPLY_DECLARATION = "apply-declaration" => p_apply_declaration,
    SELECT = "select" => p_select,
    CONS = "cons" => p_cons,
    EQ = "eq?" => p_eq,
    MAKE_ENCAPSULATION_TYPE = "make-encapsulation-type" => p_make_encapsulation_type,
    ENCAP_CONSTRUCT = "" => p_encap_construct,
    ENCAP_TEST = "" => p_encap_test,
    ENCAP_OPEN = "" => p_encap_open,
    PAIR_P = "pair?" => |m, a, _, _| pred(m, "pair?", a, |o| matches!(o, Some(Object::Pair(..))), |_| false),
    SYMBOL_P = "symbol?" => |m, a, _, _| pred(m, "symbol?", a, |_| false, |v| matches!(v, Value::Sym(_))),
    INTEGER_P = "integer?" => |m, a, _, _| pred(m, "integer?", a, |_| false, |v| matches!(v, Value::Int(_))),
    CHAR_P = "char?" => |m, a, _, _| pred(m, "char?", a, |_| false, |v| matches!(v, Value::Char(_))),
    STRING_P = "string?" => |m, a, _, _| pred(m, "string?", a, |o| matches!(o, Some(Object::Str(_))), |_| false),
    ENVIRONMENT_P = "environment?" => |m, a, _, _| pred(m, "environment?", a, |o| matches!(o, Some(Object::Env(_))), |_| false),
    OPERATIVE_P = "operative?" => |m, a, _, _| {
        let [v] = arity(m, "operative?", a)?;
        Ok(Ctl::Ret(Value::Bool(m.ctx.is_operative(v))))
    },
    APPLICATIVE_P = "applicative?" => |m, a, _, _| pred(m, "applicative?", a, |o| matches!(o, Some(Object::Applicative(_))), |_| false),
    DECLARATION_P = "declaration?" => |m, a, _, _| pred(m, "declaration?", a, |o| matches!(o, Some(Object::Declaration(_))), |_| false),
    PROMPT_P = "prompt?" => |m, a, _, _| pred(m, "prompt?", a, |o| matches!(o, Some(Object::Prompt)), |_| false),
    SUB_CONTINUATION_P = "sub-continuation?" => |m, a, _, _| pred(m, "sub-continuation?", a, |o| matches!(o, Some(Object::SubCont(_))), |_| false),
    MAKE_PROMPT = "make-prompt" => p_make_prompt,
    PUSH_PROMPT = "push-prompt" => p_push_prompt,
    WITH_SUB_CONT = "with-sub-cont" => p_with_sub_cont,
    PUSH_SUB_CONT = "push-sub-cont" => p_push_sub_cont,
    ADD = "+" => |m, a, _, _| int2(m, "+", a, i64::checked_add),
    SUB = "-" => |m, a, _, _| int2(m, "-", a, i64::checked_sub),
    MUL = "*" => |m, a, _, _| int2(m, "*", a, i64::checked_mul),
    DIV = "div" => p_div,
    LT = "<?" => p_lt,
    STRING_TO_LIST = "string->list" => p_string_to_list,
    LIST_TO_STRING = "list->string" => p_list_to_string,
    STRING_TO_SYMBOL = "string->symbol" => p_string_to_symbol,
    SYMBOL_TO_STRING = "symbol->string" => p_symbol_to_string,
    CHAR_TO_INTEGER = "char->integer" => p_char_to_integer,
    INTEGER_TO_CHAR = "integer->char" => p_integer_to_char,
    STRING_EQ = "string=?" => p_string_eq,
    STRING_LENGTH = "string-length" => p_string_length,
    READ = "read" => p_read,
    WRITE = "write" => |m, a, _, _| {
        let [v] = arity(m, "write", a)?;
        let s = m.ctx.write(v);
        Ok(Ctl::Ret(m.ctx.string(&s)))
    },
    DISPLAY = "display" => |m, a, _, _| {
        let [v] = arity(m, "display", a)?;
        let s = m.ctx.display(v);
        Ok(Ctl::Ret(m.ctx.string(&s)))
    },
}

/// Build the primitive environment.
pub(crate) fn install(m: &mut Machine) -> ObjId {
    let mut env = m.prim_env;
    env = m.bind_prim(env, "$vau", Value::Prim(VAU));
    for (i, p) in PRIMS.iter().enumerate().skip(1) {
        if p.name.is_empty() {
            continue;
        }
        let id = PrimId(u16::try_from(i).expect("too many primitives"));
        let app = m.prim_applicative(id);
        env = m.bind_prim(env, p.name, app);
    }
    env = m.bind_prim(env, "error-prompt", Value::Obj(m.error_prompt));
    env
}

/// Call primitive `id` with evaluated `args`.
pub(crate) fn call(
    m: &mut Machine,
    id: PrimId,
    data: Value,
    args: &[Value],
    env: ObjId,
) -> GResult<Ctl> {
    (PRIMS[id.0 as usize].f)(m, args, env, data)
}

/// Destructure exactly `N` arguments.
fn arity<const N: usize>(m: &Machine, name: &str, args: &[Value]) -> GResult<[Value; N]> {
    args.try_into().map_err(|_| {
        let plural = if N == 1 { "" } else { "s" };
        let mut f = Fault::new(
            "arity-error",
            format!("{name} expects {N} argument{plural}, got {}", args.len()),
        );
        for &a in args {
            f = f.with(a);
        }
        let _ = m;
        f
    })
}

fn pred(
    m: &mut Machine,
    name: &str,
    args: &[Value],
    obj: fn(Option<&Object>) -> bool,
    imm: fn(Value) -> bool,
) -> GResult<Ctl> {
    let [v] = arity(m, name, args)?;
    Ok(Ctl::Ret(Value::Bool(imm(v) || obj(m.ctx.obj(v)))))
}

// ---------------------------------------------------------------------------
// $vau and combiners

/// `($vau ptree eparam body)`: the only primitive operative.
pub(crate) fn vau(m: &mut Machine, operands: Value, env: ObjId) -> GResult<Ctl> {
    let items = m
        .ctx
        .list_to_vec(operands)
        .ok_or_else(|| m.ctx.type_error("a proper list of operands", operands))?;
    let [ptree, eparam, body]: [Value; 3] = items.as_slice().try_into().map_err(|_| {
        Fault::new(
            "arity-error",
            format!(
                "$vau expects a parameter tree, an environment parameter and one body expression, got {} operands",
                items.len()
            ),
        )
        .with(operands)
    })?;
    let mut syms = Vec::new();
    m.ctx.ptree_symbols(ptree, &mut syms)?;
    m.ctx.check_eparam(eparam, &syms)?;
    let op = m.ctx.heap.alloc(Object::Operative(Operative {
        ptree,
        eparam,
        body,
        env,
    }));
    Ok(Ctl::Ret(Value::Obj(op)))
}

fn p_wrap(m: &mut Machine, args: &[Value], _: ObjId, _: Value) -> GResult<Ctl> {
    let [c] = arity(m, "wrap", args)?;
    if !m.ctx.is_combiner(c) {
        return Err(m.ctx.type_error("a combiner", c));
    }
    Ok(Ctl::Ret(Value::Obj(
        m.ctx.heap.alloc(Object::Applicative(c)),
    )))
}

fn p_unwrap(m: &mut Machine, args: &[Value], _: ObjId, _: Value) -> GResult<Ctl> {
    let [a] = arity(m, "unwrap", args)?;
    match m.ctx.obj(a) {
        Some(Object::Applicative(inner)) => Ok(Ctl::Ret(*inner)),
        _ => Err(m.ctx.type_error("an applicative", a)),
    }
}

fn p_eval(m: &mut Machine, args: &[Value], _: ObjId, _: Value) -> GResult<Ctl> {
    let [expr, env] = arity(m, "eval", args)?;
    let env = m.ctx.expect_env(env)?;
    Ok(Ctl::Eval(expr, env))
}

// ---------------------------------------------------------------------------
// Environments and declarations

fn p_make_environment(m: &mut Machine, args: &[Value], _: ObjId, _: Value) -> GResult<Ctl> {
    let [] = arity(m, "make-environment", args)?;
    Ok(Ctl::Ret(Value::Obj(m.ctx.empty_env())))
}

fn p_environment_bind(m: &mut Machine, args: &[Value], _: ObjId, _: Value) -> GResult<Ctl> {
    let [env, ptree, value] = arity(m, "environment-bind", args)?;
    let env = m.ctx.expect_env(env)?;
    let mut syms = Vec::new();
    m.ctx.ptree_symbols(ptree, &mut syms)?;
    let mut binds = Vec::new();
    m.ctx.match_ptree(ptree, value, false, &mut binds)?;
    let child = m
        .ctx
        .extend(env, binds.into_iter().map(|(s, v)| (s, Some(v))));
    Ok(Ctl::Ret(Value::Obj(child)))
}

fn p_environment_bind_recursive(
    m: &mut Machine,
    args: &[Value],
    _: ObjId,
    _: Value,
) -> GResult<Ctl> {
    let [env, ptree, expr] = arity(m, "environment-bind-recursive", args)?;
    let env = m.ctx.expect_env(env)?;
    let mut syms = Vec::new();
    m.ctx.ptree_symbols(ptree, &mut syms)?;
    let child = m.ctx.extend(env, syms.into_iter().map(|s| (s, None)));
    m.push_frame(FrameKind::InitRec { env: child, ptree });
    Ok(Ctl::Eval(expr, child))
}

/// Initialize the bindings of `env` (created by
/// `environment-bind-recursive`) from `value`.
pub(crate) fn init_recursive(
    m: &mut Machine,
    env: ObjId,
    ptree: Value,
    value: Value,
) -> GResult<()> {
    let mut binds: Vec<(SymId, Value)> = Vec::new();
    m.ctx.match_ptree(ptree, value, false, &mut binds)?;
    let e = m.ctx.heap.env(env);
    let start = e.len as usize - binds.len();
    let log = m.ctx.heap.log_mut(e.log);
    for (i, (sym, _)) in binds.iter().enumerate() {
        let slot = &mut log.entries[start + i];
        debug_assert_eq!(slot.0, *sym);
        if slot.1.is_some() {
            let name = m.ctx.syms.name(*sym);
            return Err(Fault::new(
                "reinitialized-binding",
                format!("{name} was already initialized; bindings never change"),
            )
            .with(Value::Sym(*sym)));
        }
    }
    for (i, (_, v)) in binds.iter().enumerate() {
        log.entries[start + i].1 = Some(*v);
    }
    Ok(())
}

fn p_make_declaration(m: &mut Machine, args: &[Value], _: ObjId, _: Value) -> GResult<Ctl> {
    let [f] = arity(m, "make-declaration", args)?;
    if !m.ctx.is_applicative(f) {
        return Err(m.ctx.type_error("an applicative", f));
    }
    Ok(Ctl::Ret(Value::Obj(
        m.ctx.heap.alloc(Object::Declaration(f)),
    )))
}

/// `(apply-declaration decl env)`: call the declaration's transformer on
/// `env`; its result must be an environment.
///
/// Not derivable: a declaration is opaque, and both bodies (library) and the
/// host's top level need to apply one.
fn p_apply_declaration(m: &mut Machine, args: &[Value], env: ObjId, _: Value) -> GResult<Ctl> {
    let [decl, target] = arity(m, "apply-declaration", args)?;
    let f = match m.ctx.obj(decl) {
        Some(Object::Declaration(f)) => *f,
        _ => return Err(m.ctx.type_error("a declaration", decl)),
    };
    m.ctx.expect_env(target)?;
    m.push_frame(FrameKind::CheckEnv);
    m.call(f, &[target], env)
}

// ---------------------------------------------------------------------------
// Data

fn p_select(m: &mut Machine, args: &[Value], _: ObjId, _: Value) -> GResult<Ctl> {
    let [b, x, y] = arity(m, "select", args)?;
    Ok(Ctl::Ret(if m.ctx.expect_bool(b)? { x } else { y }))
}

fn p_cons(m: &mut Machine, args: &[Value], _: ObjId, _: Value) -> GResult<Ctl> {
    let [a, d] = arity(m, "cons", args)?;
    Ok(Ctl::Ret(m.ctx.cons(a, d)))
}

/// `eq?` is identity. Immutable structured data (pairs, strings,
/// encapsulated values) has no identity, so comparing two structured values
/// of the same kind is an error; comparing anything else (including a
/// structured value against a value of another kind, e.g. `(eq? x ())`)
/// answers without exposing identity.
fn p_eq(m: &mut Machine, args: &[Value], _: ObjId, _: Value) -> GResult<Ctl> {
    let [a, b] = arity(m, "eq?", args)?;
    let kind = |v: Value| match m.ctx.obj(v) {
        Some(Object::Pair(..)) => 1,
        Some(Object::Str(_)) => 2,
        Some(Object::Encap(..)) => 3,
        _ => 0,
    };
    let (ka, kb) = (kind(a), kind(b));
    if ka != 0 && ka == kb {
        return Err(Fault::new(
            "type-error",
            format!(
                "eq? cannot compare two {}s: immutable structured data has no identity; use equal?",
                m.ctx.type_name(a)
            ),
        )
        .with(a)
        .with(b));
    }
    Ok(Ctl::Ret(Value::Bool(a == b)))
}

fn p_make_encapsulation_type(m: &mut Machine, args: &[Value], _: ObjId, _: Value) -> GResult<Ctl> {
    let [] = arity(m, "make-encapsulation-type", args)?;
    let tag = m.ctx.next_tag;
    m.ctx.next_tag = tag.checked_add(1).expect("too many encapsulation types");
    let data = Value::Int(i64::from(tag));
    let mut parts = [Value::Nil; 3];
    for (slot, id) in parts
        .iter_mut()
        .zip([ENCAP_CONSTRUCT, ENCAP_TEST, ENCAP_OPEN])
    {
        let core = Value::Obj(m.ctx.heap.alloc(Object::PrimData(id, data)));
        *slot = Value::Obj(m.ctx.heap.alloc(Object::Applicative(core)));
    }
    Ok(Ctl::Ret(m.ctx.list(&parts)))
}

fn tag_of(data: Value) -> u32 {
    u32::try_from(data.as_int().expect("encapsulation tag")).expect("encapsulation tag")
}

fn p_encap_construct(m: &mut Machine, args: &[Value], _: ObjId, data: Value) -> GResult<Ctl> {
    let [v] = arity(m, "encapsulation constructor", args)?;
    let e = m.ctx.heap.alloc(Object::Encap(tag_of(data), v));
    Ok(Ctl::Ret(Value::Obj(e)))
}

fn p_encap_test(m: &mut Machine, args: &[Value], _: ObjId, data: Value) -> GResult<Ctl> {
    let [v] = arity(m, "encapsulation predicate", args)?;
    let tag = tag_of(data);
    let ok = matches!(m.ctx.obj(v), Some(Object::Encap(t, _)) if *t == tag);
    Ok(Ctl::Ret(Value::Bool(ok)))
}

fn p_encap_open(m: &mut Machine, args: &[Value], _: ObjId, data: Value) -> GResult<Ctl> {
    let [v] = arity(m, "encapsulation accessor", args)?;
    let tag = tag_of(data);
    match m.ctx.obj(v) {
        Some(Object::Encap(t, inner)) if *t == tag => Ok(Ctl::Ret(*inner)),
        _ => Err(m.ctx.type_error("a value of this encapsulation type", v)),
    }
}

// ---------------------------------------------------------------------------
// Delimited control (Dybvig, Peyton Jones and Sabry)

fn expect_prompt(m: &Machine, v: Value) -> GResult<ObjId> {
    match v {
        Value::Obj(id) if matches!(m.ctx.heap.get(id), Object::Prompt) => Ok(id),
        _ => Err(m.ctx.type_error("a prompt", v)),
    }
}

fn expect_applicative(m: &Machine, v: Value) -> GResult<()> {
    if m.ctx.is_applicative(v) {
        Ok(())
    } else {
        Err(m.ctx.type_error("an applicative", v))
    }
}

fn p_make_prompt(m: &mut Machine, args: &[Value], _: ObjId, _: Value) -> GResult<Ctl> {
    let [] = arity(m, "make-prompt", args)?;
    Ok(Ctl::Ret(Value::Obj(m.ctx.heap.alloc(Object::Prompt))))
}

/// `(push-prompt p thunk)`: call `thunk` under a frame delimited by `p`.
fn p_push_prompt(m: &mut Machine, args: &[Value], env: ObjId, _: Value) -> GResult<Ctl> {
    let [p, thunk] = arity(m, "push-prompt", args)?;
    let prompt = expect_prompt(m, p)?;
    expect_applicative(m, thunk)?;
    m.push_frame(FrameKind::Prompt {
        prompt,
        host: false,
    });
    m.call(thunk, &[], env)
}

/// `(with-sub-cont p f)`: capture the frames up to the innermost `p`,
/// remove them and the prompt, and call `f` with them.
fn p_with_sub_cont(m: &mut Machine, args: &[Value], env: ObjId, _: Value) -> GResult<Ctl> {
    let [p, f] = arity(m, "with-sub-cont", args)?;
    let prompt = expect_prompt(m, p)?;
    expect_applicative(m, f)?;
    let Some((frame, host)) = m.find_prompt(prompt) else {
        return Err(Fault::new(
            "no-prompt",
            "with-sub-cont: no matching prompt is installed",
        )
        .with(p));
    };
    let top = m.k.expect("a prompt frame was found");
    let sc = Value::Obj(
        m.ctx
            .heap
            .alloc(Object::SubCont(SubCont { top, bottom: frame })),
    );
    m.k = m.ctx.heap.frame(frame).parent;
    if host {
        // The host's error prompt: whatever `f` returns is the error the
        // host reports.
        m.k = None;
        m.push_frame(FrameKind::HostError);
    }
    m.call(f, &[sc], env)
}

/// `(push-sub-cont k thunk)`: reinstate the frames of `k` on top of the
/// current continuation and call `thunk` in them. Frames are copied, so a
/// subcontinuation may be reinstated any number of times.
fn p_push_sub_cont(m: &mut Machine, args: &[Value], env: ObjId, _: Value) -> GResult<Ctl> {
    let [k, thunk] = arity(m, "push-sub-cont", args)?;
    let sc = match m.ctx.obj(k) {
        Some(Object::SubCont(sc)) => *sc,
        _ => return Err(m.ctx.type_error("a sub-continuation", k)),
    };
    expect_applicative(m, thunk)?;
    let mut segment = Vec::new();
    let mut cur = sc.top;
    while cur != sc.bottom {
        let frame = m.ctx.heap.frame(cur);
        segment.push(frame.kind);
        cur = frame
            .parent
            .expect("sub-continuation segment ends at its prompt");
    }
    for kind in segment.into_iter().rev() {
        m.push_frame(kind);
    }
    m.call(thunk, &[], env)
}

// ---------------------------------------------------------------------------
// Integers

fn int2(
    m: &mut Machine,
    name: &str,
    args: &[Value],
    op: fn(i64, i64) -> Option<i64>,
) -> GResult<Ctl> {
    let [a, b] = arity(m, name, args)?;
    let x = m.ctx.expect_int(a)?;
    let y = m.ctx.expect_int(b)?;
    op(x, y).map(|n| Ctl::Ret(Value::Int(n))).ok_or_else(|| {
        Fault::new(
            "overflow",
            format!("{name}: result does not fit in 64 bits"),
        )
        .with(a)
        .with(b)
    })
}

/// Euclidean division: `(div n d)` is the `q` with `n = d*q + r`,
/// `0 <= r < |d|`.
fn p_div(m: &mut Machine, args: &[Value], _: ObjId, _: Value) -> GResult<Ctl> {
    let [a, b] = arity(m, "div", args)?;
    let x = m.ctx.expect_int(a)?;
    let y = m.ctx.expect_int(b)?;
    if y == 0 {
        return Err(Fault::new("division-by-zero", "div: division by zero").with(a));
    }
    x.checked_div_euclid(y)
        .map(|n| Ctl::Ret(Value::Int(n)))
        .ok_or_else(|| {
            Fault::new("overflow", "div: result does not fit in 64 bits")
                .with(a)
                .with(b)
        })
}

fn p_lt(m: &mut Machine, args: &[Value], _: ObjId, _: Value) -> GResult<Ctl> {
    let [a, b] = arity(m, "<?", args)?;
    let x = m.ctx.expect_int(a)?;
    let y = m.ctx.expect_int(b)?;
    Ok(Ctl::Ret(Value::Bool(x < y)))
}

// ---------------------------------------------------------------------------
// Text

fn p_string_to_list(m: &mut Machine, args: &[Value], _: ObjId, _: Value) -> GResult<Ctl> {
    let [s] = arity(m, "string->list", args)?;
    let chars: Vec<Value> = m.ctx.expect_str(s)?.chars().map(Value::Char).collect();
    Ok(Ctl::Ret(m.ctx.list(&chars)))
}

fn p_list_to_string(m: &mut Machine, args: &[Value], _: ObjId, _: Value) -> GResult<Ctl> {
    let [l] = arity(m, "list->string", args)?;
    let items = m.ctx.expect_list(l)?;
    let mut s = String::with_capacity(items.len());
    for v in items {
        s.push(m.ctx.expect_char(v)?);
    }
    Ok(Ctl::Ret(m.ctx.string(&s)))
}

fn p_string_to_symbol(m: &mut Machine, args: &[Value], _: ObjId, _: Value) -> GResult<Ctl> {
    let [s] = arity(m, "string->symbol", args)?;
    let name = String::from(m.ctx.expect_str(s)?);
    Ok(Ctl::Ret(m.ctx.symbol(&name)))
}

fn p_symbol_to_string(m: &mut Machine, args: &[Value], _: ObjId, _: Value) -> GResult<Ctl> {
    let [s] = arity(m, "symbol->string", args)?;
    let name = String::from(
        m.ctx
            .symbol_name(s)
            .ok_or_else(|| m.ctx.type_error("a symbol", s))?,
    );
    Ok(Ctl::Ret(m.ctx.string(&name)))
}

fn p_char_to_integer(m: &mut Machine, args: &[Value], _: ObjId, _: Value) -> GResult<Ctl> {
    let [c] = arity(m, "char->integer", args)?;
    Ok(Ctl::Ret(Value::Int(i64::from(u32::from(
        m.ctx.expect_char(c)?,
    )))))
}

fn p_integer_to_char(m: &mut Machine, args: &[Value], _: ObjId, _: Value) -> GResult<Ctl> {
    let [n] = arity(m, "integer->char", args)?;
    let i = m.ctx.expect_int(n)?;
    u32::try_from(i)
        .ok()
        .and_then(char::from_u32)
        .map(|c| Ctl::Ret(Value::Char(c)))
        .ok_or_else(|| {
            Fault::new("range-error", "integer->char: not a Unicode scalar value").with(n)
        })
}

/// Performance primitive: derivable via `string->list`, but string
/// comparison is pervasive (`equal?`).
fn p_string_eq(m: &mut Machine, args: &[Value], _: ObjId, _: Value) -> GResult<Ctl> {
    let [a, b] = arity(m, "string=?", args)?;
    let eq = m.ctx.expect_str(a)? == m.ctx.expect_str(b)?;
    Ok(Ctl::Ret(Value::Bool(eq)))
}

/// Performance primitive: derivable as `(length (string->list s))`, which
/// allocates a list per call.
fn p_string_length(m: &mut Machine, args: &[Value], _: ObjId, _: Value) -> GResult<Ctl> {
    let [s] = arity(m, "string-length", args)?;
    let n = m.ctx.expect_str(s)?.chars().count();
    Ok(Ctl::Ret(Value::Int(
        i64::try_from(n).expect("string length"),
    )))
}

/// `(read string)`: the single datum written in `string`.
fn p_read(m: &mut Machine, args: &[Value], _: ObjId, _: Value) -> GResult<Ctl> {
    let [s] = arity(m, "read", args)?;
    let src = String::from(m.ctx.expect_str(s)?);
    let forms = m.ctx.read_all(&src)?;
    match forms.as_slice() {
        [v] => Ok(Ctl::Ret(*v)),
        _ => Err(Fault::new(
            "parse-error",
            format!("read: expected exactly one datum, found {}", forms.len()),
        )
        .with(s)),
    }
}
