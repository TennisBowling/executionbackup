// Copyright 2017, 2018 Parity Technologies
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
use ethereum_types::H256;
use hash256_std_hasher::Hash256StdHasher;
use hash_db::Hasher;

pub fn keccak256(bytes: &[u8]) -> H256 {
    H256::from(ethers_core::utils::keccak256(bytes))
}

/// Keccak hasher.
#[derive(Default, Debug, Clone, PartialEq)]
pub struct KeccakHasher;

impl Hasher for KeccakHasher {
    type Out = H256;
    type StdHasher = Hash256StdHasher;

    const LENGTH: usize = 32;

    fn hash(x: &[u8]) -> Self::Out {
        keccak256(x)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use array_bytes::hex_n_into;

    #[test]
    fn test_keccakhasher() -> Result<(), Box<dyn std::error::Error>> {
        let res = keccak256(b"hello world");
        // value from https://emn178.github.io/online-tools/keccak_256.html
        let real: H256 = hex_n_into::<_, H256, 32>(
            "47173285a8d7341e5e972fc677286384f802f8ef42a5ec5f03bbfa254cb01fad",
        )
        .map_err(|e| format!("Value is not a valid hex string: {:?}", e))?;
        println!("res: {:?}", res);
        println!("real: {:?}", real);
        assert!(res == real);

        Ok(())
    }
}
