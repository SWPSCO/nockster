//! Single-signer transaction planning shared by software and hardware wallets.
use tx_types::transaction_types::{Chal, Sig, F6LT};
use tx_types::{
    block_types::build_lock_merkle_proof, lock_data_to_untyped_noun, Coins, Hash, LockData,
    LockPrimitive, LockPrimitiveBody, NName, NNoteV1, NoteData, Pkh, PkhSignature,
    PkhSignatureValue, SchnorrPubkey, SchnorrSignature, SeedV1, SeedsV1, Spend, SpendBody,
    SpendCondition, SpendV1, SpendsV1, Witness, ZMap, ZSet,
};

#[derive(Debug, Clone)]
pub(crate) struct Output {
    pub lock_root: Hash,
    pub note_data: NoteData,
    pub amount: u64,
}

pub(crate) fn pkh_lock(pkh: &Hash) -> SpendCondition {
    SpendCondition {
        p: vec![LockPrimitive {
            header: "pkh".into(),
            body: LockPrimitiveBody::Pkh(Pkh {
                m: 1,
                h: ZSet::gas(vec![pkh.clone()]),
            }),
        }],
    }
}

pub(crate) fn payment(pkh: &Hash, amount: u64, private: bool) -> Output {
    let lock = pkh_lock(pkh);
    let mut map = ZMap::new();
    if !private {
        map.put(
            "lock".into(),
            lock_data_to_untyped_noun(&LockData::V0(lock.clone())),
        );
    }
    Output {
        lock_root: lock.to_hash(),
        note_data: NoteData { map },
        amount,
    }
}

/// Resolve the disclosed lock or a standard payment/coinbase lock and enforce its timelocks.
pub(crate) fn spendable_lock(
    note: &NNoteV1,
    source: &Hash,
    height: Option<u64>,
) -> Option<SpendCondition> {
    let mut lock = pkh_lock(source);
    if let Some(noun) = note.note_data.map.get(&"lock".into()) {
        let LockData::V0(disclosed) = noun.to_typed::<LockData>().ok()?;
        lock = disclosed;
    } else if note.name.p.first()? != &NName::first_v1(lock.to_hash()) {
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
    }
    if note.name.p.len() != 2 || note.name.p[0] != NName::first_v1(lock.to_hash()) {
        return None;
    }
    let mut has_key = false;
    for primitive in &lock.p {
        match &primitive.body {
            LockPrimitiveBody::Pkh(pkh) if pkh.m == 1 && pkh.h.wyt() == 1 && pkh.h.has(source) => {
                has_key = true;
            }
            LockPrimitiveBody::Tim(tim) => {
                let height = height?;
                let relative = height.checked_sub(note.origin_page.value)?;
                for (range, value) in [(&tim.rel, relative), (&tim.abs, height)] {
                    if range.min.as_ref().is_some_and(|min| value < min.value)
                        || range.max.as_ref().is_some_and(|max| value > max.value)
                    {
                        return None;
                    }
                }
            }
            _ => return None,
        }
    }
    has_key.then_some(lock)
}

/// Every input balances independently; every seed commits to the full input note.
pub(crate) fn plan_spends(
    notes: &[NNoteV1],
    outputs: &[Output],
    fee: u64,
    source: &Hash,
    private: bool,
    height: Option<u64>,
) -> Result<SpendsV1, String> {
    let total = outputs.iter().try_fold(fee, |sum, output| {
        sum.checked_add(output.amount)
            .ok_or("transaction amount overflow")
    })?;
    let mut notes = notes.to_vec();
    notes.sort_by(|a, b| b.assets.value.cmp(&a.assets.value));
    let mut names = std::collections::HashSet::new();
    for note in &notes {
        if !names.insert(note.name.clone()) {
            return Err("duplicate input note".into());
        }
    }
    notes.retain(|note| spendable_lock(note, source, height).is_some());
    let available = notes.iter().try_fold(0u64, |sum, note| {
        sum.checked_add(note.assets.value)
            .ok_or("input amount overflow")
    })?;
    if available < total {
        return Err("insufficient funds for payments and network fee".into());
    }

    let mut remaining: Vec<u64> = outputs.iter().map(|o| o.amount).collect();
    let mut remaining_fee = fee;
    let mut needed = total;
    let mut spends = ZMap::new();
    for note in notes {
        if needed == 0 {
            break;
        }
        let mut capacity = note.assets.value;
        let input_fee = capacity.min(remaining_fee);
        remaining_fee -= input_fee;
        capacity -= input_fee;
        let mut allocated: Vec<Output> = Vec::new();
        for (output, left) in outputs.iter().zip(remaining.iter_mut()) {
            let amount = capacity.min(*left);
            if amount > 0 {
                let mut part = output.clone();
                part.amount = amount;
                allocated.push(part);
                *left -= amount;
                capacity -= amount;
            }
        }
        needed -= note.assets.value - capacity;
        if capacity > 0 {
            allocated.push(payment(source, capacity, private));
        }

        // Equal seeds are a set element. Merge allocations before constructing the set.
        let mut merged: Vec<Output> = Vec::new();
        for output in allocated {
            if let Some(existing) = merged.iter_mut().find(|o| o.lock_root == output.lock_root) {
                if existing.note_data.to_hash() != output.note_data.to_hash() {
                    return Err("conflicting note data for the same output lock".into());
                }
                existing.amount = existing
                    .amount
                    .checked_add(output.amount)
                    .ok_or("output overflow")?;
            } else {
                merged.push(output);
            }
        }
        let seeds = SeedsV1 {
            set: ZSet::gas(
                merged
                    .into_iter()
                    .map(|output| SeedV1 {
                        output_source: None,
                        lock_root: output.lock_root,
                        note_data: output.note_data,
                        gift: Coins {
                            value: output.amount,
                        },
                        parent_hash: note.to_hash(),
                    })
                    .collect::<Vec<_>>(),
            ),
        };
        spends.put(
            note.name.clone(),
            Spend {
                version: 1,
                body: SpendBody::V1(SpendV1 {
                    witness: Witness {
                        lmp: build_lock_merkle_proof(
                            spendable_lock(&note, source, height).expect("selected spendable note"),
                            1,
                        ),
                        pkh: PkhSignature { map: ZMap::new() },
                        hax: ZMap::new(),
                        tim: 0,
                    },
                    seeds,
                    fee: Coins { value: input_fee },
                }),
            },
        );
    }
    Ok(SpendsV1 { map: spends })
}

/// Fee accounting counts noun leaves, so fixed-size signature fields give an exact estimate.
/// These placeholders are used only for sizing and never enter an exported transaction.
pub(crate) fn signed_fee(spends: &SpendsV1, source: &Hash) -> u64 {
    let mut map = ZMap::new();
    for (name, mut spend) in spends.map.tap() {
        if let SpendBody::V1(ref mut body) = spend.body {
            body.witness.pkh.map.put(
                source.clone(),
                PkhSignatureValue {
                    pk: SchnorrPubkey {
                        x: F6LT { values: [1; 6] },
                        y: F6LT { values: [1; 6] },
                        inf: false,
                    },
                    sig: SchnorrSignature {
                        chal: Chal {
                            values: tx_types::T8 { values: [1; 8] },
                        },
                        sig: Sig {
                            values: tx_types::T8 { values: [1; 8] },
                        },
                    },
                },
            );
        }
        map.put(name, spend);
    }
    tx_types::fee_calculator::calculate_min_fee(&map).value
}

#[cfg(test)]
mod tests {
    use super::*;
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
                assert_eq!(seed.parent_hash, input.to_hash());
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
        let bridge = Output {
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
