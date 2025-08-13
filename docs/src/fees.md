# Fees

Miden transactions pay a fee that is computed and charged automatically by the transaction kernel during the epilogue.

## How fees are computed

- The fee depends on the number of VM cycles the transaction executes and grows logarithmically with that count. The kernel estimates verification cycles by taking log2 of the estimated total cycles, rounding up to the next power of two. The fee amount is then computed as:
  - `fee_amount = verification_base_fee * ceil(log2(estimated_cycles_rounded_up_to_pow2))`
- The `verification_base_fee` comes from the current reference block’s fee parameters.

Relevant kernel logic:
```246:270:crates/miden-lib/asm/kernels/transaction/lib/epilogue.masm
#! - fee_amount is the computed fee amount of the transaction in the native asset.
proc.compute_fee
    # get the number of cycles the transaction has taken to execute up this point
    clk
    # => [num_current_cycles]

    emit.EPILOGUE_AFTER_TX_FEE_COMPUTED

    # estimate the number of cycles the transaction will take
    add.ESTIMATED_AFTER_COMPUTE_FEE_CYCLES
    # => [num_tx_cycles]

    # ilog2 will round down, but we need to round up, so we add 1 afterwards.
    ilog2 add.1
    # => [num_estimated_verification_cycles]

    exec.memory::get_verification_base_fee
    # => [verification_base_fee, num_estimated_verification_cycles]

    mul
    # => [verification_cost]
end
```

And the block header fee parameters:
```309:351:crates/miden-objects/src/block/header.rs
/// This defines how to compute the fees of a transaction and which asset fees can be paid in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeeParameters {
    /// The [`AccountId`] of the fungible faucet whose assets are accepted for fee payments in the
    /// transaction kernel, or in other words, the native asset of the blockchain.
    native_asset_id: AccountId,
    /// The base fee (in base units) capturing the cost for the verification of a transaction.
    verification_base_fee: u32,
}
...
/// Returns the base fee capturing the cost for the verification of a transaction.
pub fn verification_base_fee(&self) -> u32 {
    self.verification_base_fee
}
```

For testing/utilities, there is also a Rust helper mirroring the kernel’s computation at a high level:
```3:11:crates/miden-objects/src/testing/tx.rs
impl ExecutedTransaction {
    /// A Rust implementation of the compute_fee epilogue procedure.
    pub fn compute_fee(&self) -> u64 {
        // Round up the number of cycles to the next power of two and take log2 of it.
        let verification_cycles = self.measurements().trace_length().ilog2();
        let fee_amount =
            self.block_header().fee_parameters().verification_base_fee() * verification_cycles;
        fee_amount as u64
    }
}
```

## Which asset is used to pay fees

- Fees are paid in the chain’s native asset, defined by the current reference block’s `FeeParameters.native_asset_id`.
- The kernel constructs a fee asset using that native asset ID and removes it from the account’s vault:
```272:319:crates/miden-lib/asm/kernels/transaction/lib/epilogue.masm
proc.build_native_fee_asset
    exec.memory::get_native_asset_id
    # => [native_asset_id_prefix, native_asset_id_suffix, fee_amount]

    push.0 movdn.2
    # => [native_asset_id_prefix, native_asset_id_suffix, 0, fee_amount]
    # => [FEE_ASSET]
end

proc.compute_and_remove_fee
    # compute the fee the tx needs to pay
    exec.compute_fee
    # => [fee_amount]

    # build the native asset from the fee amount
    exec.build_native_fee_asset
    # => [FEE_ASSET]

    # remove the fee from the native account's vault
    exec.account::remove_asset_from_vault
end
```

If the account does not contain enough of the native asset to cover the computed fee, the transaction fails during the epilogue.

## Where the fee appears in outputs

- The transaction kernel outputs the fee asset as part of the epilogue, and the transaction outputs carry it explicitly:
```31:44:crates/miden-objects/src/transaction/outputs.rs
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransactionOutputs {
    /// Information related to the account's final state.
    pub account: AccountHeader,
    /// The commitment to the delta computed by the transaction kernel.
    pub account_delta_commitment: Word,
    /// Set of output notes created by the transaction.
    pub output_notes: OutputNotes,
    /// The fee of the transaction.
    pub fee: FungibleAsset,
    /// Defines up to which block the transaction is considered valid.
    pub expiration_block_num: BlockNumber,
}
```

## Practical implication for users

- Ensure your account’s vault holds sufficient balance of the native asset of the current reference block to cover the computed fee. The fee is automatically charged during the transaction epilogue; no explicit fee-sending step is required. 
