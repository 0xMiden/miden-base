// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "forge-std/Test.sol";
import "@agglayer/v2/lib/DepositContractBase.sol";

/**
 * @title MMRTestVectors
 * @notice Test contract that generates test vectors for verifying compatibility
 *         between Solidity's DepositContractBase and Miden's MMR Frontier implementation.
 * 
 * Run with: forge test -vv --match-test test_generateVectors
 * 
 * The output can be compared against the Rust KeccakMmrFrontier32 implementation
 * in crates/miden-testing/tests/agglayer/mmr_frontier.rs
 */
contract MMRTestVectors is Test, DepositContractBase {
    
    /**
     * @notice Generates the canonical zeros used in the MMR.
     *         ZERO_0 = 0x0...0 (32 zero bytes)
     *         ZERO_n = keccak256(ZERO_{n-1} || ZERO_{n-1})
     *         
     *         These should match the values in canonical_zeros.masm
     */
    function test_generateCanonicalZeros() public pure {
        console.log("=== Canonical Zeros ===");
        console.log("ZERO_n = keccak256(ZERO_{n-1} || ZERO_{n-1})");
        console.log("");
        
        bytes32 zero = bytes32(0);
        
        for (uint256 height = 0; height < 32; height++) {
            console.log("ZERO_%d:", height);
            console.logBytes32(zero);
            
            // Compute next zero: hash(zero || zero)
            zero = keccak256(abi.encodePacked(zero, zero));
        }
    }
    
    /**
     * @notice Test with zero leaves only - the root should remain constant
     *         as "empty MMR root" regardless of how many zero leaves are added.
     */
    function test_zeroLeavesRoot() public {
        console.log("=== Zero Leaves Test ===");
        console.log("Adding 32 zero leaves, root should be consistent with empty tree");
        console.log("");
        
        for (uint256 i = 0; i < 32; i++) {
            bytes32 zeroLeaf = bytes32(0);
            _addLeaf(zeroLeaf);
            bytes32 root = getRoot();
            
            console.log("After %d zero leaves:", i + 1);
            console.logBytes32(root);
        }
    }
    
    /**
     * @notice Outputs vectors in JSON format and saves to file
     *         Run with: forge test -vv --match-test test_generateVectors
     *         Output file: test-vectors/mmr_frontier_vectors.json
     */
    function test_generateVectors() public {
        string memory json = "{\n";
        json = string.concat(json, '  "description": "Test vectors from DepositContractBase.sol",\n');
        json = string.concat(json, '  "source_commit": "e468f9b0967334403069aa650d9f1164b1731ebb",\n');
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
        
        json = string.concat(json, "  ],\n");
        
        // Add canonical zeros
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
        string memory outputPath = "test-vectors/mmr_frontier_vectors.json";
        vm.writeFile(outputPath, json);
        console.log("\nSaved to:", outputPath);
    }
}
