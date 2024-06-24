use powdr::riscv::continuations::{
    bootloader::default_input, rust_continuations, rust_continuations_dry_run,
};
use powdr::riscv::{compile_rust, Runtime};
use powdr::GoldilocksField;
use powdr::Pipeline;

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
    test: PathBuf,

    #[clap(short, long, default_value = ".")]
    output: PathBuf,

    #[clap(short, long)]
    fast_tracer: bool,

    #[clap(short, long)]
    witgen: bool,

    #[clap(short, long)]
    proofs: bool,
}

fn run_all_tests(options: &Options) {
    let all_tests = find_all_json_tests(&options.test);

    log::info!("{}", format!("All tests: {:?}", all_tests));
    log::info!("Compiling powdr-revme...");
    let (asm_file_path, asm_contents) = compile_rust::<GoldilocksField>(
        "./evm",
        &options.output,
        true,
        &Runtime::base().with_poseidon(),
        true,
    )
    .ok_or_else(|| vec!["could not compile rust".to_string()])
    .unwrap();

    log::debug!("powdr-asm code:\n{asm_contents}");

    // Create a pipeline from the asm program
    let mut pipeline = Pipeline::<GoldilocksField>::default()
        .from_asm_string(asm_contents, Some(asm_file_path.clone()))
        .with_output(options.output.clone(), true)
        .with_prover_inputs(vec![42.into()])
        .with_backend(powdr::backend::BackendType::EStarkDump, None);

    assert!(pipeline.compute_fixed_cols().is_ok());

    for t in all_tests {
        log::info!("Running test {}", t.display());

        log::info!("Reading JSON test...");
        let suite_json = std::fs::read_to_string(&t).unwrap();

        let pipeline_with_data = pipeline.clone().add_data(42, &suite_json);

        if options.fast_tracer {
            log::info!("Running powdr-riscv executor in fast mode...");
            let start = Instant::now();

            let program = pipeline.compute_analyzed_asm().unwrap().clone();
            let initial_memory = powdr::riscv::continuations::load_initial_memory(&program);
            let (trace, _mem) = powdr::riscv_executor::execute_ast::<GoldilocksField>(
                &program,
                initial_memory,
                pipeline_with_data.data_callback().unwrap(),
                &default_input(&[]),
                usize::MAX,
                powdr::riscv_executor::ExecMode::Fast,
                None,
            );

            let duration = start.elapsed();
            log::info!("Fast executor took: {:?}", duration);
            log::info!("Trace length: {}", trace.len);
        }

        if options.witgen || options.proofs {
            log::info!("Running powdr-riscv executor in trace mode for continuations...");
            let start = Instant::now();

            let bootloader_inputs =
                rust_continuations_dry_run(&mut pipeline_with_data.clone(), None);

            let duration = start.elapsed();
            log::info!("Trace executor took: {:?}", duration);

            let generate_witness =
                |mut pipeline: Pipeline<GoldilocksField>| -> Result<(), Vec<String>> {
                    let start = Instant::now();
                    println!("Generating witness...");
                    pipeline.compute_witness()?;
                    let duration = start.elapsed();
                    println!("Generating witness took: {:?}", duration);

                    if options.proofs {
                        log::info!("Generating proof...");
                        let start = Instant::now();

                        pipeline.compute_proof().unwrap();

                        let duration = start.elapsed();
                        log::info!("Proof generation took: {:?}", duration);
                    }

                    Ok(())
                };

            log::info!("Running witness and proof generation for all chunks...");
            let start = Instant::now();
            rust_continuations(
                pipeline_with_data.clone(),
                generate_witness,
                bootloader_inputs,
            )
            .unwrap();
            let duration = start.elapsed();
            log::info!(
                "Witness and proof generation for all chunks took: {:?}",
                duration
            );
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
