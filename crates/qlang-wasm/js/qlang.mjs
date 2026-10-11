// qlang in the browser (or Node): loads qlang.wasm and speaks the JSON protocol.
//
//   import { load } from './qlang.mjs';
//   const qlang = await load('qlang.wasm');            // a URL, a Response, or the bytes
//   const answer = qlang.handle({ files: { 'main.q': 'print(6 * 7)' } });
//   answer.output                                      // "42\n"
//
// The request and the answer are those of `qlang run --json` (see the README): `files`,
// `entry`, `input` (the lines `read()` returns), `now` (milliseconds since 1970, for `time.now()`), `seed` (for `random`, `uuid` and `ulid`), `limits`, `mode: "check"`, `info: true`.
//
// Run it in a Web Worker: the limits (steps, depth, output, memory) stop runaway programs, but
// only a worker you can terminate protects the page from a bug in the module itself. After
// a `QlangCrash`, the instance must not be used again: `load` a new one.

export class QlangCrash extends Error {
  constructor(cause) {
    super(`the qlang module crashed (${cause && cause.message ? cause.message : cause}): load it again`);
    this.name = 'QlangCrash';
  }
}

async function instantiate(source) {
  if (source instanceof URL || typeof source === 'string') source = await fetch(source);
  if (typeof Response !== 'undefined' && source instanceof Response) {
    if (typeof WebAssembly.instantiateStreaming === 'function' && source.headers.get('content-type') === 'application/wasm') {
      return (await WebAssembly.instantiateStreaming(source, {})).instance;
    }
    source = await source.arrayBuffer();
  }
  return (await WebAssembly.instantiate(source, {})).instance;
}

export async function load(source) {
  const { exports: wasm } = await instantiate(source);
  const encoder = new TextEncoder();
  const decoder = new TextDecoder();
  let crashed = false;

  // memory can grow (and its buffer change) during a call: always take a fresh view
  const bytes = () => new Uint8Array(wasm.memory.buffer);

  const readFramed = (ptr) => {
    const length = new DataView(wasm.memory.buffer).getUint32(ptr, true);
    const text = decoder.decode(bytes().subarray(ptr + 4, ptr + 4 + length));
    wasm.qlang_free(ptr, 4 + length);
    return text;
  };

  const guard = (fn) => {
    if (crashed) throw new QlangCrash('an earlier call crashed');
    try {
      return fn();
    } catch (e) {
      crashed = true;
      throw new QlangCrash(e);
    }
  };

  const version = guard(() => readFramed(wasm.qlang_version()));

  return {
    version,
    /** Answer a request (an object, or JSON text); returns the parsed answer. */
    handle(request) {
      const text = typeof request === 'string' ? request : JSON.stringify(request);
      const input = encoder.encode(text);
      return JSON.parse(
        guard(() => {
          const ptr = wasm.qlang_alloc(input.length);
          bytes().set(input, ptr);
          const answer = readFramed(wasm.qlang_handle(ptr, input.length));
          wasm.qlang_free(ptr, input.length);
          return answer;
        })
      );
    },
  };
}
