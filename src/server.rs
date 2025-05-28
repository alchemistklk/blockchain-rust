use std::{
    collections::{HashMap, HashSet},
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex},
    thread,
    time::Duration,
    vec,
};

use failure::format_err;
use log::{debug, info};
use serde::{Deserialize, Serialize};

use crate::{block::Block, errors::Result, transaction::Transaction, utxoset::Utxoset};

const KNOWN_NODE_1: &str = "localhost:3000";
const CMD_LEN: usize = 12;
const VERSION: i32 = 1;

pub struct Server {
    // current node address
    node_address: String,
    // wallet address for mining rewards
    mining_address: String,
    inner: Arc<Mutex<ServerInner>>,
}

pub struct ServerInner {
    // store collections the current peer nodes
    known_nodes: HashSet<String>,
    // hold state of all unspent transaction outputs
    utxo: Utxoset,
    // keep track of the hashes from other peer nodes, that're not processed yet
    blocks_in_transit: Vec<String>,
    // received and validated by this node
    mempool: HashMap<String, Transaction>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct BlockMsg {
    addr_from: String,
    block: Block,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct GetBlockMsg {
    addr_from: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct GetDataMsg {
    addr_from: String,
    kind: String,
    id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct InvMsg {
    addr_from: String,
    kind: String,
    items: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct TxMsg {
    addr_from: String,
    transaction: Transaction,
}

// used for initial handshake
#[derive(Serialize, Deserialize, Debug, Clone)]
struct VersionMsg {
    addr_from: String,
    version: i32,
    // the height of the longest valid blockchain
    best_height: i32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
enum Message {
    // address
    Addr(Vec<String>),
    // version message
    Version(VersionMsg),
    // transaction message
    Tx(TxMsg),
    // get data message
    GetData(GetDataMsg),
    // get block message
    GetBlock(GetBlockMsg),
    Inv(InvMsg),
    // block message
    Block(BlockMsg),
}

impl Server {
    pub fn new(port: &str, minter_address: &str, utxo: Utxoset) -> Result<Server> {
        let mut known_nodes = HashSet::new();
        known_nodes.insert(String::from(KNOWN_NODE_1));
        Ok(Server {
            node_address: format!("localhost:{}", port),
            mining_address: minter_address.to_string(),
            inner: Arc::new(Mutex::new(ServerInner {
                known_nodes: known_nodes,
                utxo,
                blocks_in_transit: Vec::new(),
                mempool: HashMap::new(),
            })),
        })
    }

    pub fn start(&self) -> Result<()> {
        // init new server instance
        let server1 = Server {
            node_address: self.node_address.clone(),
            mining_address: self.mining_address.clone(),
            inner: Arc::clone(&self.inner),
        };

        info!(
            "start server at {}, minting address: {}",
            &self.node_address, &self.mining_address
        );
        // schedule a thread to send version to master node
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(1000));
            if server1.get_best_height() == -1 {
                server1.request_blocks()
            } else {
                server1.send_version(KNOWN_NODE_1)
            }
        });

        let listener = TcpListener::bind(&self.node_address)?;
        info!("Server listen...");

        for stream in listener.incoming() {
            let stream = stream?;
            let server1 = Server {
                node_address: self.node_address.clone(),
                mining_address: self.mining_address.clone(),
                inner: Arc::clone(&self.inner),
            };
            thread::spawn(move || server1.handle_connection(stream));
        }
        Ok(())
    }

    // handle incoming connection
    fn handle_connection(&self, mut stream: TcpStream) -> Result<()> {
        let mut buffer = vec![];
        let count = stream.read_to_end(&mut buffer)?;
        info!("Accept request: length {}", count);

        // serialize the bytes to command
        let cmd = bytes_to_cmd(&buffer)?;

        match cmd {
            Message::Addr(data) => self.handle_addr(data)?,
            Message::Version(data) => self.handle_version(data)?,
            Message::Tx(data) => self.handle_tx(data)?,
            Message::GetData(data) => self.handle_get_data(data)?,
            Message::GetBlock(data) => self.handle_get_block(data)?,
            Message::Inv(data) => self.handle_inv(data)?,
            Message::Block(data) => self.handle_block(data)?,
        }

        Ok(())
    }

    // sync the address of the peer nodes
    fn handle_addr(&self, msg: Vec<String>) -> Result<()> {
        info!("handle addr message: {:?}", msg);
        for node in msg {
            self.add_nodes(&node);
        }
        Ok(())
    }

    fn handle_block(&self, msg: BlockMsg) -> Result<()> {
        info!(
            "receive block msg: {}, {}",
            msg.addr_from,
            msg.block.get_hash()
        );
        self.add_block(msg.block)?;

        let mut in_transit = self.get_in_transit();
        if in_transit.len() > 0 {
            let block_hash = &in_transit[0];
            self.send_get_data(&msg.addr_from, "block", block_hash)?;
            in_transit.remove(0);
            self.replace_in_transit(in_transit);
        } else {
            self.utxo_reindex()?;
        }
        Ok(())
    }

    fn handle_get_block(&self, msg: GetBlockMsg) -> Result<()> {
        info!("receive get block msg: {}", msg.addr_from);
        let block_hashed = self.get_block_hashes();
        self.send_inv(&msg.addr_from, "block", block_hashed)?;
        Ok(())
    }

    fn handle_get_data(&self, msg: GetDataMsg) -> Result<()> {
        info!(
            "receive get data msg: {}, kind: {}, id: {}",
            msg.addr_from, msg.kind, msg.id
        );
        if msg.kind == "block" {
            let block = self.get_block(&msg.id)?;
            self.send_block(&msg.addr_from, &block)?
        } else if msg.kind == "tx" {
            let tx = self.get_mempool_tx(&msg.id).unwrap();
            self.send_tx(&msg.addr_from, &tx)?;
        }
        Ok(())
    }

    // process version message
    fn handle_version(&self, msg: VersionMsg) -> Result<()> {
        info!(
            "receive version msg: {}, version: {}, best height: {}",
            msg.addr_from, msg.version, msg.best_height
        );
        let my_best_height = self.get_best_height();
        if my_best_height < msg.best_height {
            // send getblock message to the address
            self.send_get_blocks(&msg.addr_from)?;
        } else if my_best_height > msg.best_height {
            // send itself version to the address
            self.send_version(&msg.addr_from)?;
        }

        // send itself known address to the target address
        self.send_addr(&msg.addr_from)?;

        if !self.node_is_known(&msg.addr_from) {
            self.add_nodes(&msg.addr_from);
        }

        Ok(())
    }

    fn handle_tx(&self, msg: TxMsg) -> Result<()> {
        info!(
            "receive tx msg: {}, tx id: {}",
            msg.addr_from, msg.transaction.id
        );

        // add the transaction to the mempool(processed or verified by current node)
        self.insert_mempool(msg.transaction.clone());

        let known_nodes = self.get_known_nodes();
        if self.node_address == KNOWN_NODE_1 {
            // if the node is the master node, send inv message to all known nodes
            for node in known_nodes {
                // do not send to itself or the sender
                if node != self.node_address && node != msg.addr_from {
                    self.send_inv(&node, "tx", vec![msg.transaction.id.clone()])?;
                }
            }
        } else {
            let mut mempool = self.get_mempool();
            debug!("Current mempool: {:#?}", &mempool);
            if mempool.len() >= 1 && !self.mining_address.is_empty() {
                loop {
                    // iterate through the mempool and verify each transaction
                    let mut txs = vec![];
                    for (_, tx) in &mempool {
                        if self.verify_tx(tx)? {
                            txs.push(tx.clone());
                        }
                    }

                    if txs.is_empty() {
                        return Ok(());
                    }

                    let cb_tx =
                        Transaction::new_coinbase(self.mining_address.clone(), String::new())?;
                    txs.push(cb_tx);

                    for tx in &txs {
                        mempool.remove(&tx.id);
                    }

                    // mine a new block with the transactions
                    let new_block = self.mine_block(txs)?;
                    self.utxo_reindex()?;

                    for node in self.get_known_nodes() {
                        if node != self.node_address {
                            // send the new block to all known nodes
                            self.send_inv(&node, "block", vec![new_block.get_hash()])?;
                        }
                    }

                    if mempool.len() == 0 {
                        break;
                    }
                }
                self.clear_mempool();
            }
        }
        Ok(())
    }

    fn handle_inv(&self, msg: InvMsg) -> Result<()> {
        info!("receive inv msg: {:#?}", msg);
        if msg.kind == "block" {
            let block_hash = &msg.items[0];
            self.send_get_data(&msg.addr_from, "block", block_hash)?;

            let mut new_in_transit = vec![];

            for b in &msg.items {
                if !self.get_in_transit().contains(b) {
                    new_in_transit.push(b.clone());
                }
            }
            self.replace_in_transit(new_in_transit);
        } else if msg.kind == "tx" {
            let tx_id = &msg.items[0];
            match self.get_mempool_tx(tx_id) {
                Some(tx) => {
                    if tx.id.is_empty() {
                        self.send_get_data(&msg.addr_from, "tx", &tx_id)?
                    }
                }
                None => self.send_get_data(&msg.addr_from, "tx", &tx_id)?,
            }
        }
        Ok(())
    }

    fn get_block_hashes(&self) -> Vec<String> {
        self.inner
            .lock()
            .unwrap()
            .utxo
            .blockchain
            .get_block_hashes()
    }

    // send to all known nodes
    fn send_addr(&self, addr: &str) -> Result<()> {
        info!("send address info to {}", addr);
        let nodes = self.get_known_nodes();
        let data = bincode::serialize(&(Server::cmd_to_bytes("addr"), nodes))?;

        self.send_data(addr, &data)
    }

    // send data to block
    fn send_block(&self, addr: &str, b: &Block) -> Result<()> {
        info!("send block data to: {} block hash:{}", addr, b.get_hash());
        let data = BlockMsg {
            addr_from: self.node_address.clone(),
            block: b.clone(),
        };
        let data = bincode::serialize(&(Server::cmd_to_bytes("block"), data))?;
        self.send_data(addr, &data)
    }

    // send itself inv message to the address
    fn send_inv(&self, addr: &str, kind: &str, items: Vec<String>) -> Result<()> {
        info!(
            "send inv message to: {} kind: {} data:{:?}",
            addr, kind, items
        );

        let data = InvMsg {
            addr_from: self.node_address.clone(),
            kind: kind.to_string(),
            items,
        };

        let data = bincode::serialize(&(Server::cmd_to_bytes("inv"), data))?;
        self.send_data(addr, &data)
    }

    fn send_tx(&self, addr: &str, tx: &Transaction) -> Result<()> {
        info!("send transaction to: {} tx id:{}", addr, tx.id);
        let data = TxMsg {
            addr_from: self.node_address.clone(),
            transaction: tx.clone(),
        };
        let data = bincode::serialize(&(Server::cmd_to_bytes("tx"), data))?;
        self.send_data(addr, &data)
    }

    // report their version message to the peer address
    fn send_version(&self, addr: &str) -> Result<()> {
        info!("send version message to: {}", addr);
        let data = VersionMsg {
            addr_from: self.node_address.clone(),
            version: VERSION,
            best_height: self.get_best_height(),
        };
        let data = bincode::serialize(&(Server::cmd_to_bytes("version"), data))?;
        self.send_data(addr, &data)
    }

    // send get block message to the address
    fn send_get_blocks(&self, addr: &str) -> Result<()> {
        info!("send get block message to: {}", addr);
        let data = GetBlockMsg {
            addr_from: self.node_address.clone(),
        };
        let data = bincode::serialize(&(Server::cmd_to_bytes("getblock"), data))?;
        self.send_data(addr, &data)
    }

    fn send_get_data(&self, addr: &str, kind: &str, id: &str) -> Result<()> {
        info!(
            "send get data message to: {} kind: {} id: {}",
            addr, kind, id
        );
        let data = GetDataMsg {
            addr_from: self.node_address.clone(),
            kind: kind.to_string(),
            id: id.to_string(),
        };
        let data = bincode::serialize(&(Server::cmd_to_bytes("getdata"), data))?;
        self.send_data(addr, &data)
    }

    // send data to the address
    fn send_data(&self, addr: &str, data: &[u8]) -> Result<()> {
        if addr == &self.node_address {
            return Ok(());
        }
        let mut stream = match TcpStream::connect(addr) {
            Ok(s) => s,
            Err(_) => {
                self.remove_node(addr);
                return Ok(());
            }
        };
        stream.write(data)?;
        Ok(())
    }

    fn add_nodes(&self, addr: &str) {
        self.inner
            .lock()
            .unwrap()
            .known_nodes
            .insert(addr.to_string());
    }

    fn get_known_nodes(&self) -> HashSet<String> {
        self.inner.lock().unwrap().known_nodes.clone()
    }

    fn remove_node(&self, addr: &str) {
        let mut inner = self.inner.lock().unwrap();
        inner.known_nodes.remove(addr);
    }

    fn get_in_transit(&self) -> Vec<String> {
        self.inner.lock().unwrap().blocks_in_transit.clone()
    }

    fn replace_in_transit(&self, blocks_in_transit: Vec<String>) {
        let bit = &mut self.inner.lock().unwrap().blocks_in_transit;
        bit.clone_from(&blocks_in_transit);
    }

    fn node_is_known(&self, addr: &str) -> bool {
        self.inner.lock().unwrap().known_nodes.contains(addr)
    }

    fn mine_block(&self, txs: Vec<Transaction>) -> Result<Block> {
        self.inner.lock().unwrap().utxo.blockchain.mine_block(txs)
    }

    fn get_best_height(&self) -> i32 {
        self.inner
            .lock()
            .unwrap()
            .utxo
            .blockchain
            .get_best_height()
            .unwrap() as i32
    }

    // convert str command to bytes
    fn cmd_to_bytes(cmd: &str) -> [u8; CMD_LEN] {
        let mut data = [0; CMD_LEN];
        for (i, b) in cmd.as_bytes().iter().enumerate() {
            data[i] = *b;
        }
        data
    }

    fn insert_mempool(&self, tx: Transaction) {
        self.inner.lock().unwrap().mempool.insert(tx.id.clone(), tx);
    }

    fn clear_mempool(&self) {
        self.inner.lock().unwrap().mempool.clear();
    }

    fn get_mempool_tx(&self, addr: &str) -> Option<Transaction> {
        match self.inner.lock().unwrap().mempool.get(addr) {
            Some(tx) => Some(tx.clone()),
            None => None,
        }
    }

    fn get_mempool(&self) -> HashMap<String, Transaction> {
        self.inner.lock().unwrap().mempool.clone()
    }

    fn request_blocks(&self) -> Result<()> {
        for node in self.get_known_nodes() {
            self.send_get_blocks(&node)?;
        }
        Ok(())
    }

    fn add_block(&self, block: Block) -> Result<()> {
        self.inner.lock().unwrap().utxo.blockchain.add_block(block)
    }

    fn get_block(&self, id: &str) -> Result<Block> {
        self.inner.lock().unwrap().utxo.blockchain.get_block(id)
    }

    fn utxo_reindex(&self) -> Result<()> {
        self.inner.lock().unwrap().utxo.reindex()
    }

    fn verify_tx(&self, tx: &Transaction) -> Result<bool> {
        self.inner
            .lock()
            .unwrap()
            .utxo
            .blockchain
            .verify_transaction(tx)
    }

    pub fn send_transaction(tx: &Transaction, utxoset: Utxoset) -> Result<()> {
        let server = Server::new("7000", "", utxoset)?;
        server.send_tx(KNOWN_NODE_1, tx)?;
        Ok(())
    }
}

// convert bytes to command
fn bytes_to_cmd(bytes: &[u8]) -> Result<Message> {
    let mut cmd = Vec::new();
    let cmd_bytes = &bytes[..CMD_LEN];
    let data = &bytes[CMD_LEN..];
    for b in cmd_bytes {
        // check if the byte is not zero
        if 0 as u8 != *b {
            cmd.push(*b);
        }
    }
    info!("cmd:{}", String::from_utf8(cmd.clone())?);
    if cmd == "addr".as_bytes() {
        let data: Vec<String> = bincode::deserialize(data)?;
        return Ok(Message::Addr(data));
    } else if cmd == "block".as_bytes() {
        let data: BlockMsg = bincode::deserialize(data)?;
        return Ok(Message::Block(data));
    } else if cmd == "getblock".as_bytes() {
        let data: GetBlockMsg = bincode::deserialize(data)?;
        return Ok(Message::GetBlock(data));
    } else if cmd == "getdata".as_bytes() {
        let data: GetDataMsg = bincode::deserialize(data)?;
        return Ok(Message::GetData(data));
    } else if cmd == "inv".as_bytes() {
        let data: InvMsg = bincode::deserialize(data)?;
        return Ok(Message::Inv(data));
    } else if cmd == "tx".as_bytes() {
        let data: TxMsg = bincode::deserialize(data)?;
        return Ok(Message::Tx(data));
    } else if cmd == "version".as_bytes() {
        let data: VersionMsg = bincode::deserialize(data)?;
        return Ok(Message::Version(data));
    } else {
        Err(format_err!("Unknown command in the server"))
    }
}
