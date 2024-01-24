use powdr::riscv::continuations::{
    bootloader::default_input, rust_continuations, rust_continuations_dry_run,
};
use powdr::riscv::{compile_rust, CoProcessors};
use powdr::riscv_executor;
use powdr::GoldilocksField;
use powdr::{pipeline::Stage, Pipeline};

use std::path::{Path, PathBuf};
use std::time::Instant;

use walkdir::{DirEntry, WalkDir};

use clap::Parser;

fn main() {
    let mut options = Options::parse();
    if options.proofs {
        options.witgen = true;
    }

    env_logger::init();

    run_all_tests(&options)
}

#[derive(Parser)]
struct Options {
    #[clap(short, long, required = true)]
    path: PathBuf,

    #[clap(short, long)]
    fast_tracer: bool,

    #[clap(short, long)]
    witgen: bool,

    #[clap(short, long)]
    proofs: bool,
}

fn run_all_tests(options: &Options) {
    let all_tests = find_all_json_tests(&options.path);

    log::info!("{}", format!("All tests: {:?}", all_tests));
    log::info!("Compiling powdr-revme...");
    let (asm_file_path, asm_contents) = compile_rust(
        "./evm",
        Path::new("/tmp/test"),
        true,
        &CoProcessors::base().with_poseidon(),
        true,
    )
    .ok_or_else(|| vec!["could not compile rust".to_string()])
    .unwrap();

    log::debug!("powdr-asm code:\n{asm_contents}");

    let mk_pipeline = || {
        Pipeline::<GoldilocksField>::default()
            .from_asm_string(asm_contents.clone(), Some(asm_file_path.clone()))
            .with_prover_inputs(vec![])
    };

    log::info!("Creating pipeline from powdr-asm...");
    let start = Instant::now();
    let pipeline = mk_pipeline();
    let duration = start.elapsed();
    log::info!("Pipeline from powdr-asm took: {:?}", duration);

    log::info!("Advancing pipeline to fixed columns...");
    let start = Instant::now();
    let pil_with_evaluated_fixed_cols = pipeline.pil_with_evaluated_fixed_cols().unwrap();
    let duration = start.elapsed();
    log::info!("Advancing pipeline took: {:?}", duration);

    for t in all_tests {
        log::info!("Running test {}", t.display());

        log::info!("Reading JSON test...");
        let suite_json = std::fs::read_to_string(&t).unwrap();

        let mk_pipeline_with_data = || mk_pipeline().add_data(42, &suite_json);

        let mk_pipeline_opt = || {
            mk_pipeline_with_data()
                .from_pil_with_evaluated_fixed_cols(pil_with_evaluated_fixed_cols.clone())
        };

        if options.fast_tracer {
            log::info!("Running powdr-riscv executor in fast mode...");
            let start = Instant::now();
            let (trace, _mem) = riscv_executor::execute::<GoldilocksField>(
                &asm_contents,
                mk_pipeline_with_data().data_callback().unwrap(),
                &default_input(&[]),
                riscv_executor::ExecMode::Fast,
            );
            let duration = start.elapsed();
            log::info!("Fast executor took: {:?}", duration);
            log::info!("Trace length: {}", trace.len);
        }

        if options.witgen {
            log::info!("Running powdr-riscv executor in trace mode for continuations...");
            let start = Instant::now();
            let bootloader_inputs = rust_continuations_dry_run(mk_pipeline_with_data());
            let duration = start.elapsed();
            log::info!("Trace executor took: {:?}", duration);

            let generate_witness =
                |mut pipeline: Pipeline<GoldilocksField>| -> Result<(), Vec<String>> {
                    let start = Instant::now();
                    println!("Generating witness...");
                    pipeline.advance_to(Stage::GeneratedWitness)?;
                    let duration = start.elapsed();
                    println!("Generating witness took: {:?}", duration);
                    Ok(())
                };

            log::info!("Running witness generation...");
            let start = Instant::now();
            rust_continuations(mk_pipeline_opt, generate_witness, bootloader_inputs).unwrap();
            let duration = start.elapsed();
            log::info!("Witness generation took: {:?}", duration);
        }

        if options.proofs {
            log::info!("Proofs requested but not implemented yet in this test.");
        }

        log::info!("Done.");
    }
}

fn find_all_json_tests(path: &Path) -> Vec<PathBuf> {
    WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().ends_with(".json"))
        .map(DirEntry::into_path)
        .collect::<Vec<PathBuf>>()
}
