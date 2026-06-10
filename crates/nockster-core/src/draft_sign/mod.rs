//! no_std draft signing helpers (noun cue/jam + TIP5 hashing + minimal tx parsing).
//!
//! This module exists so firmware can sign a jammed transaction draft without
//! pulling in `nockvm`/`noun-serde` (std-only in this repo).
#![cfg_attr(
    not(feature = "std"),
    allow(clippy::panic, reason = "firmware uses panic-halt")
)]

extern crate alloc;

mod noun_codec;
mod tip5;
mod tx_v1;
mod zmap;

pub use noun_codec::{cue, jam, Arena, Noun};
pub use tx_v1::{
    cheetah_pubkey_pkh_v1, draft_outputs_v1, draft_review_v1, draft_review_v1_for_pkh,
    message_digest_v1, noun_commitment_v1,
    rewrite_txid_v1, sign_draft_v1, tip5_digest_b58, DraftOutputV1, DraftReviewV1, LockPrimitiveV1,
    LockSummaryV1, MultisigInputV1, RewriteTxIdV1, SignDraftError, SignerConfig,
};
