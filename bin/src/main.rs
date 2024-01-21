use powdr::number::{FieldElement, GoldilocksField};
use powdr::pipeline::{Pipeline, Stage, parse_query};

use powdr::riscv::continuations::{
    bootloader::default_input, rust_continuations, rust_continuations_dry_run,
};
use powdr::riscv::{compile_rust, CoProcessors};
use powdr::riscv_executor;
use powdr::executor::witgen::QueryCallback;

use std::path::{Path, PathBuf};
use std::time::Instant;

use thiserror::Error;
use walkdir::{DirEntry, WalkDir};

use revm::primitives::B256;

fn main() {
    env_logger::init();

    eth_test_simple();
}

fn eth_test_simple() {
    let eth_tests_path = Path::new("../ethereum-tests/simple");
    //let eth_tests_path = Path::new("../ethereum-tests/GeneralStateTests/VMTests");
    //let eth_tests_path = Path::new("../ethereum-tests/long");
    let all_tests = find_all_json_tests(eth_tests_path);
    println!("{all_tests:?}");

    println!("Compiling Rust...");
    let (asm_file_path, asm_contents) = compile_rust(
        "./evm",
        Path::new("/tmp/test"),
        true,
        &CoProcessors::base().with_poseidon(),
        true,
    )
    .ok_or_else(|| vec!["could not compile rust".to_string()])
    .unwrap();

    //println!("{asm_contents}");

    let mk_pipeline = || {
        Pipeline::<GoldilocksField>::default()
            .from_asm_string(asm_contents.clone(), Some(asm_file_path.clone()))
            .with_prover_inputs(vec![])
    };

    println!("Creating pipeline from powdr-asm...");
    let start = Instant::now();
    let pipeline = mk_pipeline();
    let duration = start.elapsed();
    println!("Pipeline from powdr-asm took: {:?}", duration);

    println!("Advancing pipeline to fixed columns...");
    let start = Instant::now();
    let pil_with_evaluated_fixed_cols = pipeline.pil_with_evaluated_fixed_cols().unwrap();
    let duration = start.elapsed();
    println!("Advancing pipeline took: {:?}", duration);

    for t in all_tests {
        println!("Running test {}", t.display());

        println!("Reading JSON test...");
        let suite_json = std::fs::read_to_string(&t).unwrap();

        let mk_pipeline_with_data = || mk_pipeline().add_data(42, &suite_json);

        let mk_pipeline_opt = || {
            mk_pipeline_with_data()
                .from_pil_with_evaluated_fixed_cols(pil_with_evaluated_fixed_cols.clone())
        };

        println!("Running powdr-riscv executor in fast mode...");
        let start = Instant::now();
        let (trace, _mem) = riscv_executor::execute::<GoldilocksField>(
            &asm_contents,
            mk_pipeline_with_data().data_callback().unwrap(),
            &default_input(&[]),
            riscv_executor::ExecMode::Fast,
        );
        let duration = start.elapsed();
        println!("Fast executor took: {:?}", duration);
        println!("Trace length: {}", trace.len);

        println!("Running powdr-riscv executor in trace mode for continuations...");
        let start = Instant::now();
        let bootloader_inputs = rust_continuations_dry_run(mk_pipeline_with_data());
        let duration = start.elapsed();
        println!("Trace executor took: {:?}", duration);

        let generate_witness = |mut pipeline: Pipeline<GoldilocksField>| -> Result<(), Vec<String>> {
            let data = data_to_query_callback(data.clone());
            let mut pipeline = pipeline.add_query_callback(Box::new(data));
            let start = Instant::now();
            println!("Generating witness...");
            pipeline.advance_to(Stage::GeneratedWitness)?;
            let duration = start.elapsed();
            println!("Generating witness took: {:?}", duration);
            Ok(())
        };

        println!("Running witness generation...");
        let start = Instant::now();
        rust_continuations(
            mk_pipeline_opt,
            generate_witness,
            bootloader_inputs,
        ).unwrap();
        let duration = start.elapsed();
        println!("Witness generation took: {:?}", duration);

        /*
        println!("Compiling powdr-asm...");
        let _result = compile_asm_string_with_callback(
            asm_file_path.to_str().unwrap(),
            &asm_contents,
            data_to_query_callback(data),
            None,
            output_dir,
            force_overwrite,
            None,
            vec![],
        ).unwrap();
        */

        println!("Done.");
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

#[derive(Debug, Error)]
#[error("Test {name} failed: {kind}")]
pub struct TestError {
    pub name: String,
    pub kind: TestErrorKind,
}

#[derive(Debug, Error)]
pub enum TestErrorKind {
    #[error("logs root mismatch: expected {expected:?}, got {got:?}")]
    LogsRootMismatch { got: B256, expected: B256 },
    #[error("state root mismatch: expected {expected:?}, got {got:?}")]
    StateRootMismatch { got: B256, expected: B256 },
    #[error("Unknown private key: {0:?}")]
    UnknownPrivateKey(B256),
    #[error("Unexpected exception: {got_exception:?} but test expects:{expected_exception:?}")]
    UnexpectedException {
        expected_exception: Option<String>,
        got_exception: Option<String>,
    },
    #[error(transparent)]
    SerdeDeserialize(#[from] serde_json::Error),
}
