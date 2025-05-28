use std::time::SystemTime;

use crate::{errors::Result, transaction::Transaction};
use crypto::{digest::Digest, sha2::Sha256};
use log::info;
use merkle_cbt::{merkle_tree::Merge, CBMT};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Block {
    timestamp: u128,
    transactions: Vec<Transaction>,
    prev_block_hash: String,
    hash: String,
    height: i32,
    nonce: i32,
}

const TARGET_HEXT: usize = 4;

impl Block {
    pub fn get_transactions(&self) -> &Vec<Transaction> {
        &self.transactions
    }

    pub fn get_hash(&self) -> String {
        self.hash.clone()
    }

    pub fn get_height(&self) -> i32 {
        self.height
    }

    pub fn get_prev_hash(&self) -> String {
        self.prev_block_hash.clone()
    }

    pub fn new_genesis_block(coinbase: Transaction) -> Block {
        Block::new_block(vec![coinbase], String::new(), 0).unwrap()
    }

    pub fn new_block(
        data: Vec<Transaction>,
        prev_block_hash: String,
        height: i32,
    ) -> Result<Block> {
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_millis();

        let mut block = Block {
            timestamp,
            transactions: data,
            prev_block_hash,
            hash: String::new(),
            height,
            nonce: 0,
        };
        block.run_proof_of_work()?;
        Ok(block)
    }

    fn hash_transaction(&self) -> Result<Vec<u8>> {
        let mut transactions = Vec::new();
        for tx in &self.transactions {
            transactions.push(tx.clone().hash()?.as_bytes().to_owned());
        }

        let tree = CBMT::<Vec<u8>, MergeTX>::build_merkle_tree(&transactions);

        Ok(tree.root())
    }

    fn prepare_hash_data(&self) -> Result<Vec<u8>> {

        let content = (
            self.prev_block_hash.clone(),
            self.hash_transaction()?,
            self.timestamp,
            TARGET_HEXT,
            self.nonce
        );

        let bytes = bincode::serialize(&content)?;
        Ok(bytes)
    }

    pub fn run_proof_of_work(&mut self) -> Result<()> {
        info!("Minting the block");

        while !self.validate()? {
            self.nonce += 1
        }

        let data = self.prepare_hash_data()?;
        let mut hasher = Sha256::new();
        hasher.input(&data[..]);
        self.hash = hasher.result_str();
        Ok(())
    }

    pub fn validate(&mut self) -> Result<bool> {
        let data = self.prepare_hash_data()?;
        let mut hasher = Sha256::new();
        hasher.input(&data[..]);
        let mut vec1 = vec![];
        vec1.resize(TARGET_HEXT, '0' as u8);
        Ok(&hasher.result_str()[0..TARGET_HEXT] == String::from_utf8(vec1)?)
    }
}

struct MergeTX {}

impl Merge for MergeTX {
    type Item = Vec<u8>;

    fn merge(left: &Self::Item, right: &Self::Item) -> Self::Item {
        let mut hasher = Sha256::new();
        let mut data: Vec<u8> = left.clone();
        data.append(&mut right.clone());
        hasher.input(&data);

        let mut re: [u8; 32] = [0; 32];
        hasher.result(&mut re);
        re.to_vec()
    }
}
