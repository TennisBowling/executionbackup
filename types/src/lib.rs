use std::error::Error;
use ethereum_types::{Address, H256, H64, U256};
use metastruct::metastruct;
use serde::{Serialize, Deserialize};
pub mod keccak;

/*
This structure contains the result of processing a payload. The fields are encoded as follows:

status: enum - "VALID" | "INVALID" | "SYNCING" | "ACCEPTED" | "INVALID_BLOCK_HASH"
latestValidHash: DATA|null, 32 Bytes - the hash of the most recent valid block in the branch defined by payload and its ancestors
validationError: String|null - a message providing additional details on the validation error if the payload is classified as INVALID or INVALID_BLOCK_HASH.
 */

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum PayloadStatusV1Status {
    #[serde(rename = "VALID")]
    Valid,
    #[serde(rename = "INVALID")]
    Invalid,
    #[serde(rename = "SYNCING")]
    Syncing,
    #[serde(rename = "ACCEPTED")]
    Accepted,
    #[serde(rename = "INVALID_BLOCK_HASH")]
    InvalidBlockHash,
}
  
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PayloadStatusV1 {
    pub status: PayloadStatusV1Status,
    pub latest_valid_hash: Option<H256>,
    pub validation_error: Option<String>,
}

impl PayloadStatusV1 {
    pub fn new_invalid(latest_valid_hash: H256, validation_error: String) -> Self {
        Self {
            status: PayloadStatusV1Status::Invalid,
            latest_valid_hash: Some(latest_valid_hash),
            validation_error: Some(validation_error),
        }
    }

    pub fn new_syncing() -> Self {
        Self {
            status: PayloadStatusV1Status::Syncing,
            latest_valid_hash: None,
            validation_error: None,
        }
    }

}


pub struct ExecutionPayload {
    pub parent_hash: H256,
    pub fee_recipient: Address,
    pub state_root: H256,
    pub receipts_root: H256,
    pub logs_bloom: Vec<u8>,
    pub prev_randao: H256,
    pub block_number: u64,
    pub gas_limit: u64,
    pub gas_used: u64,
    pub timestamp: u64,
    pub extra_data: Vec<u8>,
    pub base_fee_per_gas: U256,
    pub block_hash: H256,
    pub transactions: Vec<Vec<u8>>,
}

impl ExecutionPayload {
    pub fn from_json(payload: &serde_json::Value) -> Result<Self, Box<dyn Error>> {
        let parent_hash = H256::from_slice(&hex::decode(payload["parentHash"].as_str().unwrap()[2..].to_string())?);
        let fee_recipient = Address::from_slice(&hex::decode(payload["feeRecipient"].as_str().unwrap()[2..].to_string())?);
        let state_root = H256::from_slice(&hex::decode(payload["stateRoot"].as_str().unwrap()[2..].to_string())?);
        let receipts_root = H256::from_slice(&hex::decode(payload["receiptsRoot"].as_str().unwrap()[2..].to_string())?);
        let logs_bloom = hex::decode(payload["logsBloom"].as_str().unwrap()[2..].to_string())?;
        let prev_randao = H256::from_slice(&hex::decode(payload["prevRandao"].as_str().unwrap()[2..].to_string())?);
        let block_number: u64 = u64::from_str_radix(&payload["blockNumber"].as_str().unwrap()[2..], 16)?;
        let gas_limit: u64 = u64::from_str_radix(&payload["gasLimit"].as_str().unwrap()[2..], 16)?;
        let gas_used = u64::from_str_radix(&payload["gasUsed"].as_str().unwrap()[2..], 16)?;
        let timestamp = u64::from_str_radix(&payload["timestamp"].as_str().unwrap()[2..], 16)?;
        let extra_data = hex::decode(payload["extraData"].as_str().unwrap()[2..].to_string())?;
        let base_fee_per_gas = U256::from_str_radix(&payload["baseFeePerGas"].as_str().unwrap()[2..], 16)?;
        let block_hash = H256::from_slice(&hex::decode(payload["blockHash"].as_str().unwrap()[2..].to_string())?);
        let transactions = payload["transactions"].as_array().unwrap().iter().map(|txn| txn.as_str().unwrap().to_string()).collect::<Vec<String>>().iter().map(|txn| txn[2..].to_string()).map(|txn_str| hex::decode(txn_str).unwrap()).collect::<Vec<Vec<u8>>>();

        Ok(ExecutionPayload {
            parent_hash,
            fee_recipient,
            state_root,
            receipts_root,
            logs_bloom,
            prev_randao,
            block_number,
            gas_limit,
            gas_used,
            timestamp,
            extra_data,
            base_fee_per_gas,
            block_hash,
            transactions,
        })
    }
}



#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[metastruct(mappings(map_execution_block_header_fields()))]
pub struct ExecutionBlockHeader {
    pub parent_hash: H256,
    pub ommers_hash: H256,
    pub beneficiary: Address,
    pub state_root: H256,
    pub transactions_root: H256,
    pub receipts_root: H256,
    pub logs_bloom: Vec<u8>,
    pub difficulty: U256,
    pub number: U256,
    pub gas_limit: U256,
    pub gas_used: U256,
    pub timestamp: u64,
    pub extra_data: Vec<u8>,
    pub mix_hash: H256,
    pub nonce: H64,
    pub base_fee_per_gas: U256,
}

impl ExecutionBlockHeader {
    pub fn from_payload(
        payload: &ExecutionPayload,
        rlp_empty_list_root: H256,
        rlp_transactions_root: H256,
    ) -> Self {
        // Most of these field mappings are defined in EIP-3675 except for `mixHash`, which is
        // defined in EIP-4399.
        ExecutionBlockHeader {
            parent_hash: payload.parent_hash,
            ommers_hash: rlp_empty_list_root,
            beneficiary: payload.fee_recipient,
            state_root: payload.state_root,
            transactions_root: rlp_transactions_root,
            receipts_root: payload.receipts_root,
            logs_bloom: payload.logs_bloom.clone(),
            difficulty: U256::zero(),
            number: payload.block_number.into(),
            gas_limit: payload.gas_limit.into(),
            gas_used: payload.gas_used.into(),
            timestamp: payload.timestamp,
            extra_data: payload.extra_data.clone(),
            mix_hash: payload.prev_randao,
            nonce: H64::zero(),
            base_fee_per_gas: payload.base_fee_per_gas,
        }
    }
}