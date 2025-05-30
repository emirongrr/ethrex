use std::{fs::File, io::Write};

use clap::Parser;
use ethrex_l2::utils::prover::proving_systems::ProverType;
use ethrex_prover_bench::{
    cache::{load_cache, write_cache, Cache},
    rpc::{db::RpcDB, get_block, get_latest_block_number},
};
use ethrex_prover_lib::execute;
use zkvm_interface::io::ProgramInput;

#[cfg(not(any(feature = "sp1", feature = "risc0", feature = "pico")))]
compile_error!(
    "Choose prover backends (sp1, risc0, pico).
- Pass a feature flag to cargo (--feature or -F) with the desired backed. e.g: cargo build --workspace --no-default-features -F sp1. NOTE: Don't forget to pass --no-default-features, if not, the default prover will be used instead."
);

#[derive(Parser, Debug)]
struct Args {
    #[arg(short, long)]
    rpc_url: String,
    #[arg(short, long)]
    block_number: Option<usize>,
    #[arg(short, long)]
    prove: bool,
}

#[tokio::main]
async fn main() {
    let Args {
        rpc_url,
        block_number,
        prove,
    } = Args::parse();

    let block_number = match block_number {
        Some(n) => n,
        None => {
            println!("fetching latest block number");
            get_latest_block_number(&rpc_url)
                .await
                .expect("failed to fetch latest block number")
        }
    };

    let Cache {
        block,
        parent_block_header,
        db,
    } = match load_cache(block_number) {
        Ok(cache) => cache,
        Err(err) => {
            println!("failed to load cache for block {block_number}: {err}");

            println!("fetching block {block_number} and its parent header");
            let block = get_block(&rpc_url, block_number)
                .await
                .expect("failed to fetch block");

            let parent_block_header = get_block(&rpc_url, block_number - 1)
                .await
                .expect("failed to fetch block")
                .header;

            println!("populating rpc db cache");
            let rpc_db = RpcDB::with_cache(&rpc_url, block_number - 1, &block)
                .await
                .expect("failed to create rpc db");

            let db = rpc_db
                .to_exec_db(&block)
                .expect("failed to build execution db");

            let cache = Cache {
                block,
                parent_block_header,
                db,
            };
            write_cache(&cache).expect("failed to write cache");
            cache
        }
    };

    let now = std::time::Instant::now();
    if prove {
        println!("proving");
        ethrex_prover_lib::prove(ProgramInput {
            block,
            parent_block_header,
            db,
        })
        .expect("proving failed");
    } else {
        println!("executing");
        execute(ProgramInput {
            block,
            parent_block_header,
            db,
        })
        .expect("proving failed");
    }
    let elapsed = now.elapsed().as_secs();
    println!(
        "finished in {} minutes for block {}",
        elapsed / 60,
        block_number
    );

    // TODO: Print total gas from pre-execution (to_exec_db() call)
}
