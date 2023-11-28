#![allow(non_snake_case)]
#![allow(non_camel_case_types)]
use ethereum_types::{Address, H256, H64, U256};
use metastruct::metastruct;
use serde::{Deserialize, Serialize};
use std::error::Error;
pub mod keccak;
use array_bytes::hex_n_into;
use array_bytes::TryFromHex;

fn value_to_hash(value: &serde_json::Value) -> Result<H256, Box<dyn Error>> {
    let hex = value.as_str().ok_or("value is not a string")?;
    let hash: H256 = hex_n_into::<_, H256, 32>(hex)
        .map_err(|e| format!("Value is not a valid hex string: {:?}", e))?;
    Ok(hash)
}

fn value_to_address(value: &serde_json::Value) -> Result<Address, Box<dyn Error>> {
    let hex = value.as_str().ok_or("value is not a string")?;
    let address: Address = hex_n_into::<_, Address, 20>(hex)
        .map_err(|e| format!("Value is not a valid hex string: {:?}", e))?;
    Ok(address)
}

fn value_to_u256(value: &serde_json::Value) -> Result<U256, Box<dyn Error>> {
    let hex = value.as_str().ok_or("value is not a string")?;
    let mut padded_hex = hex.to_string();

    if padded_hex.starts_with("0x") {
        padded_hex = padded_hex[2..].to_string();
    }

    // pad the string with leading zeros to make it 64 characters
    let padding_len = 64 - padded_hex.len();
    let padding = "0".repeat(padding_len);
    padded_hex = padding + &padded_hex;

    let num: U256 = array_bytes::hex_n_into::<_, U256, 32>(padded_hex)
        .map_err(|e| format!("Value is not a valid hex string: {:?}", e))?;
    Ok(num)
}

fn value_to_u64(value: &serde_json::Value) -> Result<u64, Box<dyn Error>> {
    let hex = value.as_str().ok_or("value is not a string")?;
    let num: u64 =
        u64::try_from_hex(hex).map_err(|e| format!("Value is not a valid hex string: {:?}", e))?;
    Ok(num)
}

fn value_to_withdrawls(value: &serde_json::Value) -> Result<Vec<Withdrawal>, Box<dyn Error>> {
    let withdrawals = value.as_array().ok_or("value is not an array")?;
    let withdrawals = withdrawals
        .iter()
        .map(|withdrawal| {
            let index = value_to_u64(&withdrawal["index"])?;
            let validator_index = value_to_u64(&withdrawal["validatorIndex"])?;
            let address = value_to_address(&withdrawal["address"])?;
            let amount = value_to_u64(&withdrawal["amount"])?;
            Ok(Withdrawal {
                index,
                validator_index,
                address,
                amount,
            })
        })
        .collect::<Result<Vec<Withdrawal>, Box<dyn Error>>>()?;
    Ok(withdrawals)
}

/*
This structure contains the result of processing a payload. The fields are encoded as follows:

status: enum - "VALID" | "INVALID" | "SYNCING" | "ACCEPTED" | "INVALID_BLOCK_HASH"
latestValidHash: DATA|null, 32 Bytes - the hash of the most recent valid block in the branch defined by payload and its ancestors
validationError: String|null - a message providing additional details on the validation error if the payload is classified as INVALID or INVALID_BLOCK_HASH.
 */

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, Eq, Hash)]
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Eq, Hash)]
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

pub struct Withdrawal {
    pub index: u64,
    pub validator_index: u64,
    pub address: Address,
    pub amount: u64,
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
    pub withdrawals: Vec<Withdrawal>,
}

impl ExecutionPayload {
    pub fn from_json(payload: &serde_json::Value) -> Result<Self, Box<dyn Error>> {
        let parent_hash = value_to_hash(&payload["parentHash"])?;
        let fee_recipient = value_to_address(&payload["feeRecipient"])?.into();
        let state_root = value_to_hash(&payload["stateRoot"])?;
        let receipts_root = value_to_hash(&payload["receiptsRoot"])?;
        let logs_bloom = hex::decode(payload["logsBloom"].as_str().unwrap()[2..].to_string())?;
        let prev_randao = value_to_hash(&payload["prevRandao"])?;
        let block_number = value_to_u64(&payload["blockNumber"])?;
        let gas_limit = value_to_u64(&payload["gasLimit"])?;
        let gas_used = value_to_u64(&payload["gasUsed"])?;
        let timestamp = value_to_u64(&payload["timestamp"])?;
        let extra_data = hex::decode(payload["extraData"].as_str().unwrap()[2..].to_string())?;
        let base_fee_per_gas = value_to_u256(&payload["baseFeePerGas"])?;
        let block_hash = value_to_hash(&payload["blockHash"])?;
        let transactions = payload["transactions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|txn| txn.as_str().unwrap().to_string())
            .collect::<Vec<String>>()
            .iter()
            .map(|txn| txn[2..].to_string())
            .map(|txn_str| hex::decode(txn_str).unwrap())
            .collect::<Vec<Vec<u8>>>();
        let withdrawals = value_to_withdrawls(&payload["withdrawals"])?;

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
            withdrawals,
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
    pub withdrawals_root: H256,
}

impl ExecutionBlockHeader {
    pub fn from_payload(
        payload: &ExecutionPayload,
        rlp_empty_list_root: H256,
        rlp_transactions_root: H256,
        rlp_withdrawals_root: H256,
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
            withdrawals_root: rlp_withdrawals_root,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Claims {
    /// issued-at claim. Represented as seconds passed since UNIX_EPOCH.
    pub iat: i64,
}

#[derive(PartialEq, Clone, Copy)]
pub enum SyncingStatus {
    Synced,
    Offline,
    OnlineAndSyncing,
    NodeNotInitialized,
}

#[derive(Debug)]

pub enum FcuLogicError {
    NoMajority,
    OneNodeIsInvalid,
    NoResponses,
}

#[derive(Debug)]
pub enum ParseError {
    MethodNotFound,
    NoMethod,
    NoId,
    InvalidJson,
    NoParams,
    ElError,
}

#[derive(Clone)]
pub struct NodeHealth {
    pub status: SyncingStatus,
    pub resp_time: u128,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum EngineMethod {
    engine_newPayloadV1,
    engine_forkchoiceUpdatedV1,
    engine_getPayloadV1,
    engine_exchangeTransitionConfigurationV1,
    engine_exchangeCapabilities,
    engine_newPayloadV2,
    engine_forkchoiceUpdatedV2,
    engine_getPayloadV2,
    engine_getPayloadBodiesByHashV1,
    engine_getPayloadBodiesByRangeV1,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct RpcRequest {
    pub method: EngineMethod,
    pub params: serde_json::Value,
    pub id: u64,
    pub jsonrpc: String,
}

impl RpcRequest {
    #[inline]
    pub fn as_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap()
    }
}

#[derive(Serialize, Deserialize)]
pub struct forkchoiceUpdatedResponse {
    pub payloadStatus: PayloadStatusV1,
    pub payloadId: Option<H256>,
}
