//! Wallet snapshot and output adapters for the shared transaction planner.
use tx_types::lock_resolver::{LockMatcher, SigningKeyLockMatcher};
pub(crate) use tx_types::planner::PlannedOutput;
use tx_types::planner::{
    self, CandidateVersionPolicy, ChainContext, FeePolicy, PlanRequest, PlanningMode,
    SelectionMode, SelectionOrder,
};
use tx_types::{Hash, NNote, NNoteV1, SpendCondition, SpendsV1};

pub(crate) fn matcher(source: &Hash) -> SigningKeyLockMatcher {
    SigningKeyLockMatcher {
        signer_pkh: source.clone(),
        public_key: None,
        coinbase_relative_min: Some(100),
    }
}

pub(crate) fn pkh_lock(source: &Hash) -> SpendCondition {
    tx_types::lock_resolver::simple_pkh(source.clone())
}

pub(crate) fn payment(source: &Hash, amount: u64, private: bool) -> PlannedOutput {
    PlannedOutput::payment(source.clone(), amount, private).expect("validated wallet address")
}

#[cfg(test)]
pub(crate) fn spendable_lock(
    note: &NNoteV1,
    source: &Hash,
    height: Option<u64>,
) -> Option<SpendCondition> {
    let resolved = matcher(source).resolve(note)?;
    resolved
        .admits(note, height)
        .then(|| resolved.spend_condition())
        .flatten()
}

pub(crate) fn plan(
    notes: &[NNoteV1],
    outputs: &[PlannedOutput],
    source: &Hash,
    private: bool,
    height: Option<u64>,
    fee: FeePolicy,
    mode: PlanningMode,
) -> Result<planner::PlanResult, String> {
    let candidates: Vec<_> = notes.iter().cloned().map(NNote::V1).collect();
    let refund = payment(source, 0, private);
    let chain = ChainContext {
        candidate_height: height,
        ..ChainContext::at_height(height.unwrap_or(tx_types::fee::BYTHOS_PHASE))
    };
    planner::plan(
        PlanRequest {
            candidates: &candidates,
            outputs,
            refund: &refund,
            selection: SelectionMode::Auto,
            order: SelectionOrder::Descending,
            version: CandidateVersionPolicy::V1Only,
            mode,
            fee,
            chain,
        },
        &matcher(source),
    )
    .map_err(|error| error.to_string())
}

#[cfg(test)]
pub(crate) fn plan_spends(
    notes: &[NNoteV1],
    outputs: &[PlannedOutput],
    fee: u64,
    source: &Hash,
    private: bool,
    height: Option<u64>,
) -> Result<SpendsV1, String> {
    plan(
        notes,
        outputs,
        source,
        private,
        height,
        FeePolicy::Exact(fee),
        PlanningMode::Standard,
    )
    .map(|result| result.spends)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tx_types::{Coins, LockPrimitive, LockPrimitiveBody, NName, NoteData, SpendBody, ZMap};
    fn hash(n: u64) -> Hash {
        Hash { values: [n; 5] }
    }
    fn note(source: &Hash, id: u64, amount: u64) -> NNoteV1 {
        NNoteV1 {
            version: 1,
            origin_page: tx_types::PageNumber { value: 1000 },
            name: NName {
                p: vec![NName::first_v1(pkh_lock(source).to_hash()), hash(id)],
            },
            note_data: NoteData { map: ZMap::new() },
            assets: Coins { value: amount },
        }
    }
    #[test]
    fn each_input_balances_and_commits_to_full_note() {
        let source = hash(1);
        let notes = vec![
            note(&source, 2, 100),
            note(&source, 3, 90),
            note(&source, 4, 80),
        ];
        let spends = plan_spends(
            &notes,
            &[payment(&hash(5), 110, false)],
            130,
            &source,
            false,
            None,
        )
        .unwrap();
        let mut total_fee = 0;
        for (name, spend) in spends.map.tap() {
            let input = notes.iter().find(|n| n.name == name).unwrap();
            let SpendBody::V1(body) = spend.body else {
                panic!()
            };
            assert!(matches!(
                body.witness.lmp,
                tx_types::LockMerkleProof::Full(_)
            ));
            let seeds = body.seeds.set.tap();
            assert_eq!(
                seeds.iter().map(|s| s.gift.value).sum::<u64>() + body.fee.value,
                input.assets.value
            );
            for seed in seeds {
                assert_eq!(seed.parent_hash, input.to_hash().expect("note hash"));
            }
            total_fee += body.fee.value;
        }
        assert_eq!(total_fee, 130);
    }
    #[test]
    fn duplicate_recipients_and_self_change_preserve_value() {
        let source = hash(1);
        let notes = vec![note(&source, 2, 1000)];
        let outputs = vec![payment(&source, 300, false), payment(&source, 300, false)];
        let spends = plan_spends(&notes, &outputs, 100, &source, false, None).unwrap();
        let SpendBody::V1(body) = &spends.map.tap()[0].1.body else {
            panic!()
        };
        assert_eq!(body.seeds.set.wyt(), 1);
        assert_eq!(body.seeds.set.tap()[0].gift.value, 900);
    }
    #[test]
    fn bridge_splits_do_not_redirect_equal_sized_payments() {
        let source = hash(1);
        let notes = vec![
            note(&source, 2, 100),
            note(&source, 3, 100),
            note(&source, 4, 100),
            note(&source, 5, 100),
        ];
        let bridge = PlannedOutput {
            lock_root: hash(9),
            note_data: NoteData { map: ZMap::new() },
            amount: 150,
        };
        let normal = payment(&hash(8), 150, false);
        let spends = plan_spends(
            &notes,
            &[bridge.clone(), normal.clone()],
            10,
            &source,
            false,
            None,
        )
        .unwrap();
        let mut bridge_total = 0;
        let mut normal_total = 0;
        for (_, spend) in spends.map.tap() {
            let SpendBody::V1(body) = spend.body else {
                panic!()
            };
            for seed in body.seeds.set.tap() {
                if seed.lock_root == bridge.lock_root {
                    bridge_total += seed.gift.value;
                }
                if seed.lock_root == normal.lock_root {
                    normal_total += seed.gift.value;
                }
            }
        }
        assert_eq!((bridge_total, normal_total), (150, 150));
    }
    #[test]
    fn duplicate_notes_and_overflow_are_rejected() {
        let source = hash(1);
        let n = note(&source, 2, 100);
        assert!(plan_spends(
            &[n.clone(), n],
            &[payment(&source, 10, false)],
            1,
            &source,
            false,
            None
        )
        .is_err());
        assert!(plan_spends(
            &[],
            &[payment(&source, u64::MAX, false)],
            1,
            &source,
            false,
            None
        )
        .is_err());
    }
    #[test]
    fn coinbase_requires_maturity_and_matching_owner() {
        let source = hash(1);
        let mut n = note(&source, 2, 1000);
        let mut lock = pkh_lock(&source);
        lock.p.push(LockPrimitive {
            header: "tim".into(),
            body: LockPrimitiveBody::Tim(tx_types::Tim {
                rel: tx_types::TimelockRange {
                    min: Some(tx_types::PageNumber { value: 100 }),
                    max: None,
                },
                abs: tx_types::TimelockRange {
                    min: None,
                    max: None,
                },
            }),
        });
        n.name.p[0] = NName::first_v1(lock.to_hash());
        assert!(spendable_lock(&n, &source, None).is_none());
        assert!(spendable_lock(&n, &source, Some(1099)).is_none());
        assert!(spendable_lock(&n, &source, Some(1100)).is_some());
        assert!(spendable_lock(&n, &hash(7), Some(1100)).is_none());
    }
    #[test]
    fn foreign_and_locked_notes_do_not_block_spendable_funds() {
        let source = hash(1);
        let notes = vec![note(&hash(9), 3, 9000), note(&source, 2, 1000)];
        let spends = plan_spends(
            &notes,
            &[payment(&hash(8), 50, true)],
            10,
            &source,
            true,
            None,
        )
        .unwrap();
        assert_eq!(spends.map.wyt(), 1);
        let SpendBody::V1(body) = &spends.map.tap()[0].1.body else {
            panic!()
        };
        assert!(body
            .seeds
            .set
            .tap()
            .iter()
            .all(|s| s.note_data.map.wyt() == 0));
    }
}
