# powdr-revme

This repository uses [powdr](https://github.com/powdr-labs/powdr) as a library
to make ZK proofs for the [Ethereum tests](https://github.com/ethereum/tests)
via [revm](https://github.com/bluealloy/revm). It is meant as test- and
benchmark-bed for powdr and provers.

Please see the [powdr docs](https://docs.powdr.org) for basic instructions on
how to use powdr as a library.

# Usage

```help
Usage: bin [OPTIONS] --test <TEST>

Options:
  -t, --test <TEST>
  -o, --output <OUTPUT>  [default: .]
  -f, --fast-tracer
  -w, --witgen
  -p, --proofs
  -h, --help             Print help
```

Checkout the [Ethereum tests](https://github.com/ethereum/tests).  The only
argument required is the test path. The path may be a single file or a
directory.  If only the path is given, powdr only compiles the Rust code all
the way to powdr-PIL.  The fast tracer is a powdr-IR executor that is useful
for quick checks.  Witness generation (witgen) creates the witness required for
proofs.  Proofs have not been added to this repo yet.

Example running only the fast tracer:

```bash
cargo run -r -- -t tests/accessListExample.json -f
```

# Crates

This repo contains 3 crates:

- `bin`: the main binary, meant to run natively. It reads the given tests and
  manages calls to powdr.
- `evm`: the code we want to make proofs for. It deserializes a test suite,
  runs revm, and asserts that the test expectation is met. Most of the code
  here comes from
  [revme](https://github.com/bluealloy/revm/blob/main/bins/revme/src/cmd/statetest/runner.rs#L214).
- `models`: some data structures needed for test ser/derialization. This code
  comes from
  [revme](https://github.com/bluealloy/revm/tree/main/bins/revme/src/cmd/statetest/models).
