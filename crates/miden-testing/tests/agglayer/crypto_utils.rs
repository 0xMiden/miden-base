extern crate alloc;

use alloc::sync::Arc;
use alloc::string::String;
use alloc::vec::Vec;

use miden_agglayer::agglayer_library;
use miden_assembly::{Assembler, DefaultSourceManager};
use miden_core_lib::CoreLibrary;
use miden_processor::fast::{ExecutionOutput, FastProcessor};
use miden_processor::{AdviceInputs, DefaultHost, ExecutionError, Program, StackInputs};
use miden_protocol::Felt;
use miden_protocol::transaction::TransactionKernel;

const INPUT_MEMORY_ADDR: u32 = 0x1000;

/// Execute a program with default host
async fn execute_program_with_default_host(
    program: Program,
) -> Result<ExecutionOutput, ExecutionError> {
    let mut host = DefaultHost::default();

    let test_lib = TransactionKernel::library();
    host.load_library(test_lib.mast_forest()).unwrap();

    let std_lib = CoreLibrary::default();
    host.load_library(std_lib.mast_forest()).unwrap();

    for (event_name, handler) in std_lib.handlers() {
        host.register_handler(event_name, handler)?;
    }

    let agglayer_lib = agglayer_library();
    host.load_library(agglayer_lib.mast_forest()).unwrap();

    let stack_inputs = StackInputs::new(vec![]).unwrap();
    let advice_inputs = AdviceInputs::default();

    let processor = FastProcessor::new_debug(stack_inputs.as_slice(), advice_inputs);
    processor.execute(&program, &mut host).await
}

/// Generate MASM code to store field elements in memory
fn masm_store_felts(felts: &[Felt], base_addr: u32) -> String {
    let mut code = String::new();
    
    for (i, felt) in felts.iter().enumerate() {
        let addr = base_addr + (i as u32);
        code.push_str(&format!("push.{}.{} mem_store\n", felt.as_int(), addr));
    }
    
    code
}

/// Convert bytes to field elements (u32 words packed into felts)
fn bytes_to_felts(data: &[u8]) -> Vec<Felt> {
    let mut felts = Vec::new();
    
    // Pad data to multiple of 4 bytes
    let mut padded_data = data.to_vec();
    while padded_data.len() % 4 != 0 {
        padded_data.push(0);
    }
    
    // Convert to u32 words in little-endian format
    for chunk in padded_data.chunks(4) {
        let word = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        felts.push(Felt::new(word as u64));
    }
    
    felts
}

fn u32_words_to_solidity_bytes32_hex(words: &[u64]) -> String {
    assert_eq!(words.len(), 8, "expected 8 u32 words = 32 bytes");
    let mut out = [0u8; 32];

    for (i, &w) in words.iter().enumerate() {
        let le = (w as u32).to_le_bytes();
        out[i * 4..i * 4 + 4].copy_from_slice(&le);
    }

    let mut s = String::from("0x");
    for b in out {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

#[tokio::test]
async fn test_keccak_hash_bytes_test() -> anyhow::Result<()> {
    let mut input_u8: Vec<u8> = vec![0u8; 24];
    input_u8.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);

    let len_bytes = input_u8.len();
    let input_felts = bytes_to_felts(&input_u8);
    let memory_stores_source = masm_store_felts(&input_felts, INPUT_MEMORY_ADDR);

    let agglayer_lib = agglayer_library();

    let source = format!(
        r#"
            use miden::core::sys
            use miden::core::crypto::hashes::keccak256

            begin
                # Store packed u32 values in memory
                {memory_stores_source}

                # Push wrapper inputs
                push.{len_bytes}.{INPUT_MEMORY_ADDR}
                # => [ptr, len_bytes]

                exec.keccak256::hash_bytes
                # => [DIGEST_U32[8]]

                exec.sys::truncate_stack
            end
            "#,
    );

    let program = Assembler::new(Arc::new(DefaultSourceManager::default()))
        .with_dynamic_library(CoreLibrary::default())
        .unwrap()
        .with_dynamic_library(agglayer_lib.clone())
        .unwrap()
        .assemble_program(&source)
        .unwrap();

    let exec_output = execute_program_with_default_host(program).await?;

    // Extract the digest from the stack (8 u32 values)
    let digest: Vec<u64> = exec_output.stack[0..8].iter().map(|f| f.as_int()).collect();
    let solidity_hex = u32_words_to_solidity_bytes32_hex(&digest);


    println!("solidity-style digest: {solidity_hex}");
    println!("digest: {:?}", digest);

    // Expected digest for the test case: 24 zero bytes + [1,2,3,4,5,6,7,8]
    let expected_digest = vec![3225960785, 4007474008, 2169124512, 2724332080, 2839075162, 3406483620, 4039244674, 3474684833];
    let expected_hex = "0x514148c05833ddeea0364a81300262a25ad938a9a4d00acb82fbc1f0a17b1bcf";
    
    assert_eq!(digest, expected_digest);
    assert_eq!(solidity_hex, expected_hex);

    Ok(())
}


#[tokio::test]
async fn test_keccak_hash_get_leaf_value_encode_packed() -> anyhow::Result<()> {
    // Solidity equivalent:
    // keccak256(abi.encodePacked(
    //   leafType(uint8),
    //   originNetwork(uint32),
    //   originAddress(address),
    //   destinationNetwork(uint32),
    //   destinationAddress(address),
    //   amount(uint256),
    //   metadataHash(bytes32)
    // ))

    // ---- Fixed test vector (easy to mirror in Solidity) ----
    let leaf_type: u8 = 0x01;
    let origin_network: u32 = 0x1122_3344;
    let origin_address: [u8; 20] = [0x11; 20];

    let destination_network: u32 = 0x5566_7788;
    let destination_address: [u8; 20] = [0x22; 20];

    // uint256 amount = 0x0102030405060708 (packed to 32 bytes big-endian)
    let mut amount: [u8; 32] = [0u8; 32];
    amount[24..32].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);

    // bytes32 metadataHash = 0xaaaa....aaaa
    let metadata_hash: [u8; 32] = [0xaa; 32];

    // ---- abi.encodePacked layout ----
    // uint8  -> 1 byte
    // uint32 -> 4 bytes big-endian
    // address-> 20 bytes
    // uint32 -> 4 bytes big-endian
    // address-> 20 bytes
    // uint256-> 32 bytes big-endian
    // bytes32-> 32 bytes
    let mut input_u8 = Vec::with_capacity(113);
    input_u8.push(leaf_type);
    input_u8.extend_from_slice(&origin_network.to_be_bytes());
    input_u8.extend_from_slice(&origin_address);
    input_u8.extend_from_slice(&destination_network.to_be_bytes());
    input_u8.extend_from_slice(&destination_address);
    input_u8.extend_from_slice(&amount);
    input_u8.extend_from_slice(&metadata_hash);

    let len_bytes = input_u8.len();
    assert_eq!(len_bytes, 113);

    let input_felts = bytes_to_felts(&input_u8);
    let memory_stores_source = masm_store_felts(&input_felts, INPUT_MEMORY_ADDR);

    let agglayer_lib = agglayer_library();

    let source = format!(
        r#"
            use miden::core::sys
            use miden::core::crypto::hashes::keccak256

            begin
                # Store packed u32 values in memory
                {memory_stores_source}

                # Push wrapper inputs
                push.{len_bytes}.{INPUT_MEMORY_ADDR}
                # => [ptr, len_bytes]

                exec.keccak256::hash_bytes
                # => [DIGEST_U32[8]]

                exec.sys::truncate_stack
            end
        "#
    );

    let program = Assembler::new(Arc::new(DefaultSourceManager::default()))
        .with_dynamic_library(CoreLibrary::default())
        .unwrap()
        .with_dynamic_library(agglayer_lib.clone())
        .unwrap()
        .assemble_program(&source)
        .unwrap();

    let exec_output = execute_program_with_default_host(program).await?;

    // Extract the digest from the stack (8 u32 values)
    let digest: Vec<u64> = exec_output.stack[0..8].iter().map(|f| f.as_int()).collect();
    let solidity_hex = u32_words_to_solidity_bytes32_hex(&digest);

    println!("solidity-style digest: {solidity_hex}");
    println!("digest: {:?}", digest);

    Ok(())
}


#[tokio::test]
async fn test_keccak_hash_get_leaf_value_hardhat_vector() -> anyhow::Result<()> {
    // Helper: parse 0x-prefixed hex into a fixed-size byte array
    fn hex_to_fixed<const N: usize>(s: &str) -> [u8; N] {
        let s = s.strip_prefix("0x").unwrap_or(s);
        assert_eq!(s.len(), N * 2, "expected {} hex chars", N * 2);
        let mut out = [0u8; N];
        for i in 0..N {
            out[i] = u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap();
        }
        out
    }

    // === Values from hardhat test ===
    let leaf_type: u8 = 0;
    let origin_network: u32 = 0;
    let token_address: [u8; 20] =
        hex_to_fixed("0x1234567890123456789012345678901234567890");
    let destination_network: u32 = 1;
    let destination_address: [u8; 20] =
        hex_to_fixed("0x0987654321098765432109876543210987654321");
    let amount_u64: u64 = 1; // 1e19
    let metadata_hash: [u8; 32] = hex_to_fixed(
        "0x2cdc14cacf6fec86a549f0e4d01e83027d3b10f29fa527c1535192c1ca1aac81",
    );

    // abi.encodePacked(
    //   uint8, uint32, address, uint32, address, uint256, bytes32
    // )
    let mut amount_u256_be = [0u8; 32];
    amount_u256_be[24..32].copy_from_slice(&amount_u64.to_be_bytes());

    let mut input_u8 = Vec::with_capacity(113);
    input_u8.push(leaf_type);
    input_u8.extend_from_slice(&origin_network.to_be_bytes());
    input_u8.extend_from_slice(&token_address);
    input_u8.extend_from_slice(&destination_network.to_be_bytes());
    input_u8.extend_from_slice(&destination_address);
    input_u8.extend_from_slice(&amount_u256_be);
    input_u8.extend_from_slice(&metadata_hash);

    let len_bytes = input_u8.len();
    assert_eq!(len_bytes, 113);

    let input_felts = bytes_to_felts(&input_u8);
    let memory_stores_source = masm_store_felts(&input_felts, INPUT_MEMORY_ADDR);

    let agglayer_lib = agglayer_library();

    let source = format!(
        r#"
            use miden::core::sys
            use miden::core::crypto::hashes::keccak256

            begin
                {memory_stores_source}

                push.{len_bytes}.{INPUT_MEMORY_ADDR}
                exec.keccak256::hash_bytes
                exec.sys::truncate_stack
            end
        "#
    );

    let program = Assembler::new(Arc::new(DefaultSourceManager::default()))
        .with_dynamic_library(CoreLibrary::default())
        .unwrap()
        .with_dynamic_library(agglayer_lib.clone())
        .unwrap()
        .assemble_program(&source)
        .unwrap();

    let exec_output = execute_program_with_default_host(program).await?;

    // Extract digest as 8 u32 words
    let digest: Vec<u64> = exec_output.stack[0..8].iter().map(|f| f.as_int()).collect();
    let solidity_hex = u32_words_to_solidity_bytes32_hex(&digest);

    println!("solidity-style digest: {solidity_hex}");
    println!("digest: {:?}", digest);

    Ok(())
}
