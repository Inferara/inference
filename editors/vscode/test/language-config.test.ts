import { describe, it } from "node:test";
import * as assert from "node:assert/strict";
import * as fs from "node:fs";
import * as path from "node:path";

const CONFIG_PATH = path.resolve(
  __dirname,
  "..",
  "language-configuration.json"
);

const config = JSON.parse(fs.readFileSync(CONFIG_PATH, "utf-8"));

describe("Language config: Comments", () => {
  it("lineComment is //", () => {
    assert.equal(config.comments.lineComment, "//");
  });

  it("blockComment is /* */", () => {
    assert.deepEqual(config.comments.blockComment, ["/*", "*/"]);
  });
});

describe("Language config: Brackets", () => {
  it("contains all 3 bracket pairs", () => {
    const brackets = config.brackets as [string, string][];
    assert.equal(brackets.length, 3);

    const pairs = brackets.map((b) => b.join(""));
    assert.ok(pairs.includes("{}"), "Missing {} bracket pair");
    assert.ok(pairs.includes("[]"), "Missing [] bracket pair");
    assert.ok(pairs.includes("()"), "Missing () bracket pair");
  });
});

describe("Language config: Auto-closing pairs", () => {
  it("has 5 auto-closing pairs", () => {
    assert.equal(config.autoClosingPairs.length, 5);
  });

  it("has {} pair", () => {
    const pair = config.autoClosingPairs.find(
      (p: any) => p.open === "{" && p.close === "}"
    );
    assert.ok(pair, "Missing {} auto-closing pair");
  });

  it("has [] pair", () => {
    const pair = config.autoClosingPairs.find(
      (p: any) => p.open === "[" && p.close === "]"
    );
    assert.ok(pair, "Missing [] auto-closing pair");
  });

  it("has () pair", () => {
    const pair = config.autoClosingPairs.find(
      (p: any) => p.open === "(" && p.close === ")"
    );
    assert.ok(pair, "Missing () auto-closing pair");
  });

  it('quotes have notIn: ["string"]', () => {
    const pair = config.autoClosingPairs.find(
      (p: any) => p.open === '"' && p.close === '"'
    );
    assert.ok(pair, 'Missing "" auto-closing pair');
    assert.ok(pair.notIn.includes("string"), 'Quote pair missing notIn "string"');
  });

  it('single quotes have notIn: ["string", "comment"]', () => {
    const pair = config.autoClosingPairs.find(
      (p: any) => p.open === "'" && p.close === "'"
    );
    assert.ok(pair, "Missing '' auto-closing pair");
    assert.ok(pair.notIn.includes("string"), "Single quote pair missing notIn string");
    assert.ok(pair.notIn.includes("comment"), "Single quote pair missing notIn comment");
  });
});

describe("Language config: Surrounding pairs", () => {
  it("has 5 surrounding pairs", () => {
    const pairs = config.surroundingPairs as [string, string][];
    assert.equal(pairs.length, 5);
  });

  it("includes all expected pairs", () => {
    const pairs = config.surroundingPairs as [string, string][];
    const joined = pairs.map((p) => p.join(""));
    assert.ok(joined.includes("{}"), "Missing {} surrounding pair");
    assert.ok(joined.includes("[]"), "Missing [] surrounding pair");
    assert.ok(joined.includes("()"), "Missing () surrounding pair");
    assert.ok(joined.includes('""'), 'Missing "" surrounding pair');
    assert.ok(joined.includes("''"), "Missing '' surrounding pair");
  });
});

describe("Language config: Folding markers", () => {
  it("start marker matches // #region", () => {
    const regex = new RegExp(config.folding.markers.start);
    assert.ok(regex.test("  // #region myRegion"));
    assert.ok(regex.test("  // region myRegion"));
  });

  it("end marker matches // #endregion", () => {
    const regex = new RegExp(config.folding.markers.end);
    assert.ok(regex.test("  // #endregion"));
    assert.ok(regex.test("  // endregion"));
  });
});

describe("Language config: Word pattern", () => {
  it("matches simple identifiers", () => {
    const regex = new RegExp(config.wordPattern);
    assert.ok(regex.test("foo"));
    assert.ok(regex.test("_bar"));
  });

  it("matches primed identifiers", () => {
    const regex = new RegExp(config.wordPattern);
    const match = "compute'".match(regex);
    assert.ok(match, "compute' should match word pattern");
    assert.equal(match[0], "compute'");
  });

  it("does not match digit-leading strings", () => {
    const regex = new RegExp(config.wordPattern);
    const match = "123abc".match(regex);
    // Should not match starting from the beginning
    if (match) {
      assert.notEqual(match.index, 0, "Should not match at position 0");
    }
  });
});

describe("Language config: Indentation rules", () => {
  it("increase pattern matches opening brace lines", () => {
    const regex = new RegExp(config.indentationRules.increaseIndentPattern);
    assert.ok(regex.test("fn main() {"));
    assert.ok(regex.test("if x > 0 {"));
    assert.ok(regex.test("loop {"));
    assert.ok(regex.test("forall {"));
  });

  it("increase pattern does NOT match lines with both braces", () => {
    const regex = new RegExp(config.indentationRules.increaseIndentPattern);
    assert.ok(!regex.test("let x = { }"));
  });

  it("decrease pattern matches closing brace lines", () => {
    const regex = new RegExp(config.indentationRules.decreaseIndentPattern);
    assert.ok(regex.test("}"));
    assert.ok(regex.test("    }"));
    assert.ok(regex.test("  } else {"));
  });
});
