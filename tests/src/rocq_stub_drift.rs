//! Drift gate between the vendored signature stub and the real Rocq libraries.
//!
//! The `coqc` round-trip gate in `rocq_typecheck` compiles proof-mode output
//! against `core/wasm-to-v/rocq-stub/`, a hand-written mirror of two upstream
//! libraries. That gate is only as true as the mirror: a constructor the stub
//! declares with the wrong arity, or one upstream renamed, type-checks locally
//! for as long as the stub keeps agreeing with itself. The whole gate would stay
//! green while the emitted `.v` had stopped being provable anywhere else --
//! exactly the #346 class, where the stub's `br_table`, element and data shapes
//! had to be re-derived from the real library rather than from a summary of it.
//!
//! This module holds the mirror to its originals, in both directions, because
//! one direction alone is a classic false green:
//!
//! - **fiction**: every declaration the stub makes must exist upstream, with a
//!   matching normalized type. This is the direction that catches an invented
//!   name and a mis-aritied constructor. It needs nothing committed here beyond
//!   the stub itself, because the claim is checked against the live checkout.
//! - **narrowing**: the stub is deliberately a *subset*, so the fiction
//!   direction can say nothing about upstream growing a declaration the stub
//!   should mirror, or dropping one the stub was narrowing away from. The
//!   SHA-256 of the real-only declaration set, committed in
//!   `core/wasm-to-v/wasm-verifier-pin.txt`, pins that subset relationship.
//!
//! # Two tiers, independently skippable
//!
//! [`Tier::WASM_CERT`] mirrors coq-wasm / WasmCert-Coq, which is public: no
//! credential, and CI runs it. [`Tier::WASM_VERIFIER`] mirrors wasm-verifier,
//! which is private: it runs where a checkout exists, which in practice means a
//! developer machine. Neither tier's absence affects the other.
//!
//! # What is committed, and what is not
//!
//! No wasm-verifier source, and no reconstruction of its API surface, is
//! committed here or printed by a run that CI could publish. The narrowing
//! direction commits digests and counts; the names behind a failing digest are
//! read live from the developer's own checkout and printed only there. The
//! fiction direction names only what the *stub* already says in this repository.
//!
//! # Skipping is not passing
//!
//! An absent checkout prints a `skipped:` line naming the half of the claim the
//! run did not establish, and returns without failing. `INFERENCE_ROCQ_DRIFT_
//! REQUIRE=1` turns that absence into a failure, which is how CI holds the
//! public tier to actually running.

#[cfg(test)]
mod drift {
    use crate::rocq_decls::{
        Tok, declared_names, ident_at, is_punct, strip_rocq_comments, tokenize,
        upstream_declared_names,
    };
    use sha2::{Digest, Sha256};
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::sync::Mutex;

    /// Names the tiers a run must actually establish rather than skip.
    ///
    /// `1` or `all` requires every tier; anything else is a comma-separated list
    /// of Rocq namespaces. The list form is what makes the two tiers
    /// independently skippable in practice: CI can require the public
    /// `Wasm` tier it is able to run while leaving the private `WasmVerifier`
    /// tier free to skip, where a single boolean would have forced CI to choose
    /// between requiring nothing and failing on a checkout it cannot have.
    const REQUIRE_ENV: &str = "INFERENCE_ROCQ_DRIFT_REQUIRE";

    /// Set to `1` to rewrite the pin's `absent-digest` lines from the checkouts
    /// present, after a deliberate pin bump.
    const UPDATE_ENV: &str = "INFERENCE_ROCQ_DRIFT_UPDATE";

    fn env_flag(name: &str) -> bool {
        std::env::var(name).is_ok_and(|value| value == "1")
    }

    /// Whether this run must establish `tier` rather than skip it.
    fn is_required(tier: &Tier) -> bool {
        let Ok(value) = std::env::var(REQUIRE_ENV) else {
            return false;
        };
        let value = value.trim();
        value == "1" || value == "all" || value.split(',').any(|name| name.trim() == tier.namespace)
    }

    /// Repository root, from this crate's manifest directory.
    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("tests crate has a parent directory")
            .to_path_buf()
    }

    fn stub_dir() -> PathBuf {
        repo_root().join("core").join("wasm-to-v").join("rocq-stub")
    }

    fn pin_path() -> PathBuf {
        repo_root()
            .join("core")
            .join("wasm-to-v")
            .join("wasm-verifier-pin.txt")
    }

    // ---------------------------------------------------------------- the pin

    /// The committed pin: which upstream revisions the stub mirrors, which
    /// standard-library names it is allowed to re-declare, and one digest of the
    /// real-only declaration set per logical module.
    struct Pin {
        scalars: BTreeMap<String, String>,
        stdlib_names: BTreeSet<String>,
        absent: BTreeMap<String, (String, usize)>,
    }

    impl Pin {
        fn read() -> Self {
            let path = pin_path();
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
            let mut scalars = BTreeMap::new();
            let mut stdlib_names = BTreeSet::new();
            let mut absent = BTreeMap::new();
            for line in text.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                let mut parts = line.split_whitespace();
                let key = parts.next().expect("a non-empty line has a first word");
                match key {
                    "stdlib-name" => {
                        let name = parts.next().unwrap_or_else(|| {
                            panic!("`stdlib-name` needs a name: {line:?}");
                        });
                        stdlib_names.insert(name.to_string());
                    }
                    "absent-digest" => {
                        let (module, digest, count) =
                            match (parts.next(), parts.next(), parts.next()) {
                                (Some(m), Some(d), Some(c)) => (m, d, c),
                                _ => panic!(
                                    "`absent-digest` needs a module, a digest and a count: \
                                     {line:?}"
                                ),
                            };
                        let count = count.parse().unwrap_or_else(|e| {
                            panic!("`absent-digest` count for {module} is not a number: {e}");
                        });
                        absent.insert(module.to_string(), (digest.to_string(), count));
                    }
                    _ => {
                        let value = parts.next().unwrap_or_else(|| {
                            panic!("pin line {line:?} has a key but no value");
                        });
                        scalars.insert(key.to_string(), value.to_string());
                    }
                }
            }
            Self {
                scalars,
                stdlib_names,
                absent,
            }
        }

        fn scalar(&self, key: &str) -> &str {
            self.scalars
                .get(key)
                .unwrap_or_else(|| panic!("pin has no `{key}` line"))
        }

        /// Every revision the pin records, for the prose scan to accept.
        fn revisions(&self) -> Vec<&str> {
            vec![self.scalar("revision"), self.scalar("coq-wasm-commit")]
        }
    }

    /// Rewrites the `absent-digest` lines for `updates`, leaving every other line
    /// untouched.
    ///
    /// Serialized because the two tier tests run on separate threads and would
    /// otherwise read-modify-write the same file concurrently, each dropping the
    /// other's lines.
    fn update_pin(updates: &BTreeMap<String, (String, usize)>) {
        static LOCK: Mutex<()> = Mutex::new(());
        let _guard = LOCK.lock().expect("pin update lock");
        let path = pin_path();
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let rewritten: Vec<String> = text
            .lines()
            .map(|line| {
                let module = line
                    .strip_prefix("absent-digest ")
                    .and_then(|rest| rest.split_whitespace().next());
                match module.and_then(|m| updates.get(m).map(|u| (m, u))) {
                    Some((module, (digest, count))) => {
                        format!("absent-digest {module} {digest} {count}")
                    }
                    None => line.to_string(),
                }
            })
            .collect();
        std::fs::write(&path, rewritten.join("\n") + "\n")
            .unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
    }

    // --------------------------------------------------------------- the tiers

    /// One upstream library, the stub directory mirroring it, and how to find a
    /// checkout of it.
    struct Tier {
        /// Names the tier in skip lines and failures.
        label: &'static str,
        /// Rocq logical namespace, and the prefix of this tier's pin lines.
        namespace: &'static str,
        /// Directory under `rocq-stub/` holding this tier's mirror.
        stub_subdir: &'static str,
        /// Environment variable naming a checkout, consulted first.
        repo_env: &'static str,
        /// Sibling directory of this repository, used when the variable is unset.
        sibling: &'static str,
        /// Pin key holding the revision to read the upstream sources at.
        revision_key: &'static str,
        /// Stub module name paired with the upstream path it mirrors.
        modules: &'static [(&'static str, &'static str)],
        /// What a run establishes, for the skip line to say what it did not.
        claim: &'static str,
    }

    impl Tier {
        const WASM_CERT: Self = Self {
            label: "WasmCert-Coq",
            namespace: "Wasm",
            stub_subdir: "wasm",
            repo_env: "WASM_CERT_REPO",
            sibling: "WasmCert-Coq",
            revision_key: "coq-wasm-commit",
            modules: &[
                ("datatypes", "theories/datatypes.v"),
                ("bytes", "theories/bytes.v"),
                ("numerics", "theories/numerics.v"),
                ("host", "theories/host.v"),
            ],
            claim: "the stub's WASM datatypes mirror still matches the public \
                    coq-wasm library",
        };

        const WASM_VERIFIER: Self = Self {
            label: "wasm-verifier",
            namespace: "WasmVerifier",
            stub_subdir: "wasm_verifier",
            repo_env: "WASM_VERIFIER_REPO",
            sibling: "wasm-verifier",
            revision_key: "revision",
            modules: &[
                ("Assertions", "theories/Assertions.v"),
                ("Verifier", "theories/Verifier.v"),
                ("Exists", "theories/Exists.v"),
            ],
            claim: "the stub's assertion and obligation mirror still matches the \
                    private wasm-verifier library",
        };

        const ALL: &'static [&'static Self] = &[&Self::WASM_CERT, &Self::WASM_VERIFIER];

        /// Pin key for one of this tier's modules: `Wasm.datatypes` and friends.
        fn pin_key(&self, module: &str) -> String {
            format!("{}.{module}", self.namespace)
        }

        /// Locates a checkout, or explains why this tier cannot run.
        ///
        /// An explicitly set variable is authoritative: pointing it at something
        /// that is not a checkout fails rather than quietly falling back, so a
        /// typo in CI cannot turn a real run into a skipped one.
        fn find_checkout(&self) -> Result<PathBuf, String> {
            if let Ok(from_env) = std::env::var(self.repo_env) {
                let path = PathBuf::from(&from_env);
                return if path.join(".git").exists() {
                    Ok(path)
                } else {
                    Err(format!(
                        "${} is set to {from_env}, which is not a git checkout",
                        self.repo_env
                    ))
                };
            }
            let sibling = repo_root()
                .parent()
                .map(|p| p.join(self.sibling))
                .filter(|p| p.join(".git").exists());
            sibling.ok_or_else(|| {
                format!(
                    "no {} checkout (set ${} or place one at ../{})",
                    self.label, self.repo_env, self.sibling
                )
            })
        }
    }

    /// Reads a file at a pinned revision.
    ///
    /// Everything this module concludes is read this way, never from the working
    /// tree, so a checkout parked on another branch cannot change the verdict --
    /// the same discipline wasm-verifier's reciprocal pin gate applies in the
    /// other direction.
    fn source_at(repo: &Path, revision: &str, path: &str) -> Result<String, String> {
        let output = Command::new("git")
            .arg("-C")
            .arg(repo)
            .arg("show")
            .arg(format!("{revision}:{path}"))
            .output()
            .map_err(|e| format!("failed to run git in {}: {e}", repo.display()))?;
        if output.status.success() {
            return String::from_utf8(output.stdout)
                .map_err(|e| format!("{path} at {revision} is not UTF-8: {e}"));
        }
        // Git's own words are the diagnosis: "fetch the pinned revision" is
        // the repair for a missing commit, and the wrong one for a checkout git
        // refuses to read at all (dubious ownership, not a repository) or for
        // one whose object of that name is not the commit this repository
        // knows (a replace ref, a promisor remote that could not supply the
        // tree). Those are told apart only by what the checkout itself says,
        // and the checkout is on a machine the person reading this may not be
        // able to reach, so it is asked here.
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!(
            "{} has no {path} at {revision}; fetch the pinned revision (git: {})\n{}",
            repo.display(),
            redact_url_credentials(stderr.trim()),
            describe_checkout(repo, revision)
        ))
    }

    /// Replaces the userinfo of every URL in `text` — `https://user:token@host`
    /// becomes `https://***@host`. A checkout's remote may carry a token in its
    /// URL, and everything this module says about a checkout ends up in a CI
    /// log, so nothing it quotes may repeat one.
    fn redact_url_credentials(text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(start) = rest.find("://") {
            let after = &rest[start + 3..];
            let authority_end = after
                .find(|c: char| c == '/' || c.is_whitespace())
                .unwrap_or(after.len());
            let authority = &after[..authority_end];
            match authority.rfind('@') {
                Some(at) => {
                    out.push_str(&rest[..start + 3]);
                    out.push_str("***");
                    out.push_str(&authority[at..]);
                    rest = &after[authority_end..];
                }
                None => {
                    out.push_str(&rest[..start + 3 + authority_end]);
                    rest = &after[authority_end..];
                }
            }
        }
        out.push_str(rest);
        out
    }

    /// What a checkout is, in the terms that decide whether a pinned revision
    /// can be read from it: its toplevel, its remotes, whether it is a partial
    /// clone, whether the revision is replaced, and the top of the tree the
    /// revision names. Every probe is best-effort; a probe that fails reports
    /// its own failure rather than hiding the others.
    fn describe_checkout(repo: &Path, revision: &str) -> String {
        let probe = |args: &[&str]| -> String {
            let output = Command::new("git").arg("-C").arg(repo).args(args).output();
            match output {
                Ok(out) if out.status.success() => {
                    let text = String::from_utf8_lossy(&out.stdout);
                    let lines: Vec<&str> = text.lines().take(8).collect();
                    if lines.is_empty() {
                        "(empty)".to_string()
                    } else {
                        redact_url_credentials(&lines.join(" | "))
                    }
                }
                // `git config --get` of an absent key exits 1 and says nothing;
                // that is an answer, not a failure.
                Ok(out) if out.stderr.is_empty() => "(unset)".to_string(),
                Ok(out) => format!(
                    "(failed: {})",
                    redact_url_credentials(String::from_utf8_lossy(&out.stderr).trim())
                ),
                Err(e) => format!("(could not run git: {e})"),
            }
        };
        [
            ("toplevel", probe(&["rev-parse", "--show-toplevel"])),
            ("remotes", probe(&["remote", "-v"])),
            (
                "partial clone filter",
                probe(&["config", "--get", "remote.origin.partialclonefilter"]),
            ),
            ("replace refs", probe(&["replace", "-l"])),
            (
                "revision",
                probe(&["log", "-1", "--format=%H %T %s", revision]),
            ),
            ("tree top", probe(&["ls-tree", "--name-only", revision])),
        ]
        .into_iter()
        .map(|(what, said)| format!("  {what}: {said}"))
        .collect::<Vec<_>>()
        .join("\n")
    }

    // ------------------------------------------------------- reading a `.v`

    /// Stands in for a Rocq sentence terminator.
    ///
    /// Rocq ends a sentence with a `.` followed by whitespace or end of input;
    /// the `.` inside a qualified name like `Byte.byte` is followed immediately
    /// by more name. [`tokenize`] discards whitespace, so the two arrive as the
    /// same `Punct('.')` unless the distinction is made before tokenizing. No
    /// Rocq source outside a string literal contains this character, so the
    /// scanners below can steer by it.
    const COMMAND_END: char = '§';

    /// Rewrites every sentence-terminating `.` to [`COMMAND_END`].
    ///
    /// String literals are copied through untouched, so a `. ` inside one cannot
    /// be mistaken for the end of a sentence.
    fn mark_command_ends(stripped: &str) -> String {
        let mut out = String::with_capacity(stripped.len());
        let mut chars = stripped.chars().peekable();
        let mut in_string = false;
        while let Some(c) = chars.next() {
            if in_string {
                out.push(c);
                in_string = c != '"';
            } else if c == '"' {
                out.push(c);
                in_string = true;
            } else if c == '.' && chars.peek().is_none_or(|next| next.is_whitespace()) {
                out.push(COMMAND_END);
            } else {
                out.push(c);
            }
        }
        out
    }

    /// Comment-stripped, sentence-marked source, ready for [`tokenize`].
    fn prepared(source: &str) -> String {
        mark_command_ends(&strip_rocq_comments(source))
    }

    fn opens_group(token: &Tok<'_>) -> bool {
        matches!(token, Tok::Punct('(' | '{' | '['))
    }

    fn closes_group(token: &Tok<'_>) -> bool {
        matches!(token, Tok::Punct(')' | '}' | ']'))
    }

    /// Index of the [`COMMAND_END`] closing the sentence that starts at `at`, or
    /// the end of the token stream.
    fn sentence_end(tokens: &[Tok<'_>], at: usize) -> usize {
        let mut depth = 0i32;
        for (offset, token) in tokens[at..].iter().enumerate() {
            if opens_group(token) {
                depth += 1;
            } else if closes_group(token) {
                depth -= 1;
            } else if depth <= 0 && matches!(token, Tok::Punct(COMMAND_END)) {
                return at + offset;
            }
        }
        tokens.len()
    }

    /// Index of the `:=` at depth zero within `range`, if any.
    fn assign_at(tokens: &[Tok<'_>], from: usize, to: usize) -> Option<usize> {
        let mut depth = 0i32;
        for at in from..to {
            if opens_group(&tokens[at]) {
                depth += 1;
            } else if closes_group(&tokens[at]) {
                depth -= 1;
            } else if depth == 0 && is_punct(tokens, at, ':') && is_punct(tokens, at + 1, '=') {
                return Some(at);
            }
        }
        None
    }

    /// A token span as owned strings, which is the form normalization works in.
    ///
    /// A string literal renders as a pair of bare quotes: its content is not part
    /// of any type, and [`tokenize`] has already discarded it.
    fn owned(tokens: &[Tok<'_>]) -> Vec<String> {
        tokens
            .iter()
            .map(|token| match token {
                Tok::Ident(name) => (*name).to_string(),
                Tok::Punct(c) => c.to_string(),
                Tok::Str => "\"\"".to_string(),
            })
            .collect()
    }

    fn is_identifier(token: &str) -> bool {
        token
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
    }

    /// Splits a token span at every depth-zero `->`.
    fn split_arrows(tokens: &[String]) -> Vec<Vec<String>> {
        let mut parts = vec![Vec::new()];
        let mut depth = 0i32;
        let mut at = 0;
        while at < tokens.len() {
            match tokens[at].as_str() {
                "(" | "{" | "[" => depth += 1,
                ")" | "}" | "]" => depth -= 1,
                _ => {}
            }
            if depth == 0 && tokens[at] == "-" && tokens.get(at + 1).is_some_and(|t| t == ">") {
                parts.push(Vec::new());
                at += 2;
                continue;
            }
            parts
                .last_mut()
                .expect("parts always holds the part being filled")
                .push(tokens[at].clone());
            at += 1;
        }
        parts
    }

    /// Renders one type as a comparable string.
    ///
    /// Parentheses are dropped here, and only here: they are arity-neutral in
    /// these declarations once aliases are expanded -- the stub writes
    /// `list (list basic_instruction)` where coq-wasm writes `list expr` and
    /// `expr` unfolds to `list basic_instruction` -- but they are *not* neutral
    /// to [`split_arrows`], which is why the arrows are split off first.
    fn flatten(tokens: &[String]) -> String {
        tokens
            .iter()
            .filter(|token| !matches!(token.as_str(), "(" | ")"))
            .cloned()
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Transparent type abbreviations, as `Definition name [: Set | Type] := path`.
    ///
    /// Only bodies made of identifiers and qualifier dots are collected. A
    /// `Definition` taking binders, or one whose body computes, is not an
    /// abbreviation a declared type can be spelled through, and expanding it
    /// would compare implementations rather than contracts.
    fn alias_table(tokens: &[Tok<'_>]) -> BTreeMap<String, Vec<String>> {
        let mut aliases = BTreeMap::new();
        let mut at = 0;
        while at < tokens.len() {
            if ident_at(tokens, at) != Some("Definition") {
                at += 1;
                continue;
            }
            let end = sentence_end(tokens, at);
            let (Some(name), Some(assign)) =
                (ident_at(tokens, at + 1), assign_at(tokens, at + 2, end))
            else {
                at = end.max(at + 1);
                continue;
            };
            let header_is_bare = assign == at + 2
                || (assign == at + 4
                    && is_punct(tokens, at + 2, ':')
                    && matches!(ident_at(tokens, at + 3), Some("Set" | "Type")));
            let body = owned(&tokens[assign + 2..end]);
            let body_is_a_path = !body.is_empty()
                && body
                    .iter()
                    .all(|token| is_identifier(token) || token == ".");
            if header_is_bare && body_is_a_path {
                aliases.entry(name.to_string()).or_insert(body);
            }
            at = end.max(at + 1);
        }
        aliases
    }

    /// Expands abbreviations until the token span stops changing.
    ///
    /// Three guards earn the fixed point:
    ///
    /// - a name is never expanded into a body that mentions it, or coq-wasm's
    ///   `Definition byte := Integers.byte` would unfold forever;
    /// - a name is never expanded where it is the tail of a qualified path, so
    ///   `Wasm_int.Int32.T` keeps its `T` rather than picking up an unrelated
    ///   module-local `Definition T := int`;
    /// - `seq` is mathcomp's spelling of `list`, and CompCert's `byte` reaches
    ///   these files under several qualifications of the same type.
    fn expand(tokens: &[String], aliases: &BTreeMap<String, Vec<String>>) -> Vec<String> {
        let mut current = tokens.to_vec();
        for _ in 0..8 {
            let mut next: Vec<String> = Vec::with_capacity(current.len());
            for (at, token) in current.iter().enumerate() {
                let qualified = at > 0 && current[at - 1] == ".";
                let body = (!qualified).then(|| aliases.get(token)).flatten();
                match body {
                    Some(body) if !body.contains(token) => next.extend(body.iter().cloned()),
                    _ if token == "seq" && !qualified => next.push("list".to_string()),
                    _ => {
                        if token == "byte" {
                            while next.len() >= 2
                                && next[next.len() - 1] == "."
                                && matches!(next[next.len() - 2].as_str(), "Integers" | "Byte")
                            {
                                next.truncate(next.len() - 2);
                            }
                        }
                        next.push(token.clone());
                    }
                }
            }
            if next == current {
                break;
            }
            current = next;
        }
        current
    }

    /// The shape of a constructor: its argument list, with the result type
    /// dropped.
    ///
    /// Upstream writes a nullary constructor bare -- coq-wasm's `number_type` is
    /// `| T_i32 | T_i64` -- while the stub writes `| T_i32 : number_type`.
    /// Dropping the result type makes the two spellings the same zero-argument
    /// shape, and leaves arity, which is what a drifting constructor changes.
    fn constructor_shape(declared: &[String], aliases: &BTreeMap<String, Vec<String>>) -> String {
        let expanded = expand(declared, aliases);
        let parts = split_arrows(&expanded);
        let arguments = &parts[..parts.len() - 1];
        format!(
            "({})",
            arguments
                .iter()
                .map(|part| flatten(part))
                .collect::<Vec<_>>()
                .join(" -> ")
        )
    }

    fn field_shape(declared: &[String], aliases: &BTreeMap<String, Vec<String>>) -> String {
        flatten(&expand(declared, aliases))
    }

    /// Every inductive, record and class shape a file declares.
    ///
    /// Keys are the type's own name, and `Type.member` for each constructor or
    /// field; values are the marker `<inductive>` / `<record>` for a type and the
    /// normalized shape for a member. A name declared twice keeps its first
    /// declaration, so the map does not depend on scan order.
    ///
    /// A mutual `Inductive a := … with b := …` contributes only its first block.
    /// Neither the stub nor either pinned upstream module declares one, and the
    /// narrowing digest would notice one appearing.
    fn shapes(
        tokens: &[Tok<'_>],
        aliases: &BTreeMap<String, Vec<String>>,
    ) -> BTreeMap<String, String> {
        let mut out: BTreeMap<String, String> = BTreeMap::new();
        let mut at = 0;
        while at < tokens.len() {
            match ident_at(tokens, at) {
                Some("Inductive" | "Variant") => {
                    let end = sentence_end(tokens, at);
                    let Some(name) = ident_at(tokens, at + 1) else {
                        at = end.max(at + 1);
                        continue;
                    };
                    out.entry(name.to_string())
                        .or_insert_with(|| "<inductive>".to_string());
                    if let Some(assign) = assign_at(tokens, at + 2, end) {
                        for (constructor, declared) in constructors(tokens, assign + 2, end) {
                            out.entry(format!("{name}.{constructor}"))
                                .or_insert_with(|| constructor_shape(&declared, aliases));
                        }
                    }
                    at = end.max(at + 1);
                }
                Some("Record" | "Class" | "Structure") => {
                    let end = sentence_end(tokens, at);
                    let Some(name) = ident_at(tokens, at + 1) else {
                        at = end.max(at + 1);
                        continue;
                    };
                    out.entry(name.to_string())
                        .or_insert_with(|| "<record>".to_string());
                    for (field, declared) in fields(tokens, at, end) {
                        out.entry(format!("{name}.{field}"))
                            .or_insert_with(|| field_shape(&declared, aliases));
                    }
                    at = end.max(at + 1);
                }
                _ => at += 1,
            }
        }
        out
    }

    /// Constructors of the inductive body spanning `from..to`, each paired with
    /// its declared type as a token span. A bare `| T_i32` yields an empty span.
    fn constructors(tokens: &[Tok<'_>], from: usize, to: usize) -> Vec<(String, Vec<String>)> {
        let mut arms: Vec<(usize, usize)> = Vec::new();
        let mut depth = 0i32;
        let mut start = from;
        for at in from..to {
            if opens_group(&tokens[at]) {
                depth += 1;
            } else if closes_group(&tokens[at]) {
                depth -= 1;
            } else if depth == 0 && ident_at(tokens, at) == Some("with") {
                arms.push((start, at));
                return finish_arms(tokens, &arms);
            } else if depth == 0 && is_punct(tokens, at, '|') {
                arms.push((start, at));
                start = at + 1;
            }
        }
        arms.push((start, to));
        finish_arms(tokens, &arms)
    }

    /// Splits each arm into its constructor name and its declared type.
    fn finish_arms(tokens: &[Tok<'_>], arms: &[(usize, usize)]) -> Vec<(String, Vec<String>)> {
        arms.iter()
            .filter_map(|&(start, end)| {
                let name = ident_at(tokens, start).filter(|_| start < end)?;
                let declared = if is_punct(tokens, start + 1, ':') {
                    owned(&tokens[start + 2..end])
                } else {
                    Vec::new()
                };
                Some((name.to_string(), declared))
            })
            .collect()
    }

    /// Fields of the record or class whose keyword sits at `at`, each paired with
    /// its declared type as a token span.
    fn fields(tokens: &[Tok<'_>], at: usize, end: usize) -> Vec<(String, Vec<String>)> {
        let Some(open) = (at..end).find(|&i| is_punct(tokens, i, '{')) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let mut depth = 1i32;
        let mut start = open + 1;
        for at in open + 1..end {
            if opens_group(&tokens[at]) {
                depth += 1;
            } else if closes_group(&tokens[at]) {
                depth -= 1;
                if depth == 0 {
                    push_field(tokens, start, at, &mut out);
                    break;
                }
            } else if depth == 1 && is_punct(tokens, at, ';') {
                push_field(tokens, start, at, &mut out);
                start = at + 1;
            }
        }
        out
    }

    fn push_field(
        tokens: &[Tok<'_>],
        start: usize,
        end: usize,
        out: &mut Vec<(String, Vec<String>)>,
    ) {
        if start < end
            && let Some(name) = ident_at(tokens, start)
            && is_punct(tokens, start + 1, ':')
        {
            out.push((name.to_string(), owned(&tokens[start + 2..end])));
        }
    }

    /// Names the stub declares in their own right: inductive, record and class
    /// types, and opaque `Parameter`/`Axiom` bindings.
    ///
    /// These are leaves of the contract on both sides. The stub says there is a
    /// type called `i32`; coq-wasm agrees, and additionally makes it transparent
    /// with `Definition i32 := Wasm_int.Int32.T`. Expanding that would compare
    /// the stub's deliberate opacity against upstream's implementation and call
    /// the difference drift, when no emitted `.v` ever spells the expansion.
    fn leaf_names(tokens: &[Tok<'_>]) -> BTreeSet<String> {
        let mut leaves: BTreeSet<String> = shapes(tokens, &BTreeMap::new())
            .keys()
            .filter(|key| !key.contains('.'))
            .cloned()
            .collect();
        let mut at = 0;
        while at < tokens.len() {
            if matches!(ident_at(tokens, at), Some("Parameter" | "Axiom")) {
                at += 1;
                while let Some(name) = ident_at(tokens, at) {
                    leaves.insert(name.to_string());
                    at += 1;
                }
            } else {
                at += 1;
            }
        }
        leaves
    }

    // ---------------------------------------------------------- one tier's run

    fn digest_of(names: &BTreeSet<String>) -> String {
        let mut hasher = Sha256::new();
        hasher.update(
            names
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
                .as_bytes(),
        );
        hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    /// One module held to one upstream module.
    struct Site {
        /// Pin key, and the prefix every finding about this module carries.
        key: String,
        /// How a finding should describe the upstream text.
        upstream: String,
        stub: String,
        real: String,
    }

    /// The outcome of holding a set of stub modules to their upstream originals.
    struct Comparison {
        /// Every disagreement, in module order.
        findings: Vec<String>,
        /// Per site key, the names upstream declares and the stub does not.
        real_only: BTreeMap<String, BTreeSet<String>>,
        /// How many shapes were actually compared, so a run that compared none
        /// can be told from a run that found nothing wrong.
        compared: usize,
    }

    /// Runs the fiction direction over a whole tier's worth of sources, and
    /// computes the set the narrowing direction digests.
    ///
    /// [`check_tier`] supplies text read at the pinned revision and turns
    /// `real_only` into a digest comparison; the unit tests supply hand-written
    /// pairs. Both reach the scanners and the normalization through this one
    /// function, so the rules the unit tests prove load-bearing are the rules the
    /// tier gates apply.
    fn compare(sites: &[Site], stdlib_names: &BTreeSet<String>) -> Comparison {
        let stub_text: Vec<String> = sites.iter().map(|site| prepared(&site.stub)).collect();
        let real_text: Vec<String> = sites.iter().map(|site| prepared(&site.real)).collect();
        let stub_tokens: Vec<Vec<Tok<'_>>> = stub_text.iter().map(|t| tokenize(t)).collect();
        let real_tokens: Vec<Vec<Tok<'_>>> = real_text.iter().map(|t| tokenize(t)).collect();

        // The stub's own vocabulary fixes what stays a leaf on both sides, so one
        // set filters both alias tables rather than each side filtering its own.
        let leaves: BTreeSet<String> = stub_tokens
            .iter()
            .flat_map(|tokens| leaf_names(tokens))
            .collect();
        let table = |all: &[Vec<Tok<'_>>]| -> BTreeMap<String, Vec<String>> {
            all.iter()
                .flat_map(|tokens| alias_table(tokens))
                .filter(|(name, _)| !leaves.contains(name))
                .collect()
        };
        let stub_aliases = table(&stub_tokens);
        let real_aliases = table(&real_tokens);

        let mut out = Comparison {
            findings: Vec::new(),
            real_only: BTreeMap::new(),
            compared: 0,
        };
        for (at, site) in sites.iter().enumerate() {
            let key = &site.key;
            let upstream = &site.upstream;
            let stub_names: BTreeSet<String> = declared_names(&site.stub).into_iter().collect();
            let real_names: BTreeSet<String> =
                upstream_declared_names(&site.real).into_iter().collect();
            let stub_shapes = shapes(&stub_tokens[at], &stub_aliases);
            let real_shapes = shapes(&real_tokens[at], &real_aliases);

            for name in &stub_names {
                if stdlib_names.contains(name) {
                    if real_names.contains(name) {
                        out.findings.push(format!(
                            "{key}: `{name}` is exempted as a Coq standard-library name, but \
                             {upstream} declares it -- drop the `stdlib-name` line"
                        ));
                    }
                    continue;
                }
                if !real_names.contains(name) {
                    out.findings.push(format!(
                        "{key}: the stub declares `{name}`, which {upstream} does not declare"
                    ));
                }
            }
            for (name, stub_shape) in &stub_shapes {
                match real_shapes.get(name) {
                    None => out.findings.push(format!(
                        "{key}: the stub declares the shape `{name}`, absent from {upstream}"
                    )),
                    Some(real_shape) if real_shape != stub_shape => out.findings.push(format!(
                        "{key}: `{name}` is `{stub_shape}` in the stub and `{real_shape}` in \
                         {upstream}"
                    )),
                    Some(_) => {}
                }
                out.compared += 1;
            }
            // The stub's own inductive, record and class TYPE names reach it
            // only through the shape scanner: [`declared_names`] deliberately
            // skips them, because the coverage audit it was written for asks
            // about terms an emitted module can apply. Left out here they would
            // be counted as declarations only upstream has, and the count a
            // failure reports would name types the stub plainly does declare.
            let stub_declared: BTreeSet<String> = stub_names
                .iter()
                .cloned()
                .chain(stub_shapes.keys().filter(|key| !key.contains('.')).cloned())
                .collect();
            out.real_only.insert(
                key.clone(),
                real_names.difference(&stub_declared).cloned().collect(),
            );
        }
        out
    }

    /// Runs both directions for one tier, or says which claim it could not
    /// establish.
    fn check_tier(tier: &Tier) {
        let pin = Pin::read();
        let checkout = match tier.find_checkout() {
            Ok(path) => path,
            Err(why) => {
                assert!(
                    !is_required(tier),
                    "{REQUIRE_ENV} names {}, but {why}: this run was required to establish \
                     that {}, and did not",
                    tier.namespace,
                    tier.claim
                );
                eprintln!(
                    "skipped: {why}. NOT established by this run: {}. Neither the fiction \
                     direction (does every stub declaration exist upstream?) nor the \
                     narrowing direction (is the real-only declaration set still the one the \
                     pin records?) was checked for {}.",
                    tier.claim, tier.namespace
                );
                return;
            }
        };
        let revision = pin.scalar(tier.revision_key).to_string();
        let sites: Vec<Site> = tier
            .modules
            .iter()
            .map(|&(module, upstream_path)| {
                let path = stub_dir()
                    .join(tier.stub_subdir)
                    .join(format!("{module}.v"));
                Site {
                    key: tier.pin_key(module),
                    upstream: format!("{upstream_path} at {revision}"),
                    stub: std::fs::read_to_string(&path)
                        .unwrap_or_else(|e| panic!("read {}: {e}", path.display())),
                    real: source_at(&checkout, &revision, upstream_path)
                        .unwrap_or_else(|why| panic!("{}: {why}", tier.label)),
                }
            })
            .collect();

        let mut result = compare(&sites, &pin.stdlib_names);
        let updating = env_flag(UPDATE_ENV);
        let mut digests: BTreeMap<String, (String, usize)> = BTreeMap::new();
        for (key, real_only) in &result.real_only {
            let digest = digest_of(real_only);
            digests.insert(key.clone(), (digest.clone(), real_only.len()));
            if updating {
                continue;
            }
            let (recorded, count) = pin
                .absent
                .get(key)
                .unwrap_or_else(|| panic!("the pin has no `absent-digest {key}` line"));
            if &digest != recorded || *count != real_only.len() {
                let names: Vec<&String> = real_only.iter().collect();
                result.findings.push(format!(
                    "{key}: the real-only declaration set moved. The pin records {count} \
                     name(s) digesting to {recorded}; the checkout at {revision} now has {} \
                     digesting to {digest}. Names read live from {}:\n      {names:?}\n    \
                     Re-run with {UPDATE_ENV}=1 once the move is understood.",
                    real_only.len(),
                    checkout.display()
                ));
            }
        }

        assert!(
            result.compared > 0,
            "{}: no shape was compared, so this run established nothing",
            tier.namespace
        );
        assert!(
            result.findings.is_empty(),
            "the vendored stub disagrees with {} at {revision}:\n  - {}",
            tier.label,
            result.findings.join("\n  - ")
        );
        if updating {
            update_pin(&digests);
            eprintln!(
                "{UPDATE_ENV}=1: rewrote {} `absent-digest` line(s) for {}",
                digests.len(),
                tier.namespace
            );
        }
    }

    /// Holds the stub's `Wasm.*` mirror to the public coq-wasm library.
    ///
    /// Public, so no credential is involved and CI runs it with
    /// `INFERENCE_ROCQ_DRIFT_REQUIRE=1`.
    #[test]
    fn stub_matches_wasm_cert_coq_at_the_pinned_commit() {
        check_tier(&Tier::WASM_CERT);
    }

    /// Holds the stub's `WasmVerifier.*` mirror to the private wasm-verifier
    /// library.
    ///
    /// Private, so this runs where a checkout exists and skips loudly elsewhere.
    /// A green run of the suite in CI does not mean this ran.
    #[test]
    fn stub_matches_wasm_verifier_at_the_pinned_revision() {
        check_tier(&Tier::WASM_VERIFIER);
    }

    // -------------------------------------------------- always-on pin gates

    /// Prose files restating the pin. Both are the contract documents a reader
    /// consults instead of the pin file, so both have to agree with it.
    const PROSE: &[&str] = &[
        "README.md",
        "core/wasm-to-v/ROCQ_CONTRACT.md",
        "core/wasm-to-v/rocq-stub/README.md",
    ];

    /// The number of pin restatements the prose held when this gate was written.
    ///
    /// A scan that passes because it found nothing is the false green this whole
    /// module exists to remove, so the count is asserted and not merely the
    /// agreement. Adding a restatement raises the count and still passes;
    /// deleting one fails, which is the point -- a restatement silently dropped
    /// is a claim that stops being checked.
    const PROSE_RESTATEMENT_FLOOR: usize = 10;

    /// A pin value restated in prose.
    #[derive(Debug, PartialEq)]
    enum Restatement {
        /// A backticked git revision, possibly abbreviated.
        Revision(String),
        /// A `vMAJOR.MINOR.PATCH` library tag.
        Tag(String),
    }

    /// Every pin restatement in one prose file.
    ///
    /// A revision is read only from inside a code span, which is how both
    /// documents write one and which keeps ordinary prose -- `decade`, `faced` --
    /// from being read as an abbreviated SHA. The span is matched locally, a
    /// backtick and a hex run and a backtick, rather than by counting backticks
    /// from the top of the file: both documents fence code blocks with ``` and
    /// three backticks put every later span on the wrong side of a parity count,
    /// which silently hid the README's two revisions entirely.
    ///
    /// A tag is read anywhere, because the documents write one in bold and in
    /// parentheses as often as in a code span. Two dotted components are enough
    /// to be a tag, so a `v2.3` that should have said `v2.2.0` is caught rather
    /// than skipped; one component is not, or WASM's own `v128` would be read as
    /// a library version.
    fn restatements(text: &str) -> Vec<Restatement> {
        let chars: Vec<char> = text.chars().collect();
        let run_from = |at: usize, accept: &dyn Fn(char) -> bool| {
            (at..chars.len())
                .find(|&i| !accept(chars[i]))
                .unwrap_or(chars.len())
        };
        let mut found = Vec::new();
        for at in 0..chars.len() {
            let starts_a_word = at == 0 || !chars[at - 1].is_ascii_alphanumeric();
            if chars[at] == '`' {
                let end = run_from(at + 1, &|c: char| {
                    c.is_ascii_hexdigit() && !c.is_ascii_uppercase()
                });
                if (7..=40).contains(&(end - at - 1)) && chars.get(end) == Some(&'`') {
                    found.push(Restatement::Revision(chars[at + 1..end].iter().collect()));
                }
            } else if chars[at] == 'v' && starts_a_word {
                let end = run_from(at + 1, &|c: char| c.is_ascii_digit() || c == '.');
                let tag: String = chars[at..end].iter().collect();
                // A tag at the end of a sentence carries the sentence's full stop.
                let tag = tag.trim_end_matches('.');
                let parts: Vec<&str> = tag[1..].split('.').collect();
                if parts.len() >= 2 && parts.iter().all(|part| !part.is_empty()) {
                    found.push(Restatement::Tag(tag.to_string()));
                }
            }
        }
        found
    }

    /// Every prose restatement of the pin agrees with the pin file.
    ///
    /// The pin file is the one place the upstream revisions are recorded, but it
    /// is not the place a reader looks: the contract document and the stub README
    /// both name them in prose, and both said `0c5d525e` for a wasm-verifier
    /// commit that no gate had checked for as long as it had been written down.
    /// Prose that restates a machine-checked fact is exactly where a stale claim
    /// hides, because nothing ever turns it red.
    #[test]
    fn prose_restatements_of_the_pin_agree_with_the_pin_file() {
        let pin = Pin::read();
        let tag = pin.scalar("coq-wasm-tag");
        let revisions = pin.revisions();
        let mut total = 0usize;
        let mut wrong: Vec<String> = Vec::new();
        for relative in PROSE {
            let path = repo_root().join(relative);
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
            assert!(!text.is_empty(), "{relative} is empty");
            let found = restatements(&text);
            assert!(
                !found.is_empty(),
                "{relative} restates no pin value, so scanning it establishes nothing"
            );
            for restatement in &found {
                match restatement {
                    Restatement::Revision(revision) => {
                        if !revisions.iter().any(|pinned| pinned.starts_with(revision)) {
                            wrong.push(format!(
                                "{relative} names revision `{revision}`, which is none of the \
                                 pinned {revisions:?}"
                            ));
                        }
                    }
                    Restatement::Tag(found) => {
                        if found != tag {
                            wrong.push(format!(
                                "{relative} names library tag {found}, but the pin records {tag}"
                            ));
                        }
                    }
                }
            }
            total += found.len();
        }
        assert!(
            wrong.is_empty(),
            "prose disagrees with {}:\n  - {}",
            pin_path().display(),
            wrong.join("\n  - ")
        );
        assert!(
            total >= PROSE_RESTATEMENT_FLOOR,
            "the prose scan found {total} restatement(s), below the floor of \
             {PROSE_RESTATEMENT_FLOOR}: a restatement was deleted, or the scan stopped \
             recognising one"
        );
    }

    /// Every vendored stub module is drift-checked and pinned.
    ///
    /// The tier tables and the pin are both written by hand, and a stub file
    /// added to neither would be mirrored by nothing and checked by nothing while
    /// the `coqc` gate went on compiling against it.
    #[test]
    fn every_vendored_stub_module_is_covered_by_a_tier_and_a_pin_line() {
        let pin = Pin::read();
        let mut covered: BTreeSet<String> = BTreeSet::new();
        for tier in Tier::ALL {
            for &(module, upstream) in tier.modules {
                let path = stub_dir()
                    .join(tier.stub_subdir)
                    .join(format!("{module}.v"));
                assert!(
                    path.exists(),
                    "{} mirrors {upstream} but {} does not exist",
                    tier.label,
                    path.display()
                );
                let key = tier.pin_key(module);
                assert!(
                    pin.absent.contains_key(&key),
                    "the pin has no `absent-digest {key}` line"
                );
                covered.insert(
                    path.strip_prefix(stub_dir())
                        .expect("built from the stub directory")
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
        let mut on_disk: BTreeSet<String> = BTreeSet::new();
        for tier in Tier::ALL {
            let dir = stub_dir().join(tier.stub_subdir);
            for entry in
                std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
            {
                let entry = entry.expect("stub directory entry");
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "v") {
                    on_disk.insert(format!(
                        "{}/{}",
                        tier.stub_subdir,
                        path.file_name()
                            .expect("a file with an extension has a name")
                            .to_string_lossy()
                    ));
                }
            }
        }
        assert_eq!(
            covered, on_disk,
            "every `.v` in the vendored stub must be mirrored by a tier entry and pinned"
        );
        assert_eq!(
            pin.absent.len(),
            covered.len(),
            "the pin records {} `absent-digest` line(s) for {} stub module(s)",
            pin.absent.len(),
            covered.len()
        );
    }

    // ------------------------------------------- the measuring instrument itself

    /// Holds one hand-written stub module to one hand-written upstream module,
    /// through the same [`compare`] the tier gates use.
    fn findings(stub: &str, real: &str) -> Vec<String> {
        let sites = [Site {
            key: "Probe.probe".to_string(),
            upstream: "the probe's upstream".to_string(),
            stub: stub.to_string(),
            real: real.to_string(),
        }];
        let result = compare(&sites, &BTreeSet::new());
        assert!(
            result.compared > 0,
            "the probe compared no shape, so it proves nothing"
        );
        result.findings
    }

    /// Each normalization rule is load-bearing, and the check still has teeth
    /// with all of them in place.
    ///
    /// The stub and the two upstream libraries state the same declarations in
    /// different dialects, and every rule here exists because some dialect
    /// difference otherwise reads as drift. Each case pairs a spelling the stub
    /// uses with the spelling upstream uses for the same declaration, and asserts
    /// they compare equal — which is only interesting alongside the last case,
    /// where a genuinely different arity still compares unequal.
    #[test]
    fn dialect_differences_normalize_away_and_real_differences_do_not() {
        // Aliases, transitively: upstream abbreviates `expr` and `labelidx`,
        // the stub spells both out. Without expansion every use of one looks
        // like a stub declaration upstream does not have.
        assert_eq!(
            findings(
                "Inductive i : Type := | BI_block : list (list b) -> nat -> i.",
                "Definition expr := list b.\n\
                 Definition labelidx := u32.\n\
                 Definition u32 := nat.\n\
                 Inductive i : Type := | BI_block : list expr -> labelidx -> i.",
            ),
            Vec::<String>::new()
        );

        // Self-mentioning abbreviation: coq-wasm writes `Definition byte :=
        // Integers.byte`. Expanding it naively never terminates, and each
        // rewrite adds a qualifier the stub does not have.
        assert_eq!(
            findings(
                "Parameter byte : Type.\nRecord r : Type := { f : list byte }.",
                "Definition byte := Integers.byte.\n\
                 Record r : Type := { f : list Integers.byte }.",
            ),
            Vec::<String>::new()
        );

        // Nullary constructors: upstream writes `| T_i32` with no colon at all,
        // the stub writes `| T_i32 : number_type`.
        assert_eq!(
            findings(
                "Inductive number_type : Type := | T_i32 : number_type | T_i64 : number_type.",
                "Inductive number_type : Type := | T_i32 | T_i64.",
            ),
            Vec::<String>::new()
        );

        // A qualified path keeps its own tail: an unrelated module-local
        // `Definition T := int` must not reach into `Wasm_int.Int32.T`.
        assert_eq!(
            findings(
                "Parameter i32 : Type.\nRecord r : Type := { f : i32 }.",
                "Definition T := int.\n\
                 Definition i32 := Wasm_int.Int32.T.\n\
                 Record r : Type := { f : i32 }.",
            ),
            Vec::<String>::new()
        );

        // Teeth. An argument dropped from a constructor is the #346 class, and
        // survives every rule above.
        assert_eq!(
            findings(
                "Inductive i : Type := | BI_br_table : list nat -> i.",
                "Inductive i : Type := | BI_br_table : list nat -> nat -> i.",
            ),
            [
                "Probe.probe: `i.BI_br_table` is `(list nat)` in the stub and \
              `(list nat -> nat)` in the probe's upstream"
            ]
        );

        // Teeth. A renamed record type is reported as a missing shape, and again
        // through every field whose type names it. This is the shape of the one
        // real finding this check was first driven against, where the stub had
        // `module_glob` for coq-wasm's `module_global`.
        assert_eq!(
            findings(
                "Record module_glob : Type := { g : nat }.\n\
                 Record m : Type := { mod_globals : list module_glob }.",
                "Record module_global : Type := { g : nat }.\n\
                 Record m : Type := { mod_globals : list module_global }.",
            ),
            [
                "Probe.probe: `m.mod_globals` is `list module_glob` in the stub and \
                 `list module_global` in the probe's upstream",
                "Probe.probe: the stub declares the shape `module_glob`, absent from the \
                 probe's upstream",
                "Probe.probe: the stub declares the shape `module_glob.g`, absent from the \
                 probe's upstream",
            ]
        );

        // Teeth. A name upstream never declares is fiction even where no shape
        // is involved, which is the only thing holding `Parameter`-only modules
        // such as the obligation predicates.
        assert_eq!(
            findings(
                "Parameter ValidModule : module -> Prop.\nInductive t : Type := | C : t.",
                "Definition ValidSpec (m : module) : Prop := True.\n\
                 Inductive t : Type := | C.",
            ),
            [
                "Probe.probe: the stub declares `ValidModule`, which the probe's upstream \
                 does not declare"
            ]
        );
    }

    /// A Rocq sentence ends at a `.` followed by whitespace; the `.` inside
    /// `Byte.byte` does not.
    ///
    /// The scanners find the end of a declaration by that marker, so a stripper
    /// that confused the two would end an inductive at its first qualified name
    /// and read the constructors after it as nothing at all.
    #[test]
    fn a_remote_url_credential_never_reaches_the_failure_text() {
        let quoted = redact_url_credentials(
            "origin\thttps://user:ghp_secret@github.com/org/repo.git (fetch) | \
             origin\tgit@github-alias:org/repo.git (push) | \
             fatal: unable to access 'https://tok@example.com/x/': timeout",
        );
        assert!(!quoted.contains("ghp_secret"), "{quoted}");
        assert!(!quoted.contains("tok@"), "{quoted}");
        assert!(quoted.contains("https://***@github.com/org/repo.git"), "{quoted}");
        assert!(quoted.contains("https://***@example.com/x/"), "{quoted}");
        assert!(quoted.contains("git@github-alias:org/repo.git"), "{quoted}");
        assert_eq!(redact_url_credentials("no urls here"), "no urls here");
    }

    #[test]
    fn only_a_sentence_ending_dot_is_marked() {
        assert_eq!(
            mark_command_ends("Definition byte := Integers.byte.\nParameter p : nat."),
            format!(
                "Definition byte := Integers.byte{COMMAND_END}\nParameter p : nat{COMMAND_END}"
            )
        );
        // A `. ` inside a string literal is not a sentence end either.
        assert_eq!(
            mark_command_ends("Notation \"a . b\" := c."),
            format!("Notation \"a . b\" := c{COMMAND_END}")
        );
    }
}
