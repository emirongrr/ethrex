use crate::error::MempoolError;
use ethereum_rust_core::{
    types::{BlockHeader, ChainConfig, Transaction, MAX_INITCODE_SIZE},
    H256,
};
use ethereum_rust_storage::Store;

pub fn add_transaction(transaction: Transaction, store: Store) -> Result<H256, MempoolError> {
    // Validate transaction
    validate_transaction(&transaction, store.clone())?;

    let hash = transaction.compute_hash();

    // Add transaction to storage
    store.add_transaction_to_pool(hash, transaction)?;

    Ok(hash)
}

pub fn get_transaction(hash: H256, store: Store) -> Result<Option<Transaction>, MempoolError> {
    Ok(store.get_transaction_from_pool(hash)?)
}

// Gas cost for each non zero byte on transaction data
pub const TX_DATA_NON_ZERO_GAS: u64 = 68;
// Gas cost for each non zero byte on transaction data, modified on [EIP-2028](https://github.com/ethereum/EIPs/blob/master/EIPS/eip-2028.md)
pub const TX_DATA_NON_ZERO_GAS_EIP2028: u64 = 16;

// Minimum base fee per blob [EIP-4844](https://eips.ethereum.org/EIPS/eip-4844)
pub const MIN_BASE_FEE_PER_BLOB_GAS: u64 = 1;

//TODO: THIS ALL SHOULD BE MOVED TO chain/constants

// YELLOW PAPER CONSTANTS

/// Base gas cost for each non contract creating transaction
pub const TX_GAS_COST: u64 = 21000;

/// Base gas cost for each contract creating transaction
pub const TX_CREATE_GAS_COST: u64 = 53000;

// Gas cost for each zero byte on transaction data
pub const TX_DATA_ZERO_GAS_COST: u64 = 4;

// Gas cost for each init code word on transaction data
pub const TX_INIT_CODE_WORD_GAS_COST: u64 = 2;

// Gas cost for each init code word on transaction data
pub const TX_ACCESS_LIST_ADDRESS_GAS: u64 = 2400;

// Gas cost for each init code word on transaction data
pub const TX_ACCESS_LIST_STORAGE_KEY_GAS: u64 = 1900;

/*

SOME VALIDATIONS THAT WE COULD INCLUDE

Stateless validations

1. This transaction is valid on current mempool
    -> Depends on mempool transaction filtering logic
2. Ensure the maxPriorityFeePerGas is high enough to cover the requirement of the calling pool (the minimum to be included in)
    -> Depends on mempool transaction filtering logic
3. Transaction's encoded size is smaller than maximum allowed
    -> I think that this is not in the spec, but it may be a good idea
4. Make sure the transaction is signed properly
5. Ensure a Blob Transaction comes with its sidecar:
  1. Validate number of BlobHashes is positive
  2. Validate number of BlobHashes is less than the maximum allowed per block,
     which may be computed as `maxBlobGasPerBlock / blobTxBlobGasPerBlob`
  3. Ensure number of BlobHashes is equal to:
    - The number of blobs
    - The number of commitments
    - The number of proofs
  4. Validate that the hashes matches with the commitments, performing a `kzg4844` hash.
  5. Verify the blob proofs with the `kzg4844`

Stateful validations

1. Ensure transaction nonce is higher than the `from` address stored nonce
2. Certain pools do not allow for nonce gaps. Ensure a gap is not produced (that is, the transaction nonce is exactly the following of the stored one)
3. Ensure the transactor has enough funds to cover transaction cost:
    - Transaction cost is calculated as `(gas * gasPrice) + (blobGas * blobGasPrice) + value`
4. In case of transaction reorg, ensure the transactor has enough funds to cover for transaction replacements without overdrafts.
- This is done by comparing the total spent gas of the transactor from all pooled transactions, and accounting for the necessary gas spenditure if any of those transactions is replaced.
5. Ensure the transactor is able to add a new transaction. The number of transactions sent by an account may be limited by a certain configured value

*/
fn validate_transaction(tx: &Transaction, store: Store) -> Result<(), MempoolError> {
    // TODO: Add validations here
    let header_no = store
        .get_latest_block_number()?
        .ok_or(MempoolError::NoBlockHeaderError)?;
    let header = store
        .get_block_header(header_no)?
        .ok_or(MempoolError::NoBlockHeaderError)?;
    let config = store.get_chain_config()?;

    // Check init code size
    if config.is_shanghai_activated(header.timestamp)
        && tx.is_contract_creation()
        && tx.data().len() > MAX_INITCODE_SIZE
    {
        return Err(MempoolError::TxMaxInitCodeSizeError);
    }

    // Check gas limit is less than header's gas limit
    if header.gas_limit < tx.gas_limit() {
        return Err(MempoolError::TxGasLimitExceededError);
    }

    // Check priority fee is less or equal than gas fee gap
    if tx.max_priority_fee().unwrap_or(0) > tx.max_fee_per_gas().unwrap_or(0) {
        return Err(MempoolError::TxTipAboveFeeCapError);
    }

    // Check that the gas limit is covers the gas needs for transaction metadata.
    if tx.gas_limit() < transaction_intrinsic_gas(tx, &header, &config)? {
        return Err(MempoolError::TxIntrinsicGasCostAboveLimitError);
    }

    // Check that the specified blob gas fee is above the minimum value
    if let Some(fee) = tx.max_fee_per_blob_gas() {
        // Blob tx
        if fee < MIN_BASE_FEE_PER_BLOB_GAS.into() {
            return Err(MempoolError::TxBlobBaseFeeTooLowError);
        }
    }

    Ok(())
}

fn transaction_intrinsic_gas(
    tx: &Transaction,
    header: &BlockHeader,
    config: &ChainConfig,
) -> Result<u64, MempoolError> {
    let is_contract_creation = tx.is_contract_creation();

    let mut gas = if is_contract_creation {
        TX_CREATE_GAS_COST
    } else {
        TX_GAS_COST
    };

    let data_len = tx.data().len() as u64;

    if data_len > 0 {
        let non_zero_gas_cost = if config.is_istanbul_activated(header.number) {
            TX_DATA_NON_ZERO_GAS_EIP2028
        } else {
            TX_DATA_NON_ZERO_GAS
        };

        let non_zero_count = tx.data().iter().filter(|&&x| x != 0u8).count() as u64;

        gas = gas
            .checked_add(non_zero_count * non_zero_gas_cost)
            .ok_or(MempoolError::TxGasOverflowError)?;

        let zero_count = data_len - non_zero_count;

        gas = gas
            .checked_add(zero_count * TX_DATA_ZERO_GAS_COST)
            .ok_or(MempoolError::TxGasOverflowError)?;

        if is_contract_creation && config.is_shanghai_activated(header.timestamp) {
            // Len in 32 bytes sized words
            let len_in_words = data_len.saturating_add(31) / 32;

            gas = gas
                .checked_add(len_in_words * TX_INIT_CODE_WORD_GAS_COST)
                .ok_or(MempoolError::TxGasOverflowError)?;
        }
    }

    let storage_keys_count: u64 = tx
        .access_list()
        .iter()
        .map(|(_, keys)| keys.len() as u64)
        .sum();

    gas = gas
        .checked_add(tx.access_list().len() as u64 * TX_ACCESS_LIST_ADDRESS_GAS)
        .ok_or(MempoolError::TxGasOverflowError)?;

    gas = gas
        .checked_add(storage_keys_count * TX_ACCESS_LIST_STORAGE_KEY_GAS)
        .ok_or(MempoolError::TxGasOverflowError)?;

    Ok(gas)
}

#[cfg(test)]
mod tests {
    use crate::error::MempoolError;
    use crate::mempool::{
        TX_ACCESS_LIST_ADDRESS_GAS, TX_ACCESS_LIST_STORAGE_KEY_GAS, TX_CREATE_GAS_COST,
        TX_DATA_NON_ZERO_GAS, TX_DATA_NON_ZERO_GAS_EIP2028, TX_DATA_ZERO_GAS_COST, TX_GAS_COST,
        TX_INIT_CODE_WORD_GAS_COST,
    };

    use super::{
        add_transaction, get_transaction, transaction_intrinsic_gas, validate_transaction,
    };
    use ethereum_rust_core::types::{
        BlockHeader, ChainConfig, EIP1559Transaction, EIP4844Transaction, Transaction, TxKind,
        MAX_INITCODE_SIZE,
    };
    use ethereum_rust_core::{Address, Bytes, H256, U256};
    use ethereum_rust_storage::EngineType;
    use ethereum_rust_storage::{error::StoreError, Store};

    fn setup_storage(config: ChainConfig, header: BlockHeader) -> Result<Store, StoreError> {
        let store = Store::new("test", EngineType::InMemory)?;
        let block_number = header.number;
        store.add_block_header(block_number, header)?;
        store.update_latest_block_number(block_number)?;
        store.set_chain_config(&config)?;

        Ok(store)
    }

    fn tx_equal(t1: Transaction, t2: Transaction) -> bool {
        t1.nonce() == t2.nonce()
            && t1.max_priority_fee().unwrap_or_default()
                == t2.max_priority_fee().unwrap_or_default()
            && t1.max_fee_per_gas().unwrap_or_default() == t2.max_fee_per_gas().unwrap_or_default()
            && t1.gas_limit() == t2.gas_limit()
            && t1.value() == t2.value()
            && *t1.data() == *t2.data()
    }

    fn build_basic_config_and_header(
        istanbul_active: bool,
        shanghai_active: bool,
    ) -> (ChainConfig, BlockHeader) {
        let config = ChainConfig {
            shanghai_time: Some(if shanghai_active { 1 } else { 10 }),
            istanbul_block: Some(if istanbul_active { 1 } else { 10 }),
            ..Default::default()
        };

        let header = BlockHeader {
            number: 5,
            timestamp: 5,
            gas_limit: 100_000_000,
            gas_used: 0,
            ..Default::default()
        };

        (config, header)
    }

    #[test]
    fn store_and_fetch_transaction_happy_path() {
        let config = ChainConfig {
            shanghai_time: Some(10),
            ..Default::default()
        };

        let header = BlockHeader {
            number: 123,
            gas_limit: 30_000_000,
            gas_used: 0,
            timestamp: 20,
            ..Default::default()
        };

        let store = setup_storage(config, header).expect("Setup failed: ");

        let tx = EIP1559Transaction {
            nonce: 3,
            max_priority_fee_per_gas: 0,
            max_fee_per_gas: 0,
            gas_limit: 100_000,
            to: TxKind::Call(Address::from_low_u64_be(1)),
            value: U256::zero(),
            data: Bytes::default(),
            access_list: Default::default(),
            ..Default::default()
        };

        let tx = Transaction::EIP1559Transaction(tx);
        let hash = add_transaction(tx.clone(), store.clone()).expect("Add transaction");
        let ret_tx = get_transaction(hash, store).expect("Get transaction");
        assert!(ret_tx.is_some());
        let ret_tx = ret_tx.unwrap();
        assert!(tx_equal(tx, ret_tx))
    }

    #[test]
    fn normal_transaction_intrinsic_gas() {
        let (config, header) = build_basic_config_and_header(false, false);

        let tx = EIP1559Transaction {
            nonce: 3,
            max_priority_fee_per_gas: 0,
            max_fee_per_gas: 0,
            gas_limit: 100_000,
            to: TxKind::Call(Address::from_low_u64_be(1)), // Normal tx
            value: U256::zero(),                           // Value zero
            data: Bytes::default(),                        // No data
            access_list: Default::default(),               // No access list
            ..Default::default()
        };

        let tx = Transaction::EIP1559Transaction(tx);
        let expected_gas_cost = TX_GAS_COST;
        let intrinsic_gas =
            transaction_intrinsic_gas(&tx, &header, &config).expect("Intrinsic gas");
        assert_eq!(intrinsic_gas, expected_gas_cost);
    }

    #[test]
    fn create_transaction_intrinsic_gas() {
        let (config, header) = build_basic_config_and_header(false, false);

        let tx = EIP1559Transaction {
            nonce: 3,
            max_priority_fee_per_gas: 0,
            max_fee_per_gas: 0,
            gas_limit: 100_000,
            to: TxKind::Create,              // Create tx
            value: U256::zero(),             // Value zero
            data: Bytes::default(),          // No data
            access_list: Default::default(), // No access list
            ..Default::default()
        };

        let tx = Transaction::EIP1559Transaction(tx);
        let expected_gas_cost = TX_CREATE_GAS_COST;
        let intrinsic_gas =
            transaction_intrinsic_gas(&tx, &header, &config).expect("Intrinsic gas");
        assert_eq!(intrinsic_gas, expected_gas_cost);
    }

    #[test]
    fn transaction_intrinsic_data_gas_pre_istanbul() {
        let (config, header) = build_basic_config_and_header(false, false);

        let tx = EIP1559Transaction {
            nonce: 3,
            max_priority_fee_per_gas: 0,
            max_fee_per_gas: 0,
            gas_limit: 100_000,
            to: TxKind::Call(Address::from_low_u64_be(1)), // Normal tx
            value: U256::zero(),                           // Value zero
            data: Bytes::from(vec![0x0, 0x1, 0x1, 0x0, 0x1, 0x1]), // 6 bytes of data
            access_list: Default::default(),               // No access list
            ..Default::default()
        };

        let tx = Transaction::EIP1559Transaction(tx);
        let expected_gas_cost = TX_GAS_COST + 2 * TX_DATA_ZERO_GAS_COST + 4 * TX_DATA_NON_ZERO_GAS;
        let intrinsic_gas =
            transaction_intrinsic_gas(&tx, &header, &config).expect("Intrinsic gas");
        assert_eq!(intrinsic_gas, expected_gas_cost);
    }

    #[test]
    fn transaction_intrinsic_data_gas_post_istanbul() {
        let (config, header) = build_basic_config_and_header(true, false);

        let tx = EIP1559Transaction {
            nonce: 3,
            max_priority_fee_per_gas: 0,
            max_fee_per_gas: 0,
            gas_limit: 100_000,
            to: TxKind::Call(Address::from_low_u64_be(1)), // Normal tx
            value: U256::zero(),                           // Value zero
            data: Bytes::from(vec![0x0, 0x1, 0x1, 0x0, 0x1, 0x1]), // 6 bytes of data
            access_list: Default::default(),               // No access list
            ..Default::default()
        };

        let tx = Transaction::EIP1559Transaction(tx);
        let expected_gas_cost =
            TX_GAS_COST + 2 * TX_DATA_ZERO_GAS_COST + 4 * TX_DATA_NON_ZERO_GAS_EIP2028;
        let intrinsic_gas =
            transaction_intrinsic_gas(&tx, &header, &config).expect("Intrinsic gas");
        assert_eq!(intrinsic_gas, expected_gas_cost);
    }

    #[test]
    fn transaction_create_intrinsic_gas_pre_shanghai() {
        let (config, header) = build_basic_config_and_header(false, false);

        let n_words: u64 = 10;
        let n_bytes: u64 = 32 * n_words - 3; // Test word rounding

        let tx = EIP1559Transaction {
            nonce: 3,
            max_priority_fee_per_gas: 0,
            max_fee_per_gas: 0,
            gas_limit: 100_000,
            to: TxKind::Create,                                // Create tx
            value: U256::zero(),                               // Value zero
            data: Bytes::from(vec![0x1_u8; n_bytes as usize]), // Bytecode data
            access_list: Default::default(),                   // No access list
            ..Default::default()
        };

        let tx = Transaction::EIP1559Transaction(tx);
        let expected_gas_cost = TX_CREATE_GAS_COST + n_bytes * TX_DATA_NON_ZERO_GAS;
        let intrinsic_gas =
            transaction_intrinsic_gas(&tx, &header, &config).expect("Intrinsic gas");
        assert_eq!(intrinsic_gas, expected_gas_cost);
    }

    #[test]
    fn transaction_create_intrinsic_gas_post_shanghai() {
        let (config, header) = build_basic_config_and_header(false, true);

        let n_words: u64 = 10;
        let n_bytes: u64 = 32 * n_words - 3; // Test word rounding

        let tx = EIP1559Transaction {
            nonce: 3,
            max_priority_fee_per_gas: 0,
            max_fee_per_gas: 0,
            gas_limit: 100_000,
            to: TxKind::Create,                                // Create tx
            value: U256::zero(),                               // Value zero
            data: Bytes::from(vec![0x1_u8; n_bytes as usize]), // Bytecode data
            access_list: Default::default(),                   // No access list
            ..Default::default()
        };

        let tx = Transaction::EIP1559Transaction(tx);
        let expected_gas_cost = TX_CREATE_GAS_COST
            + n_bytes * TX_DATA_NON_ZERO_GAS
            + n_words * TX_INIT_CODE_WORD_GAS_COST;
        let intrinsic_gas =
            transaction_intrinsic_gas(&tx, &header, &config).expect("Intrinsic gas");
        assert_eq!(intrinsic_gas, expected_gas_cost);
    }

    #[test]
    fn transaction_intrinsic_gas_access_list() {
        let (config, header) = build_basic_config_and_header(false, false);

        let access_list = vec![
            (Address::zero(), vec![H256::default(); 10]),
            (Address::zero(), vec![]),
            (Address::zero(), vec![H256::default(); 5]),
        ];

        let tx = EIP1559Transaction {
            nonce: 3,
            max_priority_fee_per_gas: 0,
            max_fee_per_gas: 0,
            gas_limit: 100_000,
            to: TxKind::Call(Address::from_low_u64_be(1)), // Normal tx
            value: U256::zero(),                           // Value zero
            data: Bytes::default(),                        // No data
            access_list,
            ..Default::default()
        };

        let tx = Transaction::EIP1559Transaction(tx);
        let expected_gas_cost =
            TX_GAS_COST + 3 * TX_ACCESS_LIST_ADDRESS_GAS + 15 * TX_ACCESS_LIST_STORAGE_KEY_GAS;
        let intrinsic_gas =
            transaction_intrinsic_gas(&tx, &header, &config).expect("Intrinsic gas");
        assert_eq!(intrinsic_gas, expected_gas_cost);
    }

    #[test]
    fn transaction_with_big_init_code_in_shanghai_fails() {
        let (config, header) = build_basic_config_and_header(false, true);

        let store = setup_storage(config, header).expect("Storage setup");

        let tx = EIP1559Transaction {
            nonce: 3,
            max_priority_fee_per_gas: 0,
            max_fee_per_gas: 0,
            gas_limit: 99_000_000,
            to: TxKind::Create,                                  // Create tx
            value: U256::zero(),                                 // Value zero
            data: Bytes::from(vec![0x1; MAX_INITCODE_SIZE + 1]), // Large init code
            access_list: Default::default(),                     // No access list
            ..Default::default()
        };

        let tx = Transaction::EIP1559Transaction(tx);
        let validation = validate_transaction(&tx, store);
        assert!(matches!(
            validation,
            Err(MempoolError::TxMaxInitCodeSizeError)
        ));
    }

    #[test]
    fn transaction_with_gas_limit_higher_than_of_the_block_should_fail() {
        let (config, header) = build_basic_config_and_header(false, false);

        let store = setup_storage(config, header).expect("Storage setup");

        let tx = EIP1559Transaction {
            nonce: 3,
            max_priority_fee_per_gas: 0,
            max_fee_per_gas: 0,
            gas_limit: 100_000_001,
            to: TxKind::Call(Address::from_low_u64_be(1)), // Normal tx
            value: U256::zero(),                           // Value zero
            data: Bytes::default(),                        // No data
            access_list: Default::default(),               // No access list
            ..Default::default()
        };

        let tx = Transaction::EIP1559Transaction(tx);
        let validation = validate_transaction(&tx, store);
        assert!(matches!(
            validation,
            Err(MempoolError::TxGasLimitExceededError)
        ));
    }

    #[test]
    fn transaction_with_priority_fee_higher_than_gas_fee_should_fail() {
        let (config, header) = build_basic_config_and_header(false, false);

        let store = setup_storage(config, header).expect("Storage setup");

        let tx = EIP1559Transaction {
            nonce: 3,
            max_priority_fee_per_gas: 101,
            max_fee_per_gas: 100,
            gas_limit: 50_000_000,
            to: TxKind::Call(Address::from_low_u64_be(1)), // Normal tx
            value: U256::zero(),                           // Value zero
            data: Bytes::default(),                        // No data
            access_list: Default::default(),               // No access list
            ..Default::default()
        };

        let tx = Transaction::EIP1559Transaction(tx);
        let validation = validate_transaction(&tx, store);
        assert!(matches!(
            validation,
            Err(MempoolError::TxTipAboveFeeCapError)
        ));
    }

    #[test]
    fn transaction_with_gas_limit_lower_than_intrinsic_gas_should_fail() {
        let (config, header) = build_basic_config_and_header(false, false);
        let store = setup_storage(config, header).expect("Storage setup");

        let intrinsic_gas_cost = TX_GAS_COST;

        let tx = EIP1559Transaction {
            nonce: 3,
            max_priority_fee_per_gas: 0,
            max_fee_per_gas: 0,
            gas_limit: intrinsic_gas_cost - 1,
            to: TxKind::Call(Address::from_low_u64_be(1)), // Normal tx
            value: U256::zero(),                           // Value zero
            data: Bytes::default(),                        // No data
            access_list: Default::default(),               // No access list
            ..Default::default()
        };

        let tx = Transaction::EIP1559Transaction(tx);
        let validation = validate_transaction(&tx, store);
        assert!(matches!(
            validation,
            Err(MempoolError::TxIntrinsicGasCostAboveLimitError)
        ));
    }

    #[test]
    fn transaction_with_blob_base_fee_below_min_should_fail() {
        let (config, header) = build_basic_config_and_header(false, false);
        let store = setup_storage(config, header).expect("Storage setup");

        let tx = EIP4844Transaction {
            nonce: 3,
            max_priority_fee_per_gas: 0,
            max_fee_per_gas: 0,
            max_fee_per_blob_gas: 0.into(),
            gas: 15_000_000,
            to: Address::from_low_u64_be(1), // Normal tx
            value: U256::zero(),             // Value zero
            data: Bytes::default(),          // No data
            access_list: Default::default(), // No access list
            ..Default::default()
        };

        let tx = Transaction::EIP4844Transaction(tx);
        let validation = validate_transaction(&tx, store);
        assert!(matches!(
            validation,
            Err(MempoolError::TxBlobBaseFeeTooLowError)
        ));
    }
}
