# Changelog

## 0.10.0 (TBD)

- [BREAKING] Refactor `NoteTag` to an enum (#1322).
- Add `bench-prover` crate to benchmark proving times (#1378).
- [BREAKING] Remove `AccountIdAnchor` from account ID generation process (#1391).
- Allow NOOP transactions and state-updating transactions against the same account in the same block (#1393).
- Implement map based on a sorted linked list in transaction kernel library (#1396, #1428, #1478).
- Add P2IDE standard note (#1421).
- Added shutdown configuration options to the `miden-proving-service` proxy (#1405).
- [BREAKING] Implement transaction script arguments for the `TransactionScript` (#1406).
- Add support for workers configuration in the proxy with environment variables (#1412).
- Implement Display for `NoteType` (#1420).
- [BREAKING] Remove `NoteExecutionMode` from `from_account_id` (#1422).
- [BREAKING] Refactor transaction kernel advice inputs (#1425).
- [BREAKING] Move transaction script argument from `TransactionScript` to `TransactionArgs`. (#1426).
- [BREAKING] Remove transaction inputs from `TransactionScript`. (#1426).
- Remove miden-proving-service binary crate and miden-proving-service-client crate (#1427).
- Remove doc update checks on CI (#1435).
- [BREAKING] Introduce `ScriptMastForestStore` and refactor MAST forest provisioning in the `TransactionExecutor` (#1438).
- [BREAKING] Allow list of keys in `AccountFile` (#1451).
- [BREAKING] `TransactionHost::new` now expects `&PartialAccount` instead `AccountHeader` (#1452).
- Load account and input notes advice maps into the advice provider before executing them (#1452).
- Support private accounts in `MockChain` (#1453).
- Improve error message quality in `CodeExecutor::run` and `TransactionContext::execute_code` (#1458).
- [BREAKING] Forbid the execution of the empty transactions (#1459).
- Temporarily bump ACCOUNT_UPDATE_MAX_SIZE to 256 KiB for compiler testing (#1464).
- [BREAKING] `TransactionExecutor` now holds plain references instead of `Arc` for its trait objects (#1469).
- [BREAKING] Implemented in-kernel account delta tracking (#1471, #1404, #1460, #1481, #1491).
- [BREAKING] Store account ID in account delta (#1493).
- [BREAKING] Remove P2IDR and replace with P2IDE (#1483).
- [BREAKING] Refactor nonce in delta from `Option<Felt>` to `Felt` (#1492).
- Normalize account deltas to avoid including no-op updates (#1496).
- [BREAKING] Refactor transaction to output `ACCOUNT_UPDATE_COMMITMENT` (#1500).
- Added `Note::is_network_note()` accessor (#1485).
- [BREAKING] Update handling of the shared modules (#1490).
- Added a new constructor for `TransactionExecutor` that accepts `ExecutionOptions` (#1502).
- [BREAKING] Introduce errors in `MockChain` API (#1508).
- [BREAKING] `TransactionAdviceInputs` cannot return `Err` anymore (#1517).
- Implement serialization for `LexicographicWord` (#1524).
- Add `with_auth_component` to `AccountBuilder` (#1480).
- [BREAKING] Refactor account authentication to require a procedure containing `auth__` in its name (#1480).

## 0.9.5 (2025-06-20) - `miden-lib` crate only

- Added `symbol()`, `decimals()`, and `max_supply()` accessors to the `TokenSymbol` struct.

## 0.9.4 (2025-06-12)

- Refactor proving service client errors (#1448)

## 0.9.3 (2025-06-12)

- Add TLS support to `miden-proving-service-client` (#1447)

## 0.9.2 (2025-06-10)

- Refreshed Cargo.lock file.

## 0.9.1 (2025-05-30)

### Fixes

- Expose types used in public APIs (#1385).
- Version check always fails in proxy (#1407).

## 0.9.0 (2025-05-20)

### Features

- Added pretty print for `AccountCode` (#1273).
- Add iterators over concrete asset types in `NoteAssets` (#1346).
- Add the ability to create `BasicFungibleFaucet` from `Account` (#1376).

### Fixes

- [BREAKING] Hash keys in storage maps before insertion into the SMT (#1250).
- Fix error when creating accounts with empty storage (#1307).
- [BREAKING] Move the number of note inputs to the separate memory address (#1327).
- [BREAKING] Change Token Symbol encoding (#1334).

### Changes

- [BREAKING] Refactored how foreign account inputs are passed to `TransactionExecutor` (#1229).
- [BREAKING] Add `TransactionHeader` and include it in batches and blocks (#1247).
- Add `AccountTree` and `PartialAccountTree` wrappers and enforce ID prefix uniqueness (#1254, #1301).
- Added getter for proof security level in `ProvenBatch` and `ProvenBlock` (#1259).
- [BREAKING] Replaced the `ProvenBatch::new_unchecked` with the `ProvenBatch::new` method to initialize the struct with validations (#1260).
- [BREAKING] Add `AccountStorageMode::Network` for network accounts (#1275, #1349).
- Added support for environment variables to set up the `miden-proving-service` worker (#1281).
- Added field identifier structs for component metadata (#1292).
- Move `NullifierTree` and `BlockChain` from node to base (#1304).
- Rename `ChainMmr` to `PartialBlockchain` (#1305).
- Add safe `PartialBlockchain` constructor (#1308).
- [BREAKING] Move `MockChain` and `TransactionContext` to new `miden-testing` crate (#1309).
- [BREAKING] Add support for private notes in `MockChain` (#1310).
- Generalized account-related inputs to the transaction kernel (#1311).
- [BREAKING] Refactor `MockChain` to use batch and block provers (#1315).
- [BREAKING] Upgrade VM to 0.14 and refactor transaction kernel error extraction (#1353).
- [BREAKING] Update MSRV to 1.87.

## 0.8.3 (2025-04-22) - `miden-proving-service` crate only

### Fixes

- Version check always fails (#1300).

## 0.8.2 (2025-04-18) - `miden-proving-service` crate only

### Changes

- Added a retry strategy for worker's health check (#1255).
- Added a status endpoint for the `miden-proving-service` worker and proxy (#1255).

## 0.8.1 (2025-03-26) - `miden-objects` and `miden-tx` crates only.

### Changes

- [BREAKING] Changed `TransactionArgs` API to accept `AsRef<NoteRecipient>` for extending the advice map in relation to output notes (#1251).

## 0.8.0 (2025-03-21)

### Features

- Added an endpoint to the `miden-proving-service` to update the workers (#1107).
- [BREAKING] Added the `get_block_timestamp` procedure to the `miden` library (#1138).
- Implemented `AccountInterface` structure (#1171).
- Implement user-facing bech32 encoding for `AccountId`s (#1185).
- Implemented `execute_tx_view_script` procedure for the `TransactionExecutor` (#1197).
- Enabled nested FPI calls (#1227).
- Implement `check_notes_consumability` procedure for the `TransactionExecutor` (#1269).

### Changes

- [BREAKING] Moved `generated` module from `miden-proving-service-client` crate to `tx_prover::generated` hierarchy (#1102).
- Renamed the protobuf file of the transaction prover to `tx_prover.proto` (#1110).
- [BREAKING] Renamed `AccountData` to `AccountFile` (#1116).
- Implement transaction batch prover in Rust (#1112).
- Added the `is_non_fungible_asset_issued` procedure to the `miden` library (#1125).
- [BREAKING] Refactored config file for `miden-proving-service` to be based on environment variables (#1120).
- Added block number as a public input to the transaction kernel. Updated prologue logic to validate the global input block number is consistent with the commitment block number (#1126).
- Made NoteFile and AccountFile more consistent (#1133).
- [BREAKING] Implement most block constraints in `ProposedBlock` (#1123, #1141).
- Added serialization for `ProposedBatch`, `BatchId`, `BatchNoteTree` and `ProvenBatch` (#1140).
- Added `prefix` to `Nullifier` (#1153).
- [BREAKING] Implemented a `RemoteBatchProver`. `miden-proving-service` workers can prove batches (#1142).
- [BREAKING] Implement `LocalBlockProver` and rename `Block` to `ProvenBlock` (#1152, #1168, #1172).
- [BREAKING] Added native types to `AccountComponentTemplate` (#1124).
- Implemented `RemoteBlockProver`. `miden-proving-service` workers can prove blocks (#1169).
- Used `Smt::with_entries` to error on duplicates in `StorageMap::with_entries` (#1167).
- [BREAKING] Added `InitStorageData::from_toml()`, improved storage entry validations in `AccountComponentMetadata` (#1170).
- [BREAKING] Rework miden-lib error codes into categories (#1196).
- [BREAKING] Moved the `TransactionScriptBuilder` from `miden-client` to `miden-base` (#1206).
- [BREAKING] Enable timestamp customization on `MockChain::seal_block` (#1208).
- [BREAKING] Renamed constants and comments: `OnChain` -> `Public` and `OffChain` -> `Private` (#1218).
- [BREAKING] Replace "hash" with "commitment" in `BlockHeader::{prev_hash, chain_root, kernel_root, tx_hash, proof_hash, sub_hash, hash}` (#1209, #1221, #1226).
- [BREAKING] Incremented minimum supported Rust version to 1.85.
- [BREAKING] Change advice for Falcon signature verification (#1183).
- Added `info` log level by default in the proving service (#1200).
- Made Prometheus metrics optional in the proving service proxy via the `enable_metrics` configuration option (#1200).
- Improved logging in the proving service proxy for better diagnostics (#1200).
- Fixed issues with the proving service proxy's signal handling and port binding (#1200).
- [BREAKING] Simplified worker update configuration by using a single URL parameter instead of separate host and port (#1249).

## 0.7.2 (2025-01-28) - `miden-objects` crate only

### Changes

- Added serialization for `ExecutedTransaction` (#1113).

## 0.7.1 (2025-01-24) - `miden-objects` crate only

### Fixes

- Added missing doc comments (#1100).
- Fixed setting of supporting types when instantiating `AccountComponent` from templates (#1103).

## 0.7.0 (2025-01-22)

### Highlights

- [BREAKING] Extend `AccountId` to two `Felt`s and require block hash in derivation (#982).
- Introduced `AccountComponentTemplate` with TOML serialization and templating (#1015, #1027).
- Introduce `AccountIdBuilder` to simplify `AccountId` generation in tests (#1045).
- [BREAKING] Migrate to the element-addressable memory (#1084).

### Changes

- Implemented serialization for `AccountHeader` (#996).
- Updated Pingora crates to 0.4 and added polling time to the configuration file (#997).
- Added support for `miden-tx-prover` proxy to update workers on a running proxy (#989).
- Refactored `miden-tx-prover` proxy load balancing strategy (#976).
- [BREAKING] Implemented better error display when queues are full in the prover service (#967).
- [BREAKING] Removed `AccountBuilder::build_testing` and make `Account::initialize_from_components` private (#969).
- [BREAKING] Added error messages to errors and implement `core::error::Error` (#974).
- Implemented new `digest!` macro (#984).
- Added Format Guidebook to the `miden-lib` crate (#987).
- Added conversion from `Account` to `AccountDelta` for initial account state representation as delta (#983).
- [BREAKING] Added `miden::note::get_script_hash` procedure (#995).
- [BREAKING] Refactor error messages in `miden-lib` and `miden-tx` and use `thiserror` 2.0 (#1005, #1090).
- Added health check endpoints to the prover service (#1006).
- Removed workers list from the proxy configuration file (#1018).
- Added tracing to the `miden-tx-prover` CLI (#1014).
- Added metrics to the `miden-tx-prover` proxy (#1017).
- Implemented `to_hex` for `AccountIdPrefix` and `epoch_block_num` for `BlockHeader` (#1039).
- [BREAKING] Updated the names and values of the kernel procedure offsets and corresponding kernel procedures (#1037).
- Introduce `AccountIdError` and make account ID byte representations (`u128`, `[u8; 15]`) consistent (#1055).
- Refactor `AccountId` and `AccountIdPrefix` into version wrappers (#1058).
- Remove multi-threaded account seed generation due to single-threaded generation being faster (#1061).
- Made `AccountIdError` public (#1067).
- Made `BasicFungibleFaucet::MAX_DECIMALS` public (#1063).
- [BREAKING] Removed `miden-tx-prover` crate and created `miden-proving-service` and `miden-proving-service-client` (#1047).
- Removed deduplicate `masm` procedures across kernel and miden lib to a shared `util` module (#1070).
- [BREAKING] Added `BlockNumber` struct (#1043, #1080, #1082).
- [BREAKING] Removed `GENESIS_BLOCK` public constant (#1088).
- Add CI check for unused dependencies (#1075).
- Added storage placeholder types and support for templated map (#1074).
- [BREAKING] Move crates into `crates/` and rename plural modules to singular (#1091).

## 0.6.2 (2024-11-20)

- Avoid writing to the filesystem during docs.rs build (#970).

## 0.6.1 (2024-11-08)

### Features

- [BREAKING] Added CLI for the transaction prover services both the workers and the proxy (#955).

### Fixes

- Fixed `AccountId::new_with_type_and_mode()` (#958).
- Updated the ABI for the assembly procedures (#971).

## 0.6.0 (2024-11-05)

### Features

- Created a proving service that receives `TransactionWitness` and returns the proof using gRPC (#881).
- Implemented ability to invoke procedures against the foreign account (#882, #890, #896).
- Implemented kernel procedure to set transaction expiration block delta (#897).
- [BREAKING] Introduce a new way to build `Account`s from `AccountComponent`s (#941).
- [BREAKING] Introduce an `AccountBuilder` (#952).

### Changes

- [BREAKING] Changed `TransactionExecutor` and `TransactionHost` to use trait objects (#897).
- Made note scripts public (#880).
- Implemented serialization for `TransactionWitness`, `ChainMmr`, `TransactionInputs` and `TransactionArgs` (#888).
- [BREAKING] Renamed the `TransactionProver` struct to `LocalTransactionProver` and added the `TransactionProver` trait (#865).
- Implemented `Display`, `TryFrom<&str>` and `FromStr` for `AccountStorageMode` (#861).
- Implemented offset based storage access (#843).
- [BREAKING] `AccountStorageType` enum was renamed to `AccountStorageMode` along with its variants (#854).
- [BREAKING] `AccountStub` structure was renamed to `AccountHeader` (#855).
- [BREAKING] Kernel procedures now have to be invoked using `dynexec` instruction (#803).
- Refactored `AccountStorage` from `Smt` to sequential hash (#846).
- [BREAKING] Refactored batch/block note trees (#834).
- Set all procedures storage offsets of faucet accounts to `1` (#875).
- Added `AccountStorageHeader` (#876).
- Implemented generation of transaction kernel procedure hashes in build.rs (#887).
- [BREAKING] `send_asset` procedure was removed from the basic wallet (#829).
- [BREAKING] Updated limits, introduced additional limits (#889).
- Introduced `AccountDelta` maximum size limit of 32 KiB (#889).
- [BREAKING] Moved `MAX_NUM_FOREIGN_ACCOUNTS` into `miden-objects` (#904).
- Implemented `storage_size`, updated storage bounds (#886).
- [BREAKING] Auto-generate `KERNEL_ERRORS` list from the transaction kernel's MASM files and rework error constant names (#906).
- Implement `Serializable` for `FungibleAsset` (#907).
- [BREAKING] Changed `TransactionProver` trait to be `maybe_async_trait` based on the `async` feature (#913).
- [BREAKING] Changed type of `EMPTY_STORAGE_MAP_ROOT` constant to `RpoDigst`, which references constant from `miden-crypto` (#916).
- Added `RemoteTransactionProver` struct to `miden-tx-prover` (#921).
- [BREAKING] Migrated to v0.11 version of Miden VM (#929).
- Added `total_cycles` and `trace_length` to the `TransactionMeasurements` (#953).
- Added ability to load libraries into `TransactionExecutor` and `LocalTransactionProver` (#954).

## 0.5.1 (2024-08-28) - `miden-objects` crate only

- Implemented `PrettyPrint` and `Display` for `NoteScript`.

## 0.5.0 (2024-08-27)

### Features

- [BREAKING] Increase of nonce does not require changes in account state any more (#796).
- Changed `AccountCode` procedures from merkle tree to sequential hash + added storage_offset support (#763).
- Implemented merging of account deltas (#797).
- Implemented `create_note` and `move_asset_into_note` basic wallet procedures (#808).
- Made `miden_lib::notes::build_swap_tag()` function public (#817).
- [BREAKING] Changed the `NoteFile::NoteDetails` type to struct and added a `after_block_num` field (#823).

### Changes

- Renamed "consumed" and "created" notes into "input" and "output" respectively (#791).
- [BREAKING] Renamed `NoteType::OffChain` into `NoteType::Private`.
- [BREAKING] Renamed public accessors of the `Block` struct to match the updated fields (#791).
- [BREAKING] Changed the `TransactionArgs` to use `AdviceInputs` (#793).
- Setters in `memory` module don't drop the setting `Word` anymore (#795).
- Added `CHANGELOG.md` warning message on CI (#799).
- Added high-level methods for `MockChain` and related structures (#807).
- [BREAKING] Renamed `NoteExecutionHint` to `NoteExecutionMode` and added new `NoteExecutionHint` to `NoteMetadata` (#812, #816).
- [BREAKING] Changed the interface of the `miden::tx::add_asset_to_note` (#808).
- [BREAKING] Refactored and simplified `NoteOrigin` and `NoteInclusionProof` structs (#810, #814).
- [BREAKING] Refactored account storage and vault deltas (#822).
- Added serialization and equality comparison for `TransactionScript` (#824).
- [BREAKING] Migrated to Miden VM v0.10 (#826).
- Added conversions for `NoteExecutionHint` (#827).
- [BREAKING] Removed `serde`-based serialization from `miden-object` structs (#838).

## 0.4.0 (2024-07-03)

### Features

- [BREAKING] Introduce `OutputNote::Partial` variant (#698).
- [BREAKING] Added support for input notes with delayed verification of inclusion proofs (#724, #732, #759, #770, #772).
- Added new `NoteFile` object to represent serialized notes (#721).
- Added transaction IDs to the `Block` struct (#734).
- Added ability for users to set the aux field when creating a note (#752).

### Enhancements

- Replaced `cargo-make` with just `make` for running tasks (#696).
- [BREAKING] Split `Account` struct constructor into `new()` and `from_parts()` (#699).
- Generalized `build_recipient_hash` procedure to build recipient hash for custom notes (#706).
- [BREAKING] Changed the encoding of inputs notes in the advice map for consumed notes (#707).
- Created additional `emit` events for kernel related `.masm` procedures (#708).
- Implemented `build_recipient_hash` procedure to build recipient hash for custom notes (#710).
- Removed the `mock` crate in favor of having mock code behind the `testing` flag in remaining crates (#711).
- [BREAKING] Created `auth` module for `TransactionAuthenticator` and other related objects (#714).
- Added validation for the output stack to make sure it was properly cleaned (#717).
- Made `DataStore` conditionally async using `winter-maybe-async` (#725).
- Changed note pointer from Memory `note_ptr` to `note_index` (#728).
- [BREAKING] Changed rng to mutable reference in note creation functions (#733).
- [BREAKING] Replaced `ToNullifier` trait with `ToInputNoteCommitments`, which includes the `note_id` for delayed note authentication (#732).
- Added `Option<NoteTag>`to `NoteFile` (#741).
- Fixed documentation and added `make doc` CI job (#746).
- Updated and improved [.pre-commit-config.yaml](.pre-commit-config.yaml) file (#748).
- Created `get_serial_number` procedure to get the serial num of the currently processed note (#760).
- [BREAKING] Added support for conversion from `Nullifier` to `InputNoteCommitment`, commitment header return reference (#774).
- Added `compute_inputs_hash` procedure for hash computation of the arbitrary number of note inputs (#750).

## 0.3.1 (2024-06-12)

- Replaced `cargo-make` with just `make` for running tasks (#696).
- Made `DataStore` conditionally async using `winter-maybe-async` (#725)
- Fixed `StorageMap`s implementation and included into apply_delta (#745)

## 0.3.0 (2024-05-14)

- Introduce the `miden-bench-tx` crate used for transactions benchmarking (#577).
- [BREAKING] Removed the transaction script root output from the transaction kernel (#608).
- [BREAKING] Refactored account update details, moved `Block` to `miden-objects` (#618, #621).
- [BREAKING] Made `TransactionExecutor` generic over `TransactionAuthenticator` (#628).
- [BREAKING] Changed type of `version` and `timestamp` fields to `u32`, moved `version` to the beginning of block header (#639).
- [BREAKING] Renamed `NoteEnvelope` into `NoteHeader` and introduced `NoteDetails` (#664).
- [BREAKING] Updated `create_swap_note()` procedure to return `NoteDetails` and defined SWAP note tag format (#665).
- Implemented `OutputNoteBuilder` (#669).
- [BREAKING] Added support for full details of private notes, renamed `OutputNote` variants and changed their meaning (#673).
- [BREAKING] Added `add_asset_to_note` procedure to the transaction kernel (#674).
- Made `TransactionArgs::add_expected_output_note()` more flexible (#681).
- [BREAKING] Enabled support for notes without assets and refactored `create_note` procedure in the transaction kernel (#686).

## 0.2.3 (2024-04-26) - `miden-tx` crate only

- Fixed handling of debug mode in `TransactionExecutor` (#627)

## 0.2.2 (2024-04-23) - `miden-tx` crate only

- Added `with_debug_mode()` methods to `TransactionCompiler` and `TransactionExecutor` (#562).

## 0.2.1 (2024-04-12)

- [BREAKING] Return a reference to `NoteMetadata` from output notes (#593).
- Add more type conversions for `NoteType` (#597).
- Fix note input padding for expected output notes (#598).

## 0.2.0 (2024-04-11)

- [BREAKING] Implement support for public accounts (#481, #485, #538).
- [BREAKING] Implement support for public notes (#515, #540, #572).
- Improved `ProvenTransaction` validation (#532).
- [BREAKING] Updated `no-std` setup (#533).
- Improved `ProvenTransaction` serialization (#543).
- Implemented note tree wrapper structs (#560).
- [BREAKING] Migrated to v0.9 version of Miden VM (#567).
- [BREAKING] Added account storage type parameter to `create_basic_wallet` and `create_basic_fungible_faucet` (miden-lib
  crate only) (#587).
- Removed serialization of source locations from account code (#590).

## 0.1.1 (2024-03-07) - `miden-objects` crate only

- Added `BlockHeader::mock()` method (#511)

## 0.1.0 (2024-03-05)

- Initial release.
