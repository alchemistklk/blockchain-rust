use crate::{block::Block, errors::Result, tx::TXOutputs};
use std::{collections::HashMap, fs::remove_dir_all};

use crate::blockchain::BlockChain;

pub struct Utxoset {
    // allow us to access the data that are connected to the blockchain
    // we can create a new layer inside of the database
    pub blockchain: BlockChain,
}

impl Utxoset {
    // store into database
    pub fn reindex(&self) -> Result<()> {
        // reset the db files
        if std::path::Path::new("data/utxos").exists() {
            remove_dir_all("data/utxos")?;
        }
        let db = sled::open("data/utxos")?;

        let utxos = self.blockchain.find_utxo();

        for (txid, tx_outputs) in utxos {
            db.insert(txid.as_bytes(), bincode::serialize(&tx_outputs)?)?;
        }
        Ok(())
    }

    pub fn update(&self, block: &Block) -> Result<()> {
        let db = sled::open("data/utxos")?;

        for tx in block.get_transactions() {
            if !tx.is_coinbase() {
                for tx_i in &tx.vin {
                    let db_data = db.get(&tx_i.txid)?.unwrap();
                    let outs: TXOutputs = bincode::deserialize(&db_data)?;

                    let mut update_outs = TXOutputs { outputs: vec![] };

                    for out_idx in 0..outs.outputs.len() {
                        if out_idx != tx_i.vout as usize {
                            update_outs.outputs.push(outs.outputs[out_idx].clone());
                        }
                    }

                    if update_outs.outputs.is_empty() {
                        db.remove(&tx_i.txid)?;
                    } else {
                        db.insert(&tx_i.txid, bincode::serialize(&update_outs)?)?;
                    }
                }
            }

            let mut new_output = TXOutputs { outputs: vec![] };

            for out in &tx.vout {
                new_output.outputs.push(out.clone());
            }
            db.insert(tx.id.as_bytes(), bincode::serialize(&new_output)?)?;
        }
        Ok(())
    }

    pub fn count_transaction(&self) -> Result<i32> {
        let mut counter = 0;
        let db = sled::open("data/utxos")?;

        for kv in db.iter() {
            kv?;
            counter += 1;
        }
        Ok(counter)
    }

    pub fn find_spendable_outputs(
        &self,
        address: &[u8],
        amount: i32,
    ) -> (i32, HashMap<String, Vec<i32>>) {
        let mut unspent_outputs: HashMap<String, Vec<i32>> = HashMap::new();

        let mut accumulated: i32 = 0;
        let db = sled::open("data/utxos").unwrap();
        for kv in db.iter() {
            let (k, v) = kv.unwrap();
            let txid = String::from_utf8(k.to_vec()).unwrap();
            let outs: TXOutputs = bincode::deserialize(&v.to_vec()).unwrap();

            for out_idx in 0..outs.outputs.len() {
                if outs.outputs[out_idx].can_be_unlock_with(address) && accumulated < amount {
                    accumulated += outs.outputs[out_idx].value;
                    match unspent_outputs.get_mut(&txid) {
                        Some(e) => {
                            e.push(out_idx as i32);
                        }
                        None => {
                            unspent_outputs.insert(txid.clone(), vec![out_idx as i32]);
                        }
                    }
                }
            }
        }
        (accumulated, unspent_outputs)
    }

    pub fn find_utxo(&self, pub_key_hash: &[u8]) -> Result<TXOutputs> {
        let mut utxos = TXOutputs { outputs: vec![] };

        let db = sled::open("data/utxos")?;

        for kv in db.iter() {
            let (_, v) = kv?;
            let outs: TXOutputs = bincode::deserialize(&v.to_vec())?;

            for out in outs.outputs {
                if out.can_be_unlock_with(pub_key_hash) {
                    utxos.outputs.push(out.clone());
                }
            }
        }
        Ok(utxos)
    }
}
