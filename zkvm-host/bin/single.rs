use std::{env, fs};

use anyhow::Result;
use clap::Parser;
use host_utils::{fetcher::SP1KonaDataFetcher, get_proof_stdin, ProgramType};
use kona_host::start_server_and_native_client;
use num_format::{Locale, ToFormattedString};
use sp1_sdk::{utils, ProverClient};

use client_utils::precompiles::PRECOMPILE_HOOK_FD;
use zkvm_host::precompile_hook;

pub const SINGLE_BLOCK_ELF: &[u8] = include_bytes!("../../elf/zkvm-client-elf");

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Start block number.
    #[arg(short, long)]
    l2_block: u64,

    /// Skip running native execution.
    #[arg(short, long)]
    use_cache: bool,
}

/// Execute the Kona program for a single block.
#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    let args = Args::parse();
    utils::setup_logger();

    let data_fetcher = SP1KonaDataFetcher {
        l2_rpc: env::var("L2_RPC").expect("L2_RPC is not set."),
        ..Default::default()
    };

    // TODO: Use `optimism_outputAtBlock` to fetch the L2 block at head
    // https://github.com/ethereum-optimism/kona/blob/d9dfff37e2c5aef473f84bf2f28277186040b79f/bin/client/justfile#L26-L32
    let l2_safe_head = args.l2_block - 1;

    let host_cli = data_fetcher
        .get_host_cli_args(l2_safe_head, args.l2_block, ProgramType::Single)
        .await?;

    let data_dir = host_cli
        .data_dir
        .clone()
        .expect("Data directory is not set.");

    // By default, re-run the native execution unless the user passes `--use-cache`.
    if !args.use_cache {
        // Overwrite existing data directory.
        fs::create_dir_all(&data_dir).unwrap();

        // Start the server and native client.
        start_server_and_native_client(host_cli.clone())
            .await
            .unwrap();
    }

    // Get the stdin for the block.
    let sp1_stdin = get_proof_stdin(&host_cli)?;

    let prover = ProverClient::new();
    let (_, report) = prover
        .execute(SINGLE_BLOCK_ELF, sp1_stdin)
        .with_hook(PRECOMPILE_HOOK_FD, precompile_hook)
        .run()
        .unwrap();

    println!(
        "Block {} cycle count: {}",
        args.l2_block,
        report
            .total_instruction_count()
            .to_formatted_string(&Locale::en)
    );

    Ok(())
}
