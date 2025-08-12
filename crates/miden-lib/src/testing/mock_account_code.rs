use miden_objects::account::AccountCode;
use miden_objects::assembly::Library;
use miden_objects::assembly::diagnostics::NamedSource;
use miden_objects::utils::sync::LazyLock;

use crate::transaction::TransactionKernel;

const MOCK_ACCOUNT_CODE: &str = "
    use.miden::account
    use.miden::faucet
    use.miden::tx

    export.::miden::contracts::wallets::basic::receive_asset
    export.::miden::contracts::wallets::basic::move_asset_to_note
    export.::miden::contracts::faucets::basic_fungible::distribute

    # Note: all account's export procedures below should be only called or dyncall'ed, so it
    # is assumed that the operand stack at the beginning of their execution is pad'ed and
    # doesn't have any other valuable information.

    # Stack:  [index, VALUE_TO_SET, pad(11)]
    # Output: [PREVIOUS_STORAGE_VALUE, pad(12)]
    export.set_item
        exec.account::set_item
        # => [V, pad(12)]
    end

    # Stack:  [index, pad(15)]
    # Output: [VALUE, pad(12)]
    export.get_item
        exec.account::get_item
        # => [VALUE, pad(15)]

        # truncate the stack
        movup.8 drop movup.8 drop movup.8 drop
        # => [VALUE, pad(12)]
    end

    # Stack:  [index, KEY, VALUE, pad(7)]
    # Output: [OLD_MAP_ROOT, OLD_MAP_VALUE, pad(8)]
    export.set_map_item
        exec.account::set_map_item
        # => [R', V, pad(8)]
    end

    # Stack:  [index, KEY, pad(11)]
    # Output: [VALUE, pad(12)]
    export.get_map_item
        exec.account::get_map_item
    end

    # Stack:  [pad(16)]
    # Output: [CODE_COMMITMENT, pad(12)]
    export.get_code
        exec.account::get_code_commitment
        # => [CODE_COMMITMENT, pad(12)]
    end

    # Stack:  [pad(16)]
    # Output: [CODE_COMMITMENT, pad(12)]
    export.get_storage_commitment
        exec.account::get_storage_commitment
        # => [STORAGE_COMMITMENT, pad(16)]

        swapw dropw
        # => [STORAGE_COMMITMENT, pad(12)]
    end

    # Stack:  [ASSET, pad(12)]
    # Output: [ASSET', pad(12)]
    export.add_asset
        exec.account::add_asset
        # => [ASSET', pad(12)]
    end

    # Stack:  [ASSET, pad(12)]
    # Output: [ASSET, pad(12)]
    export.remove_asset
        exec.account::remove_asset
        # => [ASSET, pad(12)]
    end

    # Stack:  [pad(16)]
    # Output: [3, pad(12)]
    export.account_procedure_1
        push.1.2 add

        # truncate the stack
        swap drop
    end

    # Stack:  [pad(16)]
    # Output: [1, pad(12)]
    export.account_procedure_2
        push.2.1 sub

        # truncate the stack
        swap drop
    end

    # Stack:  [ASSET, pad(12)]
    # Output: [ASSET, pad(12)]
    export.mint
        exec.faucet::mint
        # => [ASSET, pad(12)]
    end

    # Stack:  [ASSET, pad(12)]
    # Output: [ASSET, pad(12)]
    export.burn
        exec.faucet::burn
        # => [ASSET, pad(12)]
    end
";

static MOCK_ACCOUNT_LIBRARY: LazyLock<Library> = LazyLock::new(|| {
    let source = NamedSource::new("mock::account", MOCK_ACCOUNT_CODE);
    TransactionKernel::assembler()
        .assemble_library([source])
        .expect("mock account code should be valid")
});

// MOCK ACCOUNT CODE EXT
// ================================================================================================

/// Extension trait for [`AccountCode`] to access the mock library.
pub trait MockAccountCodeExt {
    /// Returns the [`Library`] of the mock account under the `mock::account` namespace.
    ///
    /// This account interface wraps most account kernel APIs for testing purposes.
    fn mock_library() -> Library;
}

impl MockAccountCodeExt for AccountCode {
    fn mock_library() -> Library {
        MOCK_ACCOUNT_LIBRARY.clone()
    }
}
