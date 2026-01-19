// SPDX-License-Identifier: MIT
// Based on OpenZeppelin's Hashes.sol
pragma solidity ^0.8.20;

/**
 * @dev Library of hashing functions used by the agglayer contracts.
 */
library Hashes {
    /**
     * @dev Computes keccak256(abi.encode(a, b)) more efficiently using assembly.
     * This is equivalent to keccak256(a || b) where || is concatenation.
     */
    function efficientKeccak256(bytes32 a, bytes32 b) internal pure returns (bytes32 value) {
        assembly ("memory-safe") {
            mstore(0x00, a)
            mstore(0x20, b)
            value := keccak256(0x00, 0x40)
        }
    }
}
