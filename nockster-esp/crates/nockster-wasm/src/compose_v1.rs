//! Browser DTOs and wallet-envelope serialization around the shared planner.
use serde::{Deserialize, Serialize};
use tx_types::lock::Lock;
use tx_types::lock_resolver::{LockMatcher, LockResolution, SigningKeyLockMatcher};
use tx_types::planner::{self, ChainContext, PlanRequest, PlannedOutput, SelectionOrder};
use tx_types::pokenoun::codec::{tuple, Decode, Encode, Error, Fields};
use tx_types::pokenoun::{Arena, Noun};
use tx_types::{
    Brn, Coins, Hash, Hax, LockPrimitive, LockPrimitiveBody, NName, NNote, NNoteV1, NoteData,
    PageNumber, Pkh, RawTransactionV1, SpendBody, SpendCondition, SpendsV1, Tim, TimelockRange,
    UntypedNoun, Witness, ZMap, ZSet,
};
use wasm_bindgen::prelude::*;

fn js_err(error: impl core::fmt::Display) -> JsValue {
    JsValue::from_str(&error.to_string())
}

#[derive(Debug, Clone, Deserialize)]
pub struct ComposeTxV1Input {
    pub source_pkh: String,
    #[serde(default)]
    pub manual_inputs: bool,
    pub notes: Vec<NoteInput>,
    pub outputs: Vec<OutputInput>,
    #[serde(default)]
    pub coinbase_rel_min: Option<u64>,
    #[serde(default)]
    pub current_height: Option<u64>,
    #[serde(default)]
    pub bythos_phase: Option<u64>,
    #[serde(default)]
    pub base_fee: Option<u64>,
    #[serde(default)]
    pub input_fee_divisor: Option<u64>,
    #[serde(default)]
    pub min_fee: Option<u64>,
    /// When the source notes are held under an m-of-n multisig lock, this
    /// supplies the lock so inputs are reconstructed as multisig (and change
    /// returns to the multisig). The composed `.psnt` then round-trips through
    /// each co-signer's device, which fills its own signature slot.
    #[serde(default)]
    pub source_multisig: Option<MultisigSourceInput>,
    /// When the source notes are held under an OR-composed lock (e.g. an HTLC),
    /// this supplies the full branch set and which branch to spend. The witness
    /// carries a full lock-merkle-proof for that branch.
    #[serde(default)]
    pub source_or_lock: Option<OrSourceInput>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MultisigSourceInput {
    pub m: u64,
    pub pkhs: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OrSourceInput {
    pub branches: Vec<LockBranch>,
    pub spend_branch: u64,
    #[serde(default)]
    pub preimages: Vec<Vec<u8>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NoteInput {
    /// Actual RPC note data; an empty object represents an empty on-chain map.
    pub note_data: serde_json::Value,
    pub name_first: String,
    pub name_last: String,
    pub origin_page: u64,
    pub assets: u64,
    #[serde(default = "default_note_version")]
    pub version: u64,
}

fn default_note_version() -> u64 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RecipientInput {
    Pkh(String),
    Multisig { m: u64, pkhs: Vec<String> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputInput {
    pub recipient: RecipientInput,
    pub amount: u64,
    #[serde(default)]
    pub alias: Option<String>,
    /// Optional timelock: appends a `%tim` primitive (recipient AND time).
    #[serde(default)]
    pub timelock: Option<TimelockSpec>,
    /// Optional hashlock: appends a `%hax` primitive committing to these
    /// preimage-hash commitments (base58); the spender must reveal preimages.
    #[serde(default)]
    pub hashlock: Option<Vec<String>>,
    /// Burn: the spend-condition is `[%brn]` (unspendable); recipient ignored.
    #[serde(default)]
    pub burn: bool,
    /// Semi-anonymous output: commit only the lock-root (empty note_data) so the
    /// lock structure isn't revealed on-chain until the recipient spends
    /// (nockchain `include_data = false`).
    #[serde(default)]
    pub lock_root_only: bool,
    /// OR-composed lock: any one branch can spend (HTLC / refund-after-timeout
    /// patterns). When set, the flat recipient/timelock/hashlock/burn fields
    /// above are ignored and the lock is the `%2/%4/%8/%16` tree of branches.
    #[serde(default)]
    pub or_branches: Option<Vec<LockBranch>>,
}

/// One branch of an OR-composed lock — a single spend-condition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockBranch {
    #[serde(default)]
    pub recipient: Option<RecipientInput>,
    #[serde(default)]
    pub timelock: Option<TimelockSpec>,
    #[serde(default)]
    pub hashlock: Option<Vec<String>>,
    #[serde(default)]
    pub burn: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelockSpec {
    #[serde(default)]
    pub rel_min: Option<u64>,
    #[serde(default)]
    pub rel_max: Option<u64>,
    #[serde(default)]
    pub abs_min: Option<u64>,
    #[serde(default)]
    pub abs_max: Option<u64>,
}

#[wasm_bindgen]
pub struct ComposedTransactionV1 {
    tx_id: String,
    raw_jam: Vec<u8>,
    wallet_jam: Vec<u8>,
    summary_json: String,
}

#[wasm_bindgen]
impl ComposedTransactionV1 {
    #[wasm_bindgen(getter)]
    pub fn tx_id(&self) -> String {
        self.tx_id.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn raw_jam(&self) -> Vec<u8> {
        self.raw_jam.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn wallet_jam(&self) -> Vec<u8> {
        self.wallet_jam.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn summary_json(&self) -> String {
        self.summary_json.clone()
    }
}

fn hash(text: &str) -> Result<Hash, String> {
    Hash::from_b58(text)
}

fn recipient_condition(recipient: &RecipientInput) -> Result<SpendCondition, String> {
    let (m, hashes) = match recipient {
        RecipientInput::Pkh(text) => (1, vec![hash(text)?]),
        RecipientInput::Multisig { m, pkhs } => (
            *m,
            pkhs.iter()
                .map(|text| hash(text))
                .collect::<Result<_, _>>()?,
        ),
    };
    let h = ZSet::gas(hashes);
    if m == 0 || m > h.wyt() as u64 {
        return Err("invalid multisig threshold".into());
    }
    Ok(SpendCondition {
        p: vec![LockPrimitive {
            header: "pkh".into(),
            body: LockPrimitiveBody::Pkh(Pkh { m, h }),
        }],
    })
}

fn condition(branch: &LockBranch) -> Result<SpendCondition, String> {
    if branch.burn {
        return Ok(SpendCondition {
            p: vec![LockPrimitive {
                header: "brn".into(),
                body: LockPrimitiveBody::Brn(Brn { value: 0 }),
            }],
        });
    }
    let mut result = recipient_condition(
        branch
            .recipient
            .as_ref()
            .ok_or("condition needs a recipient or burn")?,
    )?;
    if let Some(tim) = &branch.timelock {
        if [tim.rel_min, tim.rel_max, tim.abs_min, tim.abs_max]
            .iter()
            .all(Option::is_none)
        {
            return Err("timelock requires at least one bound".into());
        }
        let range = |min: Option<u64>, max: Option<u64>| TimelockRange {
            min: min.map(|value| PageNumber { value }),
            max: max.map(|value| PageNumber { value }),
        };
        result.p.push(LockPrimitive {
            header: "tim".into(),
            body: LockPrimitiveBody::Tim(Tim {
                rel: range(tim.rel_min, tim.rel_max),
                abs: range(tim.abs_min, tim.abs_max),
            }),
        });
    }
    if let Some(commitments) = &branch.hashlock {
        if commitments.is_empty() {
            return Err("hashlock requires a commitment".into());
        }
        let set = ZSet::gas(
            commitments
                .iter()
                .map(|text| hash(text))
                .collect::<Result<Vec<_>, _>>()?,
        );
        result.p.push(LockPrimitive {
            header: "hax".into(),
            body: LockPrimitiveBody::Hax(Hax { set }),
        });
    }
    Ok(result)
}

fn branch_lock(branches: &[LockBranch]) -> Result<Lock, String> {
    Lock::from_list(branches.iter().map(condition).collect::<Result<_, _>>()?)
        .map_err(|error| error.to_string())
}

fn output(input: &OutputInput) -> Result<PlannedOutput, String> {
    let lock = if let Some(branches) = &input.or_branches {
        branch_lock(branches)?
    } else {
        Lock::SpendCondition(condition(&LockBranch {
            recipient: Some(input.recipient.clone()),
            timelock: input.timelock.clone(),
            hashlock: input.hashlock.clone(),
            burn: input.burn,
        })?)
    };
    Ok(PlannedOutput {
        lock_root: lock.hash(),
        amount: input.amount,
        note_data: if input.lock_root_only {
            NoteData { map: ZMap::new() }
        } else {
            NoteData::from_lock(lock).map_err(|error| error.to_string())?
        },
    })
}

struct SourceMatcher {
    signer: SigningKeyLockMatcher,
    supplied: Option<LockResolution>,
}

impl LockMatcher for SourceMatcher {
    fn resolve(&self, note: &NNoteV1) -> Option<LockResolution> {
        if let Some(supplied) = &self.supplied {
            Some(supplied.clone())
        } else {
            self.signer.resolve(note)
        }
    }
}

fn source_matcher(input: &ComposeTxV1Input) -> Result<SourceMatcher, String> {
    if input.source_multisig.is_some() && input.source_or_lock.is_some() {
        return Err("choose one source lock description".into());
    }
    let signer = SigningKeyLockMatcher {
        signer_pkh: hash(&input.source_pkh)?,
        public_key: None,
        coinbase_relative_min: Some(input.coinbase_rel_min.unwrap_or(100)),
    };
    let supplied = if let Some(source) = &input.source_or_lock {
        let lock = branch_lock(&source.branches)?;
        if source.spend_branch >= source.branches.len() as u64 {
            return Err("spend branch is out of range".into());
        }
        let mut preimages = ZMap::new();
        for bytes in &source.preimages {
            let noun = UntypedNoun {
                p: bytes.clone().into(),
            };
            let digest =
                tx_types::hash_hashable(&noun.to_hashable().map_err(|error| error.to_string())?)
                    .map_err(|error| error.to_string())?;
            preimages.put(digest, noun);
        }
        let selected = &lock.flatten_spend_conditions()[source.spend_branch as usize];
        let signature_count = selected
            .required_pkh_policy()
            .map_or(0, |policy| policy.threshold as u64);
        Some(LockResolution {
            lock,
            leaf_number: source.spend_branch + 1,
            signature_count,
            preimages,
            fund_maturity: None,
        })
    } else if let Some(source) = &input.source_multisig {
        let condition = recipient_condition(&RecipientInput::Multisig {
            m: source.m,
            pkhs: source.pkhs.clone(),
        })?;
        Some(LockResolution {
            lock: Lock::SpendCondition(condition),
            leaf_number: 1,
            signature_count: source.m,
            preimages: ZMap::new(),
            fund_maturity: None,
        })
    } else {
        None
    };
    Ok(SourceMatcher { signer, supplied })
}

#[derive(Clone)]
struct WalletTransaction {
    name: String,
    spends: SpendsV1,
    display: UntypedNoun,
    witnesses: ZMap<NName, Witness>,
}

impl Encode for WalletTransaction {
    fn encode(&self, arena: &mut Arena) -> Result<Noun, Error> {
        let witnesses = (1u64, &self.witnesses).encode(arena)?;
        let fields = [
            1u64.encode(arena)?,
            self.name.encode(arena)?,
            self.spends.encode(arena)?,
            self.display.encode(arena)?,
            witnesses,
        ];
        tuple(arena, &fields)
    }
}

impl Decode for WalletTransaction {
    fn decode(noun: Noun, arena: &Arena) -> Result<Self, Error> {
        let mut fields = Fields::new(noun, 5);
        if u64::decode(fields.next(arena)?, arena)? != 1 {
            return Err(Error("wallet transaction must be V1"));
        }
        let name = String::decode(fields.next(arena)?, arena)?;
        let spends = SpendsV1::decode(fields.next(arena)?, arena)?;
        let display = UntypedNoun::decode(fields.next(arena)?, arena)?;
        let (tag, witnesses) = <(u64, ZMap<NName, Witness>)>::decode(fields.next(arena)?, arena)?;
        if tag != 1 {
            return Err(Error("wallet witness data must be V1"));
        }
        Ok(Self {
            name,
            spends,
            display,
            witnesses,
        })
    }
}

fn compose(
    input: ComposeTxV1Input,
    order: SelectionOrder,
) -> Result<ComposedTransactionV1, String> {
    let matcher = source_matcher(&input)?;
    let notes = input
        .notes
        .iter()
        .map(|note| {
            Ok(NNote::V1(NNoteV1 {
                version: note.version,
                origin_page: PageNumber {
                    value: note.origin_page,
                },
                name: NName {
                    p: vec![hash(&note.name_first)?, hash(&note.name_last)?],
                },
                assets: Coins { value: note.assets },
                note_data: tx_types::rpc_note_data::note_data_from_json(&note.note_data)?,
            }))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let outputs = input
        .outputs
        .iter()
        .map(output)
        .collect::<Result<Vec<_>, _>>()?;
    let refund_lock = matcher
        .supplied
        .as_ref()
        .map(|source| source.lock.clone())
        .unwrap_or_else(|| {
            Lock::SpendCondition(tx_types::lock_resolver::simple_pkh(
                matcher.signer.signer_pkh.clone(),
            ))
        });
    let refund = PlannedOutput {
        lock_root: refund_lock.hash(),
        amount: 0,
        note_data: NoteData::from_lock(refund_lock).map_err(|error| error.to_string())?,
    };
    let chain = ChainContext {
        candidate_height: input.current_height,
        fee_height: input
            .current_height
            .unwrap_or(input.bythos_phase.unwrap_or(tx_types::fee::BYTHOS_PHASE)),
        bythos_phase: input.bythos_phase.unwrap_or(tx_types::fee::BYTHOS_PHASE),
        base_fee: input.base_fee.unwrap_or(tx_types::fee::BASE_FEE),
        input_fee_divisor: input
            .input_fee_divisor
            .unwrap_or(tx_types::fee::INPUT_FEE_DIVISOR),
        min_fee: input.min_fee.unwrap_or(tx_types::fee::MIN_FEE),
    };
    let plan = planner::plan(
        PlanRequest {
            candidates: &notes,
            outputs: &outputs,
            refund: &refund,
            selection: if input.manual_inputs {
                planner::SelectionMode::Manual(
                    notes
                        .iter()
                        .map(|note| match note {
                            NNote::V1(note) => note.name.clone(),
                            _ => unreachable!(),
                        })
                        .collect(),
                )
            } else {
                planner::SelectionMode::Auto
            },
            order,
            version: planner::CandidateVersionPolicy::V1Only,
            mode: planner::PlanningMode::Standard,
            fee: planner::FeePolicy::Minimum,
            chain: chain.clone(),
        },
        &matcher,
    )
    .map_err(|error| error.to_string())?;
    let id = tx_types::compute_tx_id_v1(&plan.spends).map_err(|error| error.to_string())?;
    let mut display = ZMap::new();
    let mut witnesses = ZMap::new();
    let mut summary_spends = Vec::new();
    for (name, spend) in plan.spends.map.tap() {
        let SpendBody::V1(body) = spend.body else {
            return Err("composer requires V1 inputs".into());
        };
        display.put(name.clone(), body.witness.lmp.spend_condition().clone());
        witnesses.put(name.clone(), body.witness);
        summary_spends.push(serde_json::json!({
            "input": format!("{} {}", name.p[0].to_b58(), name.p[1].to_b58()), "fee": body.fee.value,
            "refund": plan.refunds.iter().find(|(input, _)| input == &name).map_or(0, |(_, value)| *value),
            "seeds": body.seeds.set.tap().iter().map(|seed| serde_json::json!({
                "recipient": seed.lock_root.to_b58(), "gift": seed.gift.value,
            })).collect::<Vec<_>>(),
        }));
    }
    let wallet = WalletTransaction {
        name: id.to_b58(),
        spends: plan.spends.clone(),
        witnesses,
        display: UntypedNoun::from_value(&((1u64, display), 0u64))
            .map_err(|error| error.to_string())?,
    };
    let summary_inputs = plan
        .selected
        .iter()
        .map(|name| {
            let note = notes
                .iter()
                .find_map(|note| match note {
                    NNote::V1(note) if note.name == *name => Some(note),
                    _ => None,
                })
                .unwrap();
            serde_json::json!({ "name_first": name.p[0].to_b58(), "name_last": name.p[1].to_b58(),
            "assets": note.assets.value, "capacity": note.assets.value })
        })
        .collect::<Vec<_>>();
    let raw = RawTransactionV1 {
        version: 1,
        id: id.clone(),
        spends: plan.spends,
    };
    let summary = serde_json::json!({
        "tx_id": id.to_b58(), "source_pkh": input.source_pkh, "outputs": input.outputs,
        "inputs_used": summary_inputs, "spends": summary_spends, "total_fees": plan.final_fee,
        "minimum_fee": plan.minimum_fee, "seed_words": plan.word_counts.seed_words,
        "witness_words": plan.word_counts.witness_words, "coinbase_rel_min": input.coinbase_rel_min.unwrap_or(100),
        "current_height": chain.candidate_height, "bythos_phase": chain.bythos_phase,
        "base_fee": chain.base_fee, "input_fee_divisor": chain.input_fee_divisor, "min_fee": chain.min_fee,
    });
    Ok(ComposedTransactionV1 {
        tx_id: id.to_b58(),
        raw_jam: raw.to_jam().map_err(|error| error.to_string())?,
        wallet_jam: wallet.to_jam().map_err(|error| error.to_string())?,
        summary_json: summary.to_string(),
    })
}

#[wasm_bindgen]
pub fn compose_tx_v1_unsigned(input: JsValue) -> Result<ComposedTransactionV1, JsValue> {
    let input = serde_wasm_bindgen::from_value(input).map_err(js_err)?;
    compose(input, SelectionOrder::Ascending).map_err(js_err)
}

#[wasm_bindgen]
pub fn compose_tx_v1_min_inputs(input: JsValue) -> Result<ComposedTransactionV1, JsValue> {
    let input = serde_wasm_bindgen::from_value(input).map_err(js_err)?;
    compose(input, SelectionOrder::Descending).map_err(js_err)
}

pub fn estimate_tx_v1_fee(input: ComposeTxV1Input) -> Result<u64, String> {
    let composed = compose(input, SelectionOrder::Descending)?;
    let summary: serde_json::Value =
        serde_json::from_str(&composed.summary_json).map_err(|error| error.to_string())?;
    summary["total_fees"]
        .as_u64()
        .ok_or_else(|| "missing planned fee".into())
}

#[wasm_bindgen]
pub fn compose_tx_v1_recipient_address(recipient: JsValue) -> Result<String, JsValue> {
    let recipient: RecipientInput = serde_wasm_bindgen::from_value(recipient).map_err(js_err)?;
    let condition = recipient_condition(&recipient).map_err(js_err)?;
    Ok(match recipient {
        RecipientInput::Pkh(text) => text,
        _ => condition.to_hash().to_b58(),
    })
}

fn merge(a: &[u8], b: &[u8]) -> Result<Vec<u8>, String> {
    let mut a = WalletTransaction::from_jam(a).map_err(|error| error.to_string())?;
    let b = WalletTransaction::from_jam(b).map_err(|error| error.to_string())?;
    if a.spends.map.wyt() != b.spends.map.wyt() {
        return Err("transaction inputs differ".into());
    }
    let mut spends = ZMap::new();
    let mut witnesses = ZMap::new();
    for (name, mut spend) in a.spends.map.tap() {
        let mut extra = b
            .spends
            .map
            .get(&name)
            .ok_or("transaction inputs differ")?
            .clone();
        let (SpendBody::V1(body), SpendBody::V1(extra_body)) = (&mut spend.body, &mut extra.body)
        else {
            return Err("merge requires V1 witness spends".into());
        };
        let sigs = core::mem::replace(&mut body.witness.pkh.map, ZMap::new());
        let extra_sigs = core::mem::replace(&mut extra_body.witness.pkh.map, ZMap::new());
        if body.to_jam().map_err(|error| error.to_string())?
            != extra_body.to_jam().map_err(|error| error.to_string())?
        {
            return Err("transactions authorize different spends".into());
        }
        body.witness.pkh.map = sigs;
        for (key, value) in extra_sigs.tap() {
            if let Some(existing) = body.witness.pkh.map.get(&key) {
                if existing.to_jam().map_err(|error| error.to_string())?
                    != value.to_jam().map_err(|error| error.to_string())?
                {
                    return Err("conflicting signatures for the same key".into());
                }
            }
            body.witness.pkh.map.put(key, value);
        }
        witnesses.put(name.clone(), body.witness.clone());
        spends.put(name, spend);
    }
    a.spends = SpendsV1 { map: spends };
    a.witnesses = witnesses;
    a.name = tx_types::compute_tx_id_v1(&a.spends)
        .map_err(|error| error.to_string())?
        .to_b58();
    a.to_jam().map_err(|error| error.to_string())
}

#[wasm_bindgen]
pub fn merge_signed_tx(a: &[u8], b: &[u8]) -> Result<Vec<u8>, JsValue> {
    merge(a, b).map_err(js_err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    const A: &str = "9yPePjfWAdUnzaQKyxcRXKRa5PpUzKKEwtpECBZsUYt9Jd7egSDEWoV";
    const B: &str = "9phXGACnW4238oqgvn2gpwaUjG3RAqcxq2Ash2vaKp8KjzSd3MQ56Jt";

    fn input() -> ComposeTxV1Input {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../tests/fixtures/nockster-signed.json"
        ))
        .unwrap();
        let note = &fixture["note"];
        serde_json::from_value(json!({
            "source_pkh": fixture["source"], "current_height": 60000,
            "notes": [{"name_first": note["firstName"], "name_last": note["lastName"],
                "origin_page": note["originPage"], "assets": 10000000, "note_data": note["noteData"]}],
            "outputs": [{"recipient": A, "amount": 1000000}],
        })).unwrap()
    }

    #[test]
    fn recipient_roots_match_official_wallet_vectors() {
        assert_eq!(
            recipient_condition(&RecipientInput::Pkh(A.into()))
                .unwrap()
                .to_hash()
                .to_b58(),
            "DKrgXqE8bXR1uBZ3t4vU13m2KquGCDbnn1PeoPL7dxSHTucGPFDPt53"
        );
        assert_eq!(
            recipient_condition(&RecipientInput::Multisig {
                m: 2,
                pkhs: vec![B.into(), A.into()]
            })
            .unwrap()
            .to_hash()
            .to_b58(),
            "4eMAT3BuhLPjYFronoYJ9RSLVSgveCL3nQB7RHSLZzjBTiYCxEzkzEH"
        );
    }

    #[test]
    fn drafts_bind_actual_note_data_and_estimate_fully_signed_fees() {
        for height in [53999, 54000, 170000] {
            let mut request = input();
            request.current_height = Some(height);
            let data =
                tx_types::rpc_note_data::note_data_from_json(&request.notes[0].note_data).unwrap();
            let note = NNoteV1 {
                version: 1,
                origin_page: PageNumber {
                    value: request.notes[0].origin_page,
                },
                name: NName {
                    p: vec![
                        hash(&request.notes[0].name_first).unwrap(),
                        hash(&request.notes[0].name_last).unwrap(),
                    ],
                },
                assets: Coins {
                    value: request.notes[0].assets,
                },
                note_data: data,
            };
            let result = compose(request, SelectionOrder::Descending).unwrap();
            let raw = RawTransactionV1::from_jam(&result.raw_jam).unwrap();
            let wallet = WalletTransaction::from_jam(&result.wallet_jam).unwrap();
            assert_eq!(
                raw.spends.to_jam().unwrap(),
                wallet.spends.to_jam().unwrap()
            );
            assert_eq!(tx_types::compute_tx_id_v1(&raw.spends).unwrap(), raw.id);
            assert_eq!(wallet.witnesses.wyt(), 1);
            let SpendBody::V1(body) = &raw.spends.map.tap()[0].1.body else {
                panic!()
            };
            assert!(body.witness.lmp.check(&note.name.p[0]));
            assert_eq!(body.witness.pkh.map.wyt(), 0);
            assert!(body
                .seeds
                .set
                .tap()
                .iter()
                .all(|seed| seed.parent_hash == note.to_hash().unwrap()));
            assert_eq!(
                body.seeds
                    .set
                    .tap()
                    .iter()
                    .map(|seed| seed.gift.value)
                    .sum::<u64>()
                    + body.fee.value,
                note.assets.value
            );
            let summary: serde_json::Value = serde_json::from_str(&result.summary_json).unwrap();
            let words =
                tx_types::word_count::transaction_words(&raw.spends.map, height >= 54000).unwrap();
            assert_eq!(
                summary["witness_words"].as_u64().unwrap(),
                words.witness_words + 35
            );
            assert!(
                summary["total_fees"].as_u64().unwrap() >= summary["minimum_fee"].as_u64().unwrap()
            );
        }
    }

    #[test]
    fn tagged_source_requires_preimages_and_preserves_the_full_change_lock() {
        let bytes = 42u64.to_jam().unwrap();
        let commitment = tx_types::hash_hashable(
            &UntypedNoun {
                p: bytes.clone().into(),
            }
            .to_hashable()
            .unwrap(),
        )
        .unwrap();
        let branches: Vec<LockBranch> = serde_json::from_value(json!([
            {"recipient": A, "hashlock": [commitment.to_b58()]},
            {"recipient": B, "timelock": {"abs_min": 70000}}
        ]))
        .unwrap();
        let lock = branch_lock(&branches).unwrap();
        let mut request = input();
        request.source_or_lock = Some(OrSourceInput {
            branches,
            spend_branch: 0,
            preimages: vec![],
        });
        request.notes[0].name_first = NName::first_v1(lock.hash()).to_b58();
        request.notes[0].note_data = json!({});
        assert!(compose(request.clone(), SelectionOrder::Descending).is_err());
        request
            .source_or_lock
            .as_mut()
            .unwrap()
            .preimages
            .push(bytes);
        let result = compose(request.clone(), SelectionOrder::Descending).unwrap();
        let raw = RawTransactionV1::from_jam(&result.raw_jam).unwrap();
        let SpendBody::V1(body) = &raw.spends.map.tap()[0].1.body else {
            panic!()
        };
        assert_eq!(body.witness.lmp.axis(), 6);
        assert!(body
            .witness
            .lmp
            .check(&hash(&request.notes[0].name_first).unwrap()));
        assert!(body
            .seeds
            .set
            .tap()
            .iter()
            .any(|seed| seed.lock_root == lock.hash()
                && seed.note_data.to_jam().unwrap()
                    == NoteData::from_lock(lock.clone()).unwrap().to_jam().unwrap()));
        request.source_or_lock.as_mut().unwrap().spend_branch = 1;
        assert!(compose(request.clone(), SelectionOrder::Descending).is_err());
        request.current_height = Some(70000);
        assert!(compose(request, SelectionOrder::Descending).is_ok());
    }

    #[test]
    fn merger_rejects_different_authorized_spends() {
        let a = compose(input(), SelectionOrder::Descending).unwrap();
        let merged = merge(&a.wallet_jam, &a.wallet_jam).unwrap();
        assert_eq!(merged, a.wallet_jam);
        let mut request = input();
        request.outputs[0].amount += 1;
        let b = compose(request, SelectionOrder::Descending).unwrap();
        assert!(merge(&a.wallet_jam, &b.wallet_jam).is_err());
        let mut request = input();
        request.notes[0].name_last = A.into();
        let b = compose(request, SelectionOrder::Descending).unwrap();
        assert!(merge(&a.wallet_jam, &b.wallet_jam).is_err());
    }
}
