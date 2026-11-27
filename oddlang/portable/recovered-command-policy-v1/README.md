# Command-policy recovery

The command-policy workspace extends the retained-poll workspace with the required policy field. The older workspace keeps its published lock. Run the following from the companion root with a new destination.

```sh
python3 oddlang/portable/recovered-command-policy-v1/replay.py \
    /path/to/oddlang /path/to/pinned-marmish /tmp/marmish-command-policy
```

The selection names the Rust records and fields to inspect. Oddlang checks the source digest, containing field types, Serde attributes, defaults and explicit-null adapter before emitting the portable schema. The replay embeds that schema and the reviewed decision binding in the foreground graph and rebuilds the workspace. The decision binding was transcribed from the native normalizer and engine at commit `af54339d250290490d62d4475a77539b7e739071`. The script verifies both source files. [Native configuration](https://github.com/Epiphytic/marmish/blob/af54339d250290490d62d4475a77539b7e739071/crates/marmish-native/src/command_config.rs) · [Engine policy](https://github.com/Epiphytic/marmish/blob/af54339d250290490d62d4475a77539b7e739071/crates/marmish-engine/src/policy.rs)

The policy, principal entry and restrictions each accept either a JSON object or an array in declaration order. The recovered schema records the positional order for each. Missing trailing values use the same declared defaults.

## Named executable decision

- Load a version-1 policy. Count an omitted default allowlist as one implicit `/**` pattern. Explicit empty allowlists deny every path. Omitted per-principal allowlists impose no additional allow requirement.
- Verify each listed principal is admitted. Apply the default layer and every matching principal layer. Within each layer, any allow pattern can match the executable. A deny match on either the executable or its invoked alias rejects it. Every applicable layer must accept it.
- Match the whole byte path. A single star spans zero or more non-slash bytes. A run of two or more stars can span slashes. A backslash can escape a literal star or backslash. Patterns must start with a slash and contain no control characters. Executable paths must be absolute and contain no empty, dot or parent components.
- Charge all applicable patterns against the declared match budget. Each charge is the token count plus one multiplied by the executable byte length plus one. For deny patterns with an alias, add its length plus one to the length factor.
- A successful decision requires a separate canonical-path check before launch. Parsing, principal validation, path selection and launch are separate operations.

The policy observer exercises the generated Rust and the pinned native implementation offline. Remote receipt, command status and byte-stream fidelity are separate interoperability results. [Policy comparison](../../../verification/whole/command-policy)
