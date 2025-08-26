# Address

> [!Note]
> A human-readable identifier for `Account`s or public keys.


## Purpose

An address is a unique identifier that facilitates sending and receiving of [notes](../note.md). It has essentially two purposes that are explained in this section.

### Access control for notes

An address determines who is allowed to consume a note. In this sense, it acts as an access-control mechanism.

Consider two examples that use different access control mechanisms:

- The [Pay-to-ID note](../note.md#p2id-pay-to-id) can only be consumed if the account ID stored in the note matches the ID of the account that tries to consume it.
- Let's imagine a different "Pay-to-Public-Key" note that stores a public key and checks if the receiver can provide a valid cryptographic signature for that key, which the receiver can only do if they have the matching private key.

The address lets the sender know how the receiver wants to access the note: It could be via account ID or by proving ownership of a private key.

To allow for both of these use cases, addresses must be able to represent account IDs but also other identifiers such as public keys. Since accounts are central in Miden, most addresses are likely to represent account IDs, but other identifiers (like public keys) are also possible.

### Account interface discovery

An address allows the sender of the note to easily discover the interface of the receiving account. As explained in the [account interface](./code.md#interface) section, every account can have a different set of procedures that note scripts can call, which is the _interface_ of the account. In order for the sender of a note to create a note that the receiver can consume, the sender needs to know the interface of the receiving account. This can be communicated via the address, which encodes a mapping of standard interfaces like the basic wallet. An address can encode exactly one such interface in order to keep address sizes small, but users can generate multiple addresses for the same account in order to communicate different interfaces to senders.

If a sender wants to create a note, it is up to them to check whether the receiver account has an interface that it compatible with that note. The notion of an address doesn't exist at protocol level and so it is up to wallets or clients to implement this interface compatibility check.

## Types & Interfaces

An address encodes an address type and an address interface:
- The type determines what the address fundamentally points to, e.g. an account ID or, in the future, a public key.
- The interface informs the sender of the capabilities of the receiver's account.

> [!Note]
> Adding a public key-based address type is planned.

The currently supported **address types** are:
- `AccountIdAddress` (type `0`): An address pointing to an account ID.

The currently supported **address interfaces** are:
- `BasicWallet` (type `0`): The standard basic wallet interface. See the [account code](./code.md#interface) docs for details.

## Encoding

An address is encoded in [**bech32 format**](https://github.com/bitcoin/bips/blob/master/bip-0173.mediawiki), which has the following benefits:
- Built-in error detection via checksum algorithm
- Human-readable prefix indicates network type
- Less prone to errors when typed or spoken compared to hex format

An example of a bech32-encoded address is `mm1qrzqeg8kneq2wypcahq87774m3cqq4ejcg7`, which encodes and `AccountIdAddress` with the `BasicWallet` interface.

The structure of a bech32-encoded address is:
- [Human-readable prefix](https://github.com/satoshilabs/slips/blob/master/slip-0173.md) that
determines the network:
  - `mm` (indicates **M**iden **M**ainnet)
  - `mtst` (indicates Miden Testnet)
  - `mdev` (indicates Miden Devnet)
- Separator: `1`
- Data part with integrated checksum

The data part is where the underlying address type is encoded (e.g. `AccountIdAddress` with `BasicWallet`).
