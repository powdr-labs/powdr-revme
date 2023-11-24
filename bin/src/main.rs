use riscv::{compile_rust, CoProcessors};
use compiler::compile_asm_string_with_callback;
use number::{GoldilocksField, FieldElement};

use std::path::{Path, PathBuf};

use models::*;

use thiserror::Error;
use walkdir::{DirEntry, WalkDir};

use revm::{
    db::{CacheDB, EmptyDB, CacheState},
    interpreter::CreateScheme,
    primitives::{
        address, b256, calc_excess_blob_gas, keccak256, Env, HashMap, SpecId, ruint::Uint, AccountInfo, Address, Bytecode, Bytes, TransactTo, B256, U256,
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
    let all_tests = find_all_json_tests(&eth_tests_path);
    println!("{all_tests:?}");
    for t in all_tests {
        //let suite = read_suite(&t);

        println!("Reading JSON test...");
        let suite_json = std::fs::read_to_string(&t).unwrap();
        println!("Creating data callback...");
        let mut suite_json_bytes: Vec<GoldilocksField> = suite_json.into_bytes().iter().map(|b| (*b as u32).into()).collect();
        suite_json_bytes.insert(0, (suite_json_bytes.len() as u32).into());

        let mut data: STDHashMap<GoldilocksField, Vec<GoldilocksField>> = STDHashMap::default();
        data.insert(666.into(), suite_json_bytes);

        let output_dir = Path::new("/tmp/test");
        let force_overwrite = true;

        println!("Compiling Rust...");
        let (asm_file_path, asm_contents) =
            compile_rust("./evm", Path::new("/tmp/test"), true, &CoProcessors::base())
            .ok_or_else(|| vec!["could not compile rust".to_string()]).unwrap();

        println!("Running powdr-riscv executor...");
        riscv_executor::execute::<GoldilocksField>(&asm_contents, &data);
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

static BYTECODE: &str = "61029a60005260206000f3";
fn simple_test() {
    let output_dir = Path::new("/tmp/test");
    let force_overwrite = true;

    println!("Compiling Rust...");
    let (asm_file_path, asm_contents) =
        compile_rust("./evm", Path::new("/tmp/test"), true, &CoProcessors::base())
        .ok_or_else(|| vec!["could not compile rust".to_string()]).unwrap();

    let bytes = hex::decode(BYTECODE).unwrap();
    let bytecode_len = bytes.len();
    let mut input: Vec<GoldilocksField> = bytes.into_iter().map(|x| (x as u64).into()).collect();
    input.insert(0, (bytecode_len as u64).into());

    let mut data: HashMap<usize, Vec<GoldilocksField>> = HashMap::default();
    data.insert(0, input);

    println!("Compiling powdr-asm...");

    /*
    let _result = compile_asm_string(
        asm_file_path.to_str().unwrap(),
        &asm_contents,
        input,
        output_dir,
        force_overwrite,
        None,
        vec![],
    ).unwrap();
    */
    
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

    println!("Done.");
}

fn find_all_json_tests(path: &Path) -> Vec<PathBuf> {
    WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().ends_with(".json"))
        .map(DirEntry::into_path)
        .collect::<Vec<PathBuf>>()
}

fn data_to_query_callback<T: FieldElement>(data: HashMap<usize, Vec<T>>) -> impl Fn(&str) -> Option<T> {
    move |query: &str| -> Option<T> {
        let items = query.split(',').map(|s| s.trim()).collect::<Vec<_>>();
        match items[0] {
            "\"input\"" => {
                assert_eq!(items.len(), 2);
                let index = items[1].parse::<usize>().unwrap();
                // 0 = "input"
                let values = &data[&0];
                values.get(index).cloned()
            }
            "\"data\"" => {
                assert_eq!(items.len(), 3);
                let index = items[1].parse::<usize>().unwrap();
                let what = items[2].parse::<usize>().unwrap();
                let values = &data[&what];
                values.get(index).cloned()
            }
            "\"print_char\"" => {
                assert_eq!(items.len(), 2);
                print!("{}", items[1].parse::<u8>().unwrap() as char);
                Some(0.into())
            }
            "\"hint\"" => {
                assert_eq!(items.len(), 2);
                Some(T::from_str(items[1]))
            }
            _ => None,
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


