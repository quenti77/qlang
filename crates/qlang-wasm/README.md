# qlang-wasm

qlang for the browser: `qlang-core` compiled to WebAssembly, speaking the same JSON protocol as
`qlang run --json` (see the main README). There is no JavaScript glue to generate and no extra
tool to install: `qlang.wasm` plus the 70-line wrapper `js/qlang.mjs`.

```sh
rustup target add wasm32-unknown-unknown
cargo build --profile wasm --target wasm32-unknown-unknown -p qlang-wasm
# -> target/wasm32-unknown-unknown/wasm/qlang_wasm.wasm  (about 700 kB)
```

Each release publishes `qlang.wasm` and `qlang.mjs`.

```js
import { load } from './qlang.mjs';

const qlang = await load('qlang.wasm');          // a URL, a Response or the bytes
const answer = qlang.handle({
  files: { 'main.q': 'print("Hello {6 * 7}")' },
  input: [],                                      // the lines that read() returns
  now: Date.now(),                                // for time.now(), uuid.v7(), ulid.new()
  seed: 123456789,                                // for random, uuid, ulid (a number below 2**53)
});
answer.ok, answer.output, answer.diagnostics
```

- `mode: "check"` only compiles; `info: true` also returns what editors need (types, members…).
- Run it in a **Web Worker**, and terminate the worker after a few seconds: the limits of qlang
  (steps, call depth, output, memory) stop runaway programs, but only a worker protects the page
  if something goes wrong in the module itself. After a `QlangCrash`, `load` a new instance.
- The module has no I/O, no clock and no randomness: imported files, `read()` lines, the time (`now`) and the seed of the random generator (`seed`) come from the request. Without them, `time.now()` and `random` are refused at compile time.
- The stack is set to 16 MiB in `.cargo/config.toml` (the default 1 MiB is too small for the
  1000 nested calls that qlang allows).

Tests: `node --test crates/qlang-wasm/test/*.test.mjs` (after building).
