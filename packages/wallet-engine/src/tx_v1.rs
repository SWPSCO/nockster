use tx_types::{Inputs, PkhSignature, Transaction, ZMap};

pub fn sign_transaction_v1(
    mut tx: Transaction,
    key: &tx_types::T8,
    pubkey: &tx_types::SchnorrPubkey,
) -> Result<Transaction, String> {
    let Inputs::V1(ref mut inputs) = tx.p else {
        return Err("expected v1 transaction".into());
    };
    let mut signed = ZMap::new();
    let mut spends = ZMap::new();
    for (name, mut input) in inputs.map.tap() {
        input.spend.witness.pkh = PkhSignature { map: ZMap::new() };
        let spend = tx_types::sign_spend_v1(input.spend, key.clone(), pubkey.clone())?;
        let tx_types::SpendBody::V1(ref body) = spend.body else {
            unreachable!()
        };
        input.spend = body.clone();
        signed.put(name.clone(), input);
        spends.put(name, spend);
    }
    inputs.map = signed;
    tx.name = tx_types::compute_tx_id_v1(&tx_types::SpendsV1 { map: spends })
        .map_err(|err| format!("compute tx id: {err}"))?
        .to_b58();
    Ok(tx)
}
