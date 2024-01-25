#![allow(non_snake_case)]
#![allow(non_camel_case_types)]
use std::collections::HashMap;
use ethereum_types::{Address, H256, H64, U256};
use metastruct::metastruct;
use serde::{Deserialize, Serialize};
use ssz_types::{VariableList, typenum::{U1073741824, U1048576}};
pub mod keccak;
use superstruct::superstruct;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, Eq, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PayloadStatusV1Status {
    Valid,
    Invalid,
    Syncing,
    Accepted,
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

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Withdrawal {
    #[serde(with = "serde_utils::u64_hex_be")]
    pub index: u64,
    #[serde(with = "serde_utils::u64_hex_be")]
    pub validator_index: u64,
    pub address: Address,
    #[serde(with = "serde_utils::u64_hex_be")]
    pub amount: u64,
}


#[superstruct(variants(V1, V2, V3), variant_attributes(derive(Serialize, Deserialize, Clone), serde(rename_all = "camelCase", deny_unknown_fields)))]
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase", untagged, deny_unknown_fields)]
pub struct ExecutionPayload {
    #[superstruct(getter(copy))]
    pub parent_hash: H256,
    #[superstruct(getter(copy))]
    pub fee_recipient: Address,
    #[superstruct(getter(copy))]
    pub state_root: H256,
    #[superstruct(getter(copy))]
    pub receipts_root: H256,
    #[serde(with = "serde_utils::hex_vec")]
    pub logs_bloom: Vec<u8>,
    #[superstruct(getter(copy))]
    pub prev_randao: H256,
    #[superstruct(getter(copy))]
    #[serde(with = "serde_utils::u64_hex_be")]
    pub block_number: u64,
    #[serde(with = "serde_utils::u64_hex_be")]
    #[superstruct(getter(copy))]
    pub gas_limit: u64,
    #[superstruct(getter(copy))]
    #[serde(with = "serde_utils::u64_hex_be")]
    pub gas_used: u64,
    #[superstruct(getter(copy))]
    #[serde(with = "serde_utils::u64_hex_be")]
    pub timestamp: u64,
    #[serde(with = "serde_utils::hex_vec")]
    pub extra_data: Vec<u8>,
    #[superstruct(getter(copy))]
    #[serde(with = "serde_utils::u256_hex_be")]
    pub base_fee_per_gas: U256,
    #[superstruct(getter(copy))]
    pub block_hash: H256,
    #[serde(with = "ssz_types::serde_utils::list_of_hex_var_list")]
    pub transactions: VariableList<VariableList<u8, U1073741824>, U1048576>,    // larger one is max bytes per transaction, smaller one is max transactions per payload
    #[superstruct(only(V2, V3))]
    pub withdrawals: Vec<Withdrawal>,
    #[superstruct(only(V3), partial_getter(copy))]
    #[serde(with = "serde_utils::u64_hex_be")]
    pub blob_gas_used: u64,
    #[superstruct(only(V3), partial_getter(copy))]
    #[serde(with = "serde_utils::u64_hex_be")]
    pub excess_blob_gas: u64,
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
            parent_hash: payload.parent_hash(),
            ommers_hash: rlp_empty_list_root,
            beneficiary: payload.fee_recipient(),
            state_root: payload.state_root(),
            transactions_root: rlp_transactions_root,
            receipts_root: payload.receipts_root(),
            logs_bloom: payload.logs_bloom().clone(),
            difficulty: U256::zero(),
            number: payload.block_number().into(),
            gas_limit: payload.gas_limit().into(),
            gas_used: payload.gas_used().into(),
            timestamp: payload.timestamp(),
            extra_data: payload.extra_data().clone(),
            mix_hash: payload.prev_randao(),
            nonce: H64::zero(),
            base_fee_per_gas: payload.base_fee_per_gas(),
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

pub struct NodeTiming {
    pub node: String,
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
    engine_newPayloadV3,
    engine_forkchoiceUpdatedV3,
    engine_getPayloadV3,
    
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
    pub payloadId: Option<String>,
}

#[superstruct(variants(V1, V2, V3), variant_attributes(derive(Serialize, Deserialize, Clone), serde(rename_all = "camelCase")))]
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase", untagged)]
pub struct getPayloadResponse {
    #[superstruct(only(V1), partial_getter(rename = "execution_payload_v1"))]       // V1, V2
    pub execution_payload: ExecutionPayloadV1,
    #[superstruct(only(V2), partial_getter(rename = "execution_payload_v2"))]
    pub execution_payload: ExecutionPayloadV2,
    #[superstruct(only(V3), partial_getter(rename = "execution_payload_v3"))]
    pub execution_payload: ExecutionPayloadV3,
    #[serde(with = "serde_utils::u256_hex_be")]
    #[superstruct(getter(copy))]
    pub block_value: U256,
    #[superstruct(only(V3))]
    pub blobs_bundle: serde_json::Value,
    #[superstruct(only(V3), partial_getter(copy))]
    pub should_override_builder: bool
}

#[derive(Serialize, Deserialize)]
pub struct MetricsReport {
    pub response_times: HashMap<String, u128>,
    pub alive_nodes: Vec<String>,
    pub syncing_nodes: Vec<String>,
    pub dead_nodes: Vec<String>,
    pub primary_node: String,
}