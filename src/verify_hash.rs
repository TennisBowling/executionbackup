/*
Copyright 2018 Sigma Prime Pty Ltd

   Licensed under the Apache License, Version 2.0 (the "License");
   you may not use this file except in compliance with the License.
   You may obtain a copy of the License at

       http://www.apache.org/licenses/LICENSE-2.0

   Unless required by applicable law or agreed to in writing, software
   distributed under the License is distributed on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
   See the License for the specific language governing permissions and
   limitations under the License.

Thank you Lighthouse Team for everything you do!
- <3 Tennis
*/


use std::error::Error;
use triehash::ordered_trie_root;
use rlp::RlpStream;
use ethereum_types::{H256, U256, H64, Address};
use keccak_hash::KECCAK_EMPTY_LIST_RLP;
use types::keccak::{KeccakHasher, keccak256};
use types::{ExecutionBlockHeader, ExecutionPayload, map_execution_block_header_fields, Withdrawal};


// Thank you lighthouse team! https://github.com/sigp/lighthouse/blob/stable/beacon_node/execution_layer/src/block_hash.rs#L50-L59
/// RLP encode an execution block header.
pub fn rlp_encode_block_header(header: &ExecutionBlockHeader) -> Vec<u8> {
    let mut rlp_header_stream = RlpStream::new();
    rlp_header_stream.begin_unbounded_list();
    map_execution_block_header_fields!(&header, |_, field| {
        rlp_header_stream.append(field);
    });
    rlp_header_stream.finalize_unbounded_list();
    rlp_header_stream.out().into()
}

/// RLP encode a withdrawal.
pub fn rlp_encode_withdrawal(withdrawal: &Withdrawal) -> Vec<u8> {
    let mut rlp_stream = RlpStream::new();
    rlp_stream.begin_list(4);
    rlp_stream.append(&withdrawal.index);
    rlp_stream.append(&withdrawal.validator_index);
    rlp_stream.append(&withdrawal.address);
    rlp_stream.append(&withdrawal.amount);
    rlp_stream.out().into()
}

// Thank you lighthouse team again! https://github.com/sigp/lighthouse/blob/stable/beacon_node/execution_layer/src/block_hash.rs#L17-L48
pub fn verify_payload_block_hash(payload: &ExecutionPayload) -> Result<(), Box<dyn Error>> {

    // Calculate the transactions root.
    // We're currently using a deprecated Parity library for this. We should move to a
    // better alternative when one appears, possibly following Reth.
    let rlp_transactions_root = ordered_trie_root::<KeccakHasher, _>(
        payload.transactions.iter().map(|txn_bytes| &**txn_bytes),
    );

    // Calculate withdrawals root (post-Capella).
    let rlp_withdrawals_root = ordered_trie_root::<KeccakHasher, _>(
        payload.withdrawals.iter().map(|withdrawal| {
            rlp_encode_withdrawal(withdrawal)
        }),
    );

    // Construct the block header.
    let exec_block_header = ExecutionBlockHeader::from_payload(
        payload,
        KECCAK_EMPTY_LIST_RLP.as_fixed_bytes().into(),
        rlp_transactions_root,
        rlp_withdrawals_root,
    );


    // Hash the RLP encoding of the block header.
    let rlp_block_header = rlp_encode_block_header(&exec_block_header);
    let header_hash = H256(keccak256(&rlp_block_header).into());

    if header_hash != payload.block_hash {
        return Err(format!(
            "Block hash mismatch: expected {:?}, got {:?}",
            header_hash, payload.block_hash
        ).into());
    }

    Ok(())
}