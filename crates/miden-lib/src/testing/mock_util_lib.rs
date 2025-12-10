use miden_objects::assembly::Library;
use miden_objects::utils::sync::LazyLock;

use crate::utils::ProtocolAssembler;

const MOCK_UTIL_LIBRARY_CODE: &str = "
    use.miden::output_note

    # Inputs:  []
    # Outputs: [note_idx]
    export.create_random_note
        push.1.2.3.4           # = RECIPIENT
        push.1                 # = NoteExecutionHint::Always
        push.2                 # = NoteType::Private
        push.0                 # = aux
        push.0xc0000000        # = NoteTag::LocalAny
        # => [tag, aux, note_type, execution_hint, RECIPIENT, pad(8)]

        exec.output_note::create
        # => [note_idx]
    end

    # Inputs:  [ASSET]
    # Outputs: []
    export.create_random_note_with_asset
        exec.create_random_note
        # => [note_idx, ASSET]

        movdn.4
        # => [ASSET, note_idx]

        exec.output_note::add_asset
        # => []
    end
";

static MOCK_UTIL_LIBRARY: LazyLock<Library> = LazyLock::new(|| {
    ProtocolAssembler::new(false)
        .compile_component_code("mock::util", MOCK_UTIL_LIBRARY_CODE)
        .expect("mock util library should be valid")
        .into_library()
});

/// Returns the mock test [`Library`] under the `mock::util` namespace.
///
/// This provides convenient wrappers for testing purposes.
pub fn mock_util_library() -> Library {
    MOCK_UTIL_LIBRARY.clone()
}
