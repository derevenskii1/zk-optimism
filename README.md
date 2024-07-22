# kona-sp1

Standalone repo to use Kona & SP1 to verify Optimism blocks.

## Usage

To run the program in witness gen mode, then use that witness to create a proof in the ZKVM:

```bash
just run [l2_block_num]
```

If you already have witness data, run the program in the ZKVM by passing the L1 head, L2 output root, L2 claim, L2 claim block number, and chain ID:

```bash
just run-zkvm-host \
bb3c26e67fd8acb1a2baa15cd9affc57347f8549775657537d2f2ae359384ba4 \
91c0ff7cdc5b59ff251b1c137b1f46c4c27e2b9f2ab17bb3b31c63d2f792a0a0 \
bfbec731f443c09bbfdcef53358458644ac2cbe1c5f68e53ad38599a52d65b5b \
121866428 \
10
```

## Run the Cost Estimator

The cost estimator currently prints out the cycle counts for each block in a range. TODO: Add cost estimation once this is exposed in the SDK.

```bash
cargo run --bin cost-estimator --profile release-client-lto -- --start-block <START_BLOCK> --end-block <END_BLOCK> --rpc-url <L2_OP_GETH_ARCHIVE_NODE>
```

Output:
```

```
