/**
 * Minimal, self-contained BLAKE3-256 (default 32-byte output) for the Worker.
 *
 * WHY a hand-rolled impl and not a dependency: this is used by EXACTLY ONE,
 * currently-DORMANT code path — the optional `ac_output_name` exact-key
 * narrowing of a runner-job PAT (cf-multitenant WP5a). The launch path never
 * calls it (it uses the `"*"` sentinel). Per the WP brief we do NOT pull a heavy
 * new dependency into the worker bundle for a dormant path; instead we ship a
 * focused, spec-correct implementation pinned by a unit test against the
 * official BLAKE3 test vectors (see `worker/tests/blake3.test.ts`).
 *
 * Scope: the SINGLE-CHUNK-and-multi-chunk hash of a short UTF-8 string in the
 * default (unkeyed, no-context) mode, root output = 32 bytes. That is all the
 * narrowing derivation needs (`blake3("clw/ref/runner/v1/" + name)`), and it is
 * validated against the reference vectors so it matches the Rust `blake3` crate
 * the container uses byte-for-byte.
 *
 * NOT a general-purpose BLAKE3 (no keyed/derive-key modes, no XOF beyond 32
 * bytes, no streaming) — deliberately minimal to keep the surface auditable.
 *
 * Implementation note: the module runs under `noUncheckedIndexedAccess`, so all
 * fixed-shape internal buffers are plain `number[]` sized exactly to the BLAKE3
 * word counts (8 CV words / 16 state+message words); every index is a compile-
 * time-known constant `< length`, so the reads are provably in-bounds — the
 * non-null assertions on those reads assert exactly that.
 */

// ── Constants (BLAKE3 spec) ────────────────────────────────────────────────
const OUT_LEN = 32;
const BLOCK_LEN = 64;
const CHUNK_LEN = 1024;

// Domain-separation flags.
const CHUNK_START = 1 << 0;
const CHUNK_END = 1 << 1;
const PARENT = 1 << 2;
const ROOT = 1 << 3;

// IV = first 32 bits of the fractional parts of the square roots of the first
// 8 primes (identical to SHA-256's H0..H7).
const IV: readonly number[] = [
  0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c,
  0x1f83d9ab, 0x5be0cd19,
];

// Message-word permutation applied between rounds.
const MSG_PERMUTATION: readonly number[] = [
  2, 6, 3, 10, 7, 0, 4, 13, 1, 11, 12, 5, 9, 14, 15, 8,
];

/** 8-word chaining value. */
type Cv = number[];
/** 16-word state / message-block buffer. */
type Words = number[];

function rotr(x: number, n: number): number {
  return ((x >>> n) | (x << (32 - n))) >>> 0;
}

function g(state: Words, a: number, b: number, c: number, d: number, mx: number, my: number): void {
  state[a] = (state[a]! + state[b]! + mx) >>> 0;
  state[d] = rotr(state[d]! ^ state[a]!, 16);
  state[c] = (state[c]! + state[d]!) >>> 0;
  state[b] = rotr(state[b]! ^ state[c]!, 12);
  state[a] = (state[a]! + state[b]! + my) >>> 0;
  state[d] = rotr(state[d]! ^ state[a]!, 8);
  state[c] = (state[c]! + state[d]!) >>> 0;
  state[b] = rotr(state[b]! ^ state[c]!, 7);
}

function round(state: Words, m: Words): void {
  // Columns.
  g(state, 0, 4, 8, 12, m[0]!, m[1]!);
  g(state, 1, 5, 9, 13, m[2]!, m[3]!);
  g(state, 2, 6, 10, 14, m[4]!, m[5]!);
  g(state, 3, 7, 11, 15, m[6]!, m[7]!);
  // Diagonals.
  g(state, 0, 5, 10, 15, m[8]!, m[9]!);
  g(state, 1, 6, 11, 12, m[10]!, m[11]!);
  g(state, 2, 7, 8, 13, m[12]!, m[13]!);
  g(state, 3, 4, 9, 14, m[14]!, m[15]!);
}

/**
 * The BLAKE3 compression function. Returns the full 16-word state (words 0..7
 * are the output chaining value). `counter` is the 64-bit chunk counter split
 * into [lo, hi] 32-bit words.
 */
function compress(
  cv: Cv,
  blockWords: Words,
  counterLo: number,
  counterHi: number,
  blockLen: number,
  flags: number,
): Words {
  const state: Words = [
    cv[0]!, cv[1]!, cv[2]!, cv[3]!, cv[4]!, cv[5]!, cv[6]!, cv[7]!,
    IV[0]!, IV[1]!, IV[2]!, IV[3]!,
    counterLo >>> 0, counterHi >>> 0, blockLen >>> 0, flags >>> 0,
  ];

  let m: Words = blockWords.slice();
  for (let r = 0; r < 7; r++) {
    round(state, m);
    if (r < 6) {
      const permuted: Words = new Array<number>(16);
      for (let i = 0; i < 16; i++) {
        permuted[i] = m[MSG_PERMUTATION[i]!]!;
      }
      m = permuted;
    }
  }

  // Feed-forward: out[i] = state[i] ^ state[i+8]; out[i+8] = state[i+8] ^ cv[i].
  const out: Words = new Array<number>(16);
  for (let i = 0; i < 8; i++) {
    out[i] = (state[i]! ^ state[i + 8]!) >>> 0;
    out[i + 8] = (state[i + 8]! ^ cv[i]!) >>> 0;
  }
  return out;
}

/** Little-endian read 16 message words from a 64-byte (zero-padded) block. */
function wordsFromBlock(block: Uint8Array): Words {
  const words: Words = new Array<number>(16);
  for (let i = 0; i < 16; i++) {
    const o = i * 4;
    words[i] =
      (block[o]! | (block[o + 1]! << 8) | (block[o + 2]! << 16) | (block[o + 3]! << 24)) >>> 0;
  }
  return words;
}

/** First 8 words of a 16-word state → the chaining value. */
function firstEight(words: Words): Cv {
  return [words[0]!, words[1]!, words[2]!, words[3]!, words[4]!, words[5]!, words[6]!, words[7]!];
}

/**
 * Hash one chunk (≤ 1024 bytes) into its 8-word chaining value. `isRoot` marks
 * the single-chunk root case so the ROOT flag is applied to the final block.
 */
function hashChunk(chunk: Uint8Array, counterLo: number, counterHi: number, isRoot: boolean): Cv {
  let cv: Cv = IV.slice();
  const nBlocks = Math.max(1, Math.ceil(chunk.length / BLOCK_LEN));
  for (let b = 0; b < nBlocks; b++) {
    const start = b * BLOCK_LEN;
    const slice = chunk.subarray(start, Math.min(start + BLOCK_LEN, chunk.length));
    const block = new Uint8Array(BLOCK_LEN);
    block.set(slice);
    const blockLen = slice.length;

    let flags = 0;
    if (b === 0) flags |= CHUNK_START;
    if (b === nBlocks - 1) {
      flags |= CHUNK_END;
      if (isRoot) flags |= ROOT;
    }

    cv = firstEight(compress(cv, wordsFromBlock(block), counterLo, counterHi, blockLen, flags));
  }
  return cv;
}

/**
 * Compress a parent node (two child chaining values) into its 8-word CV. The
 * parent block is the 16 words = leftCV ‖ rightCV; counter is always 0.
 */
function parentCv(left: Cv, right: Cv, isRoot: boolean): Cv {
  const blockWords: Words = [
    left[0]!, left[1]!, left[2]!, left[3]!, left[4]!, left[5]!, left[6]!, left[7]!,
    right[0]!, right[1]!, right[2]!, right[3]!, right[4]!, right[5]!, right[6]!, right[7]!,
  ];
  let flags = PARENT;
  if (isRoot) flags |= ROOT;
  return firstEight(compress(IV.slice(), blockWords, 0, 0, BLOCK_LEN, flags));
}

/**
 * Full BLAKE3-256 of `input` bytes → 32 raw output bytes (default unkeyed,
 * no-context mode). Handles the single-chunk and multi-chunk (binary tree) cases
 * via the standard largest-power-of-two left-subtree recursion.
 */
function blake3(input: Uint8Array): Uint8Array {
  const nChunks = Math.max(1, Math.ceil(input.length / CHUNK_LEN));

  if (nChunks === 1) {
    // Single chunk IS the root.
    return cvToBytes(hashChunk(input, 0, 0, true));
  }

  const chunkCvs: Cv[] = [];
  for (let i = 0; i < nChunks; i++) {
    const start = i * CHUNK_LEN;
    const chunk = input.subarray(start, Math.min(start + CHUNK_LEN, input.length));
    const counterLo = i >>> 0;
    const counterHi = Math.floor(i / 0x100000000) >>> 0;
    chunkCvs.push(hashChunk(chunk, counterLo, counterHi, false));
  }

  return cvToBytes(combineToRoot(chunkCvs));
}

/**
 * Combine an ordered list of chunk chaining values into the single root CV. The
 * left subtree is the largest power of two strictly less than the count; the top
 * combine of the two subtrees is the ROOT.
 */
function combineToRoot(cvs: Cv[]): Cv {
  if (cvs.length === 1) return cvs[0]!;
  let left = 1;
  while (left * 2 < cvs.length) left *= 2;
  return parentCv(combineSubtree(cvs.slice(0, left)), combineSubtree(cvs.slice(left)), true);
}

/** Combine a subtree of chunk CVs into ONE non-root CV (same tree shape). */
function combineSubtree(cvs: Cv[]): Cv {
  if (cvs.length === 1) return cvs[0]!;
  let left = 1;
  while (left * 2 < cvs.length) left *= 2;
  return parentCv(combineSubtree(cvs.slice(0, left)), combineSubtree(cvs.slice(left)), false);
}

/** Serialize an 8-word chaining value to 32 little-endian bytes. */
function cvToBytes(cv: Cv): Uint8Array {
  const out = new Uint8Array(OUT_LEN);
  for (let i = 0; i < 8; i++) {
    const w = cv[i]! >>> 0;
    out[i * 4] = w & 0xff;
    out[i * 4 + 1] = (w >>> 8) & 0xff;
    out[i * 4 + 2] = (w >>> 16) & 0xff;
    out[i * 4 + 3] = (w >>> 24) & 0xff;
  }
  return out;
}

function toHex(bytes: Uint8Array): string {
  let s = "";
  for (const b of bytes) {
    s += b.toString(16).padStart(2, "0");
  }
  return s;
}

/**
 * BLAKE3-256 of a UTF-8 string → 64-char lowercase hex. This is the ONE exported
 * entry point used by the runner-job exact-key narrowing (WP5a). Returns a
 * Promise for call-site symmetry with the WebCrypto hashers elsewhere in the
 * worker, though the computation itself is synchronous.
 */
export async function blake3Hex(input: string): Promise<string> {
  const bytes = new TextEncoder().encode(input);
  return toHex(blake3(bytes));
}

/** Raw-bytes variant (test/interop convenience). */
export async function blake3HexBytes(input: Uint8Array): Promise<string> {
  return toHex(blake3(input));
}
