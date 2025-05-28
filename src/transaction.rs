use std::collections::HashMap;

use crate::{
    errors::Result, tx::{TXInput, TXOutput}, utxoset::Utxoset, wallet::{Wallet}
};

use crypto::{digest::Digest, ed25519, ripemd160::Ripemd160, sha2::Sha256};
use failure::format_err;
use log::error;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Transaction {
    pub id: String,
    pub vin: Vec<TXInput>,
    pub vout: Vec<TXOutput>,
}

impl Transaction {
    pub fn new_utxo(wallet: &Wallet, to: &str, amount: i32, ut: &Utxoset) -> Result<Transaction> {
        let mut vin = Vec::<TXInput>::new();
        let mut pub_key_hash = wallet.public_key.clone();
        hash_pub_key(&mut pub_key_hash);
        let acc_v = ut.find_spendable_outputs(&pub_key_hash, amount);
        if acc_v.0 < amount {
            error!("Not Enough Balance");
            return Err(format_err!(
                "No Enough Balance: Current Balance {}",
                acc_v.0
            ));
        }
        // create inputs
        for tx in acc_v.1 {
            for out in tx.1 {
                let input = TXInput {
                    txid: tx.0.clone(),
                    vout: out,
                    signature: Vec::new(),
                    pub_key: wallet.public_key.clone(),
                };
                vin.push(input);
            }
        }
        let mut vout = vec![TXOutput::new(amount, to.to_string())?];

        // create change output
        if acc_v.0 > amount {
            vout.push(TXOutput::new(acc_v.0 - amount, wallet.get_address())?);
        }

        // create transaction
        let mut tx = Transaction {
            id: String::new(),
            vin,
            vout,
        };

        tx.id = tx.hash()?;


        // 
        ut.blockchain
            .sign_transaction(&mut tx, &wallet.secret_key)?;
        Ok(tx)
    }

    pub fn new_coinbase(to: String, mut data: String) -> Result<Transaction> {
        if data == String::from("") {
            data += &format!("Reward to {}", to);
        }

        let mut tx = Transaction {
            id: String::new(),
            vin: vec![TXInput {
                txid: String::new(),
                vout: -1,
                signature: Vec::new(),
                pub_key: Vec::from(data.as_bytes()),
            }],
            vout: vec![TXOutput::new(100, to)?],
        };
        tx.id = tx.hash()?;
        Ok(tx)
    }

    pub fn is_coinbase(&self) -> bool {
        return self.vin.len() == 1 && self.vin[0].txid.is_empty() && self.vin[0].vout == -1;
    }

    pub fn sign(
        &mut self,
        private_key: &[u8],
        prev_txs: HashMap<String, Transaction>,
    ) -> Result<()> {
        if self.is_coinbase() {
            return Ok(());
        }

        for vin in &self.vin {
            if prev_txs.get(&vin.txid).unwrap().id.is_empty() {
                return Err(format_err!("Transaction not found"));
            }
        }
        let mut tx_copy = self.trim_copy();

        for in_id in 0..self.vin.len() {
            // --- Two-step signing ---
            // step a: inject the prev_hash_key for hash computation. Bind this input with previous output
            let prev_tx = prev_txs.get(&tx_copy.vin[in_id].txid).unwrap();
            tx_copy.vin[in_id].signature.clear();
            tx_copy.vin[in_id].pub_key = prev_tx.vout[tx_copy.vin[in_id].vout as usize]
                .pub_key_hash
                .clone();
            tx_copy.id = tx_copy.hash()?;

            // step b: remove the pubkey, sign the hash. Sign this input
            tx_copy.vin[in_id].pub_key = Vec::new();
            let signature = ed25519::signature(tx_copy.id.as_bytes(), private_key);
            self.vin[in_id].signature = signature.to_vec();
        }
        Ok(())
    }

    pub fn hash(&mut self) -> Result<String> {
        self.id = String::new();
        let data = bincode::serialize(self).unwrap();
        let mut hasher = Sha256::new();
        hasher.input(&data[..]);
        Ok(hasher.result_str())
    }

    pub fn verify(&self, prev_txs: HashMap<String, Transaction>) -> Result<bool> {
        if self.is_coinbase() {
            return Ok(true);
        }

        for tx_input in &self.vin {
            if prev_txs.get(&tx_input.txid).unwrap().id.is_empty() {
                return Err(format_err!("Error: Previous transaction is not correct"));
            }
            let mut tx_copy = self.trim_copy();

            for in_id in 0..tx_copy.vin.len() {
                let prev_tx = prev_txs.get(&tx_copy.vin[in_id].txid).unwrap();
                let idx = tx_copy.vin[in_id].vout;
                tx_copy.vin[in_id].pub_key = prev_tx.vout[idx as usize].pub_key_hash.clone();
                tx_copy.vin[in_id].signature.clear();
                tx_copy.id = tx_copy.hash()?;
                if !ed25519::verify(
                    &tx_copy.id.as_bytes(),
                    &self.vin[in_id].pub_key,
                    &self.vin[in_id].signature,
                ) {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }

    pub fn trim_copy(&self) -> Transaction {
        let mut vin = Vec::<TXInput>::new();
        let mut vout = Vec::<TXOutput>::new();

        for i in &self.vin {
            vin.push(TXInput {
                txid: i.txid.clone(),
                vout: i.vout,
                signature: Vec::new(),
                pub_key: Vec::new(),
            });
        }
        for i in &self.vout {
            vout.push(TXOutput {
                value: i.value,
                pub_key_hash: i.pub_key_hash.clone(),
            });
        }
        Transaction {
            id: self.id.clone(),
            vin,
            vout,
        }
    }
}

pub fn hash_pub_key(pub_key: &mut Vec<u8>) {
    let mut hasher1 = Sha256::new();
    hasher1.input(pub_key);
    hasher1.result(pub_key);

    let mut hasher2 = Ripemd160::new();

    hasher2.input(pub_key);
    pub_key.resize(20, 0);
    hasher2.result(pub_key);
}
