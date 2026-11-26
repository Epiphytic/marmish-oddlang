const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");

const [compiled, corpus] = process.argv.slice(2);
const { roundTrip } = require(path.resolve(compiled));
const cases = JSON.parse(fs.readFileSync(corpus, "utf8"));
for (const sample of cases) {
    assert.equal(roundTrip(sample.wire), sample.expected, sample.id);
}

// Ensure the shared corpus can distinguish a consumer that rounds through Number.
const precisionCase = cases.find(sample => sample.id === "two-to-53-plus-one");
assert.ok(precisionCase, "expected the shared precision witness");
assert.match(precisionCase.expected, /"end":9007199254740993[,}]/);
assert.notEqual(JSON.stringify(JSON.parse(precisionCase.wire)), precisionCase.expected);
console.log(`Matched ${cases.length} endpoint cases, including the precision-loss control.`);
