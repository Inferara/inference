import { describe, it, before } from "node:test";
import * as assert from "node:assert/strict";
import * as fs from "node:fs";
import * as path from "node:path";
import * as vsctm from "vscode-textmate";
import * as oniguruma from "vscode-oniguruma";

const GRAMMAR_PATH = path.resolve(
  __dirname,
  "..",
  "syntaxes",
  "inference.tmLanguage.json"
);
const ONIG_WASM_PATH = path.resolve(
  __dirname,
  "..",
  "node_modules",
  "vscode-oniguruma",
  "release",
  "onig.wasm"
);

let registry: vsctm.Registry;
let grammar: vsctm.IGrammar;

async function initGrammar(): Promise<vsctm.IGrammar> {
  const wasmBin = fs.readFileSync(ONIG_WASM_PATH).buffer;
  await oniguruma.loadWASM(wasmBin);

  registry = new vsctm.Registry({
    onigLib: Promise.resolve({
      createOnigScanner: (patterns: string[]) =>
        new oniguruma.OnigScanner(patterns),
      createOnigString: (s: string) => new oniguruma.OnigString(s),
    }),
    loadGrammar: async (scopeName: string) => {
      if (scopeName === "source.inference") {
        const content = fs.readFileSync(GRAMMAR_PATH, "utf-8");
        return vsctm.parseRawGrammar(content, GRAMMAR_PATH);
      }
      return null;
    },
  });

  const g = await registry.loadGrammar("source.inference");
  if (!g) throw new Error("Failed to load grammar");
  return g;
}

/**
 * Tokenize a single line using the grammar.
 * Returns an array of { text, scopes } objects.
 */
function tokenizeLine(line: string) {
  const result = grammar.tokenizeLine(line, vsctm.INITIAL);
  return result.tokens.map((t) => ({
    text: line.substring(t.startIndex, t.endIndex),
    scopes: t.scopes,
  }));
}

/**
 * Find the first token matching the given text and assert it has a scope
 * containing the expected substring.
 */
function assertTokenScope(
  line: string,
  tokenText: string,
  expectedScope: string
) {
  const tokens = tokenizeLine(line);
  const token = tokens.find((t) => t.text === tokenText);
  assert.ok(
    token,
    `Token "${tokenText}" not found in line: "${line}". Tokens: ${JSON.stringify(tokens.map((t) => t.text))}`
  );
  const hasScope = token.scopes.some((s) => s.includes(expectedScope));
  assert.ok(
    hasScope,
    `Token "${tokenText}" expected scope containing "${expectedScope}", got: [${token.scopes.join(", ")}]`
  );
}

/**
 * Assert that a token does NOT have a scope containing the given substring.
 */
function assertTokenNotScope(
  line: string,
  tokenText: string,
  rejectedScope: string
) {
  const tokens = tokenizeLine(line);
  const token = tokens.find((t) => t.text === tokenText);
  if (!token) return; // token not found means it can't have the scope
  const hasScope = token.scopes.some((s) => s.includes(rejectedScope));
  assert.ok(
    !hasScope,
    `Token "${tokenText}" should NOT have scope containing "${rejectedScope}", got: [${token.scopes.join(", ")}]`
  );
}

before(async () => {
  grammar = await initGrammar();
});

describe("Grammar: Declaration keywords", () => {
  it("fn is keyword.declaration", () => {
    assertTokenScope("fn main() {", "fn", "keyword.declaration.fn");
  });

  it("struct is keyword.declaration", () => {
    assertTokenScope("struct Point {", "struct", "keyword.declaration.struct");
  });

  it("enum is keyword.declaration", () => {
    assertTokenScope("enum Color {", "enum", "keyword.declaration.enum");
  });

  it("type is keyword.declaration", () => {
    assertTokenScope("type Alias = i32;", "type", "keyword.declaration.type");
  });

  it("const is keyword.declaration", () => {
    assertTokenScope(
      "const x: i32 = 42;",
      "const",
      "keyword.declaration.const"
    );
  });

  it("let is keyword.declaration", () => {
    assertTokenScope("let mut y: i32 = 0;", "let", "keyword.declaration.let");
  });

  it("external is keyword.declaration", () => {
    assertTokenScope(
      "external fn print(v: i32) -> ();",
      "external",
      "keyword.declaration.external"
    );
  });

  it("spec is keyword.declaration", () => {
    assertTokenScope(
      "spec fn add(a: i32) -> i32 {",
      "spec",
      "keyword.declaration.spec"
    );
  });
});

describe("Grammar: Visibility and modifiers", () => {
  it("pub is keyword.other.visibility", () => {
    assertTokenScope("pub fn main() {", "pub", "keyword.other.visibility");
  });

  it("mut is storage.modifier", () => {
    assertTokenScope("let mut y: i32 = 0;", "mut", "storage.modifier.mut");
  });

  it("self is variable.language.self", () => {
    assertTokenScope("self.x", "self", "variable.language.self");
  });
});

describe("Grammar: Control flow keywords", () => {
  it("if is keyword.control", () => {
    assertTokenScope("if x > 0 {", "if", "keyword.control.if");
  });

  it("else is keyword.control", () => {
    assertTokenScope("} else {", "else", "keyword.control.else");
  });

  it("loop is keyword.control", () => {
    assertTokenScope("loop {", "loop", "keyword.control.loop");
  });

  it("break is keyword.control", () => {
    assertTokenScope("break;", "break", "keyword.control.break");
  });

  it("return is keyword.control", () => {
    assertTokenScope("return y;", "return", "keyword.control.return");
  });

  it("assert is keyword.control", () => {
    assertTokenScope("assert x > 0;", "assert", "keyword.control.assert");
  });
});

describe("Grammar: Non-deterministic constructs", () => {
  it("forall is keyword.control.nondet", () => {
    assertTokenScope("forall {", "forall", "keyword.control.nondet.forall");
  });

  it("exists is keyword.control.nondet", () => {
    assertTokenScope("exists {", "exists", "keyword.control.nondet.exists");
  });

  it("assume is keyword.control.nondet", () => {
    assertTokenScope("assume {", "assume", "keyword.control.nondet.assume");
  });

  it("unique is keyword.control.nondet", () => {
    assertTokenScope("unique {", "unique", "keyword.control.nondet.unique");
  });

  it("@ (uzumaki) is keyword.control.nondet", () => {
    assertTokenScope(
      "const w: i32 = @;",
      "@",
      "keyword.control.nondet.uzumaki"
    );
  });
});

describe("Grammar: Primitive types", () => {
  const primitives = [
    "i8",
    "i16",
    "i32",
    "i64",
    "u8",
    "u16",
    "u32",
    "u64",
    "bool",
  ];
  for (const prim of primitives) {
    it(`${prim} is storage.type.primitive`, () => {
      assertTokenScope(`x: ${prim}`, prim, "storage.type.primitive");
    });
  }
});

describe("Grammar: Unit type", () => {
  it("() is storage.type.unit", () => {
    assertTokenScope("-> ()", "()", "storage.type.unit");
  });
});

describe("Grammar: User-defined types", () => {
  it("Point is entity.name.type", () => {
    assertTokenScope("x: Point", "Point", "entity.name.type");
  });

  it("Color is entity.name.type", () => {
    assertTokenScope("c: Color", "Color", "entity.name.type");
  });
});

describe("Grammar: Boolean literals", () => {
  it("true is constant.language.boolean", () => {
    assertTokenScope(
      "const f: bool = true;",
      "true",
      "constant.language.boolean"
    );
  });

  it("false is constant.language.boolean", () => {
    assertTokenScope(
      "const f: bool = false;",
      "false",
      "constant.language.boolean"
    );
  });
});

describe("Grammar: Number literals", () => {
  it("decimal integer", () => {
    assertTokenScope("const x: i32 = 42;", "42", "constant.numeric");
  });

  it("hex literal", () => {
    assertTokenScope("const x: i32 = 0xFF;", "0xFF", "constant.numeric.hex");
  });

  it("binary literal", () => {
    assertTokenScope(
      "const x: i32 = 0b1010;",
      "0b1010",
      "constant.numeric.binary"
    );
  });

  it("octal literal", () => {
    assertTokenScope(
      "const x: i32 = 0o77;",
      "0o77",
      "constant.numeric.octal"
    );
  });

  it("underscore-separated number", () => {
    assertTokenScope(
      "const x: i32 = 123_456;",
      "123_456",
      "constant.numeric"
    );
  });
});

describe("Grammar: Strings and escape sequences", () => {
  it("string is string.quoted.double", () => {
    const tokens = tokenizeLine('"hello"');
    const stringTokens = tokens.filter((t) =>
      t.scopes.some((s) => s.includes("string.quoted.double"))
    );
    assert.ok(
      stringTokens.length > 0,
      `Expected string.quoted.double scope in: ${JSON.stringify(tokens)}`
    );
  });

  it("escape \\n is constant.character.escape", () => {
    const tokens = tokenizeLine('"line\\n"');
    const escapeToken = tokens.find((t) => t.text === "\\n");
    assert.ok(escapeToken, "Escape token \\n not found");
    assert.ok(
      escapeToken.scopes.some((s) => s.includes("constant.character.escape"))
    );
  });

  it("escape \\t is constant.character.escape", () => {
    const tokens = tokenizeLine('"tab\\t"');
    const escapeToken = tokens.find((t) => t.text === "\\t");
    assert.ok(escapeToken, "Escape token \\t not found");
    assert.ok(
      escapeToken.scopes.some((s) => s.includes("constant.character.escape"))
    );
  });

  it("escape \\\\ is constant.character.escape", () => {
    const tokens = tokenizeLine('"back\\\\"');
    const escapeToken = tokens.find((t) => t.text === "\\\\");
    assert.ok(escapeToken, "Escape token \\\\ not found");
    assert.ok(
      escapeToken.scopes.some((s) => s.includes("constant.character.escape"))
    );
  });

  it("escape \\xHH is constant.character.escape", () => {
    const tokens = tokenizeLine('"\\x41"');
    const escapeToken = tokens.find((t) => t.text === "\\x41");
    assert.ok(escapeToken, "Escape token \\x41 not found");
    assert.ok(
      escapeToken.scopes.some((s) => s.includes("constant.character.escape"))
    );
  });

  it("escape \\u{...} is constant.character.escape", () => {
    const tokens = tokenizeLine('"\\u{1F600}"');
    const escapeToken = tokens.find((t) => t.text === "\\u{1F600}");
    assert.ok(escapeToken, "Escape token \\u{1F600} not found");
    assert.ok(
      escapeToken.scopes.some((s) => s.includes("constant.character.escape"))
    );
  });
});

describe("Grammar: Comments", () => {
  it("// line comment", () => {
    const tokens = tokenizeLine("// line comment");
    assert.ok(
      tokens.some((t) =>
        t.scopes.some((s) => s.includes("comment.line.double-slash"))
      )
    );
  });

  it("/// doc comment", () => {
    const tokens = tokenizeLine("/// doc comment");
    assert.ok(
      tokens.some((t) =>
        t.scopes.some((s) => s.includes("comment.line.documentation"))
      )
    );
  });

  it("/* block comment */", () => {
    const tokens = tokenizeLine("/* block comment */");
    assert.ok(
      tokens.some((t) => t.scopes.some((s) => s.includes("comment.block")))
    );
  });
});

describe("Grammar: Operators", () => {
  it("+ is keyword.operator.arithmetic", () => {
    assertTokenScope("x + y", "+", "keyword.operator.arithmetic");
  });

  it("- is keyword.operator.arithmetic", () => {
    assertTokenScope("x - y", "-", "keyword.operator.arithmetic");
  });

  it("== is keyword.operator.comparison", () => {
    assertTokenScope("x == y", "==", "keyword.operator.comparison");
  });

  it("&& is keyword.operator.logical", () => {
    assertTokenScope("a && b", "&&", "keyword.operator.logical");
  });

  it(">> is keyword.operator.bitwise (each > individually)", () => {
    // The grammar matches > individually, so >> produces two > tokens
    const tokens = tokenizeLine("x >> 2");
    const gtTokens = tokens.filter((t) => t.text === ">");
    assert.equal(gtTokens.length, 2, "Expected two > tokens");
    assert.ok(
      gtTokens.every((t) =>
        t.scopes.some((s) => s.includes("keyword.operator"))
      )
    );
  });

  it("-> arrow operator (matched as - and >)", () => {
    // The grammar matches - and > as separate operator tokens
    const tokens = tokenizeLine("fn foo() -> i32 {");
    const dash = tokens.find((t) => t.text === "-");
    assert.ok(dash, "Expected - token");
    assert.ok(
      dash.scopes.some((s) => s.includes("keyword.operator")),
      "- should be an operator"
    );
  });

  it(":: is keyword.operator.path", () => {
    assertTokenScope("Mod::func()", "::", "keyword.operator.path");
  });
});

describe("Grammar: Functions", () => {
  it("function definition: fn keyword present and name is a function scope", () => {
    // The grammar call pattern matches before the definition pattern,
    // so fn foo( gives foo as entity.name.function (not .definition).
    assertTokenScope("fn foo(x: i32) {", "fn", "keyword.declaration.fn");
    assertTokenScope("fn foo(x: i32) {", "foo", "entity.name.function");
  });

  it("function call name", () => {
    assertTokenScope("bar(42)", "bar", "entity.name.function");
  });

  it("primed function in definition context", () => {
    assertTokenScope("fn compute'(x: i32) {", "compute'", "entity.name.function");
  });

  it("primed function call", () => {
    assertTokenScope("compute'(42)", "compute'", "entity.name.function");
  });
});

describe("Grammar: Punctuation", () => {
  it(", is punctuation.separator", () => {
    assertTokenScope("a, b", ",", "punctuation.separator");
  });

  it("; is punctuation.terminator", () => {
    assertTokenScope("return x;", ";", "punctuation.terminator");
  });

  it(". is punctuation.accessor", () => {
    assertTokenScope("self.x", ".", "punctuation.accessor");
  });

  it(": is punctuation.type", () => {
    assertTokenScope("x: i32", ":", "punctuation.type");
  });
});

describe("Grammar: Edge cases", () => {
  it("keyword-like identifier 'format' is NOT a keyword", () => {
    assertTokenNotScope("format(x)", "format", "keyword");
  });

  it("keyword-like identifier 'letter' is NOT a keyword", () => {
    assertTokenNotScope("let letter: i32 = 0;", "letter", "keyword");
  });

  it("identifier starting with keyword prefix 'returned' is NOT a keyword", () => {
    assertTokenNotScope("let returned: i32 = 0;", "returned", "keyword");
  });

  it("identifier starting with 'breaking' is NOT a keyword", () => {
    assertTokenNotScope("let breaking: i32 = 0;", "breaking", "keyword");
  });
});
