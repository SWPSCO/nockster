use log::*;
use bytes::Bytes;
use nockapp::noun::slab::NounSlab;
use crate::wallet;
use noun_serde::{NounDecode, NounEncode};
use tx_types::{
    transaction_types::*,
    hashing::compute_tx_id,
    signer::schnorr_sign_digest,
    collections::{ZMap, ZSet},
    tx_to_noun::calculate_timelock_range,
};


/// Core function to create a transaction struct (Rust-only, no WASM bindings)
/// 
/// This function implements a three-pass greedy UTXO selection algorithm:
/// 
/// **First Pass:**
/// 1. Sort notes by asset value (largest first)
/// 2. Sort gifts by amount (largest first) 
/// 3. For each note, subtract fee, then assign gifts until we can't fit more
/// 4. Move to next note if more gifts remain (no refunds calculated yet)
/// 
/// **Second Pass (if recipients remain unassigned):**
/// 1. Collect notes that still have capacity after fees and first-pass assignments
/// 2. Sort these notes by remaining capacity (largest first)
/// 3. Split large recipients across multiple notes by creating partial seeds
/// 4. Update existing inputs with additional seeds for split recipients
/// 
/// **Third Pass:**
/// 1. Calculate remaining capacity for each note after all recipient assignments
/// 2. Create refund seeds for any remaining amounts back to refund address
/// 
/// Input = Note + Spend
/// Spend = Seeds + Fee + (optional) Signature  
/// Seeds = Set of individual Seed outputs
/// Each Seed = Recipient + Gift + Parent Hash + Timelock Intent
pub fn create_transaction_struct(
    refund_address: String,
    mut notes_with_fees: Vec<(NNote, u64)>,
    mut recipients: Vec<(String, u64)>,
) -> Result<Transaction, String> {
    info!("Creating transaction with {} notes and {} recipients", notes_with_fees.len(), recipients.len());
    
    if notes_with_fees.is_empty() {
        return Err("No input notes provided".to_string());
    }
    
    if recipients.is_empty() {
        return Err("No recipients provided".to_string());
    }
    
    // Parse refund address using Lock helper
    let refund_lock = Lock::from_b58(1, vec![refund_address.clone()])
        .map_err(|e| format!("Invalid refund address '{}': {}", refund_address, e))?;
    
    // Sort notes by asset value (largest first)
    notes_with_fees.sort_by(|a, b| b.0.assets.value.cmp(&a.0.assets.value));
    
    // Sort recipients by gift amount (largest first)
    recipients.sort_by(|a, b| b.1.cmp(&a.1));
    
    info!("Sorted {} notes by asset value (largest first)", notes_with_fees.len());
    info!("Sorted {} recipients by gift amount (largest first)", recipients.len());
    
    let mut input_pairs = Vec::new(); // Collect (name, input) pairs
    let mut remaining_recipients = recipients;
    
    // Process each note in order (largest first)
    for (note, fee) in notes_with_fees {
        
        info!("Processing note with {} coins, fee: {}", note.assets.value, fee);
        
        // Check if note can cover its own fee
        if note.assets.value < fee {
            return Err(format!(
                "Note has insufficient funds to cover fee: need {}, have {}",
                fee, note.assets.value
            ));
        }
        
        let mut available_after_fee = note.assets.value - fee;
        let parent_hash = note.to_hash();
        let mut seeds_for_this_note = Vec::new();
        
        // Greedily assign gifts to this note (largest gifts first)
        // Always check index 0 since we remove elements as we process them
        while !remaining_recipients.is_empty() && available_after_fee >= remaining_recipients[0].1 {
            let (recipient_address, recipient_amount) = &remaining_recipients[0];
            
            // Create recipient lock using Lock helper (1-of-1 multisig)
            let recipient_lock = Lock::from_b58(1, vec![recipient_address.clone()])
                .map_err(|e| format!("Invalid recipient address '{}': {}", recipient_address, e))?;
            
            // Create seed for this recipient
            let seed = Seed {
                output_source: None,
                recipient: recipient_lock,
                timelock_intent: None,
                gift: Coins { value: *recipient_amount },
                parent_hash: parent_hash.clone(),
            };
            
            seeds_for_this_note.push(seed);
            available_after_fee -= recipient_amount;
            
            info!("Assigned {} coins to {} from this note", recipient_amount, recipient_address);
            
            // Remove this recipient from remaining list (always remove index 0)
            remaining_recipients.remove(0);
        }
        
        // Note: Refunds will be calculated in the third pass after all recipient assignments are complete
        
        let spend = Spend {
            signature: None, // Will be added during signing
            seeds: Seeds { set: ZSet::gas(seeds_for_this_note) },
            fee: Coins { value: fee },
        };
        
        // Create input
        let input = Input { note: note.clone(), spend };
        
        // Add to input pairs for later ZMap creation
        input_pairs.push((note.name.clone(), input));
        
        
        // If we've assigned all recipients, we're done
        if remaining_recipients.is_empty() {
            info!("All recipients assigned, transaction complete");
            break;
        }
    }
    
    // Check if we have unassigned recipients
    if !remaining_recipients.is_empty() {
        info!("Starting second pass: splitting large recipients across multiple notes");
        
        // Second pass: handle recipients that are too large for any single note
        // by splitting them across multiple notes
        let mut notes_with_remaining_capacity = Vec::new();
        
        // Collect notes that still have capacity after fees
        for (note_name, input) in &input_pairs {
            let note = &input.note;
            let fee = input.spend.fee.value;
            // At this point (second pass), all existing seeds should be from the first pass (recipients)
            // No refund seeds have been added yet, so we can sum all seeds
            let used_amount: u64 = input.spend.seeds.set.iter()
                .map(|seed| seed.gift.value)
                .sum();
            let available_capacity = note.assets.value.saturating_sub(fee + used_amount);
            
            if available_capacity > 0 {
                notes_with_remaining_capacity.push((note_name.clone(), available_capacity, input.clone()));
                info!("Note {} has {} remaining capacity", note_name.p.len(), available_capacity);
            }
        }
        
        // Sort notes by remaining capacity (largest first)
        notes_with_remaining_capacity.sort_by(|a, b| b.1.cmp(&a.1));
        
        // Try to split remaining recipients across available notes
        let mut updated_inputs = std::collections::HashMap::new();
        
        for (recipient_address, recipient_amount) in remaining_recipients {
            let mut remaining_amount = recipient_amount;
            info!("Splitting recipient {} coins across multiple notes", remaining_amount);
            
            // Create recipient lock once
            let recipient_lock = Lock::from_b58(1, vec![recipient_address.clone()])
                .map_err(|e| format!("Invalid recipient address '{}': {}", recipient_address, e))?;
            
            for i in 0..notes_with_remaining_capacity.len() {
                if remaining_amount == 0 {
                    break;
                }
                
                let (note_name, available_capacity, input) = &mut notes_with_remaining_capacity[i];
                let amount_to_assign = std::cmp::min(remaining_amount, *available_capacity);
                if amount_to_assign == 0 {
                    continue;
                }
                
                // Create a partial seed for this recipient on this note
                let partial_seed = Seed {
                    output_source: None,
                    recipient: recipient_lock.clone(),
                    timelock_intent: None,
                    gift: Coins { value: amount_to_assign },
                    parent_hash: input.note.to_hash(),
                };
                
                // Add this seed to the input's existing seeds
                let mut existing_seeds: Vec<Seed> = input.spend.seeds.set.iter().cloned().collect();
                existing_seeds.push(partial_seed);
                
                // Update the spend with new seeds
                input.spend.seeds = Seeds { set: ZSet::gas(existing_seeds) };
                
                // Update capacity and remaining amount
                *available_capacity -= amount_to_assign;
                remaining_amount -= amount_to_assign;
                
                // Mark this input as updated
                updated_inputs.insert(note_name.clone(), input.clone());
                
                info!("Assigned {} coins to {} from note {}", amount_to_assign, recipient_address, note_name.p.len());
            }
            
            if remaining_amount > 0 {
                return Err(format!(
                    "Insufficient total funds: could not assign {} remaining coins to recipient {}",
                    remaining_amount, recipient_address
                ));
            }
        }
        
        // Update the input_pairs with modified inputs
        for (i, (note_name, input)) in input_pairs.iter_mut().enumerate() {
            if let Some(updated_input) = updated_inputs.get(note_name) {
                *input = updated_input.clone();
                info!("Updated input {} with split recipient assignments", i);
            }
        }
        
        info!("Successfully split all remaining recipients across multiple notes");
    }
    
    // Third pass: Calculate and add refunds for any remaining capacity
    info!("Starting third pass: calculating refunds for remaining capacity");
    for (i, (note_name, input)) in input_pairs.iter_mut().enumerate() {
        let note = &input.note;
        let fee = input.spend.fee.value;
        
        // Calculate how much has been used for recipients
        // At this point (third pass), all existing seeds should be recipient seeds from passes 1&2
        // No refund seeds have been added yet, so we can sum all seeds
        let used_for_recipients: u64 = input.spend.seeds.set.iter()
            .map(|seed| seed.gift.value)
            .sum();
        
        // Calculate remaining capacity for refund
        let remaining_capacity = note.assets.value.saturating_sub(fee + used_for_recipients);
        
        
        if remaining_capacity > 0 {
            // Create refund seed
            let refund_seed = Seed {
                output_source: None,
                recipient: refund_lock.clone(),
                timelock_intent: None,
                gift: Coins { value: remaining_capacity },
                parent_hash: note.to_hash(),
            };
            
            // Add refund seed to existing seeds
            let mut all_seeds: Vec<Seed> = input.spend.seeds.set.iter().cloned().collect();
            all_seeds.push(refund_seed);
            
            // Update the spend with all seeds including refund
            input.spend.seeds = Seeds { set: ZSet::gas(all_seeds) };
            
            info!("Added refund of {} coins to input {} (note {})", remaining_capacity, i, note_name.p.len());
        } else {
            info!("No refund needed for input {} (note {})", i, note_name.p.len());
        }
    }
    info!("Completed third pass: all refunds calculated");
    
    let inputs = Inputs { p: ZMap::gas(input_pairs) };
    
    // Calculate timelock range and total fees
    let input_list: Vec<Input> = inputs.p.tap().iter().map(|(_, input)| input.clone()).collect();
    let timelock_range = calculate_timelock_range(&input_list);
    let total_fees: u64 = input_list.iter().map(|input| input.spend.fee.value).sum();
    
    // Generate transaction ID
    let tx_id = compute_tx_id(&inputs, &timelock_range, Coins { value: total_fees });
    
    // Create transaction
    let transaction = Transaction {
        name: tx_id.to_b58(),
        p: inputs,
    };
    
    info!("Created transaction with ID: {}", transaction.name);
    info!("Transaction uses {} inputs with total fees: {}", input_list.len(), total_fees);
    
    Ok(transaction)
}

/// Wrapper function that creates a transaction and returns JAM bytes
pub fn create_transaction_core(
    refund_address: String,
    notes_with_fees: Vec<(NNote, u64)>,
    recipients: Vec<(String, u64)>,
) -> Result<Vec<u8>, String> {
    // Create the transaction struct
    let transaction = create_transaction_struct(refund_address, notes_with_fees, recipients)?;
    
    // Encode to JAM
    let mut slab: NounSlab = NounSlab::new();
    let tx_noun = transaction.to_noun(&mut slab);
    slab.copy_into(tx_noun);
    let jam_bytes = slab.jam();
    
    info!("Transaction encoded to {} bytes", jam_bytes.len());
    
    Ok(jam_bytes.to_vec())
}

/// Core function to sign a transaction (Rust-only, no WASM bindings)
pub fn sign_transaction_core(
    transaction_jam: &[u8], 
    wallet: &wallet::Wallet
) -> Result<Vec<u8>, String> {
    info!("Signing transaction from jam file, {} bytes", transaction_jam.len());
    
    // Convert the byte array to Bytes
    let bytes = Bytes::copy_from_slice(transaction_jam);
    
    // Create a new NounSlab to work with
    let mut slab: NounSlab = NounSlab::new();
    
    // Cue the jammed data into a Noun
    let tx_noun = slab.cue_into(bytes)
        .map_err(|e| format!("Failed to deserialize transaction jam: {}", e))?;
    
    info!("Successfully cued transaction data into noun format");
    
    // Parse the noun as a Transaction
    let transaction = Transaction::from_noun(&tx_noun)
        .map_err(|e| format!("Failed to parse transaction: {}", e))?;
    
    info!("Successfully parsed transaction with {} inputs", transaction.p.p.wyt());
    
    // Convert wallet keys to the required formats
    let private_key_t8 = wallet.get_private_key_as_t8()?;
    let public_key_schnorr = wallet.get_pubkey_as_schnorr_pubkey()?;
    
    // Calculate total fees and timelock range for transaction ID generation
    let input_list: Vec<Input> = transaction.p.p.tap().iter().map(|(_, input)| input.clone()).collect();
    let total_fees_value: u64 = input_list.iter().map(|input| input.spend.fee.value).sum();
    
    // Calculate timelock range from all inputs
    let timelock_range = calculate_timelock_range(&input_list);
    
    // Generate transaction ID for signing
    let tx_id = compute_tx_id(&transaction.p, &timelock_range, Coins { value: total_fees_value });
    info!("Generated transaction ID: {}", tx_id.to_b58());
    
    // Sign the transaction ID using Schnorr signature
    let (challenge_t8, signature_t8) = schnorr_sign_digest(
        private_key_t8,
        public_key_schnorr.clone(),
        tx_id,
    );
    
    info!("Generated Schnorr signature");
    
    // Create the signature components
    let schnorr_signature = SchnorrSignature {
        chal: Chal { values: challenge_t8 },
        sig: Sig { values: signature_t8 },
    };
    
    // Sign each input that requires our signature
    let mut new_inputs = ZMap::new();
    let mut signatures_added = 0;
    
    for (name, mut input) in transaction.p.p.tap() {
        let lock = &input.note.lock;
        
        // Check if our public key is in the lock's pubkey set
        if lock.pubkeys.has(&public_key_schnorr) {
            // Add signature to this input
            match &mut input.spend.signature {
                Some(sig_map) => {
                    sig_map.map.put(public_key_schnorr.clone(), schnorr_signature.clone());
                }
                None => {
                    let mut sig_map = ZMap::new();
                    sig_map.put(public_key_schnorr.clone(), schnorr_signature.clone());
                    input.spend.signature = Some(Signature { map: sig_map });
                }
            }
            
            signatures_added += 1;
            info!("Added signature for input: {:?}", name);
        }
        
        new_inputs.put(name, input);
    }
    
    if signatures_added == 0 {
        return Err("No inputs found that can be signed with this wallet's key".to_string());
    }
    
    info!("Added {} signature(s)", signatures_added);
    
    // Create signed transaction
    let signed_transaction = Transaction {
        name: transaction.name.clone(),
        p: Inputs { p: new_inputs },
    };
    
    // Encode signed transaction back to jam
    let mut out_slab: NounSlab = NounSlab::new();
    let signed_noun = signed_transaction.to_noun(&mut out_slab);
    out_slab.copy_into(signed_noun);
    let signed_bytes = out_slab.jam();
    
    info!("Successfully signed transaction, {} bytes", signed_bytes.len());
    
    Ok(signed_bytes.to_vec())
}
