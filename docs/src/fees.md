# Fees

Miden transactions pay a fee that is computed and charged automatically by the transaction kernel during the epilogue.

## How fees are computed

- The fee depends on the number of VM cycles the transaction executes and grows logarithmically with that count.
- The kernel estimates the number of verification cycles by taking log2 of the estimated total execution cycles (rounded up). The result is then multiplied by the `verification_base_fee` from the reference block’s fee parameters.
- In other words, the fee is proportional to the logarithm of the transaction’s number of execution cycles, scaled by the base verification fee defined in the block header.

## Which asset is used to pay fees

- Fees are paid in the chain’s native asset, defined by the current reference block’s fee parameters.
- During the epilogue, the kernel constructs the fee asset using the native asset ID and removes it from the account’s vault.
- If the account does not contain enough of the native asset to cover the computed fee, the transaction fails during the epilogue.

## Where the fee appears in outputs

- The transaction kernel outputs the computed fee as a fungible asset, and the transaction outputs include it explicitly.
- Practically, users should ensure their account’s vault holds sufficient balance of the native asset of the current reference block to cover the fee. The fee is charged automatically; no explicit fee-sending step is required. 
