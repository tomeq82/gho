# Fuzzing with AFL++

`gho` ships with two parallel fuzzing harnesses:
- **`fuzz/`** — libFuzzer harnesses (used in `cargo fuzz` and CI nightly).
- **`fuzz_afl/`** — AFL++ persistent-mode harnesses (this directory).

AFL++ is the spiritual successor to Michal Zalewski's American Fuzzy Lop
(`lcamtuf`). It uses a different coverage feedback algorithm (fork-server
model + edge coverage bitmap) than libFuzzer, so it tends to find a
different class of bugs and provides useful cross-validation.

## Setup

```bash
# Install cargo-afl (downloads + configures afl-fuzz, no sudo needed)
cargo install cargo-afl

# Build the harnesses with AFL instrumentation
cd fuzz_afl
cargo afl build --release
```

The build produces three AFL binaries in `target/release/`:
- `fastlz_decompress`
- `ghost11_extract`
- `ghostold_walk`

## Running

```bash
# Create an initial seed corpus (one or more files is fine; AFL grows it)
mkdir -p corpus/seed
printf '\x00\x00\x00\x00\xf8\xffABC' > corpus/seed/seed1.bin

# Start fuzzing. The two env vars below are needed when running on a
# system without privileged access to /proc/sys/kernel/core_pattern.
export AFL_I_DONT_CARE_ABOUT_MISSING_CRASHES=1
export AFL_NO_AFFINITY=1
export AFL_SKIP_CPUFREQ=1

cargo afl fuzz -i corpus/seed -o corpus/fastlz \
    -- target/release/fastlz_decompress
```

AFL++ shows a live TUI with execution count, coverage map, and any
crashes/ hangs found. Press `Ctrl-C` to stop; output is saved to
`corpus/<target>/default/`.

## Why AFL in addition to libFuzzer?

| Property | libFuzzer | AFL++ |
|---|---|---|
| Coverage feedback | SanCov (edge counters) | Edge bitmap |
| Mutation strategy | Default + custom mutators | MOpt / havoc / splice |
| Speed | In-process (very fast) | Fork-server (slower per exec but very thorough) |
| Best for | Quick regression runs | Long, overnight runs |

Both have found different bugs in `gho` during testing. CI runs libFuzzer
for 60s smoke tests; AFL is intended for ad-hoc local runs.

## Notes

- AFL++ requires writing harnesses with a `#[unsafe(no_mangle)] pub
  extern "C" fn afl_persistent` symbol. The `unsafe` qualifier is
  required by Rust 2024 edition's stricter attribute rules.
- The persistent-mode harness lets AFL run many inputs per fork,
  avoiding fork overhead and achieving higher throughput than
  fork-per-input mode.
- Real images from `/mnt/storage/ghost_backups_old/` make excellent
  seed inputs — drop a few into `corpus/seed/` to bootstrap coverage
  faster.
