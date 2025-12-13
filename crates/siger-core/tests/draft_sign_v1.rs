use bytes::Bytes;
use nockapp::noun::slab::NounSlab;
use nockvm::noun::{D, T};
use noun_serde::{NounDecode, NounEncode};

use tx_types::collections::{ZMap, ZSet};
use tx_types::generic_noun::UntypedNoun;
use tx_types::transaction_types::{Coins, Hash, NName, SchnorrPubkey, Source, Spend, SpendBody, F6LT};
use tx_types::transaction_types_v1::{
    compute_tx_id_v1, LockMerkleProof, MerkleProof, NoteData, PkhSignature, SeedV1, SeedsV1,
    SpendCondition, SpendV1, SpendsV1, Witness,
};

fn jam_single(noun: nockvm::noun::Noun) -> Vec<u8> {
    let mut slab: NounSlab = NounSlab::new();
    let copied = slab.copy_into(noun);
    slab.set_root(copied);
    slab.jam().to_vec()
}

#[test]
fn sign_draft_v1_inserts_expected_signature() {
    // Build a minimal V1 raw-tx with one spend and empty seeds/note-data.
    let witness = Witness {
        lmp: LockMerkleProof {
            spend_condition: SpendCondition { p: Vec::new() },
            axis: 0,
            merkle_proof: MerkleProof {
                root: Hash { values: [0; 5] },
                path: Vec::new(),
            },
        },
        pkh: PkhSignature { map: ZMap::new() },
        hax: ZMap::<Hash, UntypedNoun>::new(),
        tim: 0,
    };

    let spend_body = SpendV1 {
        witness,
        seeds: tx_types::transaction_types_v1::SeedsV1 { set: ZSet::new() },
        fee: Coins { value: 7 },
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

    // Sign using the on-device style signer (deterministic).
    let sk_be = [0x11u8; 32];
    let signed_jam =
        siger_core::draft_sign::sign_draft_v1(&draft_jam, &siger_core::draft_sign::SignerConfig { sk_be })
            .expect("sign_draft_v1");

    // Decode the signed transaction with the reference types.
    let mut slab2: NounSlab = NounSlab::new();
    let noun2 = slab2
        .cue_into(Bytes::from(signed_jam))
        .expect("cue signed jam");
    let signed_raw =
        tx_types::transaction_types_v1::RawTransactionV1::from_noun(&noun2).expect("decode v1 raw-tx");

    let computed_id = compute_tx_id_v1(&signed_raw.spends);
    assert_eq!(signed_raw.id, computed_id, "tx-id should be recomputed");

    // Compute expected pubkey + pkh.
    let pk_arr = siger_core::cheetah_pub_from_sk(sk_be);
    let pk = SchnorrPubkey {
        x: F6LT { values: pk_arr[0] },
        y: F6LT { values: pk_arr[1] },
        inf: false,
    };
    let pkh = pk.to_hash();

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
    let (chal, sig) = siger_core::schnorr_sign_tx(sk_be, (pk_arr[0], pk_arr[1]), msg5);

    let inserted = sv1
        .witness
        .pkh
        .map
        .get(&pkh)
        .expect("pkh signature inserted");

    assert_eq!(inserted.pk, pk, "inserted pubkey mismatch");
    assert_eq!(inserted.sig.chal.values.values, chal.values, "challenge mismatch");
    assert_eq!(inserted.sig.sig.values.values, sig.values, "signature mismatch");
}

#[test]
fn sign_draft_v1_handles_seeds_and_note_data() {
    // Witness can be mostly empty; signatures depend only on seeds+fee for V1.
    let witness = Witness {
        lmp: LockMerkleProof {
            spend_condition: SpendCondition { p: Vec::new() },
            axis: 0,
            merkle_proof: MerkleProof {
                root: Hash { values: [0; 5] },
                path: Vec::new(),
            },
        },
        pkh: PkhSignature { map: ZMap::new() },
        hax: ZMap::<Hash, UntypedNoun>::new(),
        tim: 0,
    };

    // note-data with a small u64-only noun value to stay in the no_std signer subset.
    let mut note_map: ZMap<String, UntypedNoun> = ZMap::new();
    let mut tmp: NounSlab = NounSlab::new();
    let val_noun = T(&mut tmp, &[D(1), D(2), D(3)]);
    let val_untyped = UntypedNoun::from_noun(&val_noun).expect("untyped noun");
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
        fee: Coins { value: 7 },
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

    let sk_be = [0x11u8; 32];
    let signed_jam =
        siger_core::draft_sign::sign_draft_v1(&draft_jam, &siger_core::draft_sign::SignerConfig { sk_be })
            .expect("sign_draft_v1");

    let mut slab2: NounSlab = NounSlab::new();
    let noun2 = slab2
        .cue_into(Bytes::from(signed_jam))
        .expect("cue signed jam");
    let signed_raw =
        tx_types::transaction_types_v1::RawTransactionV1::from_noun(&noun2).expect("decode v1 raw-tx");

    let computed_id = compute_tx_id_v1(&signed_raw.spends);
    assert_eq!(signed_raw.id, computed_id, "tx-id should be recomputed");

    // Compute expected pubkey + pkh.
    let pk_arr = siger_core::cheetah_pub_from_sk(sk_be);
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
    let (chal, sig) = siger_core::schnorr_sign_tx(sk_be, (pk_arr[0], pk_arr[1]), msg5);

    let inserted = sv1
        .witness
        .pkh
        .map
        .get(&pkh)
        .expect("pkh signature inserted");

    assert_eq!(inserted.pk, pk, "inserted pubkey mismatch");
    assert_eq!(inserted.sig.chal.values.values, chal.values, "challenge mismatch");
    assert_eq!(inserted.sig.sig.values.values, sig.values, "signature mismatch");
}

#[test]
fn sign_draft_v1_preserves_tx_transact_tail() {
    let witness = Witness {
        lmp: LockMerkleProof {
            spend_condition: SpendCondition { p: Vec::new() },
            axis: 0,
            merkle_proof: MerkleProof {
                root: Hash { values: [0; 5] },
                path: Vec::new(),
            },
        },
        pkh: PkhSignature { map: ZMap::new() },
        hax: ZMap::<Hash, UntypedNoun>::new(),
        tim: 0,
    };

    let spend_body = SpendV1 {
        witness,
        seeds: SeedsV1 { set: ZSet::new() },
        fee: Coins { value: 1 },
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

    let sk_be = [0x11u8; 32];
    let signed_jam =
        siger_core::draft_sign::sign_draft_v1(&wrapped_jam, &siger_core::draft_sign::SignerConfig { sk_be })
            .expect("sign_draft_v1");

    let mut slab2: NounSlab = NounSlab::new();
    let noun2 = slab2
        .cue_into(Bytes::from(signed_jam))
        .expect("cue signed jam");

    let cell2 = noun2.as_cell().expect("signed output is a cell");
    let tail_jam_after = jam_single(cell2.tail());
    assert_eq!(tail_jam_after, tail_jam_before, "tx:transact tail changed");

    let signed_raw =
        tx_types::transaction_types_v1::RawTransactionV1::from_noun(&cell2.head()).expect("decode head raw-tx");

    let computed_id = compute_tx_id_v1(&signed_raw.spends);
    assert_eq!(signed_raw.id, computed_id, "tx-id should be recomputed");

    let pk_arr = siger_core::cheetah_pub_from_sk(sk_be);
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
    let (chal, sig) = siger_core::schnorr_sign_tx(sk_be, (pk_arr[0], pk_arr[1]), msg5);
    let inserted = sv1
        .witness
        .pkh
        .map
        .get(&pkh)
        .expect("pkh signature inserted");
    assert_eq!(inserted.sig.chal.values.values, chal.values, "challenge mismatch");
    assert_eq!(inserted.sig.sig.values.values, sig.values, "signature mismatch");
}

#[derive(Debug, Clone, NounDecode, NounEncode)]
struct WalletTransactionV1 {
    pub name: String,
    pub spends: SpendsV1,
}

#[test]
fn sign_draft_v1_wallet_wrapper_updates_name() {
    let witness = Witness {
        lmp: LockMerkleProof {
            spend_condition: SpendCondition { p: Vec::new() },
            axis: 0,
            merkle_proof: MerkleProof {
                root: Hash { values: [0; 5] },
                path: Vec::new(),
            },
        },
        pkh: PkhSignature { map: ZMap::new() },
        hax: ZMap::<Hash, UntypedNoun>::new(),
        tim: 0,
    };

    // Note-data values must stay within the signer/TIP5 subset (u64 atoms and cells).
    let mut note_map: ZMap<String, UntypedNoun> = ZMap::new();
    let mut tmp: NounSlab = NounSlab::new();
    let val_noun = T(&mut tmp, &[D(1), D(2), D(3)]);
    let val_untyped = UntypedNoun::from_noun(&val_noun).expect("untyped noun");
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
        fee: Coins { value: 7 },
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

    let sk_be = [0x11u8; 32];
    let signed_jam =
        siger_core::draft_sign::sign_draft_v1(&wallet_jam, &siger_core::draft_sign::SignerConfig { sk_be })
            .expect("sign_draft_v1");

    let mut slab2: NounSlab = NounSlab::new();
    let noun2 = slab2
        .cue_into(Bytes::from(signed_jam))
        .expect("cue signed jam");
    let signed_wallet = WalletTransactionV1::from_noun(&noun2).expect("decode wallet v1");

    let computed_id = compute_tx_id_v1(&signed_wallet.spends);
    let parsed_id = Hash::from_b58(&signed_wallet.name).expect("wallet name decodes as hash");
    assert_eq!(parsed_id, computed_id, "wallet name should decode to new tx-id");
    assert_eq!(
        signed_wallet.name,
        computed_id.to_b58(),
        "wallet name should be canonical base58"
    );
}
