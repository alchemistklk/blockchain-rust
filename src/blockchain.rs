use std::collections::HashMap;
use std::vec;

use failure::format_err;
use log::info;

use crate::block::Block;
use crate::errors::Result;
use crate::transaction::Transaction;
use crate::tx::TXOutputs;
#[derive(Debug, Clone)]
pub struct BlockChain {
    current_hash: String,
    db: sled::Db,
}

pub struct BlockChainIter<'a> {
    current_hash: String,
    bc: &'a BlockChain,
}

impl BlockChain {
    pub fn new() -> Result<BlockChain> {
        info!("open blockchain");
        let db = sled::open("data/blocks")?;
        let hash = db
            .get("LAST")?
            .expect("Must create a new block database first");
        info!("Found block database");

        let last_hash = String::from_utf8(hash.to_vec())?;
        Ok(BlockChain {
            current_hash: last_hash.clone(),
            db: db,
        })
    }


    pub fn get_block(&self, block_hash: &str) -> Result<Block> {
        if let Some(data) = self.db.get(block_hash)? {
            let block: Block = bincode::deserialize(&data)?;
            Ok(block)
        } else {
            Err(format_err!("Block not found"))
        }
    }

    pub fn create_blockchain(address: String) -> Result<BlockChain> {
        info!("Creating new blockchain");
        let db = sled::open("data/blocks")?;
        let bctx = Transaction::new_coinbase(address, String::from("Genesis Block"))?;
        let genesis = Block::new_genesis_block(bctx);
        db.insert(genesis.get_hash(), bincode::serialize(&genesis)?)?;
        db.insert("LAST", genesis.get_hash().as_bytes())?;
        let bc = BlockChain {
            current_hash: genesis.get_hash(),
            db: db,
        };

        bc.db.flush()?;
        Ok(bc)
    }
    pub fn mine_block(&mut self, txs: Vec<Transaction>) -> Result<Block> {
        info!("mine a new block");

        for tx in &txs {
            if !self.verify_transaction(&tx)? {
                return Err(format_err!("Transaction is not valid: {}", tx.id));
            }
        }

        let last_hash = self.db.get("LAST")?.unwrap();

        let new_block = Block::new_block(
            txs,
            String::from_utf8(last_hash.to_vec())?,
            self.get_best_height()? + 1,
        )?;

        self.db
            .insert(new_block.get_hash(), bincode::serialize(&new_block)?)?;
        self.db.insert("LAST", new_block.get_hash().as_bytes())?;
        self.db.flush()?;

        self.current_hash = new_block.get_hash();
        Ok(new_block)
    }


    pub fn add_block(&mut self, block: Block) -> Result<()> {
        
        if let Some(_) = self.db.get(block.get_hash())? {
            return Ok(());
        }
        let data = bincode::serialize(&block)?;
        self.db.insert(block.get_hash(), data)?;
        let last_height = self.get_best_height()?;
        if block.get_height() > last_height {
            self.db.insert("LAST", block.get_hash().as_bytes())?;
            self.current_hash = block.get_hash();
            self.db.flush()?;
        }
        Ok(())
    }

    fn find_unspent_transactions(&self, address: &[u8]) -> Vec<Transaction> {
        let mut spend_txos: HashMap<String, Vec<i32>> = HashMap::new();
        let mut unspend_txs: Vec<Transaction> = Vec::new();

        for block in self.iter() {
            for tx in block.get_transactions() {
                for index in 0..tx.vout.len() {
                    if let Some(ids) = spend_txos.get(&tx.id) {
                        if ids.contains(&(index as i32)) {
                            continue;
                        }
                    }
                    if tx.vout[index].can_be_unlock_with(address) {
                        unspend_txs.push(tx.to_owned());
                    }

                    if !tx.is_coinbase() {
                        for i in &tx.vin {
                            if i.can_unlock_output_with(address) {
                                match spend_txos.get_mut(&i.txid) {
                                    Some(v) => {
                                        v.push(i.vout);
                                    }
                                    None => {
                                        spend_txos.insert(i.txid.clone(), vec![i.vout]);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        unspend_txs
    }

    pub fn find_utxo(&self) -> HashMap<String, TXOutputs> {
        let mut utxos: HashMap<String, TXOutputs> = HashMap::new();
        let mut spend_txos: HashMap<String, Vec<i32>> = HashMap::new();

        for block in self.iter() {
            for tx in block.get_transactions() {
                for index in 0..tx.vout.len() {
                    if let Some(ids) = spend_txos.get(&tx.id) {
                        if ids.contains(&(index as i32)) {
                            continue;
                        }
                    }

                    match utxos.get_mut(&tx.id) {
                        Some(v) => {
                            v.outputs.push(tx.vout[index].clone());
                        }
                        None => {
                            utxos.insert(
                                tx.id.clone(),
                                TXOutputs {
                                    outputs: vec![tx.vout[index].clone()],
                                },
                            );
                        }
                    }
                }

                if !tx.is_coinbase() {
                    for i in &tx.vin {
                        match spend_txos.get_mut(&i.txid) {
                            Some(v) => {
                                v.push(i.vout);
                            }
                            None => {
                                spend_txos.insert(i.txid.clone(), vec![i.vout]);
                            }
                        }
                    }
                }
            }
        }

        utxos
    }

    pub fn find_transaction(&self, id: &str) -> Result<Transaction> {
        for block in self.iter() {
            for tx in block.get_transactions() {
                if tx.id == id {
                    return Ok(tx.clone());
                }
            }
        }
        Err(format_err!("Transaction is not found"))
    }

    pub fn sign_transaction(&self, tx: &mut Transaction, private_key: &[u8]) -> Result<()> {
        let prev_txs = self.get_prev_txs(tx)?;
        tx.sign(private_key, prev_txs)
    }

    pub fn verify_transaction(&self, tx: &Transaction) -> Result<bool> {
        let prev_txs = self.get_prev_txs(tx)?;
        tx.verify(prev_txs)
    }

    fn get_prev_txs(&self, tx: &Transaction) -> Result<HashMap<String, Transaction>> {
        let mut prev_txs = HashMap::<String, Transaction>::new();
        for v in &tx.vin {
            let prev_tx = self.find_transaction(&v.txid)?;
            prev_txs.insert(v.txid.clone(), prev_tx);
        }
        Ok(prev_txs)
    }

    pub fn get_block_hashes(&self) -> Vec<String> {
        let mut list = Vec::new();
        for b in self.iter() {
            list.push(b.get_hash());
        }
        list
    }

    pub fn iter(&self) -> BlockChainIter {
        BlockChainIter {
            current_hash: self.current_hash.clone(),
            bc: &self,
        }
    }

    pub fn get_best_height(&self) -> Result<i32> {
        let last_hash = if let Some(h) = self.db.get("LAST")? {
            h
        } else {
            return Ok(0);
        };

        let last_data = self.db.get(last_hash)?.unwrap();
        let last_block: Block = bincode::deserialize(&last_data)?;
        Ok(last_block.get_height())
    }
}

impl<'a> Iterator for BlockChainIter<'a> {
    type Item = Block;
    fn next(&mut self) -> Option<Self::Item> {
        if let Ok(encode_block) = self.bc.db.get(&self.current_hash) {
            return match encode_block {
                Some(b) => {
                    if let Ok(block) = bincode::deserialize::<Block>(&b) {
                        self.current_hash = block.get_prev_hash();
                        Some(block)
                    } else {
                        None
                    }
                }
                None => None,
            };
        };
        None
    }
}
