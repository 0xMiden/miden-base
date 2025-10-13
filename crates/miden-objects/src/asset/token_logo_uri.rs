use alloc::string::{String, ToString};

use miden_core::FieldElement;

use super::{Felt, TokenLogoURIError, Word};

const LOGO_URI_WORD_SIZE: usize = 8;

/// Represents a token logo URI (e.g., `\[https://www.circle.com/hubfs/Brand/USDC/USDC_icon_32x32.png\]`) as a fixed-size array of [`Word`]s of length `LOGO_URI_WORD_SIZE`.
///
/// The logo URI can contain up to 128 bytes of UTF-8 encoded characters.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct TokenLogoURI([Word; LOGO_URI_WORD_SIZE]);

impl TokenLogoURI {
    /// Maximum allowed length of logo uri
    pub const MAX_LOGO_URI_LEN: usize = 128;

    /// Creates a new [`TokenLogoURI`] instance from the provided logo URI string.
    ///
    /// # Errors
    /// Returns an error if:
    /// - The length of the provided string is 0 or greater than [`TokenLogoURI::MAX_LOGO_URI_LEN`].
    pub fn new(logo_uri: &str) -> Result<Self, TokenLogoURIError> {
        let words = encode_logo_to_words(logo_uri)?;
        Ok(TokenLogoURI(words))
    }

    /// Returns the corresponding string from the encoded [`TokenLogoURI`] value.
    ///
    /// # Errors
    /// Returns an error if:
    /// - The encoded value decodes to invalid utf8
    pub fn to_string(&self) -> Result<String, TokenLogoURIError> {
        decode_logo_from_words(&self.0)
    }

    /// Returns the underlying slice of [`Word`]s representing the logo URI.
    pub fn as_slice(&self) -> [Word; LOGO_URI_WORD_SIZE] {
        self.0
    }
}

impl TryFrom<&str> for TokenLogoURI {
    type Error = TokenLogoURIError;

    fn try_from(logo_uri: &str) -> Result<Self, Self::Error> {
        TokenLogoURI::new(logo_uri)
    }
}

impl TryFrom<[Word; LOGO_URI_WORD_SIZE]> for TokenLogoURI {
    type Error = TokenLogoURIError;

    fn try_from(words: [Word; LOGO_URI_WORD_SIZE]) -> Result<Self, Self::Error> {
        decode_logo_from_words(&words)?;
        Ok(TokenLogoURI(words))
    }
}

// HELPER FUNCTIONS
// ============================================================================================

/// Encodes the provided logo URI string into a fixed-size array of [`Word`]s of length
/// [`LOGO_URI_WORD_SIZE`].
///
/// It converts `logo_uri` to its bytes representation and then into corresponding words.
/// [b0, b1, b2 ... b127] => [ [{b0, b1, b2, b3}  .....   {b12, b13, b14, b15}] , ... ]
///                            |------------------- 1 word -------------------|
///
/// If the length of the string is smaller than 128, it pads the bytes representation to 128 bytes.
///
/// # Errors
/// Returns an error if:
/// - The length of the provided string is 0 or greater than [`TokenLogoURI::MAX_LOGO_URI_LEN`].
fn encode_logo_to_words(logo_uri: &str) -> Result<[Word; LOGO_URI_WORD_SIZE], TokenLogoURIError> {
    if logo_uri.is_empty() || logo_uri.len() > TokenLogoURI::MAX_LOGO_URI_LEN {
        return Err(TokenLogoURIError::InvalidLength(logo_uri.len()));
    }

    let mut bytes = logo_uri.as_bytes().to_vec();
    bytes.resize(128, 0);

    let mut words = [Word::empty(); LOGO_URI_WORD_SIZE];

    for i in 0..LOGO_URI_WORD_SIZE {
        let word_bytes = &bytes[16 * i..16 * (i + 1)].try_into().unwrap();
        words[i] = encode_bytes_to_word(word_bytes);
    }
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

/// Decodes the provided array of [`Word`]s of length [`LOGO_URI_WORD_SIZE`] back into a string.
///
/// It decodes each word into its corresponding bytes and then constructs the string from the bytes
/// Null bytes at the end are trimmed in case the original buffer was padded.
///
/// # Errors
/// Returns an error if:
/// - The decoded bytes do not form a valid UTF-8 string.
fn decode_logo_from_words(words: &[Word; LOGO_URI_WORD_SIZE]) -> Result<String, TokenLogoURIError> {
    let mut buf = [0u8; LOGO_URI_WORD_SIZE * 16];

    for i in 0..LOGO_URI_WORD_SIZE {
        buf[16 * i..16 * (i + 1)].copy_from_slice(&decode_word_to_bytes(&words[i]));
    }

    String::from_utf8(buf.to_vec())
        .map(|str| str.trim_end_matches('\0').to_string())
        .map_err(TokenLogoURIError::InvalidUtf8Buffer)
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

    use miden_core::assert_matches;

    use super::{TokenLogoURI, TokenLogoURIError};
    #[test]
    fn test_token_logo_uri_encoding_decoding() {
        let uris = vec![
            "https://logo.com/logo.png",
            "https://a.com/b.png",
            "https://www.circle.com/hubfs/Brand/USDC/USDC_icon_32x32.png",
        ];
        for uri in uris {
            let token_logo_uri = TokenLogoURI::try_from(uri).unwrap();
            let decoded_uri = token_logo_uri.to_string().unwrap();
            assert_eq!(decoded_uri, uri);
        }

        let logo_uri = "";
        let token_logo_uri = TokenLogoURI::new(logo_uri);
        assert_matches!(token_logo_uri.unwrap_err(), TokenLogoURIError::InvalidLength(0));

        // create a string of size 32 * 4 + 1 = 129
        let logo_uri = format!("{}{}", "\u{10FFFF}".repeat(32), "a");
        let token_logo_uri = TokenLogoURI::new(&logo_uri);
        assert_matches!(token_logo_uri.unwrap_err(), TokenLogoURIError::InvalidLength(129));
    }

    #[test]
    fn test_invalid_utf8_bytes() {
        let uri = "https://www.circle.com/hubfs/Brand/USDC/USDC_icon_32x32.png";

        let mut token_logo_uri = TokenLogoURI::try_from(uri).unwrap();

        // messup the bytes
        token_logo_uri.0[3][1] = u32::from_ne_bytes([0, 0, 0, 154]).into();

        assert_matches!(
            token_logo_uri.to_string().unwrap_err(),
            TokenLogoURIError::InvalidUtf8Buffer(_)
        );
    }
}
