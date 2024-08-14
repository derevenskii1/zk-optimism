# kona-sp1

Standalone repo to use Kona & SP1 to verify Optimism blocks.

## Overview

**`crates`**
- `client-utils`: A suite of utilities for the client program.
- `host-utils`: A suite of utilities for constructing the host which runs the SP1 Kona program.

**`sp1-kona`**
- `native-host`: The host program which runs the Kona program natively using `kona`.
- `zkvm-host`: The host program which runs the Kona program in SP1.
- `client-programs`: The programs proven in SP1. 
    - For `zkvm-client` and `validity-client`, which are used to generate proofs for single blocks 
    and batches of blocks respectively, the binary is first run in native mode on the `kona-host` to
    fetch the witness data, then uses SP1 to generate the program's proof of execution.
   - For `aggregation-client`, which is used to generate an aggregate proof for a set of batches,
   first generate proofs for `validity-client` for each batch, then use `aggregation-client` to
   generate an aggregate proof.

## Usage

Execute the SP1 Kona program for a single block.

```bash
just run-single <l2_block_num> [use-cache]
```

- [use-cache]: Optional flag to re-use the native execution cache (default: false).

Execute the SP1 Kona program for a range of blocks.

```bash
just run-multi <start> <end> [use-cache] [prove]
```

- [use-cache]: Optional flag to re-use the native execution cache (default: false).
- [prove]: Optional flag to prove the execution (default: false).

Observations: 
* For most blocks, the cycle count per transaction is around 4M cycles per transaction.
* Some example cycle count estimates can be found [here](https://www.notion.so/succinctlabs/SP1-Kona-8b025f81f28f4d149eb4816db4e6d80b?pvs=4).

## Cycle Counts

To see how to get the cycle counts for a given block range, see [CYCLE_COUNT.md](./CYCLE_COUNT.md).


## Misc

To fetch an existing proof and save it run:

```bash
cargo run --bin fetch_and_save_proof --release -- --request-id <proofrequest_id> --start <start_block> --end <end_block>
```

Ex. `cargo run --bin fetch_and_save_proof --release -- --request-id proofrequest_01j4ze00ftfjpbd4zkf250qwey --start 123812410 --end 123812412`