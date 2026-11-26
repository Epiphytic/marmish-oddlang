# TypeScript endpoint wire check

This consumer runs the shared object-form endpoint cases under Node.js 22.22.2.
It reads the original JSON token in the parse reviver and uses `bigint | null`
internally. It emits the endpoint as an unquoted JSON integer or null. The
runner compares exact output text and checks that parsing through JavaScript
`number` loses precision on the shared `9007199254740993` case. It exercises
the endpoint wire role. The Rust drop-in field requirement has its own native
caller.

From this directory with TypeScript 5.9.3 available offline:

```sh
out=$(mktemp -d)
npx --offline --package=typescript@5.9.3 tsc --strict --target ES2022 --module commonjs \
  --outDir "$out" endpoint.ts
node check.cjs "$out/endpoint.js" ../cases.json
```
