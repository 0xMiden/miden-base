# miden-macros

A collection of procedural macros for the Miden project.

## WordWrapper

The `WordWrapper` derive macro automatically implements helpful accessor methods and conversions for tuple structs that wrap a `Word` type.

### Usage

Add the derive macro to any tuple struct with a single `Word` field:

```rust
use miden_macros::WordWrapper;
use miden_crypto::word::Word;

#[derive(WordWrapper)]
pub struct NoteId(Word);
```

### Generated Methods

The macro automatically generates the following methods:

#### Accessor Methods

- **`as_elements(&self) -> &[Felt]`** - Returns the elements representation of the wrapped Word
- **`as_bytes(&self) -> [u8; 32]`** - Returns the byte representation
- **`to_hex(&self) -> String`** - Returns a big-endian, hex-encoded string
- **`as_word(&self) -> Word`** - Returns the underlying Word value

#### Conversion Traits

The macro also implements these `From` trait conversions:

- **`From<Word> for T`** - Convert from a Word to your type
- **`From<T> for Word`** - Convert from your type to Word
- **`From<&T> for Word`** - Convert from a reference to your type to Word
- **`From<T> for [u8; 32]`** - Convert to a byte array
- **`From<&T> for [u8; 32]`** - Convert from a reference to a byte array

### Example

```rust
use miden_macros::WordWrapper;
use miden_crypto::word::Word;

#[derive(Debug, Clone, Copy, PartialEq, Eq, WordWrapper)]
pub struct NoteId(Word);

// Create from Word
let word = Word::from([Felt::ONE, Felt::ZERO, Felt::ONE, Felt::ZERO]);
let note_id = NoteId::from(word);

// Use accessor methods
let elements = note_id.as_elements();
let bytes = note_id.as_bytes();
let hex = note_id.to_hex();
let word_back = note_id.as_word();

// Convert back to Word
let word: Word = note_id.into();

// Convert to bytes
let bytes: [u8; 32] = note_id.into();
```

### Requirements

The macro can only be applied to:
- Tuple structs (e.g., `struct Foo(Word)`)
- With exactly one field
- Where that field is of type `Word`

### Benefits

Using this macro eliminates boilerplate code. Instead of manually writing ~50 lines of implementation code for each Word wrapper type, you can simply add `#[derive(WordWrapper)]` to your struct definition.

This is particularly useful in the Miden codebase where many types like `NoteId`, `TransactionId`, `Nullifier`, `BatchId`, etc. all follow the same pattern of wrapping a `Word` and providing similar accessor methods.
