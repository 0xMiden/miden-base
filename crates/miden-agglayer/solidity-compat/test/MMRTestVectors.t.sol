// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "forge-std/Test.sol";
import "@agglayer/v2/lib/DepositContractBase.sol";

/**
 * @title MMRTestVectors
 * @notice Test contract that generates test vectors for verifying compatibility
 *         between Solidity's DepositContractBase and Miden's MMR Frontier implementation.
 * 
 * Run with: forge test -vv --match-contract MMRTestVectors
 * 
 * The output can be compared against the Rust KeccakMmrFrontier32 implementation
 * in crates/miden-testing/tests/agglayer/mmr_frontier.rs
 */
contract MMRTestVectors is Test, DepositContractBase {
    
    /**
     * @notice Generates the canonical zeros and saves to JSON file.
     *         ZERO_0 = 0x0...0 (32 zero bytes)
     *         ZERO_n = keccak256(ZERO_{n-1} || ZERO_{n-1})
     *         
     *         Output file: test-vectors/canonical_zeros.json
     */
    function test_generateCanonicalZeros() public {
        string memory json = "{\n";
        json = string.concat(json, '  "canonical_zeros": [\n');
        
        bytes32 zero = bytes32(0);
        for (uint256 height = 0; height < 32; height++) {
            if (height < 31) {
                json = string.concat(json, '    "', vm.toString(zero), '",\n');
            } else {
                json = string.concat(json, '    "', vm.toString(zero), '"\n');
            }
            zero = keccak256(abi.encodePacked(zero, zero));
        }
        json = string.concat(json, "  ]\n}");
        
        // Print to console
        console.log(json);
        
        // Save to file
        string memory outputPath = "test-vectors/canonical_zeros.json";
        vm.writeFile(outputPath, json);
        console.log("\nSaved to:", outputPath);
    }
    
    /**
     * @notice Generates MMR frontier vectors (leaf-root pairs) and saves to JSON file.
     *         Output file: test-vectors/mmr_frontier_vectors.json
     */
    function test_generateVectors() public {
        string memory json = "{\n";
        json = string.concat(json, '  "vectors": [\n');
        
        for (uint256 i = 0; i < 32; i++) {
            bytes32 leaf = bytes32(i);
            _addLeaf(leaf);
            bytes32 root = getRoot();
            
            // Build JSON object for this vector
            string memory vectorJson = string.concat(
                '    {"leaf": "', vm.toString(leaf), 
                '", "root": "', vm.toString(root),
                '", "count": ', vm.toString(depositCount), "}"
            );
            
            if (i < 31) {
                json = string.concat(json, vectorJson, ",\n");
            } else {
                json = string.concat(json, vectorJson, "\n");
            }
        }
        
        json = string.concat(json, "  ]\n}");
        
        // Print to console
        console.log(json);
        
        // Save to file
        string memory outputPath = "test-vectors/mmr_frontier_vectors.json";
        vm.writeFile(outputPath, json);
        console.log("\nSaved to:", outputPath);
    }
}
