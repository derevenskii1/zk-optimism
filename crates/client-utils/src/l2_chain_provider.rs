//! Contains the concrete implementation of the [L2ChainProvider] trait for the client program.

use alloc::{boxed::Box, sync::Arc, vec::Vec};
use alloy_consensus::Header;
use alloy_eips::eip2718::Decodable2718;
use alloy_primitives::{Address, Bytes, B256};
use alloy_rlp::Decodable;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use kona_client::{BootInfo, HintType};
use kona_derive::traits::L2ChainProvider;
use kona_mpt::{OrderedListWalker, TrieDBFetcher, TrieDBHinter};
use kona_preimage::{CommsClient, PreimageKey, PreimageKeyType};
use kona_primitives::{
    L2BlockInfo, L2ExecutionPayloadEnvelope, OpBlock, RollupConfig, SystemConfig,
};
use op_alloy_consensus::OpTxEnvelope;
use std::{collections::HashMap, sync::Mutex};

use crate::block_on;

/// The oracle-backed L2 chain provider for the client program.
#[derive(Debug, Clone)]
pub struct MultiblockOracleL2ChainProvider<T: CommsClient> {
    /// The boot information
    boot_info: Arc<BootInfo>,
    /// The preimage oracle client.
    oracle: Arc<T>,
    /// Cached headers by block number.
    header_by_number: Arc<Mutex<HashMap<u64, Header>>>,
    /// Cached L2 block info by block number.
    l2_block_info_by_number: Arc<Mutex<HashMap<u64, L2BlockInfo>>>,
    /// Cached payloads by block number.
    payload_by_number: Arc<Mutex<HashMap<u64, L2ExecutionPayloadEnvelope>>>,
    /// Cached system configs by block number.
    system_config_by_number: Arc<Mutex<HashMap<u64, SystemConfig>>>,
}

impl<T: CommsClient> MultiblockOracleL2ChainProvider<T> {
    /// Creates a new [MultiblockOracleL2ChainProvider] with the given boot information and oracle client.
    pub fn new(boot_info: Arc<BootInfo>, oracle: Arc<T>) -> Self {
        Self {
            boot_info,
            oracle,
            header_by_number: Arc::new(Mutex::new(HashMap::new())),
            l2_block_info_by_number: Arc::new(Mutex::new(HashMap::new())),
            payload_by_number: Arc::new(Mutex::new(HashMap::new())),
            system_config_by_number: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl<T: CommsClient> MultiblockOracleL2ChainProvider<T> {
    // After each block, update the cache with the new, executed block's data, which is now trusted.
    pub fn update_cache(
        &mut self,
        header: &Header,
        payload: L2ExecutionPayloadEnvelope,
        config: &RollupConfig,
    ) -> Result<L2BlockInfo> {
        self.header_by_number
            .lock()
            .unwrap()
            .insert(header.number, header.clone());
        self.payload_by_number
            .lock()
            .unwrap()
            .insert(header.number, payload.clone());
        self.system_config_by_number
            .lock()
            .unwrap()
            .insert(header.number, payload.to_system_config(config).unwrap());

        let l2_block_info = payload.to_l2_block_ref(config)?;
        self.l2_block_info_by_number
            .lock()
            .unwrap()
            .insert(header.number, l2_block_info);
        Ok(l2_block_info)
    }

    /// Returns a [Header] corresponding to the given L2 block number, by walking back from the
    /// L2 safe head.
    pub async fn header_by_number(&mut self, block_number: u64) -> Result<Header> {
        // First, check if it's already in the cache.
        if let Some(header) = self.header_by_number.lock().unwrap().get(&block_number) {
            return Ok(header.clone());
        }

        // Fetch the starting L2 output preimage.
        self.oracle
            .write(
                &HintType::StartingL2Output.encode_with(&[self.boot_info.l2_output_root.as_ref()]),
            )
            .await?;
        let output_preimage = self
            .oracle
            .get(PreimageKey::new(
                *self.boot_info.l2_output_root,
                PreimageKeyType::Keccak256,
            ))
            .await?;

        // Fetch the starting block header.
        let block_hash = output_preimage[96..128]
            .try_into()
            .map_err(|e| anyhow!("Failed to extract block hash from output preimage: {e}"))?;
        let mut header = self.header_by_hash(block_hash)?;

        // Check if the block number is in range. If not, we can fail early.
        if block_number > header.number {
            anyhow::bail!("Block number past L2 head.");
        }

        // Walk back the block headers to the desired block number.
        while header.number > block_number {
            header = self.header_by_hash(header.parent_hash)?;
        }

        Ok(header)
    }
}

#[async_trait]
impl<T: CommsClient + Send + Sync> L2ChainProvider for MultiblockOracleL2ChainProvider<T> {
    async fn l2_block_info_by_number(&mut self, number: u64) -> Result<L2BlockInfo> {
        // First, check if it's already in the cache.
        if let Some(l2_block_info) = self.l2_block_info_by_number.lock().unwrap().get(&number) {
            return Ok(*l2_block_info);
        }

        // Get the payload at the given block number.
        let payload = self.payload_by_number(number).await?;

        // Construct the system config from the payload.
        payload.to_l2_block_ref(&self.boot_info.rollup_config)
    }

    async fn payload_by_number(&mut self, number: u64) -> Result<L2ExecutionPayloadEnvelope> {
        // First, check if it's already in the cache.
        if let Some(payload) = self.payload_by_number.lock().unwrap().get(&number) {
            return Ok(payload.clone());
        }

        // Fetch the header for the given block number.
        let header @ Header {
            transactions_root,
            timestamp,
            ..
        } = self.header_by_number(number).await?;
        let header_hash = header.hash_slow();

        // Fetch the transactions in the block.
        self.oracle
            .write(&HintType::L2Transactions.encode_with(&[header_hash.as_ref()]))
            .await?;
        let trie_walker = OrderedListWalker::try_new_hydrated(transactions_root, self)?;

        // Decode the transactions within the transactions trie.
        let transactions = trie_walker
            .into_iter()
            .map(|(_, rlp)| {
                OpTxEnvelope::decode_2718(&mut rlp.as_ref())
                    .map_err(|e| anyhow!("Failed to decode TxEnvelope RLP: {e}"))
            })
            .collect::<Result<Vec<_>>>()?;

        let optimism_block = OpBlock {
            header,
            body: transactions,
            withdrawals: self
                .boot_info
                .rollup_config
                .is_canyon_active(timestamp)
                .then(Vec::new),
            ..Default::default()
        };
        Ok(optimism_block.into())
    }

    async fn system_config_by_number(
        &mut self,
        number: u64,
        rollup_config: Arc<RollupConfig>,
    ) -> Result<SystemConfig> {
        // First, check if it's already in the cache.
        if let Some(system_config) = self.system_config_by_number.lock().unwrap().get(&number) {
            return Ok(system_config.clone());
        }

        // Get the payload at the given block number.
        let payload = self.payload_by_number(number).await?;

        // Construct the system config from the payload.
        payload.to_system_config(rollup_config.as_ref())
    }
}

impl<T: CommsClient> TrieDBFetcher for MultiblockOracleL2ChainProvider<T> {
    fn trie_node_preimage(&self, key: B256) -> Result<Bytes> {
        // On L2, trie node preimages are stored as keccak preimage types in the oracle. We assume
        // that a hint for these preimages has already been sent, prior to this call.
        block_on(async move {
            self.oracle
                .get(PreimageKey::new(*key, PreimageKeyType::Keccak256))
                .await
                .map(Into::into)
        })
    }

    fn bytecode_by_hash(&self, hash: B256) -> Result<Bytes> {
        // Fetch the bytecode preimage from the caching oracle.
        block_on(async move {
            self.oracle
                .write(&HintType::L2Code.encode_with(&[hash.as_ref()]))
                .await?;

            self.oracle
                .get(PreimageKey::new(*hash, PreimageKeyType::Keccak256))
                .await
                .map(Into::into)
        })
    }

    fn header_by_hash(&self, hash: B256) -> Result<Header> {
        // Fetch the header from the caching oracle.
        block_on(async move {
            self.oracle
                .write(&HintType::L2BlockHeader.encode_with(&[hash.as_ref()]))
                .await?;

            let header_bytes = self
                .oracle
                .get(PreimageKey::new(*hash, PreimageKeyType::Keccak256))
                .await?;
            Header::decode(&mut header_bytes.as_slice())
                .map_err(|e| anyhow!("Failed to RLP decode Header: {e}"))
        })
    }
}

impl<T: CommsClient> TrieDBHinter for MultiblockOracleL2ChainProvider<T> {
    fn hint_trie_node(&self, hash: B256) -> Result<()> {
        block_on(async move {
            self.oracle
                .write(&HintType::L2StateNode.encode_with(&[hash.as_slice()]))
                .await
        })
    }

    fn hint_account_proof(&self, address: Address, block_number: u64) -> Result<()> {
        block_on(async move {
            self.oracle
                .write(
                    &HintType::L2AccountProof
                        .encode_with(&[block_number.to_be_bytes().as_ref(), address.as_slice()]),
                )
                .await
        })
    }

    fn hint_storage_proof(
        &self,
        address: alloy_primitives::Address,
        slot: alloy_primitives::U256,
        block_number: u64,
    ) -> Result<()> {
        block_on(async move {
            self.oracle
                .write(&HintType::L2AccountStorageProof.encode_with(&[
                    block_number.to_be_bytes().as_ref(),
                    address.as_slice(),
                    slot.to_be_bytes::<32>().as_ref(),
                ]))
                .await
        })
    }
}
