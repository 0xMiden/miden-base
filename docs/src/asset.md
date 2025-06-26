# Assets

An `Asset` is a unit of value that can be transferred from one [account](account/overview.md) to another using [notes](note.md).

## What is the purpose of an asset?

In Miden, assets serve as the primary means of expressing and transferring value between [accounts](account/overview.md) through [notes](note.md). They are designed with four key principles in mind:

1. **Parallelizable exchange:**  
    By managing ownership and transfers directly at the account level instead of relying on global structures like ERC20 contracts, accounts can exchange assets concurrently, boosting scalability and efficiency.

2. **Self-sovereign ownership:**  
   Assets are stored in the accounts directly. This ensures that users retain complete control over their assets.

3. **Censorship resistance:**  
   Users can transact freely and privately with no single contract or entity controlling `Asset` transfers. This reduces the risk of censored transactions, resulting in a more open and resilient system.

4. **Flexible fee payment:**  
   Unlike protocols that require a specific base `Asset` for fees, Miden allows users to pay fees in any supported `Asset`. This flexibility simplifies the user experience.

## Native asset

> [!Note]
> All data structures following the Miden asset model that can be exchanged.

Native assets adhere to the Miden `Asset` model (encoding, issuance, storage). Every native `Asset` is encoded using 32 bytes, including both the [ID](account/overview.md#id) of the issuing account and the `Asset` details.

### Issuance

> [!Note]
> Only [faucet](account/overview.md#account-types) accounts can issue assets.

Faucets can issue either fungible or non-fungible assets as defined at account creation. The faucet's code specifies the `Asset` minting conditions: i.e., how, when, and by whom these assets can be minted. Once minted, they can be transferred to other accounts using notes.

<p style="text-align: center;">
    <img src="img/asset/asset-issuance.png" style="width:70%;" alt="Asset issuance"/>
</p>

### Type

#### Fungible asset

Fungible assets are encoded with the amount and the `faucet_id` of the issuing faucet. The amount is always $2^{63}-1$ or smaller, representing the maximum supply for any fungible `Asset`. Examples include ETH and various stablecoins (e.g., DAI, USDT, USDC).

#### Non-fungible asset

Non-fungible assets are encoded by hashing the `Asset` data into 32 bytes and placing the `faucet_id` as the second element. Examples include NFTs like a DevCon ticket.

### Storage

[Accounts](account/overview.md) and [notes](note.md) have vaults used to store assets. Accounts use a sparse Merkle tree as a vault while notes use a simple list. This enables an account to store a practically unlimited number of assets while a note can only store 255 assets.

<p style="text-align: center;">
    <img src="img/asset/asset-storage.png" style="width:70%;" alt="Asset storage"/>
</p>

### Burning

Assets in Miden can be burned through various methods, such as rendering them unspendable by storing them in an unconsumable note, or sending them back to their original faucet for burning using it's dedicated function.

## Alternative asset models

> [!Note]
> All data structures not following the Miden asset model that can be exchanged.

Miden is flexible enough to support other `Asset` models. For example, developers can replicate Ethereum’s ERC20 pattern, where fungible `Asset` ownership is recorded in a single account. To transact, users send a note to that account, triggering updates in the global hashmap state.
