//! Harness for driving hand-assembled WebAssembly modules against the real
//! Soroban contract host, in process.
//!
//! Everything here is deliberately written without reference to the compiler:
//! modules are assembled byte by byte with `wasm-encoder`, and the `Val`
//! encoders and decoders re-derive the bit layout from the published ABI rather
//! than calling the toolchain code they exist to check. A harness that shares an
//! implementation with its subject can only prove the two agree, not that either
//! is right.
//!
//! The host is `soroban-env-host` with `testutils`, the same wasmi
//! configuration, budget and upload validation a validator runs. Nothing here
//! talks to a network.
//!
//! The budget is left at its default. `Host::test_budget` looks like the way to
//! bound a run, but it also calls `reset_models`, which zeroes every cost model
//! and would make any resource assertion built on top of it vacuously true;
//! `reset_limits` is the knob that changes limits without discarding the
//! metering.

use std::sync::{Mutex, MutexGuard, PoisonError};

use soroban_env_host::xdr::{AccountId, PublicKey, Uint256};
use soroban_env_host::{AddressObject, Env, EnvBase, Host, HostError, Symbol, TryFromVal, Val};
use wasm_encoder::{
    CodeSection, ConstExpr, CustomSection, DataSection, ExportKind, ExportSection, Function,
    GlobalSection, GlobalType, MemorySection, MemoryType, Module, TypeSection, ValType,
};

/// Serializes host sessions across the test binary's threads.
///
/// The guard never poisons: a panicking assertion in one test would otherwise
/// turn every later `lock()` into an error and bury the one real failure under a
/// wall of secondary ones.
pub fn session() -> MutexGuard<'static, ()> {
    static SESSION: Mutex<()> = Mutex::new(());
    SESSION.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A host with a recording footprint and debug info enabled.
///
/// Debug info is what carries the host's own diagnostic text (`"contract
/// missing metadata section"` and friends) into `HostError`'s `Debug` rendering;
/// without it a rejection is only a type and a code.
#[must_use]
pub fn host() -> Host {
    let host = Host::test_host_with_recording_footprint();
    host.enable_debug().expect("enable host debug info");
    host
}

/// Uploads `wasm` and instantiates it, returning the contract address.
///
/// This is the fallible upload path on purpose. `register_test_contract_wasm`
/// unwraps internally, so it cannot express a negative test, and every
/// acceptance-envelope check here is a negative test.
///
/// # Errors
///
/// Returns the host's own error when the module fails upload validation.
pub fn upload(host: &Host, wasm: &[u8]) -> Result<AddressObject, HostError> {
    let account = AccountId(PublicKey::PublicKeyTypeEd25519(Uint256([0x5a; 32])));
    host.register_test_contract_wasm_from_source_account(wasm, account, salt_for(wasm))
}

/// Derives the creation salt from the module bytes.
///
/// The contract address is a function of the source account and the salt, so a
/// fixed salt would collide the second time one host uploads two modules.
/// Hashing the bytes keeps addresses distinct per module and stable per run,
/// which a random salt would not.
fn salt_for(wasm: &[u8]) -> [u8; 32] {
    let mut acc: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in wasm {
        acc ^= u64::from(*byte);
        acc = acc.wrapping_mul(0x0000_0100_0000_01b3);
    }
    let mut salt = [0u8; 32];
    for (i, chunk) in salt.chunks_mut(8).enumerate() {
        chunk.copy_from_slice(&acc.wrapping_add(i as u64).to_le_bytes());
    }
    salt
}

/// Invokes an exported contract function.
///
/// # Errors
///
/// Returns the host's own error when the invocation traps or is rejected.
pub fn call(
    host: &Host,
    contract: AddressObject,
    name: &str,
    args: &[Val],
) -> Result<Val, HostError> {
    let func = Symbol::try_from_val(host, &name)?;
    let args = host.vec_new_from_slice(args)?;
    host.call(contract, func, args)
}

/// How many `fn_call` diagnostic events the host has recorded.
///
/// The host writes one as it enters a contract invocation, before the
/// contract's own code runs, so this counts the calls that reached dispatch —
/// which is how a test tells a call refused on the way in apart from one the
/// contract itself refused. A failed call's event stays in the log, marked
/// failed, so the count does not fall back. The events exist only because
/// [`host`] enables debug info.
///
/// # Panics
///
/// Panics if the host's event log cannot be read.
#[must_use]
pub fn fn_call_events(host: &Host) -> usize {
    host.get_events()
        .expect("the host's event log is readable")
        .0
        .iter()
        .filter(|event| event.to_string().contains("fn_call"))
        .count()
}

/// Builds the `Symbol` a caller needs in order to name an export.
///
/// `call` does this first and hides it. A name the host cannot turn into a
/// `Symbol` fails here, before any contract is reached, so measuring the step on
/// its own is what tells "the host refused the call" apart from "no caller can
/// express this name".
///
/// # Errors
///
/// Returns the host's own error when `name` is not a valid `Symbol`.
pub fn symbol_for(host: &Host, name: &str) -> Result<Symbol, HostError> {
    Ok(Symbol::try_from_val(host, &name)?)
}

/// Renders a host error the way a reader needs it: type, code, and the debug
/// text the host attached to it.
#[must_use]
pub fn describe(err: &HostError) -> String {
    format!("{err:?}")
}

// ---------------------------------------------------------------------------
// Val codec — an independent re-derivation of the published bit layout.
//
// A `Val` is one `u64`: bits 0..7 are the tag, bits 8..31 the minor, bits 32..63
// the major. The host runs `is_good()` over every word an export returns, which
// requires the minor to be zero for `U32Val`/`I32Val` and the whole body to be
// zero for `True`/`False`/`Void`.
// ---------------------------------------------------------------------------

pub const TAG_FALSE: u64 = 0;
pub const TAG_TRUE: u64 = 1;
pub const TAG_VOID: u64 = 2;
pub const TAG_U32VAL: u64 = 4;
pub const TAG_I32VAL: u64 = 5;

/// The word carrying `v` as a `U32Val`.
#[must_use]
pub fn u32_payload(v: u32) -> u64 {
    (u64::from(v) << 32) | TAG_U32VAL
}

/// The word carrying `v` as an `I32Val`. The 32-bit two's-complement pattern
/// sits in the major half unchanged, so `-1` is `0xffff_ffff_0000_0005`.
#[must_use]
pub fn i32_payload(v: i32) -> u64 {
    (u64::from(v.cast_unsigned()) << 32) | TAG_I32VAL
}

/// The word carrying `b`. `True` and `False` are distinct tags with an empty
/// body, not a payload.
#[must_use]
pub fn bool_payload(b: bool) -> u64 {
    if b { TAG_TRUE } else { TAG_FALSE }
}

#[must_use]
pub fn u32_val(v: u32) -> Val {
    Val::from_payload(u32_payload(v))
}

#[must_use]
pub fn i32_val(v: i32) -> Val {
    Val::from_payload(i32_payload(v))
}

#[must_use]
pub fn bool_val(b: bool) -> Val {
    Val::from_payload(bool_payload(b))
}

#[must_use]
pub fn void_val() -> Val {
    Val::from_payload(TAG_VOID)
}

/// What a returned word decodes to under an independent reading of the layout.
///
/// `Malformed` is this harness's own `is_good()`: a word whose tag is a scalar
/// tag but whose reserved bits are not clear. The host rejects such a word
/// before a test ever sees it, so observing `Malformed` from a successful call
/// would mean the host's check had gone away.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decoded {
    U32(u32),
    I32(i32),
    Bool(bool),
    Void,
    Malformed { tag: u8, body: u64 },
    Other { tag: u8, body: u64 },
}

#[must_use]
pub fn decode(val: Val) -> Decoded {
    let payload = val.get_payload();
    let tag = (payload & 0xff) as u8;
    let body = payload >> 8;
    let minor = (body & 0x00ff_ffff) as u32;
    let major = (body >> 24) as u32;
    match u64::from(tag) {
        TAG_U32VAL if minor == 0 => Decoded::U32(major),
        TAG_I32VAL if minor == 0 => Decoded::I32(major.cast_signed()),
        TAG_TRUE if body == 0 => Decoded::Bool(true),
        TAG_FALSE if body == 0 => Decoded::Bool(false),
        TAG_VOID if body == 0 => Decoded::Void,
        TAG_U32VAL | TAG_I32VAL | TAG_TRUE | TAG_FALSE | TAG_VOID => {
            Decoded::Malformed { tag, body }
        }
        _ => Decoded::Other { tag, body },
    }
}

// ---------------------------------------------------------------------------
// Module assembly
// ---------------------------------------------------------------------------

/// The custom section name the host looks for. Its absence is an upload
/// failure, not an invocation failure: the upload builds a throwaway VM.
pub const ENV_META_SECTION: &str = "contractenvmetav0";

/// The XDR `SCEnvMetaEntry` payload declaring an environment interface version:
/// a 4-byte big-endian discriminant of zero (`SC_ENV_META_KIND_INTERFACE_VERSION`)
/// followed by the protocol and pre-release numbers, each 4 bytes big-endian.
#[must_use]
pub fn env_meta_payload(protocol: u32, pre_release: u32) -> Vec<u8> {
    let mut payload = Vec::with_capacity(12);
    payload.extend_from_slice(&0u32.to_be_bytes());
    payload.extend_from_slice(&protocol.to_be_bytes());
    payload.extend_from_slice(&pre_release.to_be_bytes());
    payload
}

/// One function in a hand-assembled module.
pub struct FuncDef {
    pub export: Option<String>,
    pub params: Vec<ValType>,
    pub results: Vec<ValType>,
    pub body: Function,
}

impl FuncDef {
    /// A function reachable only through a `call` from a wrapper.
    #[must_use]
    pub fn internal(params: Vec<ValType>, results: Vec<ValType>, body: Function) -> Self {
        Self { export: None, params, results, body }
    }

    /// A function the host may invoke by name.
    #[must_use]
    pub fn exported(
        name: &str,
        params: Vec<ValType>,
        results: Vec<ValType>,
        body: Function,
    ) -> Self {
        Self { export: Some(name.to_owned()), params, results, body }
    }
}

/// The linear memory of a hand-assembled module, with at most one data segment.
///
/// `export_as` is a name rather than a flag because the host looks up the
/// literal `"memory"` and nothing else: exporting under any other name is a
/// distinct, measurable outcome from not exporting at all.
struct MemoryDef {
    pages: u64,
    export_as: String,
    data: Option<(u32, Vec<u8>)>,
}

/// A hand-assembled contract module.
///
/// Section order follows the binary format (type, global, export, code, then
/// custom); the host imposes no placement rule on custom sections and real
/// artifacts put them last.
pub struct ContractModule {
    funcs: Vec<FuncDef>,
    globals: Vec<(String, i64, bool)>,
    memory: Option<MemoryDef>,
    meta: Option<Vec<u8>>,
}

impl ContractModule {
    /// A module declaring protocol 20 with a zero pre-release: the oldest
    /// protocol any Soroban network has run, and therefore uploadable to all of
    /// them.
    #[must_use]
    pub fn new() -> Self {
        Self {
            funcs: Vec::new(),
            globals: Vec::new(),
            memory: None,
            meta: Some(env_meta_payload(20, 0)),
        }
    }

    /// A module declaring a chosen interface version.
    #[must_use]
    pub fn with_protocol(protocol: u32, pre_release: u32) -> Self {
        Self {
            funcs: Vec::new(),
            globals: Vec::new(),
            memory: None,
            meta: Some(env_meta_payload(protocol, pre_release)),
        }
    }

    /// A module carrying no `contractenvmetav0` section at all.
    #[must_use]
    pub fn without_meta() -> Self {
        Self { funcs: Vec::new(), globals: Vec::new(), memory: None, meta: None }
    }

    #[must_use]
    pub fn func(mut self, func: FuncDef) -> Self {
        self.funcs.push(func);
        self
    }

    /// Declares a linear memory of `pages` initial pages, exported as
    /// `export_as`.
    #[must_use]
    pub fn memory(mut self, pages: u64, export_as: &str) -> Self {
        self.memory = Some(MemoryDef { pages, export_as: export_as.to_owned(), data: None });
        self
    }

    /// Attaches the module's single data segment at `offset`.
    ///
    /// # Panics
    ///
    /// Panics if no memory has been declared: a data segment without one is a
    /// module the encoder would emit and no reader could interpret.
    #[must_use]
    pub fn data(mut self, offset: u32, bytes: &[u8]) -> Self {
        let memory = self.memory.as_mut().expect("a data segment needs a declared memory");
        memory.data = Some((offset, bytes.to_vec()));
        self
    }

    #[must_use]
    pub fn exported_global_i64(mut self, name: &str, init: i64, mutable: bool) -> Self {
        self.globals.push((name.to_owned(), init, mutable));
        self
    }

    #[must_use]
    pub fn finish(self) -> Vec<u8> {
        let mut module = Module::new();

        let mut types = TypeSection::new();
        for func in &self.funcs {
            types.ty().function(func.params.iter().copied(), func.results.iter().copied());
        }
        module.section(&types);

        let mut functions = wasm_encoder::FunctionSection::new();
        for (idx, _) in self.funcs.iter().enumerate() {
            functions.function(u32::try_from(idx).expect("type count fits in u32"));
        }
        module.section(&functions);

        if let Some(memory) = &self.memory {
            let mut section = MemorySection::new();
            section.memory(MemoryType {
                minimum: memory.pages,
                maximum: None,
                memory64: false,
                shared: false,
                page_size_log2: None,
            });
            module.section(&section);
        }

        if !self.globals.is_empty() {
            let mut section = GlobalSection::new();
            for (_, init, mutable) in &self.globals {
                section.global(
                    GlobalType { val_type: ValType::I64, mutable: *mutable, shared: false },
                    &ConstExpr::i64_const(*init),
                );
            }
            module.section(&section);
        }

        let mut exports = ExportSection::new();
        for (idx, func) in self.funcs.iter().enumerate() {
            if let Some(name) = &func.export {
                exports.export(
                    name,
                    ExportKind::Func,
                    u32::try_from(idx).expect("function count fits in u32"),
                );
            }
        }
        for (idx, (name, _, _)) in self.globals.iter().enumerate() {
            exports.export(
                name,
                ExportKind::Global,
                u32::try_from(idx).expect("global count fits in u32"),
            );
        }
        if let Some(memory) = &self.memory {
            exports.export(&memory.export_as, ExportKind::Memory, 0);
        }
        module.section(&exports);

        let mut code = CodeSection::new();
        for func in &self.funcs {
            code.function(&func.body);
        }
        module.section(&code);

        if let Some((offset, bytes)) = self.memory.as_ref().and_then(|m| m.data.as_ref()) {
            let mut section = DataSection::new();
            section.active(0, &ConstExpr::i32_const(offset.cast_signed()), bytes.iter().copied());
            module.section(&section);
        }

        if let Some(meta) = &self.meta {
            module.section(&CustomSection {
                name: ENV_META_SECTION.into(),
                data: meta.as_slice().into(),
            });
        }

        module.finish()
    }
}

impl Default for ContractModule {
    fn default() -> Self {
        Self::new()
    }
}
