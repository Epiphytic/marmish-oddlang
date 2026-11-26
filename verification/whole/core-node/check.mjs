import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {resolve} from 'node:path';
import {pathToFileURL} from 'node:url';

const folder = resolve(process.argv[2]);
const {validate, defaultValue, maximum, errorMessages} = await import(pathToFileURL(resolve(folder, 'value.mts')));
const obligations = JSON.parse(await readFile(resolve(folder, 'obligations.json')));
const id = 'urn:marmish:core:configuration:RetentionOutOfRange';
assert.equal(maximum, 31_536_000n);
assert.equal(defaultValue, 86_400n);
assert.equal(errorMessages[id], 'retention must be between one second and 365 days');
assert.deepEqual(validate(null), {ok:true, value:null});

for (const row of obligations.cases) {
    if (row.rule === 'default') {
        assert.equal(defaultValue, BigInt(row.value));
    } else {
        const input = row.input === null ? null : BigInt(row.input);
        const expected = row.error ? {ok:false,error:row.error} : {ok:true,value:input};
        assert.deepEqual(validate(input), expected);
    }
}
const samples = [0n, 1n, 2n, 86_400n, maximum - 1n, maximum, maximum + 1n, 2n**64n - 1n];
let seed = 7n;
for (let i=0; i<128; i++) {
    seed = (6364136223846793005n * seed + 1442695040888963407n) % (2n**64n);
    samples.push(seed, seed % (maximum + 2n));
}
for (const input of samples) {
    const expected = input >= 1n && input <= 31_536_000n ? {ok:true,value:input} : {ok:false,error:id};
    assert.deepEqual(validate(input), expected);
}
console.log(JSON.stringify({samples:samples.length, generatedCases:obligations.cases.length, explicitAbsence:true}));
