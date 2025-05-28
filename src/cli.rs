use crate::errors::Result;
use crate::server::Server;
use crate::utxoset::Utxoset;
use crate::wallet::Wallets;
use crate::{blockchain::BlockChain, transaction::Transaction};
use bitcoincash_addr::Address;
use clap::{arg, Command};
use log::info;
use std::fs::remove_dir_all;
use std::process::exit;

pub struct Cli {}

impl Cli {
    pub fn new() -> Result<Cli> {
        Ok(Cli {})
    }

    pub fn run(&mut self) -> Result<()> {
        let matches = Command::new("blockchain-rust-demo")
            .version("0.1")
            .author("bllock.f.zr@gmail.com")
            .about("blockchain in rust: a simple blockchain for learning")
            .subcommand(Command::new("printchain").about("print all the chain blocks"))
            .subcommand(Command::new("createwallet").about("create a wallet"))
            .subcommand(Command::new("listaddress").about("list all wallet address"))
            .subcommand(Command::new("reindex").about("re index"))
            .subcommand(
                Command::new("getbalance")
                    .about("get balance in the blockchain")
                    .arg(arg!(<ADDRESS>"'The Address it get balance for'")),
            )
            .subcommand(
                Command::new("create")
                    .about("Create new blockchain")
                    .arg(arg!(<ADDRESS>"'The address to send genesis block reward to' ")),
            )
            .subcommand(
                Command::new("send")
                    .about("send in a blockchain")
                    .arg(arg!(<From>"'Source wallet address'"))
                    .arg(arg!(<To>"'Target wallet address'"))
                    .arg(arg!(<Amount>"'Amount to transfer'")),
            )
            .subcommand(
                Command::new("startnode")
                    .about("start the node server")
                    .arg(arg!(<PORT>"'the port server bind to locally'")),
            )
            .subcommand(
                Command::new("startminer")
                    .about("start the minner server")
                    .arg(arg!(<PORT>" 'the port server bind to locally'"))
                    .arg(arg!(<ADDRESS>" 'wallet address'")),
            )
            .get_matches();

        if let Some(ref matches) = matches.subcommand_matches("getbalance") {
            if let Some(c) = matches.get_one::<String>("ADDRESS") {
                let bc = BlockChain::new()?;
                let address = String::from(c);
                let pub_key_hash = Address::decode(&address).unwrap().body;
                let utxo_set = Utxoset { blockchain: bc };
                let utxos = utxo_set.find_utxo(&pub_key_hash)?;
                let mut balance = 0;
                for item in utxos.outputs {
                    balance += item.value;
                }
                println!("Balance of {}; {}", address, balance);
            }
        }

        if let Some(matches) = matches.subcommand_matches("create") {
            if let Some(address) = matches.get_one::<String>("ADDRESS") {
                cmd_create_blockchain(address)?;
            }
        }

        if let Some(_) = matches.subcommand_matches("createwallet") {
            let mut ws = Wallets::new()?;
            let address = ws.create_wallet();
            ws.save_all()?;
            println!("success: address {}", address);
        }

        if let Some(_) = matches.subcommand_matches("listaddress") {
            let ws = Wallets::new()?;
            let addresses = ws.get_all_wallets();
            for addr in addresses {
                println!("{}", addr);
            }
        }

        if let Some(ref matches) = matches.subcommand_matches("send") {
            let from = if let Some(address) = matches.get_one::<String>("From") {
                address
            } else {
                println!("from not supply!: usage");
                exit(1);
            };

            let to = if let Some(address) = matches.get_one::<String>("To") {
                address
            } else {
                println!("to not supply!: usage");
                exit(1);
            };

            let amount: i32 = if let Some(amount) = matches.get_one::<String>("AMOUNT") {
                amount.parse()?
            } else {
                println!("amount not supply!: usage");
                exit(1);
            };

            if matches.contains_id("mine") {
                cmd_send(from, to, amount, true)?;
            } else {
                cmd_send(from, to, amount, false)?;
            }
        }

        if let Some(_) = matches.subcommand_matches("printchain") {
            cmd_print_chain()?;
        }

        if let Some(_) = matches.subcommand_matches("reindex") {
            let bc = BlockChain::new()?;
            let utxo_set = Utxoset { blockchain: bc };
            utxo_set.reindex()?;
            let count = utxo_set.count_transaction()?;
            println!("done, there are {} transactions in the utxo set", count);
        }

        if let Some(ref matches) = matches.subcommand_matches("startnode") {
            if let Some(port) = matches.get_one::<String>("PORT") {
                let bc = BlockChain::new()?;
                let utxo_set = Utxoset { blockchain: bc };
                let server = Server::new(port, "", utxo_set)?;
                server.start()?;
            }
        }

        if let Some(ref matches) = matches.subcommand_matches("startminer") {
            let port = if let Some(port) = matches.get_one::<String>("PORT") {
                port
            } else {
                println!("port not supply!: usage");
                exit(1);
            };

            let address = if let Some(address) = matches.get_one::<String>("ADDRESS") {
                address
            } else {
                println!("address not supply!: usage");
                exit(1);
            };

            let bc = BlockChain::new()?;
            let utxo_set = Utxoset { blockchain: bc };
            let server = Server::new(port, address, utxo_set)?;
            server.start()?;
        }
        Ok(())
    }
}

fn cmd_print_chain() -> Result<()> {
    let bc = BlockChain::new()?;
    for b in bc.iter() {
        println!("{:#?}", b);
    }
    Ok(())
}

fn cmd_create_blockchain(address: &str) -> Result<()> {
    println!("Creating new block");
    if let Err(e) = remove_dir_all("data/blocks") {
        info!("block not exist to delete,  {}", e);
    }
    println!("creating new block database");

    let address = String::from(address);
    let bc = BlockChain::create_blockchain(address)?;
    let utxo_set = Utxoset { blockchain: bc };
    utxo_set.reindex()?;
    Ok(())
}

fn cmd_send(from: &str, to: &str, amount: i32, mine: bool) -> Result<()> {
    let bc = BlockChain::new()?;
    let mut utxo_set = Utxoset { blockchain: bc };
    let ws = Wallets::new()?;
    let wallet = ws.get_wallet(from).unwrap();
    let tx = Transaction::new_utxo(wallet, to, amount, &utxo_set).unwrap();

    if mine {
        let cb_tx = Transaction::new_coinbase(from.to_string(), String::from("Mining Reward"))?;
        let new_block = utxo_set.blockchain.mine_block(vec![cb_tx, tx])?;
        utxo_set.update(&new_block)?;
    } else {
        Server::send_transaction(&tx, utxo_set)?;
    }

    println!("success!!!");
    Ok(())
}
