use bytes::Bytes;
use k256::elliptic_curve::rand_core::{OsRng, RngCore};
use nockapp::noun::slab::NounSlab;
use nockapp::{AtomExt, NounAllocator, NounSpace};
use nockvm::noun::{D, T};
use noun_serde::{NounDecode, NounEncode};

use tx_types::collections::{ZMap, ZSet};
use tx_types::generic_noun::UntypedNoun;
use tx_types::transaction_types::{
    Chal, Coins, Hash, NName, SchnorrPubkey, Sig, Source, Spend, SpendBody, F6LT, T8,
};
use tx_types::transaction_types_v1::{
    compute_tx_id_v1, Hax, LockMerkleProof, LockPrimitive, LockPrimitiveBody, MerkleProof,
    NoteData, Pkh, PkhSignature, PkhSignatureValue, RawTransactionV1, SeedV1, SeedsV1,
    SpendCondition, SpendV1, SpendsV1, Witness,
};

fn jam_single(noun: nockvm::noun::Noun) -> Vec<u8> {
    let mut slab: NounSlab = NounSlab::new();
    let copied = slab.copy_into(noun, &NounSpace::empty());
    slab.set_root(copied);
    slab.jam().to_vec()
}

fn empty_stub_lock_merkle_proof(spend_condition: SpendCondition) -> LockMerkleProof {
    LockMerkleProof::new_stub(
        spend_condition,
        0,
        MerkleProof {
            root: Hash { values: [0; 5] },
            path: Vec::new(),
        },
    )
}

fn empty_full_lock_merkle_proof(spend_condition: SpendCondition) -> LockMerkleProof {
    LockMerkleProof::new_full(
        spend_condition,
        0,
        MerkleProof {
            root: Hash { values: [0; 5] },
            path: Vec::new(),
        },
    )
}

fn note_data_for_lock(spend_condition: &SpendCondition) -> NoteData {
    let lock_data = tx_types::tx_builder_v1::LockData::V0(spend_condition.clone());
    let mut map = ZMap::new();
    map.put(
        "lock".to_string(),
        tx_types::tx_builder_v1::lock_data_to_untyped_noun(&lock_data),
    );
    NoteData { map }
}

fn runtime_signer_scalar() -> [u8; 32] {
    let mut rng = OsRng;
    let mut scalar = [0u8; 32];
    rng.fill_bytes(&mut scalar);
    scalar[0] = 1;
    scalar[31] |= 1;
    scalar
}

#[test]
fn cheetah_pubkey_pkh_v1_matches_tx_types_encoder() {
    let pk_xy = (
        [0x101, 0x102, 0x103, 0x104, 0x105, 0x106],
        [0x201, 0x202, 0x203, 0x204, 0x205, 0x206],
    );
    let pkh = nockster_core::draft_sign::cheetah_pubkey_pkh_v1(pk_xy).expect("pkh");
    let tx_types_pkh = SchnorrPubkey {
        x: F6LT { values: pk_xy.0 },
        y: F6LT { values: pk_xy.1 },
        inf: false,
    }
    .to_hash()
    .to_b58();
    assert_eq!(pkh, tx_types_pkh);
}

#[test]
fn sign_draft_v1_inserts_expected_signature() {
    let sk_be = runtime_signer_scalar();

    // Compute the signing pubkey + pkh that will be authorized in the lock.
    let pk_arr = nockster_core::cheetah_pub_from_sk(sk_be);
    let pk = SchnorrPubkey {
        x: F6LT { values: pk_arr[0] },
        y: F6LT { values: pk_arr[1] },
        inf: false,
    };
    let pkh = pk.to_hash();

    // Build a minimal V1 raw-tx with one spend and empty seeds/note-data.
    let mut allowed = ZSet::new();
    allowed.put(pkh.clone());
    let spend_condition = SpendCondition {
        p: vec![tx_types::transaction_types_v1::LockPrimitive {
            header: "pkh".to_string(),
            body: tx_types::transaction_types_v1::LockPrimitiveBody::Pkh(
                tx_types::transaction_types_v1::Pkh { m: 1, h: allowed },
            ),
        }],
    };
    let witness = Witness {
        lmp: empty_full_lock_merkle_proof(spend_condition),
        pkh: PkhSignature { map: ZMap::new() },
        hax: ZMap::<Hash, UntypedNoun>::new(),
        tim: 0,
    };

    let spend_body = SpendV1 {
        witness,
        seeds: tx_types::transaction_types_v1::SeedsV1 { set: ZSet::new() },
        fee: Coins { value: 10_000_000 },
    };
    let spend = Spend {
        version: 1,
        body: SpendBody::V1(spend_body.clone()),
    };

    let name = NName {
        p: vec![
            Hash {
                values: [1, 2, 3, 4, 5],
            },
            Hash {
                values: [6, 7, 8, 9, 10],
            },
        ],
    };

    let mut spends_map: ZMap<NName, Spend> = ZMap::new();
    spends_map.put(name.clone(), spend.clone());

    let raw = tx_types::transaction_types_v1::RawTransactionV1 {
        version: 1,
        id: Hash { values: [0; 5] },
        spends: SpendsV1 { map: spends_map },
    };

    let mut slab: NounSlab = NounSlab::new();
    let noun = raw.to_noun(&mut slab);
    slab.set_root(noun);
    let draft_jam = slab.jam().to_vec();

    let signed_jam = nockster_core::draft_sign::sign_draft_v1(
        &draft_jam,
        &nockster_core::draft_sign::SignerConfig { sk_be },
    )
    .expect("sign_draft_v1");

    // Decode the signed transaction with the reference types.
    let mut slab2: NounSlab = NounSlab::new();
    let noun2 = slab2
        .cue_into(Bytes::from(signed_jam))
        .expect("cue signed jam");
    let signed_raw = tx_types::transaction_types_v1::RawTransactionV1::from_noun(&noun2, &slab2.noun_space())
        .expect("decode v1 raw-tx");

    let computed_id = compute_tx_id_v1(&signed_raw.spends);
    assert_eq!(signed_raw.id, computed_id, "tx-id should be recomputed");

    // Verify the inserted signature matches signing the tx-types sig-hash.
    let spend_entry = signed_raw
        .spends
        .map
        .get(&name)
        .expect("signed spend present");

    let SpendBody::V1(sv1) = &spend_entry.body else {
        panic!("expected SpendBody::V1");
    };

    let msg5 = sv1.compute_sig_hash().values;
    let (chal, sig) = nockster_core::schnorr_sign_tx(sk_be, (pk_arr[0], pk_arr[1]), msg5);

    let inserted = sv1
        .witness
        .pkh
        .map
        .get(&pkh)
        .expect("pkh signature inserted");

    assert_eq!(inserted.pk, pk, "inserted pubkey mismatch");
    assert_eq!(
        inserted.sig.chal.values.values, chal.values,
        "challenge mismatch"
    );
    assert_eq!(
        inserted.sig.sig.values.values, sig.values,
        "signature mismatch"
    );
}

#[test]
fn sign_draft_v1_handles_seeds_and_note_data() {
    let sk_be = runtime_signer_scalar();
    let pk_arr = nockster_core::cheetah_pub_from_sk(sk_be);
    let pk = SchnorrPubkey {
        x: F6LT { values: pk_arr[0] },
        y: F6LT { values: pk_arr[1] },
        inf: false,
    };
    let pkh = pk.to_hash();

    // Witness can be mostly empty; signatures depend only on seeds+fee for V1.
    let mut allowed = ZSet::new();
    allowed.put(pkh.clone());
    let spend_condition = SpendCondition {
        p: vec![tx_types::transaction_types_v1::LockPrimitive {
            header: "pkh".to_string(),
            body: tx_types::transaction_types_v1::LockPrimitiveBody::Pkh(
                tx_types::transaction_types_v1::Pkh { m: 1, h: allowed },
            ),
        }],
    };
    let witness = Witness {
        lmp: empty_stub_lock_merkle_proof(spend_condition.clone()),
        pkh: PkhSignature { map: ZMap::new() },
        hax: ZMap::<Hash, UntypedNoun>::new(),
        tim: 0,
    };

    // note-data with a small u64-only noun value to stay in the no_std signer subset.
    let mut note_map: ZMap<String, UntypedNoun> = ZMap::new();
    let mut tmp: NounSlab = NounSlab::new();
    let val_noun = T(&mut tmp, &[D(1), D(2), D(3)]);
    let val_untyped = UntypedNoun::from_noun(&val_noun, &tmp.noun_space()).expect("untyped noun");
    note_map.put("memo".to_string(), val_untyped);
    let note_data = NoteData { map: note_map };

    let seed1 = SeedV1 {
        output_source: None,
        lock_root: Hash {
            values: [11, 12, 13, 14, 15],
        },
        note_data: note_data.clone(),
        gift: Coins { value: 5 },
        parent_hash: Hash {
            values: [21, 22, 23, 24, 25],
        },
    };

    let seed2 = SeedV1 {
        output_source: Some(Source {
            p: Hash {
                values: [31, 32, 33, 34, 35],
            },
            is_coinbase: false,
        }),
        lock_root: Hash {
            values: [41, 42, 43, 44, 45],
        },
        note_data: NoteData { map: ZMap::new() },
        gift: Coins { value: 9 },
        parent_hash: Hash {
            values: [51, 52, 53, 54, 55],
        },
    };

    let mut seed_set: ZSet<SeedV1> = ZSet::new();
    seed_set.put(seed1);
    seed_set.put(seed2);

    let spend_body = SpendV1 {
        witness,
        seeds: SeedsV1 { set: seed_set },
        fee: Coins { value: 10_000_000 },
    };
    let spend = Spend {
        version: 1,
        body: SpendBody::V1(spend_body.clone()),
    };

    let name = NName {
        p: vec![
            Hash {
                values: [1, 2, 3, 4, 5],
            },
            Hash {
                values: [6, 7, 8, 9, 10],
            },
        ],
    };

    let mut spends_map: ZMap<NName, Spend> = ZMap::new();
    spends_map.put(name.clone(), spend.clone());

    let raw = tx_types::transaction_types_v1::RawTransactionV1 {
        version: 1,
        id: Hash { values: [0; 5] },
        spends: SpendsV1 { map: spends_map },
    };

    let mut slab: NounSlab = NounSlab::new();
    let noun = raw.to_noun(&mut slab);
    slab.set_root(noun);
    let draft_jam = slab.jam().to_vec();

    let signed_jam = nockster_core::draft_sign::sign_draft_v1(
        &draft_jam,
        &nockster_core::draft_sign::SignerConfig { sk_be },
    )
    .expect("sign_draft_v1");

    let mut slab2: NounSlab = NounSlab::new();
    let noun2 = slab2
        .cue_into(Bytes::from(signed_jam))
        .expect("cue signed jam");
    let signed_raw = tx_types::transaction_types_v1::RawTransactionV1::from_noun(&noun2, &slab2.noun_space())
        .expect("decode v1 raw-tx");

    let computed_id = compute_tx_id_v1(&signed_raw.spends);
    assert_eq!(signed_raw.id, computed_id, "tx-id should be recomputed");

    // Compute expected pubkey + pkh.
    let pk_arr = nockster_core::cheetah_pub_from_sk(sk_be);
    let pk = SchnorrPubkey {
        x: F6LT { values: pk_arr[0] },
        y: F6LT { values: pk_arr[1] },
        inf: false,
    };
    let pkh = pk.to_hash();

    let spend_entry = signed_raw
        .spends
        .map
        .get(&name)
        .expect("signed spend present");
    let SpendBody::V1(sv1) = &spend_entry.body else {
        panic!("expected SpendBody::V1");
    };

    let msg5 = sv1.compute_sig_hash().values;
    let (chal, sig) = nockster_core::schnorr_sign_tx(sk_be, (pk_arr[0], pk_arr[1]), msg5);

    let inserted = sv1
        .witness
        .pkh
        .map
        .get(&pkh)
        .expect("pkh signature inserted");

    assert_eq!(inserted.pk, pk, "inserted pubkey mismatch");
    assert_eq!(
        inserted.sig.chal.values.values, chal.values,
        "challenge mismatch"
    );
    assert_eq!(
        inserted.sig.sig.values.values, sig.values,
        "signature mismatch"
    );
}

#[test]
fn draft_review_v1_reports_totals_and_refund() {
    let sk_be = runtime_signer_scalar();
    let pk_arr = nockster_core::cheetah_pub_from_sk(sk_be);
    let pk = SchnorrPubkey {
        x: F6LT { values: pk_arr[0] },
        y: F6LT { values: pk_arr[1] },
        inf: false,
    };
    let pkh = pk.to_hash();

    let mut allowed = ZSet::new();
    allowed.put(pkh.clone());
    let spend_condition = SpendCondition {
        p: vec![tx_types::transaction_types_v1::LockPrimitive {
            header: "pkh".to_string(),
            body: tx_types::transaction_types_v1::LockPrimitiveBody::Pkh(
                tx_types::transaction_types_v1::Pkh { m: 1, h: allowed },
            ),
        }],
    };
    let witness = Witness {
        lmp: empty_stub_lock_merkle_proof(spend_condition.clone()),
        pkh: PkhSignature { map: ZMap::new() },
        hax: ZMap::<Hash, UntypedNoun>::new(),
        tim: 0,
    };

    let external_hash = Hash {
        values: [41, 42, 43, 44, 45],
    };
    let mut external_allowed = ZSet::new();
    external_allowed.put(external_hash.clone());
    let external_condition = SpendCondition {
        p: vec![tx_types::transaction_types_v1::LockPrimitive {
            header: "pkh".to_string(),
            body: tx_types::transaction_types_v1::LockPrimitiveBody::Pkh(
                tx_types::transaction_types_v1::Pkh {
                    m: 1,
                    h: external_allowed,
                },
            ),
        }],
    };
    let external_lock_root = external_condition.to_hash();
    let external_note_data = note_data_for_lock(&external_condition);
    let external_seed = SeedV1 {
        output_source: None,
        lock_root: external_lock_root.clone(),
        note_data: external_note_data.clone(),
        gift: Coins { value: 9 },
        parent_hash: Hash {
            values: [51, 52, 53, 54, 55],
        },
    };
    let second_external_seed = SeedV1 {
        output_source: None,
        lock_root: external_lock_root,
        note_data: external_note_data,
        gift: Coins { value: 5 },
        parent_hash: Hash {
            values: [56, 57, 58, 59, 60],
        },
    };
    let refund_seed = SeedV1 {
        output_source: None,
        lock_root: spend_condition.to_hash(),
        note_data: note_data_for_lock(&spend_condition),
        gift: Coins { value: 4 },
        parent_hash: Hash {
            values: [61, 62, 63, 64, 65],
        },
    };
    // A lock root that merely equals our PKH, without a verified plain PKH
    // lock in note-data, must remain visible and must not be called change.
    let disguised_refund_seed = SeedV1 {
        output_source: None,
        lock_root: pkh.clone(),
        note_data: NoteData { map: ZMap::new() },
        gift: Coins { value: 2 },
        parent_hash: Hash {
            values: [66, 67, 68, 69, 70],
        },
    };

    let mut external_seeds: ZSet<SeedV1> = ZSet::new();
    external_seeds.put(external_seed);
    external_seeds.put(second_external_seed);
    external_seeds.put(disguised_refund_seed);
    let mut refund_seeds: ZSet<SeedV1> = ZSet::new();
    refund_seeds.put(refund_seed);

    let spend1 = Spend {
        version: 1,
        body: SpendBody::V1(SpendV1 {
            witness: witness.clone(),
            seeds: SeedsV1 {
                set: external_seeds,
            },
            fee: Coins { value: 10_000_000 },
        }),
    };
    let spend2 = Spend {
        version: 1,
        body: SpendBody::V1(SpendV1 {
            witness,
            seeds: SeedsV1 { set: refund_seeds },
            fee: Coins { value: 10_000_000 },
        }),
    };

    let mut spends_map: ZMap<NName, Spend> = ZMap::new();
    spends_map.put(
        NName {
            p: vec![Hash { values: [1; 5] }, Hash { values: [2; 5] }],
        },
        spend1,
    );
    spends_map.put(
        NName {
            p: vec![Hash { values: [3; 5] }, Hash { values: [4; 5] }],
        },
        spend2,
    );

    let raw = tx_types::transaction_types_v1::RawTransactionV1 {
        version: 1,
        id: Hash { values: [0; 5] },
        spends: SpendsV1 { map: spends_map },
    };

    let mut slab: NounSlab = NounSlab::new();
    let noun = raw.to_noun(&mut slab);
    slab.set_root(noun);
    let draft_jam = slab.jam().to_vec();

    let cfg = nockster_core::draft_sign::SignerConfig { sk_be };
    let review =
        nockster_core::draft_sign::draft_review_v1(&draft_jam, &cfg).expect("draft review");

    assert_eq!(review.input_count, 2);
    assert_eq!(review.external_output_count, 3);
    assert_eq!(review.external_total, 16);
    assert_eq!(review.refund_total, 4);
    assert_eq!(review.fee_total, 20_000_000);
    assert!(review.minimum_fee > 0);
    assert_eq!(
        nockster_core::draft_sign::draft_outputs_v1(&draft_jam, &cfg).expect("draft outputs"),
        review.outputs
    );

    let external: Vec<_> = review.outputs.iter().filter(|out| !out.is_refund).collect();
    assert_eq!(external.len(), 2);
    assert_eq!(external.iter().map(|out| out.gift).sum::<u64>(), 16);
    let grouped = external
        .iter()
        .find(|out| out.recipient_b58 == external_hash.to_b58())
        .expect("same-policy outputs are grouped");
    assert_eq!(grouped.gift, 14);
    assert_eq!(grouped.note_count, 2);
    let disguised = external
        .iter()
        .find(|out| out.recipient_b58 == pkh.to_b58())
        .expect("unverified signer-looking output remains visible");
    assert_eq!(disguised.gift, 2);
    assert_eq!(disguised.note_count, 1);
    assert!(!disguised.is_refund);

    let refund = review
        .outputs
        .iter()
        .find(|out| out.is_refund)
        .expect("refund output");
    assert_eq!(refund.gift, 4);
    assert_eq!(refund.note_count, 1);
    assert_eq!(refund.recipient_b58, pkh.to_b58());
}

#[test]
fn sign_draft_v1_preserves_tx_transact_tail() {
    let sk_be = runtime_signer_scalar();
    let pk_arr = nockster_core::cheetah_pub_from_sk(sk_be);
    let pk = SchnorrPubkey {
        x: F6LT { values: pk_arr[0] },
        y: F6LT { values: pk_arr[1] },
        inf: false,
    };
    let pkh = pk.to_hash();

    let mut allowed = ZSet::new();
    allowed.put(pkh.clone());
    let spend_condition = SpendCondition {
        p: vec![tx_types::transaction_types_v1::LockPrimitive {
            header: "pkh".to_string(),
            body: tx_types::transaction_types_v1::LockPrimitiveBody::Pkh(
                tx_types::transaction_types_v1::Pkh { m: 1, h: allowed },
            ),
        }],
    };

    let witness = Witness {
        lmp: empty_stub_lock_merkle_proof(spend_condition),
        pkh: PkhSignature { map: ZMap::new() },
        hax: ZMap::<Hash, UntypedNoun>::new(),
        tim: 0,
    };

    let spend_body = SpendV1 {
        witness,
        seeds: SeedsV1 { set: ZSet::new() },
        fee: Coins { value: 10_000_000 },
    };
    let spend = Spend {
        version: 1,
        body: SpendBody::V1(spend_body.clone()),
    };

    let name = NName {
        p: vec![Hash { values: [1; 5] }, Hash { values: [2; 5] }],
    };
    let mut spends_map: ZMap<NName, Spend> = ZMap::new();
    spends_map.put(name.clone(), spend.clone());

    let raw = tx_types::transaction_types_v1::RawTransactionV1 {
        version: 1,
        id: Hash { values: [0; 5] },
        spends: SpendsV1 { map: spends_map },
    };

    // Wrap as [raw-tx tail] to simulate tx:transact.
    let mut slab: NounSlab = NounSlab::new();
    let raw_noun = raw.to_noun(&mut slab);
    let tail = T(&mut slab, &[D(123), D(0)]);
    let wrapped = T(&mut slab, &[raw_noun, tail]);
    slab.set_root(wrapped);
    let wrapped_jam = slab.jam().to_vec();
    let tail_jam_before = jam_single(tail);
    let signed_jam = nockster_core::draft_sign::sign_draft_v1(
        &wrapped_jam,
        &nockster_core::draft_sign::SignerConfig { sk_be },
    )
    .expect("sign_draft_v1");

    let mut slab2: NounSlab = NounSlab::new();
    let noun2 = slab2
        .cue_into(Bytes::from(signed_jam))
        .expect("cue signed jam");

    let cell2 = noun2.as_cell().expect("signed output is a cell");
    let tail_jam_after = jam_single(cell2.tail());
    assert_eq!(tail_jam_after, tail_jam_before, "tx:transact tail changed");

    let signed_raw = tx_types::transaction_types_v1::RawTransactionV1::from_noun(&cell2.head(), &slab2.noun_space())
        .expect("decode head raw-tx");

    let computed_id = compute_tx_id_v1(&signed_raw.spends);
    assert_eq!(signed_raw.id, computed_id, "tx-id should be recomputed");

    let spend_entry = signed_raw
        .spends
        .map
        .get(&name)
        .expect("signed spend present");
    let SpendBody::V1(sv1) = &spend_entry.body else {
        panic!("expected SpendBody::V1");
    };

    let msg5 = sv1.compute_sig_hash().values;
    let (chal, sig) = nockster_core::schnorr_sign_tx(sk_be, (pk_arr[0], pk_arr[1]), msg5);
    let inserted = sv1
        .witness
        .pkh
        .map
        .get(&pkh)
        .expect("pkh signature inserted");
    assert_eq!(
        inserted.sig.chal.values.values, chal.values,
        "challenge mismatch"
    );
    assert_eq!(
        inserted.sig.sig.values.values, sig.values,
        "signature mismatch"
    );
}

#[derive(Debug, Clone, NounDecode, NounEncode)]
struct WalletTransactionV1 {
    pub name: String,
    pub spends: SpendsV1,
}

#[test]
fn sign_draft_v1_wallet_wrapper_updates_name() {
    let witness = Witness {
        lmp: empty_stub_lock_merkle_proof(SpendCondition { p: Vec::new() }),
        pkh: PkhSignature { map: ZMap::new() },
        hax: ZMap::<Hash, UntypedNoun>::new(),
        tim: 0,
    };

    // Note-data values must stay within the signer/TIP5 subset (u64 atoms and cells).
    let mut note_map: ZMap<String, UntypedNoun> = ZMap::new();
    let mut tmp: NounSlab = NounSlab::new();
    let val_noun = T(&mut tmp, &[D(1), D(2), D(3)]);
    let val_untyped = UntypedNoun::from_noun(&val_noun, &tmp.noun_space()).expect("untyped noun");
    note_map.put("memo".to_string(), val_untyped);
    let note_data = NoteData { map: note_map };

    let seed = SeedV1 {
        output_source: Some(Source {
            p: Hash {
                values: [31, 32, 33, 34, 35],
            },
            is_coinbase: false,
        }),
        lock_root: Hash {
            values: [41, 42, 43, 44, 45],
        },
        note_data,
        gift: Coins { value: 9 },
        parent_hash: Hash {
            values: [51, 52, 53, 54, 55],
        },
    };

    let mut seed_set: ZSet<SeedV1> = ZSet::new();
    seed_set.put(seed);

    let spend_body = SpendV1 {
        witness,
        seeds: SeedsV1 { set: seed_set },
        fee: Coins { value: 10_000_000 },
    };
    let spend = Spend {
        version: 1,
        body: SpendBody::V1(spend_body),
    };

    let name = NName {
        p: vec![Hash { values: [1; 5] }, Hash { values: [2; 5] }],
    };
    let mut spends_map: ZMap<NName, Spend> = ZMap::new();
    spends_map.put(name.clone(), spend);

    let wallet = WalletTransactionV1 {
        name: "placeholder".to_string(),
        spends: SpendsV1 { map: spends_map },
    };

    let mut slab: NounSlab = NounSlab::new();
    let noun = wallet.to_noun(&mut slab);
    slab.set_root(noun);
    let wallet_jam = slab.jam().to_vec();

    let expected_id = compute_tx_id_v1(&wallet.spends);
    let rewritten = nockster_core::draft_sign::rewrite_txid_v1(&wallet_jam)
        .expect("rewrite placeholder wallet tx-id");
    assert_eq!(rewritten.name, expected_id.to_b58());
    let rewritten_jam = rewritten.rewritten.expect("placeholder name is stale");

    let mut rewrite_slab: NounSlab = NounSlab::new();
    let rewrite_noun = rewrite_slab
        .cue_into(Bytes::from(rewritten_jam.clone()))
        .expect("cue rewritten wallet");
    let rewritten_wallet =
        WalletTransactionV1::from_noun(&rewrite_noun, &rewrite_slab.noun_space()).expect("decode rewritten wallet");
    assert_eq!(rewritten_wallet.name, expected_id.to_b58());

    let stable = nockster_core::draft_sign::rewrite_txid_v1(&rewritten_jam)
        .expect("rewrite canonical wallet tx-id");
    assert_eq!(stable.name, expected_id.to_b58());
    assert!(
        stable.rewritten.is_none(),
        "canonical wallet should not jam again"
    );

    let sk_be = runtime_signer_scalar();
    let signed_jam = nockster_core::draft_sign::sign_draft_v1(
        &wallet_jam,
        &nockster_core::draft_sign::SignerConfig { sk_be },
    )
    .expect("sign_draft_v1");

    let signed_stable =
        nockster_core::draft_sign::rewrite_txid_v1(&signed_jam).expect("rewrite signed wallet");
    assert!(
        signed_stable.rewritten.is_none(),
        "signed wallet should already carry its canonical tx-id"
    );

    let mut slab2: NounSlab = NounSlab::new();
    let noun2 = slab2
        .cue_into(Bytes::from(signed_jam))
        .expect("cue signed jam");
    let signed_wallet = WalletTransactionV1::from_noun(&noun2, &slab2.noun_space()).expect("decode wallet v1");

    let computed_id = compute_tx_id_v1(&signed_wallet.spends);
    let parsed_id = Hash::from_b58(&signed_wallet.name).expect("wallet name decodes as hash");
    assert_eq!(
        parsed_id, computed_id,
        "wallet name should decode to new tx-id"
    );
    assert_eq!(
        signed_wallet.name,
        computed_id.to_b58(),
        "wallet name should be canonical base58"
    );
}

#[test]
fn sign_draft_v1_wallet_tx_v1_wrapper_updates_name() {
    let witness = Witness {
        lmp: empty_stub_lock_merkle_proof(SpendCondition { p: Vec::new() }),
        pkh: PkhSignature { map: ZMap::new() },
        hax: ZMap::<Hash, UntypedNoun>::new(),
        tim: 0,
    };

    let mut note_map: ZMap<String, UntypedNoun> = ZMap::new();
    let mut tmp: NounSlab = NounSlab::new();
    let val_noun = T(&mut tmp, &[D(1), D(2), D(3)]);
    let val_untyped = UntypedNoun::from_noun(&val_noun, &tmp.noun_space()).expect("untyped noun");
    note_map.put("memo".to_string(), val_untyped);
    let note_data = NoteData { map: note_map };

    let seed = SeedV1 {
        output_source: Some(Source {
            p: Hash {
                values: [31, 32, 33, 34, 35],
            },
            is_coinbase: false,
        }),
        lock_root: Hash {
            values: [41, 42, 43, 44, 45],
        },
        note_data,
        gift: Coins { value: 9 },
        parent_hash: Hash {
            values: [51, 52, 53, 54, 55],
        },
    };

    let mut seed_set: ZSet<SeedV1> = ZSet::new();
    seed_set.put(seed);

    let spend_body = SpendV1 {
        witness,
        seeds: SeedsV1 { set: seed_set },
        fee: Coins { value: 10_000_000 },
    };
    let spend = Spend {
        version: 1,
        body: SpendBody::V1(spend_body),
    };

    let name = NName {
        p: vec![Hash { values: [1; 5] }, Hash { values: [2; 5] }],
    };
    let mut spends_map: ZMap<NName, Spend> = ZMap::new();
    spends_map.put(name, spend);

    let spends = SpendsV1 { map: spends_map };

    let mut slab: NounSlab = NounSlab::new();
    let tag = D(1);
    let placeholder_name = "placeholder".to_string().to_noun(&mut slab);
    let spends_noun = spends.to_noun(&mut slab);
    let input_display = T(&mut slab, &[tag, D(0)]);
    let display = T(&mut slab, &[input_display, D(0)]);
    let witness_data = T(&mut slab, &[tag, D(0)]);
    let tx_noun = T(
        &mut slab,
        &[tag, placeholder_name, spends_noun, display, witness_data],
    );
    slab.set_root(tx_noun);
    let tx_jam = slab.jam().to_vec();

    let sk_be = runtime_signer_scalar();
    let signed_jam = nockster_core::draft_sign::sign_draft_v1(
        &tx_jam,
        &nockster_core::draft_sign::SignerConfig { sk_be },
    )
    .expect("sign_draft_v1");

    let mut slab2: NounSlab = NounSlab::new();
    let noun2 = slab2
        .cue_into(Bytes::from(signed_jam))
        .expect("cue signed jam");

    let cell0 = noun2.as_cell().expect("tx is cell");
    let tag_atom = cell0.head().as_atom().expect("tag atom");
    assert_eq!(tag_atom.as_u64(), Ok(1));

    let cell1 = cell0.tail().as_cell().expect("rest cell");
    let name_atom = cell1.head().as_atom().expect("name atom");
    let name_bytes = name_atom.to_bytes_until_nul().expect("name bytes");
    let name_str = String::from_utf8_lossy(&name_bytes).to_string();

    let cell2 = cell1.tail().as_cell().expect("spends cell");
    let spends_noun2 = cell2.head();
    let spends2 = SpendsV1::from_noun(&spends_noun2, &slab2.noun_space()).expect("decode spends");
    let computed_id = compute_tx_id_v1(&spends2);
    assert_eq!(name_str, computed_id.to_b58());
}

#[test]
fn sign_draft_v1_replaces_placeholder_when_map_full() {
    let sk_be = runtime_signer_scalar();

    // Compute the signing pubkey + pkh that will be authorized in the lock.
    let pk_arr = nockster_core::cheetah_pub_from_sk(sk_be);
    let pk = SchnorrPubkey {
        x: F6LT { values: pk_arr[0] },
        y: F6LT { values: pk_arr[1] },
        inf: false,
    };
    let pkh = pk.to_hash();

    // Build a spend-condition authorizing only our pkh (m=1).
    let mut allowed = ZSet::new();
    allowed.put(pkh.clone());
    let spend_condition = SpendCondition {
        p: vec![tx_types::transaction_types_v1::LockPrimitive {
            header: "pkh".to_string(),
            body: tx_types::transaction_types_v1::LockPrimitiveBody::Pkh(
                tx_types::transaction_types_v1::Pkh { m: 1, h: allowed },
            ),
        }],
    };

    // Insert a placeholder signature under a different key, to simulate fee-sizing drafts.
    let placeholder_key = Hash {
        values: [99, 99, 99, 99, 99],
    };
    let placeholder_pk = SchnorrPubkey {
        x: F6LT { values: [0; 6] },
        y: F6LT { values: [0; 6] },
        inf: false,
    };
    let placeholder_sig = tx_types::transaction_types::SchnorrSignature {
        chal: Chal {
            values: T8 { values: [0; 8] },
        },
        sig: Sig {
            values: T8 { values: [0; 8] },
        },
    };
    let mut placeholder_map: ZMap<Hash, PkhSignatureValue> = ZMap::new();
    placeholder_map.put(
        placeholder_key.clone(),
        PkhSignatureValue {
            pk: placeholder_pk,
            sig: placeholder_sig,
        },
    );

    let witness = Witness {
        lmp: empty_stub_lock_merkle_proof(spend_condition),
        pkh: PkhSignature {
            map: placeholder_map,
        },
        hax: ZMap::<Hash, UntypedNoun>::new(),
        tim: 0,
    };

    let spend_body = SpendV1 {
        witness,
        seeds: tx_types::transaction_types_v1::SeedsV1 { set: ZSet::new() },
        fee: Coins { value: 10_000_000 },
    };
    let spend = Spend {
        version: 1,
        body: SpendBody::V1(spend_body.clone()),
    };

    let name = NName {
        p: vec![
            Hash {
                values: [1, 2, 3, 4, 5],
            },
            Hash {
                values: [6, 7, 8, 9, 10],
            },
        ],
    };

    let mut spends_map: ZMap<NName, Spend> = ZMap::new();
    spends_map.put(name.clone(), spend.clone());

    let raw = tx_types::transaction_types_v1::RawTransactionV1 {
        version: 1,
        id: Hash { values: [0; 5] },
        spends: SpendsV1 { map: spends_map },
    };

    let mut slab: NounSlab = NounSlab::new();
    let noun = raw.to_noun(&mut slab);
    slab.set_root(noun);
    let draft_jam = slab.jam().to_vec();

    let signed_jam = nockster_core::draft_sign::sign_draft_v1(
        &draft_jam,
        &nockster_core::draft_sign::SignerConfig { sk_be },
    )
    .expect("sign_draft_v1");

    let mut slab2: NounSlab = NounSlab::new();
    let noun2 = slab2
        .cue_into(Bytes::from(signed_jam))
        .expect("cue signed jam");
    let signed_raw = tx_types::transaction_types_v1::RawTransactionV1::from_noun(&noun2, &slab2.noun_space())
        .expect("decode v1 raw-tx");

    let computed_id = compute_tx_id_v1(&signed_raw.spends);
    assert_eq!(signed_raw.id, computed_id, "tx-id should be recomputed");

    let spend_entry = signed_raw
        .spends
        .map
        .get(&name)
        .expect("signed spend present");
    let SpendBody::V1(sv1) = &spend_entry.body else {
        panic!("expected SpendBody::V1");
    };

    assert!(
        sv1.witness.pkh.map.get(&pkh).is_some(),
        "placeholder should be replaced with our signature entry"
    );
    assert!(
        sv1.witness.pkh.map.get(&placeholder_key).is_none(),
        "placeholder key should be evicted when the map is full"
    );
}

/// The no_std signer keeps only the canonical base58 spelling of the axis
/// hash; decoding it must match tx-types' independent `Hash::from_b58`.
#[test]
fn lock_merkle_axis_hash_matches_tx_types() {
    let expected = tx_types::transaction_types::Hash::from_b58(
        nockster_core::draft_sign::LOCK_MERKLE_AXIS_HASH_B58,
    )
    .expect("canonical axis hash")
    .values;
    assert_eq!(
        nockster_core::draft_sign::digest_from_b58(
            nockster_core::draft_sign::LOCK_MERKLE_AXIS_HASH_B58
        )
        .expect("decode axis hash"),
        expected
    );
}

fn simple_pkh_draft(pkh: &Hash, fee: u64, tim: u64) -> (Vec<u8>, NName) {
    let mut allowed = ZSet::new();
    allowed.put(pkh.clone());
    let spend_condition = SpendCondition {
        p: vec![LockPrimitive {
            header: "pkh".to_string(),
            body: LockPrimitiveBody::Pkh(Pkh { m: 1, h: allowed }),
        }],
    };
    let witness = Witness {
        lmp: empty_full_lock_merkle_proof(spend_condition),
        pkh: PkhSignature { map: ZMap::new() },
        hax: ZMap::<Hash, UntypedNoun>::new(),
        tim,
    };
    let spend = Spend {
        version: 1,
        body: SpendBody::V1(SpendV1 {
            witness,
            seeds: SeedsV1 { set: ZSet::new() },
            fee: Coins { value: fee },
        }),
    };
    let name = NName {
        p: vec![Hash { values: [9; 5] }, Hash { values: [8; 5] }],
    };
    let mut spends_map: ZMap<NName, Spend> = ZMap::new();
    spends_map.put(name.clone(), spend);
    let raw = RawTransactionV1 {
        version: 1,
        id: Hash { values: [0; 5] },
        spends: SpendsV1 { map: spends_map },
    };
    let mut slab: NounSlab = NounSlab::new();
    let noun = raw.to_noun(&mut slab);
    slab.set_root(noun);
    (slab.jam().to_vec(), name)
}

fn decode_signed(jam: &[u8]) -> RawTransactionV1 {
    let mut slab: NounSlab = NounSlab::new();
    let noun = slab.cue_into(Bytes::from(jam.to_vec())).expect("cue signed");
    RawTransactionV1::from_noun(&noun, &slab.noun_space()).expect("decode signed")
}

/// `%hax` must hash its commitment z-set exactly like `%pkh`'s hash set.
/// Comparing the re-signed draft's id against tx-types' independent
/// `compute_tx_id_v1` catches the old `"fake"` placeholder digest.
#[test]
fn hax_lock_tx_id_matches_tx_types() {
    let sk_be = runtime_signer_scalar();

    let mut commitments = ZSet::new();
    commitments.put(Hash {
        values: [7, 8, 9, 10, 11],
    });
    let spend_condition = SpendCondition {
        p: vec![LockPrimitive {
            header: "hax".to_string(),
            body: LockPrimitiveBody::Hax(Hax { set: commitments }),
        }],
    };
    let witness = Witness {
        lmp: empty_full_lock_merkle_proof(spend_condition),
        pkh: PkhSignature { map: ZMap::new() },
        hax: ZMap::<Hash, UntypedNoun>::new(),
        tim: 0,
    };
    let spend = Spend {
        version: 1,
        body: SpendBody::V1(SpendV1 {
            witness,
            seeds: SeedsV1 { set: ZSet::new() },
            fee: Coins { value: 10_000_000 },
        }),
    };
    let name = NName {
        p: vec![Hash { values: [1; 5] }, Hash { values: [2; 5] }],
    };
    let mut spends_map: ZMap<NName, Spend> = ZMap::new();
    spends_map.put(name, spend);
    let raw = RawTransactionV1 {
        version: 1,
        id: Hash { values: [0; 5] },
        spends: SpendsV1 { map: spends_map },
    };
    let mut slab: NounSlab = NounSlab::new();
    let noun = raw.to_noun(&mut slab);
    slab.set_root(noun);
    let draft_jam = slab.jam().to_vec();

    let signed = nockster_core::draft_sign::sign_draft_v1(
        &draft_jam,
        &nockster_core::draft_sign::SignerConfig { sk_be },
    )
    .expect("sign hax draft");
    let signed_raw = decode_signed(&signed);
    assert_eq!(
        signed_raw.id,
        compute_tx_id_v1(&signed_raw.spends),
        "hax tx id must match tx-types/upstream hashing"
    );
}

#[test]
fn fee_policy_tracks_bythos_and_overrides() {
    use nockster_core::draft_sign::{
        draft_review_v1_with_policy, FeePolicy, BASE_FEE, BYTHOS_PHASE,
    };

    let sk_be = runtime_signer_scalar();
    let pk_arr = nockster_core::cheetah_pub_from_sk(sk_be);
    let pkh = SchnorrPubkey {
        x: F6LT { values: pk_arr[0] },
        y: F6LT { values: pk_arr[1] },
        inf: false,
    }
    .to_hash();
    let (draft_jam, _) = simple_pkh_draft(&pkh, 10_000_000, 0);
    let cfg = nockster_core::draft_sign::SignerConfig { sk_be };
    let default = FeePolicy::default();

    let post =
        draft_review_v1_with_policy(&draft_jam, &cfg, BYTHOS_PHASE, default).expect("post");
    let pre = draft_review_v1_with_policy(&draft_jam, &cfg, BYTHOS_PHASE - 1, default).expect("pre");
    assert!(
        pre.minimum_fee > post.minimum_fee,
        "pre-Bythos fees use 2x base and no witness discount"
    );

    let no_floor = FeePolicy {
        min_fee: 0,
        ..default
    };
    let single = draft_review_v1_with_policy(&draft_jam, &cfg, BYTHOS_PHASE, no_floor)
        .expect("base fee")
        .minimum_fee;
    let doubled = draft_review_v1_with_policy(
        &draft_jam,
        &cfg,
        BYTHOS_PHASE,
        FeePolicy {
            base_fee: BASE_FEE * 2,
            ..no_floor
        },
    )
    .expect("doubled base fee")
    .minimum_fee;
    assert_eq!(doubled, single * 2);

    let floor = draft_review_v1_with_policy(
        &draft_jam,
        &cfg,
        BYTHOS_PHASE,
        FeePolicy {
            base_fee: 0,
            min_fee: 12_345,
            ..default
        },
    )
    .expect("floor")
    .minimum_fee;
    assert_eq!(floor, 12_345);

    let shifted = FeePolicy {
        bythos_phase: 1_000,
        ..default
    };
    let before = draft_review_v1_with_policy(&draft_jam, &cfg, 999, shifted)
        .expect("shifted pre")
        .minimum_fee;
    let after = draft_review_v1_with_policy(&draft_jam, &cfg, 1_000, shifted)
        .expect("shifted post")
        .minimum_fee;
    assert!(before > after);
}

#[test]
fn sign_draft_v1_rejects_fee_below_minimum() {
    let sk_be = runtime_signer_scalar();
    let pk_arr = nockster_core::cheetah_pub_from_sk(sk_be);
    let pkh = SchnorrPubkey {
        x: F6LT { values: pk_arr[0] },
        y: F6LT { values: pk_arr[1] },
        inf: false,
    }
    .to_hash();
    let (draft_jam, _) = simple_pkh_draft(&pkh, 7, 0);
    let err = nockster_core::draft_sign::sign_draft_v1(
        &draft_jam,
        &nockster_core::draft_sign::SignerConfig { sk_be },
    )
    .expect_err("underpaid draft must be rejected");
    match err {
        nockster_core::draft_sign::SignDraftError::FeeBelowMinimum { minimum, provided } => {
            assert_eq!(provided, 7);
            assert!(minimum > provided);
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn sign_draft_v1_normalizes_nonzero_tim() {
    let sk_be = runtime_signer_scalar();
    let pk_arr = nockster_core::cheetah_pub_from_sk(sk_be);
    let pkh = SchnorrPubkey {
        x: F6LT { values: pk_arr[0] },
        y: F6LT { values: pk_arr[1] },
        inf: false,
    }
    .to_hash();
    let (draft_jam, name) = simple_pkh_draft(&pkh, 10_000_000, 5);
    let signed = nockster_core::draft_sign::sign_draft_v1(
        &draft_jam,
        &nockster_core::draft_sign::SignerConfig { sk_be },
    )
    .expect("sign nonzero-tim draft");
    let signed_raw = decode_signed(&signed);
    let spend = signed_raw.spends.map.get(&name).expect("spend");
    let SpendBody::V1(sv1) = &spend.body else {
        panic!("expected v1 spend");
    };
    assert_eq!(sv1.witness.tim, 0, "tim must be normalized like upstream");
    assert_eq!(signed_raw.id, compute_tx_id_v1(&signed_raw.spends));
}

fn poke_tuple(arena: &mut pokenoun::Arena, elems: &[pokenoun::Noun]) -> pokenoun::Noun {
    if elems.is_empty() {
        return arena.atom0();
    }
    let mut res = *elems.last().unwrap();
    for &n in elems[..elems.len() - 1].iter().rev() {
        res = arena.alloc_cell(n, res);
    }
    res
}

fn poke_hash_noun(arena: &mut pokenoun::Arena, digest: [u64; 5]) -> pokenoun::Noun {
    let elems = [
        arena.alloc_atom_u64(digest[0]),
        arena.alloc_atom_u64(digest[1]),
        arena.alloc_atom_u64(digest[2]),
        arena.alloc_atom_u64(digest[3]),
        arena.alloc_atom_u64(digest[4]),
    ];
    poke_tuple(arena, &elems)
}

#[test]
fn pokenoun_canonical_zset_put_matches_tx_types() {
    use pokenoun::{canonical_zset_put, jam as poke_jam, Arena as PokeArena};

    let values = vec![
        Hash {
            values: [1, 2, 3, 4, 5],
        },
        Hash {
            values: [6, 7, 8, 9, 10],
        },
        Hash {
            values: [11, 12, 13, 14, 15],
        },
    ];

    // tx-types reference encoding
    let mut tx_set: ZSet<Hash> = ZSet::new();
    for v in &values {
        tx_set.put(v.clone());
    }
    let mut slab: NounSlab = NounSlab::new();
    let noun = tx_set.to_noun(&mut slab);
    slab.set_root(noun);
    let tx_jam = slab.jam().to_vec();

    // pokenoun encoding
    let mut arena = PokeArena::new();
    let mut root = arena.atom0();
    for v in &values {
        let noun = poke_hash_noun(&mut arena, v.values);
        root = canonical_zset_put(&mut arena, root, noun).expect("canonical_zset_put");
    }
    let poke_jam_bytes = poke_jam(root, &arena);

    assert_eq!(poke_jam_bytes, tx_jam, "z-set noun jam mismatch");
}
