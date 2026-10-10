import { describe, it, before } from "node:test";
import * as assert from "node:assert/strict";
import * as fs from "node:fs";
import * as path from "node:path";
import * as vsctm from "vscode-textmate";
import * as oniguruma from "vscode-oniguruma";

const GRAMMAR_PATH = path.resolve(__dirname, "..", "syntaxes", "rocq.tmLanguage.json");
const CONFIG_PATH = path.resolve(__dirname, "..", "rocq-language-configuration.json");
const ONIG_WASM_PATH = path.resolve(
  __dirname,
  "..",
  "node_modules",
  "vscode-oniguruma",
  "release",
  "onig.wasm"
);
const SCOPE = "source.inference-rocq";

let grammar: vsctm.IGrammar;

async function initGrammar(): Promise<vsctm.IGrammar> {
  const wasmBin = fs.readFileSync(ONIG_WASM_PATH).buffer;
  await oniguruma.loadWASM(wasmBin);
  const registry = new vsctm.Registry({
    onigLib: Promise.resolve({
      createOnigScanner: (patterns: string[]) => new oniguruma.OnigScanner(patterns),
      createOnigString: (s: string) => new oniguruma.OnigString(s),
    }),
    loadGrammar: async (scopeName: string) =>
      scopeName === SCOPE
        ? vsctm.parseRawGrammar(fs.readFileSync(GRAMMAR_PATH, "utf-8"), GRAMMAR_PATH)
        : null,
  });
  const g = await registry.loadGrammar(SCOPE);
  if (!g) throw new Error("Failed to load grammar");
  return g;
}

/** Tokens of each line, tokenizing the lines in order (comments may span lines). */
function tokenize(text: string) {
  let stack = vsctm.INITIAL;
  return text.split("\n").map((line) => {
    const result = grammar.tokenizeLine(line, stack);
    stack = result.ruleStack;
    return result.tokens.map((t) => ({
      start: t.startIndex,
      end: t.endIndex,
      text: line.substring(t.startIndex, t.endIndex),
      scopes: t.scopes,
    }));
  });
}

/**
 * Scopes of the first whole-word occurrence of `word` in a one-line text.
 * Plain text is merged into one token with its neighbours, so the token is
 * found by position, and must cover the whole occurrence.
 */
function scopesOf(line: string, word: string): string[] {
  const escaped = word.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const match = new RegExp(/^[\w']/.test(word) ? `(?<![\\w'])${escaped}(?![\\w'])` : escaped).exec(line);
  assert.ok(match, `"${word}" not found in "${line}"`);
  const start = match.index;
  const end = start + word.length;
  const token = tokenize(line)[0].find((t) => t.start <= start && end <= t.end);
  assert.ok(token, `"${word}" in "${line}" is split across tokens`);
  return token.scopes;
}

function assertScope(line: string, tokenText: string, expected: string) {
  const scopes = scopesOf(line, tokenText);
  assert.ok(
    scopes.some((s) => s.startsWith(expected)),
    `"${tokenText}" in "${line}": expected ${expected}, got [${scopes.join(", ")}]`
  );
}

/** The token carries only the root scope (plain identifier text). */
function assertPlain(line: string, tokenText: string) {
  const scopes = scopesOf(line, tokenText);
  assert.deepEqual(scopes, [SCOPE], `"${tokenText}" in "${line}" should be plain, got [${scopes.join(", ")}]`);
}

before(async () => {
  grammar = await initGrammar();
});

describe("Rocq grammar: what the Inference compiler emits", () => {
  it("highlights the import header", () => {
    const line = "From WasmVerifier Require Import Assertions Verifier.";
    assertScope(line, "From", "keyword.other.vernacular");
    assertScope(line, "Require", "keyword.other.vernacular");
    assertScope(line, "Import", "keyword.other.vernacular");
    assertPlain(line, "WasmVerifier");
  });

  it("names definitions and theorems", () => {
    assertScope("Definition clamp : module_func := {|", "Definition", "keyword.other.declaration");
    assertScope("Definition clamp : module_func := {|", "clamp", "entity.name.function");
    assertScope("Definition clamp : module_func := {|", ":=", "keyword.operator");
    assertScope("Definition clamp : module_func := {|", "{|", "punctuation.definition.record");
    const theorem = "Theorem valid_bounds : ValidModule bounds.";
    assertScope(theorem, "Theorem", "keyword.other.declaration");
    assertScope(theorem, "valid_bounds", "entity.name.function");
    assertScope(theorem, ".", "punctuation.terminator");
  });

  it("scopes numbers, notation scopes and inline comments in instruction lists", () => {
    const line = "    BI_local_get 0%N (*value*) ::";
    assertScope(line, "0", "constant.numeric");
    assertScope(line, "%N", "keyword.operator.scope");
    assertScope(line, "value", "comment.block");
    assertScope(line, "::", "keyword.operator");
    assertPlain("BI_relop T_i32 (Relop_i (ROI_lt SX_S)) ::", "T_i32");
  });

  it("marks proof holes and proof terminators", () => {
    const lines = tokenize("Proof.\n  (* TODO: fill the proof *)\nQed.");
    assert.ok(lines[0][0].scopes.some((s) => s.startsWith("keyword.control.proof")));
    assert.ok(lines[1].every((t) => t.text.trim() === "" || t.scopes.some((s) => s.startsWith("comment.block"))));
    assert.ok(lines[2][0].scopes.some((s) => s.startsWith("keyword.control.proof")));
    assertScope("Admitted.", "Admitted", "keyword.control.admit");
    assertScope("  admit.", "admit", "keyword.control.admit");
  });

  it("names sections and modules", () => {
    assertScope("Section Host.", "Section", "keyword.other.module");
    assertScope("Section Host.", "Host", "entity.name.namespace");
    assertScope("End Host.", "Host", "entity.name.namespace");
    assertScope("Module Import M.", "Module", "keyword.other.module");
    assertScope("Context `{ho: host}.", "Context", "keyword.other.assumption");
  });
});

describe("Rocq grammar: proofs", () => {
  it("highlights tactics and tacticals", () => {
    const line = "  intros [HA HnA]. unfold reflects. try lia.";
    assertScope(line, "intros", "support.function.tactic");
    assertScope(line, "unfold", "support.function.tactic");
    assertScope(line, "try", "keyword.control.tactical");
    assertScope(line, "lia", "support.function.tactic");
  });

  it("does not take identifiers that contain a tactic name for tactics", () => {
    assertPlain("apply_signed H'.", "apply_signed");
    assertPlain("exact apply'.", "apply'");
  });

  it("names Ltac definitions and match goal", () => {
    assertScope("Ltac signed_constants :=", "signed_constants", "entity.name.function.tactic");
    assertScope("  repeat match goal with", "match", "keyword.control");
    assertScope("  repeat match goal with", "goal", "keyword.other.ltac");
    assertScope("  | |- context [Wasm_int.Int32.signed x] =>", "context", "keyword.other.ltac");
    assertScope("  - apply Z.eqb_eq. subst. reflexivity.", "-", "punctuation.definition.bullet");
  });
});

describe("Rocq grammar: terms and commands", () => {
  it("does not take constructor names for commands", () => {
    const line = "Inductive atom := Literal : i32 -> atom | Local : N -> i32 -> atom.";
    assertScope(line, "Inductive", "storage.type");
    assertScope(line, "atom", "entity.name.type");
    assertPlain(line, "Local");
    assertScope(line, "->", "keyword.operator");
    assertScope("Local Open Scope Z_scope.", "Local", "keyword.other.vernacular");
  });

  it("tells the Set command from the Set sort", () => {
    assertScope("Set Implicit Arguments.", "Set", "keyword.other.vernacular");
    assertScope("Definition P : nat -> Set := fun _ => nat.", "Set", "support.type.sort");
    assertScope("Definition P : nat -> Set := fun _ => nat.", "fun", "keyword.control");
  });

  it("does not take the term after Goal or an anonymous Instance for a name", () => {
    assertScope("Goal forall x, x = x.", "Goal", "keyword.other.vernacular");
    assertScope("Goal forall x, x = x.", "forall", "keyword.control");
    assertScope("Instance : C.", "Instance", "keyword.other.declaration");
  });

  it("highlights attributes, strings and nested comments", () => {
    assertScope("#[local] Instance t : C.", "local", "entity.other.attribute-name");
    assertScope("#[local] Instance t : C.", "t", "entity.name.function");
    assertScope('Definition s := "a""b".', '""', "constant.character.escape");
    const nested = tokenize("(* a (* b *) c *) d");
    const c = nested[0].find((t) => t.text.includes("c"));
    assert.ok(c && c.scopes.some((s) => s.startsWith("comment.block")), "text after a nested comment stays in the comment");
    const d = nested[0].find((t) => t.text.includes("d"));
    assert.ok(d && !d.scopes.some((s) => s.startsWith("comment")), "text after the comment is code");
  });

  it("keeps a comment open across lines", () => {
    const lines = tokenize("(* first\n   second *)\nQed.");
    assert.ok(lines[1][0].scopes.some((s) => s.startsWith("comment.block")));
    assert.ok(lines[2][0].scopes.some((s) => s.startsWith("keyword.control.proof")));
  });
});

describe("Rocq language configuration", () => {
  const config = JSON.parse(fs.readFileSync(CONFIG_PATH, "utf-8"));

  it("toggles (* *) comments and has no line comment", () => {
    assert.deepEqual(config.comments, { blockComment: ["(*", "*)"] });
  });

  it("does not auto-close quotes that are part of identifiers", () => {
    assert.ok(!config.autoClosingPairs.some((p: { open: string }) => p.open === "'"));
  });

  it("selects identifiers with primes as one word", () => {
    const word = new RegExp(config.wordPattern);
    assert.equal("H' x".match(word)?.[0], "H'");
    assert.equal("valid_m__Spec : T".match(word)?.[0], "valid_m__Spec");
  });
});
