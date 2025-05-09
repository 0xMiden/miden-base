use alloc::string::{String, ToString};

use super::{Felt, TokenSymbolError};

/// Represents a string token symbol (e.g. "POL", "ETH") as a single [`Felt`] value.
///
/// Token Symbols can consists of up to 6 capital Latin characters, e.g. "C", "ETH", "MIDENC".
#[derive(Default, Clone, Copy, Debug)]
pub struct TokenSymbol(Felt);

impl TokenSymbol {
    /// Maximum allowed length of the token string.
    pub const MAX_SYMBOL_LENGTH: usize = 6;

    /// The length of the set of characters that can be used in a token's name.
    pub const ALPHABET_LENGTH: u64 = 26;

    /// The maximum integer value of an encoded [`TokenSymbol`].
    ///
    /// This value encodes the "ZZZZZZ" token symbol.
    pub const MAX_ENCODED_VALUE: u64 = 8031810156;

    /// Creates a new [`TokenSymbol`] instance from the provided token name string.
    ///     
    /// # Errors
    /// Returns an error if:
    /// - The length of the provided string is less than 1 or greater than 6.
    /// - The provided token string contains characters that are not uppercase ASCII.
    pub fn new(symbol: &str) -> Result<Self, TokenSymbolError> {
        let felt = encode_symbol_to_felt(symbol)?;
        Ok(Self(felt))
    }

    /// Returns the token name string from the encoded [`TokenSymbol`] value.
    ///     
    /// # Errors
    /// Returns an error if:
    /// - The encoded value exceeds the maximum value of [`Self::MAX_ENCODED_VALUE`].
    /// - The encoded token string length is less than 1 or greater than 6.
    /// - The encoded token string length is less than the actual string length.
    pub fn to_string(&self) -> Result<String, TokenSymbolError> {
        decode_felt_to_symbol(self.0)
    }
}

impl From<TokenSymbol> for Felt {
    fn from(symbol: TokenSymbol) -> Self {
        symbol.0
    }
}

impl TryFrom<&str> for TokenSymbol {
    type Error = TokenSymbolError;

    fn try_from(symbol: &str) -> Result<Self, Self::Error> {
        TokenSymbol::new(symbol)
    }
}

impl TryFrom<Felt> for TokenSymbol {
    type Error = TokenSymbolError;

    fn try_from(felt: Felt) -> Result<Self, Self::Error> {
        // Check if the felt value is within the valid range
        if felt.as_int() > Self::MAX_ENCODED_VALUE {
            return Err(TokenSymbolError::ValueTooLarge(felt.as_int()));
        }
        Ok(TokenSymbol(felt))
    }
}

// HELPER FUNCTIONS
// ================================================================================================

/// Encodes the provided token symbol string into a single [`Felt`] value.
///
/// The alphabet used in the decoding process consists of the Latin capital letters as defined in
/// the ASCII table, having the length of 26 characters.
///
/// The encoding is performed by multiplying the intermediate encrypted value by the length of the
/// used alphabet and adding the relative index of the character to it. At the end of the encoding
/// process the length of the initial token string is added to the encrypted value.
///
/// Relative character index is computed by subtracting the index of the character "A" (65) from the
/// index of the currently processing character, e.g., `A = 65 - 65 = 0`, `B = 66 - 65 = 1`, `...` ,
/// `Z = 90 - 65 = 25`.
///
/// # Errors
/// Returns an error if:
/// - The length of the provided string is less than 1 or greater than 6.
/// - The provided token string contains characters that are not uppercase ASCII.
fn encode_symbol_to_felt(s: &str) -> Result<Felt, TokenSymbolError> {
    if s.is_empty() || s.len() > TokenSymbol::MAX_SYMBOL_LENGTH {
        return Err(TokenSymbolError::InvalidLength(s.len()));
    } else if s.chars().any(|c| !c.is_ascii_uppercase()) {
        return Err(TokenSymbolError::InvalidCharacter(s.to_string()));
    }

    let mut encoded_value = 0;
    for char in s.chars() {
        let digit = char as u64 - b'A' as u64;
        debug_assert!(digit < TokenSymbol::ALPHABET_LENGTH);
        encoded_value = encoded_value * TokenSymbol::ALPHABET_LENGTH + digit;
    }

    // add token length to the encoded value to be able to decode the exact number of characters
    encoded_value = encoded_value * TokenSymbol::ALPHABET_LENGTH + s.len() as u64;

    Ok(Felt::new(encoded_value))
}

/// Decodes the [`TokenSymbol`] into a token symbol string.
///
/// The alphabet used in the decoding process consists of the Latin capital letters as defined in
/// the ASCII table, having the length of 26 characters.
///
/// The decoding is performed by getting the modulus of the intermediate encrypted value by the
/// length of the used alphabet and then dividing the intermediate value by the length of the
/// alphabet to shift to the next character. At the beginning of the decoding process the length of
/// the initial token string is obtained from the encrypted value. After that the value obtained
/// after taking the modulus represents the relative character index, which then gets converted to
/// the ASCII index.
///
/// Final ASCII character idex is computed by adding the index of the character "A" (65) to the
/// index of the currently processing character, e.g., `A = 0 + 65 = 65`, `B = 1 + 65 = 66`, `...` ,
/// `Z = 25 + 65 = 90`.
///
/// # Errors
/// Returns an error if:
/// - The encoded value exceeds the maximum value of [`TokenSymbol::MAX_ENCODED_VALUE`].
/// - The encoded token string length is less than 1 or greater than 6.
/// - The encoded token string length is less than the actual string length.
fn decode_felt_to_symbol(encoded_felt: Felt) -> Result<String, TokenSymbolError> {
    let encoded_value = encoded_felt.as_int();

    // Check if the encoded value is within the valid range
    if encoded_value > TokenSymbol::MAX_ENCODED_VALUE {
        return Err(TokenSymbolError::ValueTooLarge(encoded_value));
    }

    let mut decoded_string = String::new();
    let mut remaining_value = encoded_value;

    // get the token symbol length
    let token_len = (remaining_value % TokenSymbol::ALPHABET_LENGTH) as usize;
    if token_len == 0 || token_len > TokenSymbol::MAX_SYMBOL_LENGTH {
        return Err(TokenSymbolError::InvalidLength(token_len));
    }
    remaining_value /= TokenSymbol::ALPHABET_LENGTH;

    for _ in 0..token_len {
        let digit = (remaining_value % TokenSymbol::ALPHABET_LENGTH) as u8;
        let char = (digit + b'A') as char;
        decoded_string.insert(0, char);
        remaining_value /= TokenSymbol::ALPHABET_LENGTH;
    }

    // return an error if some data still remains after specified number of characters have been
    // decoded.
    if remaining_value != 0 {
        return Err(TokenSymbolError::DataNotFullyDecoded);
    }

    Ok(decoded_string)
}

// TESTS
// ================================================================================================

#[cfg(test)]
mod test {
    use assert_matches::assert_matches;

    use super::{
        Felt, TokenSymbol, TokenSymbolError, decode_felt_to_symbol, encode_symbol_to_felt,
    };

    #[test]
    fn test_token_symbol_decoding_encoding() {
        let symbols = vec!["AAAAAA", "AAAAB", "AAAC", "ABC", "BC", "A", "B", "ZZZZZZ"];
        for symbol in symbols {
            let token_symbol = TokenSymbol::try_from(symbol).unwrap();
            let decoded_symbol = TokenSymbol::to_string(&token_symbol).unwrap();
            assert_eq!(symbol, decoded_symbol);
        }

        let symbol = "";
        let felt = encode_symbol_to_felt(symbol);
        assert_matches!(felt.unwrap_err(), TokenSymbolError::InvalidLength(0));

        let symbol = "ABCDEFG";
        let felt = encode_symbol_to_felt(symbol);
        assert_matches!(felt.unwrap_err(), TokenSymbolError::InvalidLength(7));

        let symbol = "$$$";
        let felt = encode_symbol_to_felt(symbol);
        assert_matches!(felt.unwrap_err(), TokenSymbolError::InvalidCharacter(s) if s == *"$$$");

        let symbol = "ABCDEF";
        let token_symbol = TokenSymbol::try_from(symbol);
        assert!(token_symbol.is_ok());
        let token_symbol_felt: Felt = token_symbol.unwrap().into();
        assert_eq!(token_symbol_felt, encode_symbol_to_felt(symbol).unwrap());
    }

    /// Checks that if the encoded length of the token is less than the actual number of token
    /// characters, [decode_felt_to_symbol] procedure should return the
    /// [TokenSymbolError::DataNotFullyDecoded] error.
    #[test]
    fn test_invalid_token_len() {
        // encoded value of this token has `6` as the length of the initial token string
        let encoded_symbol = TokenSymbol::try_from("ABCDEF").unwrap();

        // decrease encoded length by, for example, `3`
        let invalid_encoded_symbol_u64 = Felt::from(encoded_symbol).as_int() - 3;

        // check that `decode_felt_to_symbol()` procedure returns an error in attempt to create a
        // token from encoded token with invalid length
        let err = decode_felt_to_symbol(Felt::new(invalid_encoded_symbol_u64)).unwrap_err();
        assert_matches!(err, TokenSymbolError::DataNotFullyDecoded);
    }

    /// Utility test just to make sure that the [TokenSymbol::MAX_ENCODED_VALUE] constant still
    /// represents the maximum possible encoded value.
    #[test]
    fn test_token_symbol_max_value() {
        let token_symbol = TokenSymbol::try_from("ZZZZZZ").unwrap();
        assert_eq!(Felt::from(token_symbol).as_int(), TokenSymbol::MAX_ENCODED_VALUE);
    }
}
