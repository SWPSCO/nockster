use bytes::Bytes;
use nockapp::AtomExt;
use nockapp::noun::slab::NounSlab;
use nockvm::noun::{D, T};
use noun_serde::{NounDecode, NounEncode};

use tx_types::collections::{ZMap, ZSet};
use tx_types::generic_noun::UntypedNoun;
use tx_types::transaction_types::{
    Chal, Coins, Hash, NName, SchnorrPubkey, Sig, Source, Spend, SpendBody, T8, F6LT,
};
use tx_types::transaction_types_v1::{
    compute_tx_id_v1, LockMerkleProof, MerkleProof, NoteData, PkhSignature, SeedV1, SeedsV1,
    PkhSignatureValue, SpendCondition, SpendV1, SpendsV1, Witness,
};

fn jam_single(noun: nockvm::noun::Noun) -> Vec<u8> {
    let mut slab: NounSlab = NounSlab::new();
    let copied = slab.copy_into(noun);
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

#[test]
fn sign_draft_v1_inserts_expected_signature() {
    // Sign using the on-device style signer (deterministic).
    let sk_be = [0x11u8; 32];

    // Compute the signing pubkey + pkh that will be authorized in the lock.
    let pk_arr = siger_core::cheetah_pub_from_sk(sk_be);
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
    let sk_be = [0x11u8; 32];
    let pk_arr = siger_core::cheetah_pub_from_sk(sk_be);
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
        lmp: empty_stub_lock_merkle_proof(spend_condition),
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
    let sk_be = [0x11u8; 32];
    let pk_arr = siger_core::cheetah_pub_from_sk(sk_be);
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
        lmp: empty_stub_lock_merkle_proof(SpendCondition { p: Vec::new() }),
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
    spends_map.put(name, spend);

    let spends = SpendsV1 { map: spends_map };

    let mut slab: NounSlab = NounSlab::new();
    let tag = D(1);
    let placeholder_name = "placeholder".to_string().to_noun(&mut slab);
    let spends_noun = spends.to_noun(&mut slab);
    let input_display = T(&mut slab, &[tag, D(0)]);
    let display = T(&mut slab, &[input_display, D(0)]);
    let witness_data = T(&mut slab, &[tag, D(0)]);
    let tx_noun = T(&mut slab, &[tag, placeholder_name, spends_noun, display, witness_data]);
    slab.set_root(tx_noun);
    let tx_jam = slab.jam().to_vec();

    let sk_be = [0x11u8; 32];
    let signed_jam =
        siger_core::draft_sign::sign_draft_v1(&tx_jam, &siger_core::draft_sign::SignerConfig { sk_be })
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
    let spends2 = SpendsV1::from_noun(&spends_noun2).expect("decode spends");
    let computed_id = compute_tx_id_v1(&spends2);
    assert_eq!(name_str, computed_id.to_b58());
}

#[test]
fn sign_draft_v1_replaces_placeholder_when_map_full() {
    let sk_be = [0x11u8; 32];

    // Compute the signing pubkey + pkh that will be authorized in the lock.
    let pk_arr = siger_core::cheetah_pub_from_sk(sk_be);
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
        pkh: PkhSignature { map: placeholder_map },
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
