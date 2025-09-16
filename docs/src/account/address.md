# Address

> [!Note]
> A human-readable identifier for `Account`s or public keys.


## Purpose

An address is an identifier that facilitates sending and receiving of [notes](../note.md). It serves three main purposes explained in this section.

### Communicating receiver information

An address is designed for the note receiver to communicate information about themselves to the sender.

The receiver can choose to disclose various pieces of information that control how the note itself is structured.

Consider a few examples that use different address mechanisms:

- The [Pay-to-ID note](../note.md#p2id-pay-to-id): the note itself can only be consumed if the account ID encoded in the note details matches the ID of the account that tries to consume it. To receive a P2ID note, the receiver should communicate an `Address::AccountId` type to the sender.
- A "Pay-to-PoW" note that can only be consumed if the receiver can provide a valid seed such that the hash of the seed results in a value with n leading zero bits. The receiver communicates an `Address::PoW` type to the sender, which encodes the target number of leading zero bits (and a salt to avoid re-use of the same seed).*
- A "Pay-to-Public-Key" note that stores a public (signature) key and checks if the receiver can provide a valid cryptographic signature for that key. The `Address::PublicKey` type must encode the public key.*

These different address mechanisms provide different levels of privacy and security:
- `Address::AccountId`: the receiver is uniquely identifiable, but they are the only ones who can consume the note.
- `Address::PoW`: the receiver is not revealed publicly, but potentially many entities can consume the note. The receiver has an advantage by specifying the salt.
- `Address::PublicKey`: the receiver `AccountId` is not revealed publicly, only their public key. A fresh `Address::PublicKey` can be used for receiving each note, resulting in increased privacy.

> [!Note]
> The "Pay-to-PoW" and "Pay-to-Public-Key" notes and the corresponding address types are for illustration purposes only. They are not part of the Miden library.

### Communicating channel information

For notes which are sent privately, the sender needs to communicate the full note details to the receiver. This can be done via a side channel, such as a messenger, email, or via a QR code. We would like to avoid the necessity of operating two-way communication channels for each note. Rather, we operate under the assumption that once the receiver shares their `Address` (directly with the sender, or via a bulletin board, i.e. a one-way channel), they don't need to stay online and wait for the sender to send back the full note details.

Instead, our Miden client connects to a _Note Transport Layer_, which stores encrypted note details together with the associated public metadata for each note. The receiver can query the Note Transport Layer for `NoteTag`s they are interested in. Typically, a `NoteTag` encodes a few leading bits (14 by default) of the receiver's `AccountId`. Querying the Note Transport Layer for 14-bit `NoteTag`s reduces the receiver's privacy, but at the same time allows them to perform less work downloading and trial-decrypting the notes than if less bits were encoded.

With an `Address`, e.g. the [`Address::AccountId`](./address.md#addressaccountid) variant, the receiver could specify how many bits of their `AccountId` they want to disclose to the sender and thus choose their level of privacy.

### Account interface discovery

An address allows the sender of the note to easily discover the interface of the receiving account. As explained in the [account interface](./code.md#interface) section, every account can have a different set of procedures that note scripts can call, which is the _interface_ of the account. In order for the sender of a note to create a note that the receiver can consume, the sender needs to know the interface of the receiving account. This can be communicated via the address, which encodes a mapping of standard interfaces like the basic wallet.

If a sender wants to create a note, it is up to them to check whether the receiver account has an interface that it compatible with that note. The notion of an address doesn't exist at protocol level and so it is up to wallets or clients to implement this interface compatibility check.

## Relationship to Identifiers

An address can encode exactly one account interface, which is a deliberate limitation to keep address sizes small. Users can generate multiple addresses for the same identifier like account ID or public key, in order to communicate different interfaces to senders. In other words, there could be multiple different addresses that point to the same account, each encoding a different interface. So, the relationship from addresses to their underlying identifiers is n-to-1.

## Types & Interfaces

An address encodes an address type and an address interface:
- The type determines what the address fundamentally points to, e.g. an account ID or, in the future, a public key.
- The interface informs the sender of the capabilities of the receiver's account.

> [!Note]
> Adding a public key-based address type is planned.

The currently supported **address types** are:
- `Address::AccountId` (type `0`): An address pointing to an account ID.

The currently supported **address interfaces** are:
- `Unspecified` (type `0`): No interface is specified. Used for addresses where the interface is unknown.
- `BasicWallet` (type `1`): The standard basic wallet interface. See the [account code](./code.md#interface) docs for details.

### `Address::AccountId`

The account ID address points to an account ID and also allows specifying the [note tag](../note.md#note-discovery) length. This tag length preference determines how many bits of the account ID are encoded into note tags of notes targeted to this address. This lets the owner of the account choose their level of privacy. A higher tag length makes the account more uniquely identifiable and reduces privacy, while a shorter length increases privacy at the cost of matching more notes published onchain.

## Encoding

An address is encoded in [**bech32 format**](https://github.com/bitcoin/bips/blob/master/bip-0173.mediawiki), which has the following benefits:
- Built-in error detection via checksum algorithm
- Human-readable prefix indicates network type
- Less prone to errors when typed or spoken compared to hex format

Examples of bech32-encoded addresses that encode the same `Address::AccountId` are:
- `mm1qqttmuqgxur0jup8j8luck774rcqq58se2m`, with the `Unspecified` interface.
- `mm1qqttmuqgxur0jup8j8luck774rcqz8z36ek`, with the `BasicWallet` interface.

The structure of a bech32-encoded address is:
- [Human-readable prefix](https://github.com/satoshilabs/slips/blob/master/slip-0173.md) that
determines the network:
  - `mm` (indicates **M**iden **M**ainnet)
  - `mtst` (indicates Miden Testnet)
  - `mdev` (indicates Miden Devnet)
- Separator: `1`
- Data part with integrated checksum

The data part is where the underlying address type is encoded (e.g. `Address::AccountId` with `BasicWallet`).
