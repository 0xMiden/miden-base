use alloc::string::{String, ToString};

use miden_core::FieldElement;

use super::{Felt, TokenNameError, Word};

const NAME_WORD_SIZE: usize = 2;

/// Represents a string token name as a fixed size array of [`Word`]s of length
/// `NAME_WORD_SIZE`
///
/// Token name can contain upto 32 bytes of UTF-8 encoded characters.
#[derive(Default, Clone, Copy, Debug, PartialEq)]
pub struct TokenName([Word; NAME_WORD_SIZE]);

impl TokenName {
    /// Maximum allowed length of the name string
    pub const MAX_NAME_LEN: usize = 32;

    /// Creates a new [`TokenName`] instance from the provided token name string
    ///
    /// # Errors
    /// Returns an error if:
    /// - The length of provided string is greater than [`TokenName::MAX_NAME_LEN`]
    pub fn new(name: &str) -> Result<Self, TokenNameError> {
        let word = encode_name_to_words(name)?;
        Ok(Self(word))
    }

    /// Returns the corresponding string from the encoded [`TokenName`] value.
    ///
    /// # Errors
    /// Returns an error if
    /// - The encoded value decodes to invalid utf8
    pub fn to_string(&self) -> Result<String, TokenNameError> {
        decode_word_to_name(&self.0)
    }

    /// Returns the underlying slice of [`Word`]s representing the token name.
    pub fn as_slice(&self) -> [Word; NAME_WORD_SIZE] {
        self.0
    }
}

impl TryFrom<&str> for TokenName {
    type Error = TokenNameError;

    fn try_from(name: &str) -> Result<Self, Self::Error> {
        TokenName::new(name)
    }
}

impl From<TokenName> for [Word; NAME_WORD_SIZE] {
    fn from(name: TokenName) -> Self {
        name.0
    }
}

impl TryFrom<[Word; NAME_WORD_SIZE]> for TokenName {
    type Error = TokenNameError;

    fn try_from(words: [Word; NAME_WORD_SIZE]) -> Result<Self, Self::Error> {
        decode_word_to_name(&words)?;
        Ok(TokenName(words))
    }
}

// HELPER FUNCTIONS
// ===========================================================================================

/// Encodes the provided string convert to a fixed-size array of [`Word`]s of length
/// `NAME_WORD_SIZE`
///
/// It converts `name` to it's bytes representation and then into corresponding words
/// [b0, b1, b2 ... b31] => [ [{b0, b1, b2, b3}  .....   {b12, b13, b14, b15}] , ... ]
///                            |------------------- 1 word -------------------|
///
/// If length of string is smaller than 32 bytes it pads the bytes representation to 32 bytes
///
/// # Errors
/// Returns an error if:
/// - The length of provided string is 0 or greater than [`TokenName::MAX_NAME_LEN`]
fn encode_name_to_words(name: &str) -> Result<[Word; NAME_WORD_SIZE], TokenNameError> {
    if name.is_empty() || name.len() > TokenName::MAX_NAME_LEN {
        return Err(TokenNameError::InvalidLength(name.len()));
    }

    // created a padded bytes representation of the string
    let mut bytes = name.as_bytes().to_vec();
    bytes.resize(32, 0);

    let words = [
        encode_bytes_to_word(&bytes[0..16].try_into().unwrap()),
        encode_bytes_to_word(&bytes[16..32].try_into().unwrap()),
    ];

    Ok(words)
}

/// Encodes a 16 len `u8` buffer into a Word
fn encode_bytes_to_word(bytes: &[u8; 16]) -> Word {
    let mut felts = [Felt::ZERO; 4];
    for i in 0..4 {
        felts[i] = u32::from_ne_bytes([
            bytes[4 * i],
            bytes[4 * i + 1],
            bytes[4 * i + 2],
            bytes[4 * i + 3],
        ])
        .into();
    }
    Word::from(felts)
}

/// Decodes the provided array of [`Word`]s of length [`NAME_WORD_SIZE`] back into a string.
///
/// It decodes each word into its corresponding bytes and then constructs the string from the bytes
/// Null bytes at the end are trimmed in case the original buffer was padded.
///
/// Then the bytes are converted into string
///
/// # Errors
/// Returns an error if:
///  - if the string decoded from the word is not valid utf8
fn decode_word_to_name(words: &[Word; NAME_WORD_SIZE]) -> Result<String, TokenNameError> {
    let mut buf = [0u8; 16 * NAME_WORD_SIZE];
    buf[0..16].copy_from_slice(&decode_word_to_bytes(&words[0]));
    buf[16..32].copy_from_slice(&decode_word_to_bytes(&words[1]));

    String::from_utf8(buf.to_vec())
        .map(|str| str.trim_end_matches('\0').to_string())
        .map_err(TokenNameError::InvalidUtf8Buffer)
}

// decodes the word to `u8` buffer of len 16
fn decode_word_to_bytes(word: &Word) -> [u8; 16] {
    let mut buf = [0u8; 16];
    for i in 0..4 {
        let n: u32 = word[i].as_int().try_into().unwrap();
        buf[4 * i..4 * i + 4].copy_from_slice(&n.to_ne_bytes());
    }
    buf
}

#[cfg(test)]
mod test {

    use assert_matches::assert_matches;

    use super::{TokenName, TokenNameError};

    #[test]
    fn test_token_name_encoding_decoding() {
        let names = vec![
            "a",
            "aaaabbbbcccc",
            "\u{10FFFF}1234",
            "\u{10FFFF}\u{10FFFF}\u{10FFFF}\u{10FFFF}",
        ];
        for name in names {
            let token_name = TokenName::try_from(name).unwrap();
            let decoded_name = token_name.to_string().unwrap();
            assert_eq!(name, decoded_name);
        }

        let name = "";
        let token_name = TokenName::new(name);
        assert_matches!(token_name.unwrap_err(), TokenNameError::InvalidLength(0));

        // `\u{10FFFF}` is the largest character in unicode set of size 4 bytes
        // the string below is 4 * 8 + 1 = 33 bytes
        let name = format!("{}{}", "\u{10FFFF}".repeat(8), "a");
        let token_name = TokenName::new(&name);
        assert_matches!(token_name.unwrap_err(), TokenNameError::InvalidLength(33));
    }

    #[test]
    fn test_invalid_utf8_bytes() {
        let mut encoded_name = TokenName::try_from("hello world").unwrap();

        // messup the bytes
        encoded_name.0[1][0] = u32::from_ne_bytes([0, 0, 0, 154]).into();

        let err = encoded_name.to_string().unwrap_err();

        assert_matches!(err, TokenNameError::InvalidUtf8Buffer(_));
    }
}
