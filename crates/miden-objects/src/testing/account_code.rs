use assembly::{Assembler, Library};

use crate::account::{AccountCode, AccountComponent, AccountType};
use crate::testing::account_component::{AccountMockComponent, NoopAuthComponent};

pub const CODE: &str = "
    export.foo
        push.1 push.2 mul
    end

    export.bar
        push.1 push.2 add
    end
";

pub(crate) const MOCK_ACCOUNT_CODE: &str = "
    use.miden::account
    use.miden::faucet
    use.miden::tx

    export.::miden::contracts::wallets::basic::receive_asset
    export.::miden::contracts::wallets::basic::move_asset_to_note
    export.::miden::contracts::faucets::basic_fungible::distribute

    ### Note: all account's export procedures below should be only called or dyncall'ed, so it 
    ### is assumed that the operand stack at the beginning of their execution is pad'ed and 
    ### doesn't have any other valuable information.

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

// ACCOUNT ASSEMBLY CODE
// ================================================================================================
impl AccountCode {
    /// Creates a mock [Library] which can be used to assemble programs and as a library to create a
    /// mock [AccountCode] interface. Transaction and note scripts that make use of this interface
    /// should be assembled with this.
    pub fn mock_library(assembler: Assembler) -> Library {
        AccountMockComponent::new_with_empty_slots(assembler).unwrap().into()
    }

    /// Creates a mock [AccountCode] with default assembler and mock code
    pub fn mock() -> AccountCode {
        let component = AccountComponent::compile(CODE, Assembler::default(), vec![])
            .unwrap()
            .with_supports_all_types();

        Self::from_components(
            &[NoopAuthComponent.into(), component],
            AccountType::RegularAccountUpdatableCode,
        )
        .unwrap()
    }
}
