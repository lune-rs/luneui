//! `api-docs compat`: does a candidate build of the five library crates keep every public item of a
//! base build working for an app written against the base?
//!
//! `api-docs compat --base-json <dir> --head-json <dir> --base-meta <file> --head-meta <file>`
//! compares the rustdoc JSON of the two builds and the `cargo metadata --no-deps` output of the two
//! trees. `api-docs compat --list-members <meta>` prints the workspace members of one metadata
//! file. `scripts/check-compat.sh` makes the inputs and runs the parts that build and run code.
//!
//! Items are matched by the path a consumer writes (re-exports included, so an item moved with a
//! `pub use` kept at the old path still matches). Types are compared as strings built from the
//! definition path in each crate's `paths` table (never an `Id`, never the path as written at the
//! use site) with generic and lifetime names replaced by their position, so `Insets` and
//! `crate::layout::Insets` are one type and a renamed lifetime is no break. A definition that moved
//! (same consumer path, other definition path) is mapped back to its base path on the head side.
//!
//! The rules fail closed: an item kind this code does not handle is a finding ("cannot classify").
//! The comparison is conservative where Rust itself is loose: a changed bound or a loosened
//! signature is reported, never passed silently.
//!
//! Exit 0 held, 1 at least one `break:` line, 2 a setup problem (unreadable input, a JSON format
//! this tool does not read).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path as FsPath;
use std::process::ExitCode;

use rustdoc_types::{
    Abi, AssocItemConstraint, AssocItemConstraintKind, Attribute, Crate, DynTrait, Function,
    FunctionPointer, GenericArg, GenericArgs, GenericBound, GenericParamDef, GenericParamDefKind,
    Generics, Item, ItemEnum, Path, PreciseCapturingArg, StructKind, Term, TraitBoundModifier,
    Type, VariantKind, WherePredicate,
};

use crate::view::{Kind, Ns, view};
use crate::world::{CRATES, Fail, ItemRef, Res, World};

// ------------------------------------------------------------------------------------------------
// Canonical strings
// ------------------------------------------------------------------------------------------------

/// Writes the types, bounds and signatures of one crate's JSON as canonical strings.
pub(crate) struct Canon<'a> {
    data: &'a Crate,
    /// Head-side map from a definition path to the base path of the same item.
    rename: Option<&'a HashMap<String, String>>,
    /// Generic and lifetime names in scope, in the order they were introduced.
    scope: Vec<String>,
}

fn join(parts: Vec<String>, sep: &str) -> String {
    parts.join(sep)
}

fn sorted(mut parts: Vec<String>) -> Vec<String> {
    parts.sort();
    parts
}

impl<'a> Canon<'a> {
    pub(crate) fn new(data: &'a Crate, rename: Option<&'a HashMap<String, String>>) -> Self {
        Canon {
            data,
            rename,
            scope: Vec::new(),
        }
    }

    fn mark(&self) -> usize {
        self.scope.len()
    }

    fn reset(&mut self, mark: usize) {
        self.scope.truncate(mark);
    }

    /// A generic or lifetime name: its position when in scope, else as written (`Self`, `'static`).
    fn name(&self, n: &str) -> String {
        match self.scope.iter().rposition(|s| s == n) {
            Some(i) => format!("#{i}"),
            None => n.to_string(),
        }
    }

    fn lifetime(&self, l: &str) -> String {
        self.name(l)
    }

    /// The definition path of the item a path names, else the path as written.
    fn path_name(&self, p: &Path) -> String {
        let full = match self.data.paths.get(&p.id) {
            Some(s) => s.path.join("::"),
            None => p.path.clone(),
        };
        match self.rename.and_then(|r| r.get(&full)) {
            Some(base) => base.clone(),
            None => full,
        }
    }

    pub(crate) fn path(&mut self, p: &Path) -> String {
        let args = match &p.args {
            Some(a) => self.args(a),
            None => String::new(),
        };
        format!("{}{args}", self.path_name(p))
    }

    fn args(&mut self, a: &GenericArgs) -> String {
        match a {
            GenericArgs::AngleBracketed { args, constraints } => {
                let mut parts: Vec<String> = Vec::new();
                for a in args {
                    parts.push(match a {
                        GenericArg::Lifetime(l) => self.lifetime(l),
                        GenericArg::Type(t) => self.ty(t),
                        GenericArg::Const(c) => format!("const {}", c.expr),
                        GenericArg::Infer => "_".to_string(),
                    });
                }
                for c in constraints {
                    parts.push(self.constraint(c));
                }
                format!("<{}>", join(parts, ","))
            }
            GenericArgs::Parenthesized { inputs, output } => {
                let ins = inputs.iter().map(|t| self.ty(t)).collect();
                let out = match output {
                    Some(t) => self.ty(t),
                    None => "()".to_string(),
                };
                format!("({})->{out}", join(ins, ","))
            }
            GenericArgs::ReturnTypeNotation => "(..)".to_string(),
        }
    }

    fn constraint(&mut self, c: &AssocItemConstraint) -> String {
        let args = match &c.args {
            Some(a) => self.args(a),
            None => String::new(),
        };
        let rhs = match &c.binding {
            AssocItemConstraintKind::Equality(t) => format!("={}", self.term(t)),
            AssocItemConstraintKind::Constraint(bs) => format!(":{}", self.bounds(bs)),
        };
        format!("{}{args}{rhs}", c.name)
    }

    fn term(&mut self, t: &Term) -> String {
        match t {
            Term::Type(t) => self.ty(t),
            Term::Constant(c) => format!("const {}", c.expr),
        }
    }

    pub(crate) fn ty(&mut self, t: &Type) -> String {
        match t {
            Type::ResolvedPath(p) => self.path(p),
            Type::DynTrait(d) => self.dyn_trait(d),
            Type::Generic(n) => self.name(n),
            Type::Primitive(n) => n.clone(),
            Type::FunctionPointer(f) => self.fn_pointer(f),
            Type::Tuple(ts) => {
                let parts = ts.iter().map(|t| self.ty(t)).collect();
                format!("({})", join(parts, ","))
            }
            Type::Slice(inner) => format!("[{}]", self.ty(inner)),
            Type::Array { type_, len } => format!("[{};{len}]", self.ty(type_)),
            Type::Pat {
                type_,
                __pat_unstable_do_not_use,
            } => format!("pat({} is {__pat_unstable_do_not_use})", self.ty(type_)),
            Type::ImplTrait(bs) => format!("impl {}", self.bounds(bs)),
            Type::Infer => "_".to_string(),
            Type::RawPointer { is_mutable, type_ } => format!(
                "*{} {}",
                if *is_mutable { "mut" } else { "const" },
                self.ty(type_)
            ),
            Type::BorrowedRef {
                lifetime,
                is_mutable,
                type_,
            } => {
                let lt = lifetime
                    .as_deref()
                    .map(|l| format!("{} ", self.lifetime(l)))
                    .unwrap_or_default();
                let m = if *is_mutable { "mut " } else { "" };
                format!("&{lt}{m}{}", self.ty(type_))
            }
            Type::QualifiedPath {
                name,
                args,
                self_type,
                trait_,
            } => {
                let s = self.ty(self_type);
                let tr = match trait_ {
                    Some(p) => format!(" as {}", self.path(p)),
                    None => String::new(),
                };
                let a = match args {
                    Some(a) => self.args(a),
                    None => String::new(),
                };
                format!("<{s}{tr}>::{name}{a}")
            }
        }
    }

    fn dyn_trait(&mut self, d: &DynTrait) -> String {
        let mut parts = Vec::new();
        for pt in &d.traits {
            let m = self.mark();
            let hr = self.hrtb(&pt.generic_params);
            parts.push(format!("{hr}{}", self.path(&pt.trait_)));
            self.reset(m);
        }
        let lt = match &d.lifetime {
            Some(l) => format!("+{}", self.lifetime(l)),
            None => String::new(),
        };
        format!("dyn {}{lt}", join(parts, "+"))
    }

    fn fn_pointer(&mut self, f: &FunctionPointer) -> String {
        let m = self.mark();
        let hr = self.hrtb(&f.generic_params);
        let ins = f.sig.inputs.iter().map(|(_, t)| self.ty(t)).collect();
        let out = match &f.sig.output {
            Some(t) => self.ty(t),
            None => "()".to_string(),
        };
        self.reset(m);
        format!(
            "{hr}{} fn({}){}->{out}",
            header_word(
                f.header.is_unsafe,
                f.header.is_async,
                f.header.is_const,
                &f.header.abi
            ),
            join(ins, ","),
            if f.sig.is_c_variadic { "..." } else { "" }
        )
    }

    /// `for<...> ` for a bound's own parameters; leaves the names in scope (the caller resets).
    fn hrtb(&mut self, ps: &[GenericParamDef]) -> String {
        if ps.is_empty() {
            return String::new();
        }
        format!("for<{}> ", self.params(ps))
    }

    fn bounds(&mut self, bs: &[GenericBound]) -> String {
        let parts = bs.iter().map(|b| self.bound(b)).collect();
        join(sorted(parts), "+")
    }

    fn bound(&mut self, b: &GenericBound) -> String {
        match b {
            GenericBound::TraitBound {
                trait_,
                generic_params,
                modifier,
            } => {
                let m = self.mark();
                let hr = self.hrtb(generic_params);
                let modifier = match modifier {
                    TraitBoundModifier::None => "",
                    TraitBoundModifier::Maybe => "?",
                    TraitBoundModifier::MaybeConst => "~const ",
                };
                let s = format!("{hr}{modifier}{}", self.path(trait_));
                self.reset(m);
                s
            }
            GenericBound::Outlives(l) => self.lifetime(l),
            GenericBound::Use(args) => {
                let parts = args
                    .iter()
                    .map(|a| match a {
                        PreciseCapturingArg::Lifetime(l) => self.lifetime(l),
                        PreciseCapturingArg::Param(p) => self.name(p),
                    })
                    .collect();
                format!("use<{}>", join(sorted(parts), ","))
            }
        }
    }

    /// Pushes the parameter names, then writes each parameter.
    fn params(&mut self, ps: &[GenericParamDef]) -> String {
        for p in ps {
            self.scope.push(p.name.clone());
        }
        let mut parts = Vec::new();
        for p in ps {
            parts.push(match &p.kind {
                GenericParamDefKind::Lifetime { outlives } => {
                    let o = outlives.iter().map(|l| self.lifetime(l)).collect();
                    format!("lt:{}", join(sorted(o), "+"))
                }
                GenericParamDefKind::Type {
                    bounds,
                    default,
                    is_synthetic,
                } => {
                    let d = match default {
                        Some(t) => format!("={}", self.ty(t)),
                        None => String::new(),
                    };
                    format!(
                        "ty{}:{}{d}",
                        if *is_synthetic { "~" } else { "" },
                        self.bounds(bounds)
                    )
                }
                GenericParamDefKind::Const { type_, default } => format!(
                    "const:{}{}",
                    self.ty(type_),
                    default
                        .as_deref()
                        .map(|d| format!("={d}"))
                        .unwrap_or_default()
                ),
            });
        }
        join(parts, ";")
    }

    /// Parameters and where clauses; the names stay in scope (the caller resets).
    pub(crate) fn generics(&mut self, g: &Generics) -> String {
        let params = self.params(&g.params);
        let mut preds = Vec::new();
        for w in &g.where_predicates {
            preds.push(match w {
                WherePredicate::BoundPredicate {
                    type_,
                    bounds,
                    generic_params,
                } => {
                    let m = self.mark();
                    let hr = self.hrtb(generic_params);
                    let s = format!("{hr}{}:{}", self.ty(type_), self.bounds(bounds));
                    self.reset(m);
                    s
                }
                WherePredicate::LifetimePredicate { lifetime, outlives } => {
                    let o = outlives.iter().map(|l| self.lifetime(l)).collect();
                    format!("{}:{}", self.lifetime(lifetime), join(sorted(o), "+"))
                }
                WherePredicate::EqPredicate { lhs, rhs } => {
                    format!("{}={}", self.ty(lhs), self.term(rhs))
                }
            });
        }
        format!("<{params}> where {}", join(sorted(preds), ";"))
    }
}

fn header_word(is_unsafe: bool, is_async: bool, is_const: bool, abi: &Abi) -> String {
    format!(
        "{}{}{}{abi:?}",
        if is_const { "const " } else { "" },
        if is_async { "async " } else { "" },
        if is_unsafe { "unsafe " } else { "" },
    )
}

/// A function as two strings: the header and the rest. Only the header's `const` may loosen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FnSig {
    is_const: bool,
    /// `async`, `unsafe` and the ABI.
    header: String,
    /// Generics, parameter types (names ignored), return type.
    rest: String,
}

impl FnSig {
    fn show(&self) -> String {
        format!(
            "{}{} {}",
            if self.is_const { "const " } else { "" },
            self.header,
            self.rest
        )
    }
}

/// The canonical form of a function; `c` carries the scope of its parent (impl or trait).
pub(crate) fn fn_sig(c: &mut Canon<'_>, f: &Function) -> FnSig {
    let m = c.mark();
    let g = c.generics(&f.generics);
    let ins: Vec<String> = f.sig.inputs.iter().map(|(_, t)| c.ty(t)).collect();
    let out = match &f.sig.output {
        // `-> ()` and no return type are one thing.
        Some(Type::Tuple(ts)) if ts.is_empty() => "()".to_string(),
        Some(t) => c.ty(t),
        None => "()".to_string(),
    };
    c.reset(m);
    FnSig {
        is_const: f.header.is_const,
        header: format!(
            "{}{}{:?}",
            if f.header.is_async { "async " } else { "" },
            if f.header.is_unsafe { "unsafe " } else { "" },
            f.header.abi
        ),
        rest: format!(
            "fn{g}({}){}->{out}",
            join(ins, ","),
            if f.sig.is_c_variadic { "..." } else { "" }
        ),
    }
}

/// What differs between a base and a head function; empty when the head still serves every call.
fn fn_diff(b: &FnSig, h: &FnSig) -> Option<String> {
    let const_lost = b.is_const && !h.is_const;
    if b.header != h.header || b.rest != h.rest || const_lost {
        Some(format!("signature changed: {} -> {}", b.show(), h.show()))
    } else {
        None
    }
}

// ------------------------------------------------------------------------------------------------
// Fields and variants
// ------------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Shape {
    Unit,
    Tuple,
    Named,
}

/// The public fields of a struct or of an enum variant, with canonical field types.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Fields {
    pub(crate) shape: Shape,
    pub(crate) named: BTreeMap<String, String>,
    /// By position; `None` is a private field.
    pub(crate) tuple: Vec<Option<String>>,
    /// True when the type has a field no consumer can name.
    pub(crate) stripped: bool,
}

/// A consumer can build the type with a literal: it has no private field and is not
/// `#[non_exhaustive]`.
pub(crate) fn buildable(f: &Fields, non_exhaustive: bool) -> bool {
    !f.stripped && !non_exhaustive
}

/// What a head type's fields break for code written against the base's. `ne_base` is whether the
/// base type was `#[non_exhaustive]`.
pub(crate) fn fields_diff(b: &Fields, h: &Fields, ne_base: bool) -> Vec<String> {
    let mut out = Vec::new();
    if b.shape != h.shape {
        out.push(format!("changed shape ({:?} -> {:?})", b.shape, h.shape));
        return out;
    }
    let open = buildable(b, ne_base);
    match b.shape {
        Shape::Unit => {}
        Shape::Named => {
            for (n, t) in &b.named {
                match h.named.get(n) {
                    None => out.push(format!("public field `{n}` removed")),
                    Some(ht) if ht != t => {
                        out.push(format!("public field `{n}` retyped: {t} -> {ht}"));
                    }
                    Some(_) => {}
                }
            }
            if open {
                for n in h.named.keys() {
                    if !b.named.contains_key(n) {
                        out.push(format!(
                            "field `{n}` added to a type that is built with a literal"
                        ));
                    }
                }
            }
        }
        Shape::Tuple => {
            for (i, t) in b.tuple.iter().enumerate() {
                let Some(t) = t else { continue };
                match h.tuple.get(i) {
                    Some(Some(ht)) if ht == t => {}
                    Some(Some(ht)) => out.push(format!("public field {i} retyped: {t} -> {ht}")),
                    _ => out.push(format!("public field {i} removed")),
                }
            }
            if open && h.tuple.len() != b.tuple.len() {
                out.push(format!(
                    "tuple fields changed from {} to {} on a type that is built with a literal",
                    b.tuple.len(),
                    h.tuple.len()
                ));
            }
        }
    }
    if open && h.stripped && !b.stripped {
        out.push("private field added to a type that is built with a literal".to_string());
    }
    out
}

fn non_exhaustive(it: &Item) -> bool {
    it.attrs
        .iter()
        .any(|a| matches!(a, Attribute::NonExhaustive))
}

/// A variant: its fields, whether it is `#[non_exhaustive]`, and an explicit discriminant.
struct VariantInfo {
    fields: Fields,
    ne: bool,
    discriminant: Option<String>,
}

// ------------------------------------------------------------------------------------------------
// The public API of one build
// ------------------------------------------------------------------------------------------------

/// An item a consumer can name.
#[derive(Clone, Debug)]
struct Public {
    kind: Kind,
    target: ItemRef,
}

type PublicMap = BTreeMap<(Ns, String), Public>;

fn public_api(w: &World) -> Res<PublicMap> {
    let mut out = BTreeMap::new();
    for (ci, k) in w.crates.iter().enumerate() {
        let root = (ci, k.data.root);
        walk(w, root, &[k.lib.to_string()], &mut vec![root], &mut out)?;
    }
    Ok(out)
}

fn walk(
    w: &World,
    m: ItemRef,
    path: &[String],
    stack: &mut Vec<ItemRef>,
    out: &mut PublicMap,
) -> Res<()> {
    for e in view(w, m, &mut Vec::new())?.into_values() {
        let mut p = path.to_vec();
        p.push(e.name.clone());
        out.insert(
            (e.kind.ns(), p.join("::")),
            Public {
                kind: e.kind,
                target: e.target,
            },
        );
        if e.kind == Kind::Module && !stack.contains(&e.target) {
            stack.push(e.target);
            walk(w, e.target, &p, stack, out)?;
            stack.pop();
        }
    }
    Ok(())
}

fn def_path(w: &World, r: ItemRef) -> Option<String> {
    w.crates[r.0]
        .data
        .paths
        .get(&r.1)
        .map(|s| s.path.join("::"))
}

/// Kinds whose definition path appears in types.
fn names_a_type(k: Kind) -> bool {
    matches!(
        k,
        Kind::Struct | Kind::Enum | Kind::Union | Kind::Trait | Kind::TypeAlias
    )
}

// ------------------------------------------------------------------------------------------------
// The comparison
// ------------------------------------------------------------------------------------------------

struct Cmp<'a> {
    base: &'a World,
    head: &'a World,
    rename: HashMap<String, String>,
    out: BTreeSet<String>,
    done: HashSet<(ItemRef, ItemRef)>,
    compared: usize,
}

/// The findings and the number of public items compared.
pub(crate) struct Outcome {
    pub(crate) breaks: Vec<String>,
    pub(crate) compared: usize,
}

pub(crate) fn compare(base: &World, head: &World) -> Res<Outcome> {
    let bp = public_api(base)?;
    let hp = public_api(head)?;
    let mut rename = HashMap::new();
    for (k, b) in &bp {
        if let Some(h) = hp.get(k)
            && b.kind == h.kind
            && names_a_type(b.kind)
            && let (Some(bn), Some(hn)) = (def_path(base, b.target), def_path(head, h.target))
            && bn != hn
        {
            rename.insert(hn, bn);
        }
    }
    let mut cmp = Cmp {
        base,
        head,
        rename,
        out: BTreeSet::new(),
        done: HashSet::new(),
        compared: 0,
    };
    for ((ns, path), b) in &bp {
        match hp.get(&(*ns, path.clone())) {
            None => cmp.found(
                path,
                format!(
                    "removed or moved: no {} at this path any more",
                    b.kind.word().to_lowercase()
                ),
            ),
            Some(h) if h.kind != b.kind => cmp.found(
                path,
                format!(
                    "kind changed: {} -> {}",
                    b.kind.word().to_lowercase(),
                    h.kind.word().to_lowercase()
                ),
            ),
            Some(h) => cmp.item(path, b, h)?,
        }
    }
    cmp.impls()?;
    Ok(Outcome {
        breaks: cmp.out.into_iter().collect(),
        compared: cmp.compared,
    })
}

impl Cmp<'_> {
    fn found(&mut self, path: &str, what: impl AsRef<str>) {
        self.out.insert(format!("break: {path}: {}", what.as_ref()));
    }

    fn bcanon(&self, ci: usize) -> Canon<'_> {
        Canon::new(&self.base.crates[ci].data, None)
    }

    fn hcanon(&self, ci: usize) -> Canon<'_> {
        Canon::new(&self.head.crates[ci].data, Some(&self.rename))
    }

    fn item(&mut self, path: &str, b: &Public, h: &Public) -> Res<()> {
        if !self.done.insert((b.target, h.target)) {
            return Ok(());
        }
        self.compared += 1;
        match b.kind {
            Kind::Module | Kind::Macro | Kind::AttrMacro | Kind::DeriveMacro => Ok(()),
            Kind::Struct | Kind::Union => self.structure(path, b.target, h.target),
            Kind::Enum => self.enumeration(path, b.target, h.target),
            Kind::Trait => self.tr(path, b.target, h.target),
            Kind::Function => self.function(path, b.target, h.target),
            Kind::Constant | Kind::Static | Kind::TypeAlias => self.value(path, b.target, h.target),
            Kind::ExternCrate | Kind::TraitAlias | Kind::ExternType | Kind::Primitive => {
                self.found(
                    path,
                    format!("cannot classify {}", b.kind.word().to_lowercase()),
                );
                Ok(())
            }
        }
    }

    // --- structs and unions ------------------------------------------------------------------

    /// The canonical generics and fields of a struct or union.
    fn struct_parts(&self, w: &World, mut c: Canon<'_>, r: ItemRef) -> Res<(String, Fields, bool)> {
        let it = w.item(r)?;
        let m = c.mark();
        let (generics, fields) = match &it.inner {
            ItemEnum::Struct(s) => {
                let g = c.generics(&s.generics);
                let f = match &s.kind {
                    StructKind::Unit => Fields {
                        shape: Shape::Unit,
                        named: BTreeMap::new(),
                        tuple: Vec::new(),
                        stripped: false,
                    },
                    StructKind::Tuple(ids) => tuple_fields(w, &mut c, r.0, ids)?,
                    StructKind::Plain {
                        fields,
                        has_stripped_fields,
                    } => named_fields(w, &mut c, r.0, fields, *has_stripped_fields)?,
                };
                (g, f)
            }
            ItemEnum::Union(u) => {
                let g = c.generics(&u.generics);
                let f = named_fields(w, &mut c, r.0, &u.fields, u.has_stripped_fields)?;
                (g, f)
            }
            _ => {
                return Err(Fail::one(format!(
                    "{}: item {} is not a struct or union",
                    w.crates[r.0].lib, r.1.0
                )));
            }
        };
        c.reset(m);
        Ok((generics, fields, non_exhaustive(it)))
    }

    fn structure(&mut self, path: &str, b: ItemRef, h: ItemRef) -> Res<()> {
        let (bg, bf, bne) = self.struct_parts(self.base, self.bcanon(b.0), b)?;
        let (hg, hf, hne) = self.struct_parts(self.head, self.hcanon(h.0), h)?;
        if bg != hg {
            self.found(path, format!("generics changed: {bg} -> {hg}"));
        }
        if hne && !bne {
            self.found(path, "#[non_exhaustive] added to an existing struct");
        }
        for d in fields_diff(&bf, &hf, bne) {
            self.found(path, d);
        }
        Ok(())
    }

    // --- enums -------------------------------------------------------------------------------

    fn variants(
        &self,
        w: &World,
        mut c: Canon<'_>,
        r: ItemRef,
    ) -> Res<(String, BTreeMap<String, VariantInfo>, bool)> {
        let it = w.item(r)?;
        let ItemEnum::Enum(e) = &it.inner else {
            return Err(Fail::one(format!(
                "{}: item {} is not an enum",
                w.crates[r.0].lib, r.1.0
            )));
        };
        let m = c.mark();
        let g = c.generics(&e.generics);
        let mut out = BTreeMap::new();
        for id in &e.variants {
            let vi = w.item((r.0, *id))?;
            let ItemEnum::Variant(v) = &vi.inner else {
                continue;
            };
            let fields = match &v.kind {
                VariantKind::Plain => Fields {
                    shape: Shape::Unit,
                    named: BTreeMap::new(),
                    tuple: Vec::new(),
                    stripped: false,
                },
                VariantKind::Tuple(ids) => tuple_fields(w, &mut c, r.0, ids)?,
                VariantKind::Struct {
                    fields,
                    has_stripped_fields,
                } => named_fields(w, &mut c, r.0, fields, *has_stripped_fields)?,
            };
            out.insert(
                vi.name.clone().unwrap_or_default(),
                VariantInfo {
                    fields,
                    ne: non_exhaustive(vi),
                    discriminant: v.discriminant.as_ref().map(|d| d.value.clone()),
                },
            );
        }
        c.reset(m);
        Ok((g, out, non_exhaustive(it)))
    }

    fn enumeration(&mut self, path: &str, b: ItemRef, h: ItemRef) -> Res<()> {
        let (bg, bv, bne) = self.variants(self.base, self.bcanon(b.0), b)?;
        let (hg, hv, hne) = self.variants(self.head, self.hcanon(h.0), h)?;
        if bg != hg {
            self.found(path, format!("generics changed: {bg} -> {hg}"));
        }
        if hne && !bne {
            self.found(path, "#[non_exhaustive] added to an existing enum");
        }
        for (name, bvi) in &bv {
            let Some(hvi) = hv.get(name) else {
                self.found(path, format!("variant `{name}` removed"));
                continue;
            };
            if hvi.ne && !bvi.ne {
                self.found(path, format!("#[non_exhaustive] added to variant `{name}`"));
            }
            for d in fields_diff(&bvi.fields, &hvi.fields, bvi.ne) {
                self.found(path, format!("variant `{name}`: {d}"));
            }
            if let (Some(x), Some(y)) = (&bvi.discriminant, &hvi.discriminant)
                && x != y
            {
                self.found(
                    path,
                    format!("variant `{name}`: discriminant changed {x} -> {y}"),
                );
            }
        }
        if !bne {
            for name in hv.keys() {
                if !bv.contains_key(name) {
                    self.found(
                        path,
                        format!("variant `{name}` added to an exhaustive enum"),
                    );
                }
            }
        }
        Ok(())
    }

    // --- traits ------------------------------------------------------------------------------

    fn trait_parts(&self, w: &World, mut c: Canon<'_>, r: ItemRef) -> Res<TraitParts> {
        let ItemEnum::Trait(t) = &w.item(r)?.inner else {
            return Err(Fail::one(format!(
                "{}: item {} is not a trait",
                w.crates[r.0].lib, r.1.0
            )));
        };
        let m = c.mark();
        let g = c.generics(&t.generics);
        let supers = c.bounds(&t.bounds);
        let mut items = BTreeMap::new();
        for id in &t.items {
            let it = w.item((r.0, *id))?;
            let name = it.name.clone().unwrap_or_default();
            let (tag, member) = match &it.inner {
                ItemEnum::Function(f) => (
                    "fn",
                    TraitMember::Fn {
                        sig: fn_sig(&mut c, f),
                        provided: f.has_body,
                    },
                ),
                ItemEnum::AssocConst { type_, value } => (
                    "const",
                    TraitMember::Const {
                        ty: c.ty(type_),
                        provided: value.is_some(),
                    },
                ),
                ItemEnum::AssocType {
                    generics,
                    bounds,
                    type_,
                } => {
                    let mm = c.mark();
                    let g = c.generics(generics);
                    let b = c.bounds(bounds);
                    c.reset(mm);
                    (
                        "type",
                        TraitMember::Type {
                            shape: format!("{g} {b}"),
                            provided: type_.is_some(),
                        },
                    )
                }
                _ => ("other", TraitMember::Other),
            };
            items.insert(format!("{tag} {name}"), member);
        }
        c.reset(m);
        Ok(TraitParts {
            generics: g,
            supers,
            flags: [t.is_auto, t.is_unsafe, t.is_dyn_compatible],
            items,
        })
    }

    fn tr(&mut self, path: &str, b: ItemRef, h: ItemRef) -> Res<()> {
        let TraitParts {
            generics: bg,
            supers: bs,
            flags: bf,
            items: bi,
        } = self.trait_parts(self.base, self.bcanon(b.0), b)?;
        let TraitParts {
            generics: hg,
            supers: hs,
            flags: hf,
            items: hi,
        } = self.trait_parts(self.head, self.hcanon(h.0), h)?;
        if bg != hg {
            self.found(path, format!("generics changed: {bg} -> {hg}"));
        }
        if bs != hs {
            self.found(path, format!("supertraits changed: {bs} -> {hs}"));
        }
        if bf[0] != hf[0] || bf[1] != hf[1] {
            self.found(path, "auto or unsafe marker changed");
        }
        if bf[2] && !hf[2] {
            self.found(path, "no longer usable as a trait object");
        }
        for (key, bm) in &bi {
            let name = key.split_once(' ').map_or(key.as_str(), |(_, n)| n);
            let Some(hm) = hi.get(key) else {
                self.found(path, format!("trait item `{key}` removed"));
                continue;
            };
            match (bm, hm) {
                (
                    TraitMember::Fn {
                        sig: bs,
                        provided: bp,
                    },
                    TraitMember::Fn {
                        sig: hs,
                        provided: hp,
                    },
                ) => {
                    if let Some(d) = fn_diff(bs, hs) {
                        self.found(path, format!("method `{name}`: {d}"));
                    }
                    if *bp && !*hp {
                        self.found(path, format!("provided method `{name}` made required"));
                    }
                }
                (
                    TraitMember::Const {
                        ty: bt,
                        provided: bp,
                    },
                    TraitMember::Const {
                        ty: ht,
                        provided: hp,
                    },
                ) => {
                    if bt != ht {
                        self.found(path, format!("const `{name}` retyped: {bt} -> {ht}"));
                    }
                    if *bp && !*hp {
                        self.found(path, format!("const `{name}` lost its default"));
                    }
                }
                (
                    TraitMember::Type {
                        shape: bt,
                        provided: bp,
                    },
                    TraitMember::Type {
                        shape: ht,
                        provided: hp,
                    },
                ) => {
                    if bt != ht {
                        self.found(
                            path,
                            format!("associated type `{name}` changed: {bt} -> {ht}"),
                        );
                    }
                    if *bp && !*hp {
                        self.found(path, format!("associated type `{name}` lost its default"));
                    }
                }
                _ => self.found(path, format!("cannot classify trait item `{key}`")),
            }
        }
        for (key, hm) in &hi {
            if bi.contains_key(key) {
                continue;
            }
            let required = match hm {
                TraitMember::Fn { provided, .. }
                | TraitMember::Const { provided, .. }
                | TraitMember::Type { provided, .. } => !provided,
                TraitMember::Other => true,
            };
            if required {
                self.found(path, format!("required trait item `{key}` added"));
            }
        }
        Ok(())
    }

    // --- functions, constants, statics, aliases ----------------------------------------------

    fn function(&mut self, path: &str, b: ItemRef, h: ItemRef) -> Res<()> {
        let (ItemEnum::Function(bf), ItemEnum::Function(hf)) =
            (&self.base.item(b)?.inner, &self.head.item(h)?.inner)
        else {
            self.found(path, "cannot classify function");
            return Ok(());
        };
        let bs = fn_sig(&mut self.bcanon(b.0), bf);
        let hs = fn_sig(&mut self.hcanon(h.0), hf);
        if let Some(d) = fn_diff(&bs, &hs) {
            self.found(path, d);
        }
        Ok(())
    }

    fn value(&mut self, path: &str, b: ItemRef, h: ItemRef) -> Res<()> {
        let bs = self.value_shape(self.base, self.bcanon(b.0), b)?;
        let hs = self.value_shape(self.head, self.hcanon(h.0), h)?;
        if bs != hs {
            self.found(path, format!("type changed: {bs} -> {hs}"));
        }
        Ok(())
    }

    fn value_shape(&self, w: &World, mut c: Canon<'_>, r: ItemRef) -> Res<String> {
        Ok(match &w.item(r)?.inner {
            ItemEnum::Constant { type_, .. } => format!("const {}", c.ty(type_)),
            ItemEnum::Static(s) => format!(
                "static{} {}",
                if s.is_mutable { " mut" } else { "" },
                c.ty(&s.type_)
            ),
            ItemEnum::TypeAlias(a) => {
                let m = c.mark();
                let g = c.generics(&a.generics);
                let t = c.ty(&a.type_);
                c.reset(m);
                format!("type{g} = {t}")
            }
            _ => "?".to_string(),
        })
    }

    // --- impls -------------------------------------------------------------------------------

    /// Every non-blanket impl written in the five crates, by canonical key.
    fn impl_table(&self, w: &World, head: bool) -> Res<BTreeMap<String, ImplRec>> {
        let mut out: BTreeMap<String, ImplRec> = BTreeMap::new();
        for (ci, k) in w.crates.iter().enumerate() {
            let mut c = if head {
                self.hcanon(ci)
            } else {
                self.bcanon(ci)
            };
            for it in k.data.index.values() {
                let ItemEnum::Impl(i) = &it.inner else {
                    continue;
                };
                if it.crate_id != 0 || i.blanket_impl.is_some() {
                    continue;
                }
                let m = c.mark();
                let generics = c.generics(&i.generics);
                let for_ = c.ty(&i.for_);
                let (key, inherent) = match &i.trait_ {
                    Some(t) => (
                        format!(
                            "{}{} for {for_}",
                            if i.is_negative { "!" } else { "" },
                            c.path(t)
                        ),
                        false,
                    ),
                    None => (format!("inherent for {for_}"), true),
                };
                let mut items = BTreeMap::new();
                if inherent {
                    for id in &i.items {
                        let Some(item) = k.data.index.get(id) else {
                            continue;
                        };
                        let name = item.name.clone().unwrap_or_default();
                        match &item.inner {
                            ItemEnum::Function(f) => {
                                items.insert(format!("fn {name}"), ImplItem::Fn(fn_sig(&mut c, f)));
                            }
                            ItemEnum::AssocConst { type_, .. } => {
                                items.insert(format!("const {name}"), ImplItem::Const(c.ty(type_)));
                            }
                            _ => {
                                items.insert(format!("other {name}"), ImplItem::Other);
                            }
                        }
                    }
                }
                c.reset(m);
                let rec = out.entry(key).or_insert_with(|| ImplRec {
                    synthetic: i.is_synthetic,
                    generics: generics.clone(),
                    items: BTreeMap::new(),
                });
                rec.items.extend(items);
            }
        }
        Ok(out)
    }

    fn impls(&mut self) -> Res<()> {
        let bt = self.impl_table(self.base, false)?;
        let ht = self.impl_table(self.head, true)?;
        for (key, b) in &bt {
            let Some(h) = ht.get(key) else {
                self.found(key, "impl missing from the head");
                continue;
            };
            self.compared += 1;
            if !b.synthetic && b.generics != h.generics {
                self.found(
                    key,
                    format!(
                        "impl generics or bounds changed: {} -> {}",
                        b.generics, h.generics
                    ),
                );
            }
            let prefix = key.strip_prefix("inherent for ");
            for (ik, bi) in &b.items {
                let shown = match prefix {
                    Some(p) => format!(
                        "{p}::{}",
                        ik.split_once(' ').map_or(ik.as_str(), |(_, n)| n)
                    ),
                    None => key.clone(),
                };
                match (bi, h.items.get(ik)) {
                    (_, None) => self.found(&shown, "removed (or no longer public)"),
                    (ImplItem::Fn(x), Some(ImplItem::Fn(y))) => {
                        if let Some(d) = fn_diff(x, y) {
                            self.found(&shown, d);
                        }
                    }
                    (ImplItem::Const(x), Some(ImplItem::Const(y))) => {
                        if x != y {
                            self.found(&shown, format!("type changed: {x} -> {y}"));
                        }
                    }
                    (ImplItem::Other, _) | (_, Some(ImplItem::Other)) => {
                        self.found(&shown, "cannot classify impl item");
                    }
                    _ => self.found(&shown, "changed kind"),
                }
            }
        }
        Ok(())
    }
}

/// A trait: its generics, supertraits, `[is_auto, is_unsafe, is_dyn_compatible]` and its items.
struct TraitParts {
    generics: String,
    supers: String,
    flags: [bool; 3],
    items: BTreeMap<String, TraitMember>,
}

enum TraitMember {
    Fn { sig: FnSig, provided: bool },
    Const { ty: String, provided: bool },
    Type { shape: String, provided: bool },
    Other,
}

enum ImplItem {
    Fn(FnSig),
    Const(String),
    Other,
}

struct ImplRec {
    synthetic: bool,
    generics: String,
    items: BTreeMap<String, ImplItem>,
}

fn field_ty(w: &World, c: &mut Canon<'_>, ci: usize, id: rustdoc_types::Id) -> Res<String> {
    match &w.item((ci, id))?.inner {
        ItemEnum::StructField(t) => Ok(c.ty(t)),
        _ => Err(Fail::one(format!(
            "{}: field {} is not a field",
            w.crates[ci].lib, id.0
        ))),
    }
}

fn tuple_fields(
    w: &World,
    c: &mut Canon<'_>,
    ci: usize,
    ids: &[Option<rustdoc_types::Id>],
) -> Res<Fields> {
    let mut tuple = Vec::new();
    for id in ids {
        tuple.push(match id {
            Some(id) => Some(field_ty(w, c, ci, *id)?),
            None => None,
        });
    }
    Ok(Fields {
        shape: Shape::Tuple,
        stripped: tuple.iter().any(Option::is_none),
        named: BTreeMap::new(),
        tuple,
    })
}

fn named_fields(
    w: &World,
    c: &mut Canon<'_>,
    ci: usize,
    ids: &[rustdoc_types::Id],
    stripped: bool,
) -> Res<Fields> {
    let mut named = BTreeMap::new();
    for id in ids {
        let name = w.item((ci, *id))?.name.clone().unwrap_or_default();
        named.insert(name, field_ty(w, c, ci, *id)?);
    }
    Ok(Fields {
        shape: Shape::Named,
        named,
        tuple: Vec::new(),
        stripped,
    })
}

// ------------------------------------------------------------------------------------------------
// Manifests
// ------------------------------------------------------------------------------------------------

fn read_json(path: &str) -> Res<serde_json::Value> {
    let bytes = std::fs::read(path).map_err(|e| Fail::one(format!("cannot read {path}: {e}")))?;
    serde_json::from_slice(&bytes).map_err(|e| Fail::one(format!("{path} is not JSON: {e}")))
}

fn version_key(v: &str) -> Vec<u64> {
    v.split('.')
        .map(|p| p.parse::<u64>().unwrap_or(0))
        .collect()
}

/// The package names of a `cargo metadata --no-deps` file, one per workspace member.
fn members(meta: &serde_json::Value) -> Vec<String> {
    meta.get("packages")
        .and_then(serde_json::Value::as_array)
        .map(|ps| {
            ps.iter()
                .filter_map(|p| p.get("name").and_then(serde_json::Value::as_str))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn package<'a>(meta: &'a serde_json::Value, name: &str) -> Option<&'a serde_json::Value> {
    meta.get("packages")?
        .as_array()?
        .iter()
        .find(|p| p.get("name").and_then(serde_json::Value::as_str) == Some(name))
}

/// The requirement strings of `winit` and `wgpu` in one package, with their kind and target.
fn pinned_deps(pkg: &serde_json::Value) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(deps) = pkg
        .get("dependencies")
        .and_then(serde_json::Value::as_array)
    {
        for d in deps {
            let name = d
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            if name != "winit" && name != "wgpu" {
                continue;
            }
            let field = |k: &str| match d.get(k) {
                Some(serde_json::Value::String(s)) => s.clone(),
                Some(serde_json::Value::Null) | None => "-".to_string(),
                Some(other) => other.to_string(),
            };
            out.push(format!(
                "{name} {} (kind {}, target {})",
                field("req"),
                field("kind"),
                field("target")
            ));
        }
    }
    out.sort();
    out
}

pub(crate) fn meta_breaks(base: &serde_json::Value, head: &serde_json::Value) -> Res<Vec<String>> {
    let mut out = Vec::new();
    for (name, _) in CRATES {
        let (Some(b), Some(h)) = (package(base, name), package(head, name)) else {
            return Err(Fail::one(format!(
                "package {name} is missing from a cargo metadata file"
            )));
        };
        let rv = |p: &serde_json::Value| {
            p.get("rust_version")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        };
        match (rv(b), rv(h)) {
            (Some(x), Some(y)) if version_key(&y) > version_key(&x) => {
                out.push(format!("break: {name}: rust-version raised {x} -> {y}"));
            }
            (None, Some(y)) => {
                out.push(format!("break: {name}: rust-version {y} set (none before)"))
            }
            _ => {}
        }
        let (bd, hd) = (pinned_deps(b), pinned_deps(h));
        if bd != hd {
            out.push(format!(
                "break: {name}: winit or wgpu requirement changed: [{}] -> [{}]",
                bd.join(", "),
                hd.join(", ")
            ));
        }
    }
    Ok(out)
}

// ------------------------------------------------------------------------------------------------
// Command line
// ------------------------------------------------------------------------------------------------

const USAGE: &str = "usage: api-docs compat --base-json <dir> --head-json <dir> --base-meta <file> \
--head-meta <file>\n       api-docs compat --list-members <meta-file>";

fn run(args: &[String]) -> Res<u8> {
    let mut slots: BTreeMap<&str, String> = BTreeMap::new();
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        let key = match flag.as_str() {
            "--base-json" => "base-json",
            "--head-json" => "head-json",
            "--base-meta" => "base-meta",
            "--head-meta" => "head-meta",
            "--list-members" => "list-members",
            other => return Err(Fail::one(format!("unknown argument `{other}`\n{USAGE}"))),
        };
        let value = it
            .next()
            .ok_or_else(|| Fail::one(format!("{flag} needs a value\n{USAGE}")))?;
        slots.insert(key, value.clone());
    }
    if let Some(meta) = slots.get("list-members") {
        for name in members(&read_json(meta)?) {
            println!("{name}");
        }
        return Ok(0);
    }
    let get = |k: &str| {
        slots
            .get(k)
            .cloned()
            .ok_or_else(|| Fail::one(USAGE.to_string()))
    };
    let base = World::load(FsPath::new(&get("base-json")?))?;
    let head = World::load(FsPath::new(&get("head-json")?))?;
    let outcome = compare(&base, &head)?;
    let mut breaks = outcome.breaks;
    breaks.extend(meta_breaks(
        &read_json(&get("base-meta")?)?,
        &read_json(&get("head-meta")?)?,
    )?);
    for b in &breaks {
        println!("{b}");
    }
    if breaks.is_empty() {
        println!(
            "compat: held ({} public items compared, no break)",
            outcome.compared
        );
        Ok(0)
    } else {
        println!("compat: {} break(s)", breaks.len());
        Ok(1)
    }
}

pub(crate) fn main(args: &[String]) -> ExitCode {
    match run(args) {
        Ok(code) => ExitCode::from(code),
        Err(Fail(lines)) => {
            for l in lines {
                eprintln!("api-docs compat: {l}");
            }
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A one-crate JSON with a `paths` table of the given (id, path, kind) entries.
    fn krate(paths: &[(u32, &str, &str)]) -> Crate {
        let entries: Vec<String> = paths
            .iter()
            .map(|(id, p, kind)| {
                let segs: Vec<String> = p.split("::").map(|s| format!("\"{s}\"")).collect();
                format!(
                    "\"{id}\":{{\"crate_id\":0,\"path\":[{}],\"kind\":\"{kind}\"}}",
                    segs.join(",")
                )
            })
            .collect();
        let json = format!(
            "{{\"root\":0,\"crate_version\":null,\"includes_private\":false,\"index\":{{}},\
             \"paths\":{{{}}},\"external_crates\":{{}},\
             \"target\":{{\"triple\":\"t\",\"target_features\":[]}},\"format_version\":{}}}",
            entries.join(","),
            rustdoc_types::FORMAT_VERSION
        );
        serde_json::from_str(&json).expect("test crate parses")
    }

    fn path(written: &str, id: u32) -> Type {
        Type::ResolvedPath(Path {
            path: written.to_string(),
            id: rustdoc_types::Id(id),
            args: None,
        })
    }

    #[test]
    fn a_type_is_its_definition_path_whatever_was_written() {
        let k = krate(&[(1, "lune_ui_core::layout::Insets", "struct")]);
        let mut c = Canon::new(&k, None);
        let a = c.ty(&path("Insets", 1));
        let b = c.ty(&path("crate::layout::Insets", 1));
        let r = c.ty(&Type::BorrowedRef {
            lifetime: None,
            is_mutable: false,
            type_: Box::new(path("Insets", 1)),
        });
        assert_eq!(a, "lune_ui_core::layout::Insets");
        assert_eq!(a, b);
        assert_eq!(r, "&lune_ui_core::layout::Insets");
    }

    #[test]
    fn a_moved_definition_is_mapped_back_to_its_base_path() {
        let k = krate(&[(1, "lune_ui_core::geometry::Insets", "struct")]);
        let mut rename = HashMap::new();
        rename.insert(
            "lune_ui_core::geometry::Insets".to_string(),
            "lune_ui_core::layout::Insets".to_string(),
        );
        let mut c = Canon::new(&k, Some(&rename));
        assert_eq!(c.ty(&path("Insets", 1)), "lune_ui_core::layout::Insets");
    }

    #[test]
    fn generic_and_lifetime_names_are_positions() {
        let k = krate(&[]);
        let mut c = Canon::new(&k, None);
        let param = |n: &str| GenericParamDef {
            name: n.to_string(),
            kind: GenericParamDefKind::Type {
                bounds: Vec::new(),
                default: None,
                is_synthetic: false,
            },
        };
        let g = |n: &str| Generics {
            params: vec![param(n)],
            where_predicates: Vec::new(),
        };
        let shown = |c: &mut Canon<'_>, n: &str| {
            let m = c.mark();
            let s = format!(
                "{} {}",
                c.generics(&g(n)),
                c.ty(&Type::Generic(n.to_string()))
            );
            c.reset(m);
            s
        };
        assert_eq!(shown(&mut c, "T"), shown(&mut c, "U"));
        let lt = |c: &mut Canon<'_>, n: &str| {
            c.scope.push(n.to_string());
            let s = c.ty(&Type::BorrowedRef {
                lifetime: Some(n.to_string()),
                is_mutable: false,
                type_: Box::new(Type::Primitive("str".to_string())),
            });
            c.reset(0);
            s
        };
        assert_eq!(lt(&mut c, "'a"), lt(&mut c, "'b"));
        // `Self` is no parameter of the item: it stays as written.
        assert_eq!(c.ty(&Type::Generic("Self".to_string())), "Self");
    }

    fn named(fields: &[(&str, &str)], stripped: bool) -> Fields {
        Fields {
            shape: Shape::Named,
            named: fields
                .iter()
                .map(|(n, t)| ((*n).to_string(), (*t).to_string()))
                .collect(),
            tuple: Vec::new(),
            stripped,
        }
    }

    #[test]
    fn struct_buildability_follows_private_fields_and_non_exhaustive() {
        let plain = named(&[("a", "u8")], false);
        let private = named(&[("a", "u8")], true);
        assert!(buildable(&plain, false));
        assert!(!buildable(&plain, true));
        assert!(!buildable(&private, false));
    }

    #[test]
    fn a_field_added_breaks_only_a_buildable_struct() {
        let base = named(&[("a", "u8")], false);
        let more = named(&[("a", "u8"), ("b", "u8")], false);
        assert_eq!(fields_diff(&base, &more, false).len(), 1);
        // The same addition is fine when the base had a private field or was non-exhaustive.
        let base_private = named(&[("a", "u8")], true);
        let more_private = named(&[("a", "u8"), ("b", "u8")], true);
        assert!(fields_diff(&base_private, &more_private, false).is_empty());
        assert!(fields_diff(&base, &more, true).is_empty());
        // A public field removed or retyped breaks every struct.
        let retyped = named(&[("a", "u16")], true);
        assert_eq!(fields_diff(&base_private, &retyped, false).len(), 1);
        assert_eq!(
            fields_diff(&base_private, &named(&[], true), false).len(),
            1
        );
        // A private field added to a buildable struct breaks literals.
        assert_eq!(
            fields_diff(&base, &named(&[("a", "u8")], true), false).len(),
            1
        );
    }

    #[test]
    fn tuple_fields_compare_by_position() {
        let t = |fields: &[Option<&str>]| Fields {
            shape: Shape::Tuple,
            named: BTreeMap::new(),
            tuple: fields.iter().map(|f| f.map(str::to_string)).collect(),
            stripped: fields.iter().any(Option::is_none),
        };
        let base = t(&[Some("u8"), Some("u8")]);
        assert_eq!(fields_diff(&base, &t(&[Some("u8")]), false).len(), 2);
        assert_eq!(
            fields_diff(&base, &t(&[Some("u8"), Some("u16")]), false).len(),
            1
        );
        let hidden = t(&[Some("u8"), None]);
        assert!(fields_diff(&hidden, &t(&[Some("u8"), None, Some("u8")]), false).is_empty());
    }

    #[test]
    fn a_loosened_const_passes_and_a_lost_const_breaks() {
        let sig = |is_const: bool| FnSig {
            is_const,
            header: "Rust".to_string(),
            rest: "fn<> where ()->()".to_string(),
        };
        assert!(fn_diff(&sig(false), &sig(true)).is_none());
        assert!(fn_diff(&sig(true), &sig(false)).is_some());
    }

    #[test]
    fn the_manifest_rules() {
        let meta = |rv: &str, req: &str| {
            let pkgs: Vec<serde_json::Value> = CRATES
                .iter()
                .map(|(n, _)| {
                    serde_json::json!({
                        "name": n,
                        "rust_version": rv,
                        "dependencies": [{"name": "winit", "req": req, "kind": null, "target": null}],
                    })
                })
                .collect();
            serde_json::json!({ "packages": pkgs })
        };
        let base = meta("1.85", "0.30");
        assert!(
            meta_breaks(&base, &meta("1.85", "0.30"))
                .unwrap_or_default()
                .is_empty()
        );
        assert_eq!(
            meta_breaks(&base, &meta("1.86", "0.30"))
                .map(|v| v.len())
                .ok(),
            Some(5)
        );
        assert_eq!(
            meta_breaks(&base, &meta("1.85", "0.31"))
                .map(|v| v.len())
                .ok(),
            Some(5)
        );
        assert!(
            meta_breaks(&meta("1.86", "0.30"), &base)
                .unwrap_or_default()
                .is_empty()
        );
    }
}
