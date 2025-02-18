//! Contains the [MultiBlockDerivationDriver] struct, which handles the [L2PayloadAttributes] derivation
//! process.
//!
//! [L2PayloadAttributes]: kona_derive::types::L2PayloadAttributes

use crate::l2_chain_provider::MultiblockOracleL2ChainProvider;
use alloc::sync::Arc;
use alloy_consensus::{Header, Sealed};
use anyhow::{anyhow, Result};
use core::fmt::Debug;
use kona_client::{
    l1::{OracleBlobProvider, OracleL1ChainProvider},
    BootInfo, HintType,
};
use kona_derive::{
    pipeline::{DerivationPipeline, Pipeline, PipelineBuilder, StepResult},
    sources::EthereumDataSource,
    stages::{
        AttributesQueue, BatchQueue, ChannelBank, ChannelReader, FrameQueue, L1Retrieval,
        L1Traversal, StatefulAttributesBuilder,
    },
    traits::{ChainProvider, L2ChainProvider},
    types::StageError,
};
use kona_mpt::TrieDBFetcher;
use kona_preimage::{CommsClient, PreimageKey, PreimageKeyType};
use kona_primitives::{BlockInfo, L2AttributesWithParent, L2BlockInfo};
use log::{debug, error};

/// An oracle-backed derivation pipeline.
pub type OraclePipeline<O> = DerivationPipeline<
    OracleAttributesQueue<OracleDataProvider<O>, O>,
    MultiblockOracleL2ChainProvider<O>,
>;

/// An oracle-backed Ethereum data source.
pub type OracleDataProvider<O> =
    EthereumDataSource<OracleL1ChainProvider<O>, OracleBlobProvider<O>>;

/// An oracle-backed payload attributes builder for the `AttributesQueue` stage of the derivation
/// pipeline.
pub type OracleAttributesBuilder<O> =
    StatefulAttributesBuilder<OracleL1ChainProvider<O>, MultiblockOracleL2ChainProvider<O>>;

/// An oracle-backed attributes queue for the derivation pipeline.
pub type OracleAttributesQueue<DAP, O> = AttributesQueue<
    BatchQueue<
        ChannelReader<
            ChannelBank<FrameQueue<L1Retrieval<DAP, L1Traversal<OracleL1ChainProvider<O>>>>>,
        >,
        MultiblockOracleL2ChainProvider<O>,
    >,
    OracleAttributesBuilder<O>,
>;

/// The [MultiBlockDerivationDriver] struct is responsible for handling the [L2PayloadAttributes] derivation
/// process.
///
/// It contains an inner [OraclePipeline] that is used to derive the attributes, backed by
/// oracle-based data sources.
///
/// [L2PayloadAttributes]: kona_derive::types::L2PayloadAttributes
#[derive(Debug)]
pub struct MultiBlockDerivationDriver<O: CommsClient + Send + Sync + Debug> {
    /// The current L2 safe head.
    pub l2_safe_head: L2BlockInfo,
    /// The header of the L2 safe head.
    pub l2_safe_head_header: Sealed<Header>,
    /// The inner pipeline.
    pub pipeline: OraclePipeline<O>,
    /// The block number of the final L2 block being claimed.
    pub l2_claim_block: u64,
}

impl<O: CommsClient + Send + Sync + Debug> MultiBlockDerivationDriver<O> {
    /// Consumes self and returns the owned [Header] of the current L2 safe head.
    pub fn clone_l2_safe_head_header(&self) -> Sealed<Header> {
        self.l2_safe_head_header.clone()
    }

    /// Creates a new [MultiBlockDerivationDriver] with the given configuration, blob provider, and chain
    /// providers.
    ///
    /// ## Takes
    /// - `cfg`: The rollup configuration.
    /// - `blob_provider`: The blob provider.
    /// - `chain_provider`: The L1 chain provider.
    /// - `l2_chain_provider`: The L2 chain provider.
    ///
    /// ## Returns
    /// - A new [MultiBlockDerivationDriver] instance.
    pub async fn new(
        boot_info: &BootInfo,
        caching_oracle: &O,
        blob_provider: OracleBlobProvider<O>,
        mut chain_provider: OracleL1ChainProvider<O>,
        mut l2_chain_provider: MultiblockOracleL2ChainProvider<O>,
    ) -> Result<Self> {
        let cfg = Arc::new(boot_info.rollup_config.clone());

        // Fetch the startup information.
        let (l1_origin, l2_safe_head, l2_safe_head_header) = Self::find_startup_info(
            caching_oracle,
            boot_info,
            &mut chain_provider,
            &mut l2_chain_provider,
        )
        .await?;

        // Construct the pipeline.
        let attributes = StatefulAttributesBuilder::new(
            cfg.clone(),
            l2_chain_provider.clone(),
            chain_provider.clone(),
        );
        let dap = EthereumDataSource::new(chain_provider.clone(), blob_provider, &cfg);
        let pipeline = PipelineBuilder::new()
            .rollup_config(cfg)
            .dap_source(dap)
            .l2_chain_provider(l2_chain_provider)
            .chain_provider(chain_provider)
            .builder(attributes)
            .origin(l1_origin)
            .build();

        let l2_claim_block = boot_info.l2_claim_block;
        Ok(Self {
            l2_safe_head,
            l2_safe_head_header,
            pipeline,
            l2_claim_block,
        })
    }

    pub fn update_safe_head(
        &mut self,
        new_safe_head: L2BlockInfo,
        new_safe_head_header: Sealed<Header>,
    ) {
        self.l2_safe_head = new_safe_head;
        self.l2_safe_head_header = new_safe_head_header;
    }

    /// Produces the disputed [Vec<L2AttributesWithParent>] payloads, starting with the one after
    /// the L2 output root, for all the payloads derived in a given span batch.
    pub async fn produce_payloads(&mut self) -> Result<Vec<L2AttributesWithParent>> {
        debug!(
            "Stepping on Pipeline for L2 Block: {}",
            self.l2_safe_head.block_info.number
        );
        match self.pipeline.step(self.l2_safe_head).await {
            StepResult::PreparedAttributes => {
                debug!("Found Attributes");
                let mut payloads = Vec::new();
                for attr in self.pipeline.by_ref() {
                    let parent_block_nb = attr.parent.block_info.number;
                    payloads.push(attr);
                    if parent_block_nb + 1 == self.l2_claim_block {
                        break;
                    }
                }
                return Ok(payloads);
            }
            StepResult::AdvancedOrigin => {
                debug!("Advanced Origin");
            }
            StepResult::OriginAdvanceErr(e) => {
                error!("Origin Advance Error: {:?}", e);
            }
            StepResult::StepFailed(e) => match e {
                StageError::NotEnoughData => {
                    debug!("Failed: Not Enough Data");
                }
                _ => {
                    error!("Failed: {:?}", e);
                }
            },
        }

        Ok(Vec::new())
    }

    /// Finds the startup information for the derivation pipeline.
    ///
    /// ## Takes
    /// - `caching_oracle`: The caching oracle.
    /// - `boot_info`: The boot information.
    /// - `chain_provider`: The L1 chain provider.
    /// - `l2_chain_provider`: The L2 chain provider.
    ///
    /// ## Returns
    /// - A tuple containing the L1 origin block information and the L2 safe head information.
    async fn find_startup_info(
        caching_oracle: &O,
        boot_info: &BootInfo,
        chain_provider: &mut OracleL1ChainProvider<O>,
        l2_chain_provider: &mut MultiblockOracleL2ChainProvider<O>,
    ) -> Result<(BlockInfo, L2BlockInfo, Sealed<Header>)> {
        // Find the initial safe head, based off of the starting L2 block number in the boot info.
        caching_oracle
            .write(&HintType::StartingL2Output.encode_with(&[boot_info.l2_output_root.as_ref()]))
            .await?;
        let mut output_preimage = [0u8; 128];
        caching_oracle
            .get_exact(
                PreimageKey::new(*boot_info.l2_output_root, PreimageKeyType::Keccak256),
                &mut output_preimage,
            )
            .await?;

        let safe_hash: alloy_primitives::FixedBytes<32> = output_preimage[96..128]
            .try_into()
            .map_err(|_| anyhow!("Invalid L2 output root"))?;
        let safe_header = l2_chain_provider.header_by_hash(safe_hash)?;
        let safe_head_info = l2_chain_provider
            .l2_block_info_by_number(safe_header.number)
            .await?;

        let l1_origin = chain_provider
            .block_info_by_number(safe_head_info.l1_origin.number)
            .await?;

        Ok((
            l1_origin,
            safe_head_info,
            Sealed::new_unchecked(safe_header, safe_hash),
        ))
    }
}
