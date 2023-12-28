use powdr::number::{FieldElement, GoldilocksField};
use powdr::pipeline::{Pipeline, Stage};
use powdr::riscv::continuations::{
    bootloader::default_input, rust_continuations, rust_continuations_dry_run,
};
use powdr::riscv::{compile_rust, CoProcessors};
use powdr::riscv_executor;
use powdr::executor::witgen::QueryCallback;

use std::path::{Path, PathBuf};
use std::time::Instant;

use models::*;

use thiserror::Error;
use walkdir::{DirEntry, WalkDir};

use revm::{
    db::{CacheDB, CacheState, EmptyDB},
    interpreter::CreateScheme,
    primitives::{
        address, b256, calc_excess_blob_gas, keccak256, ruint::Uint, AccountInfo, Address,
        Bytecode, Bytes, Env, HashMap, SpecId, TransactTo, B256, U256,
    },
    EVM,
};

use std::collections::HashMap as STDHashMap;

fn main() {
    env_logger::init();

    eth_test_simple();
}

fn eth_test_simple() {
    let eth_tests_path = Path::new("../ethereum-tests/simple");
    //let eth_tests_path = Path::new("../ethereum-tests/GeneralStateTests/VMTests");
    //let eth_tests_path = Path::new("../ethereum-tests/long");
    let all_tests = find_all_json_tests(&eth_tests_path);
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

    let mk_pipeline = || {
        Pipeline::<GoldilocksField>::default()
            .from_asm_string(asm_contents.clone(), Some(asm_file_path.clone()))
    };

    //println!("{asm_contents}");

    for t in all_tests {
        println!("Running test {}", t.display());
        //let suite = read_suite(&t);

        println!("Reading JSON test...");
        let suite_json = std::fs::read_to_string(&t).unwrap();
        println!("Creating data callback...");
        let mut suite_json_bytes: Vec<GoldilocksField> = suite_json
            .into_bytes()
            .iter()
            .map(|b| (*b as u32).into())
            .collect();
        suite_json_bytes.insert(0, (suite_json_bytes.len() as u32).into());

        let mut data: STDHashMap<GoldilocksField, Vec<GoldilocksField>> = STDHashMap::default();
        data.insert(666.into(), suite_json_bytes);

        let output_dir = Path::new("/tmp/test");
        let force_overwrite = true;

        println!("Running powdr-riscv executor in fast mode...");
        let start = Instant::now();
        let (trace, mem) = riscv_executor::execute::<GoldilocksField>(
            &asm_contents,
            &data,
            &default_input(),
            riscv_executor::ExecMode::Fast,
        );
        let duration = start.elapsed();
        println!("Fast executor took: {:?}", duration);
        println!("Trace length: {}", trace.len);

        println!("Running powdr-riscv executor in trace mode for continuations...");
        let start = Instant::now();
        let bootloader_inputs = rust_continuations_dry_run(mk_pipeline(), data.clone());
        let duration = start.elapsed();
        println!("Trace executor took: {:?}", duration);

        let generate_witness = |mut pipeline: Pipeline<GoldilocksField>| -> Result<(), Vec<String>> {
            let data = data_to_query_callback(data.clone());
            let mut pipeline = pipeline.add_query_callback(Box::new(data));
            pipeline.advance_to(Stage::GeneratedWitness)?;
            Ok(())
        };

        println!("Running witness generation...");
        let start = Instant::now();
        rust_continuations(
            mk_pipeline,
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

        /*
        for (name, unit) in suite.0 {
            println!("{name}");
        }
        */
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

fn data_to_query_callback<T: FieldElement>(
    data: STDHashMap<T, Vec<T>>,
) -> impl QueryCallback<T> {
    move |query: &str| -> Result<Option<T>, String> {
        let items = query.split(',').map(|s| s.trim()).collect::<Vec<_>>();
        match items[0] {
            "\"input\"" => {
                assert_eq!(items.len(), 2);
                let index = items[1].parse::<usize>().unwrap();
                // 0 = "input"
                let values = &data[&T::zero()];
                Ok(values.get(index).cloned())
            }
            "\"data\"" => {
                assert_eq!(items.len(), 3);
                let index = items[1].parse::<usize>().unwrap();
                let what = items[2].parse::<usize>().unwrap();
                let values = &data[&(what as u64).into()];
                Ok(values.get(index).cloned())
            }
            "\"print_char\"" => {
                assert_eq!(items.len(), 2);
                print!("{}", items[1].parse::<u8>().unwrap() as char);
                Ok(Some(0.into()))
            }
            "\"hint\"" => {
                assert_eq!(items.len(), 2);
                Ok(Some(T::from_str(items[1])))
            }
            _ => Ok(None),
        }
    }
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
