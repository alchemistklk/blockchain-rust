# BlockChain-Rust

## 1. BlockChain Parts

### 1.1 Block

A block is a data structure that are used store the transactions in the blockchain. Each block contains other information such as `timestamp`, `previous_hash`, `hash`, `nonce`, that are used to ensure the integrity and security of the blockchain.

#### Component

block is the basic unit of blockchain, which contains the following parts:

-   `timestamp`: the time when the block is created
-   `Vec<Transaction>`: a series of transactions that are recorded in the block
-   `previous_hash`: the hash of the previous block
-   `hash`: the hash of the current block
-   `nonce`: a number that is used to create the hash of the block(used in proof of work)

#### create a normal block

When we want to create a new block, we need to input the previous block's hash, the transactions that we want to record in the block as well as the height(`index`)

#### create a genesis block

Genesis Block is the first block of the blockchain, which has non previous block and has coinbase transaction and zero index.

#### hash transaction

In order to record the transaction in the block using less space, we can use the hash of transaction instead of the transaction itself. In the code we use `merkle_root` to represent the whole transactions in the block.

#### run pow to verify the current block

1. We need iterate the nonce from 0 to `u64::MAX` to find the valid hash that start with `difficulty` number of zeros. This step is critical to ensure the security of the blockchain.

2. When the node finds a valid hash, it will hash data of the block to generate `itself hash` to ensure the integrity of the block.

#### Merkle Tree Merge

Append the left transaction and the right transaction firstly, then hash it.

### 1.2 Transaction

Transaction is the most important part of the block used to record the data. The model of transaction is UTXO, which means Unspent Transaction Output.

#### Model

-   `TXInput`
    -   `TXID`: the previous transaction ID
    -   `vout`: the index of the output in the previous transaction
    -   `signature`: the digital signature of the input, created by signing hash with sender's private key
    -   `pub_key`: sender's public key, used to verify the signature

unlock the output with the `specific address`

-   `TXOutput`
    -   `value`: the amount of coins in the output
    -   `pub_key_hash`: hashed public key of the recipient

lock the output with the `specific address`

#### new utxo transaction

`Wallet` want to create a new transaction with input `amount` and specific address`to`. If the wallet has enough balance, it will create a new transaction with change and send it to itself.

1. Find all the unspent transaction outputs (UTXO) that belong to the wallet, then return the total amount of UTXO(`coins`) and the UTXO list.

2. If the balance if enough, we create need to create a collection contains multi input each with a corresponding output.

3. Create a TXOutput with the specific `amount` and `address`

4. If the change is greater than 0, create a new TXOutput with the change and the wallet's address.

5. Create a new transaction with above inputs and outputs, then sign the transaction with the wallet's private key.

6. sign this transaction

#### sign transaction

We need to find all previous transactions that are used in the inputs. If the transaction is `coinbase`, we don't need to sign. Otherwise, we need to `trim_copy` the transaction(input without signature and pubkey). First we need assign the `prev_hash_key` then generate the hash of transaction. Then we use private key to sign the each input. Finally we empty the `pub_key`.

#### verify transaction

If the transaction is `coinbase`, we don't need to verify. For each input, we need to `trim_copy` the transaction without the signature and pubkey. Then we set the `prev_hash_key`, finally we use `signature`,`prev_pub_key` to verify the transaction.

### 1.3 UtxoSet

UtxoSet is a collection of unspent transaction outputs (UTXO) that are used to track the balance of each address in the blockchain. Utxo has a blockchain field.

#### reindex

store all utxos into local db.

#### update

This method is used to update the utxo set when a new block is added. We iterate through all inputs to find the previous transaction id, then we query the corresponding TXOutput. After that, we need to assert whether the `output_id` is equal to the input `vout`.

If the current transaction is coinbase, we need to assign an output.

### 1.4 BlockChain

BlockChain is a collection of blocks that link together to form a chain. It has two fields: `current_block` and `db`.

#### create blockchain

We need to open the local file to load the blockchain then we have created by using new method.
Then we create a new coinbase transaction with fixed input String.

#### mine block

When we want to add a bunch of transactions to the blockchain, we need to mine a new block.

1. we need to get the last hash from db, then we create a new block

2. we need to insert the block into db and refresh the last

#### find utxo

We need iter through all blocks, then we iterate through all transactions of this block, then we iterate through all TXOutput. If the current `tx.id` is recorded in the spend_txos, we need to confirm the index of output is not in the spend_txos. If the output is not in the spend_txos, we can add it to the utxo set.

#### 1.5 wallet

Wallet is a collection of private key and public key pairs that are used to sign and verify transactions. It has two fields: `private_key` and `public_key`.
