# QFN Faucet Server

## Usage
For update a chain metadata, you need to run the command:
```bash
curl -X POST -H "Content-Type: application/json" -d @<path_to_metadata.json> http://<host>:<port>/metadata
```
or
```bash
subxt metadata -f bytes > metadata.scale
```

## Installation
```bash
cargo build --release
cp ./target/release/qfn-faucet-server /<your_path>/qfn-faucet-server
/<your_path>/qfn-faucet-server -h
```
```
QF server for crediting by dev tokens

Usage: qf-faucet-server [OPTIONS]

Options:
  -H, --host <IP>          Host can set from env HOST or this option [default: 0.0.0.0]
  -P, --port <PORT>        Port can set from env PORT or this option [default: 8080]
  -r, --rpc-url <RPC_URL>  RPC url can set from env RPC_URL or this option [default: wss://dev.qfnetwork.xyz/socket]
  -d, --debug              In debug mode the sender is Alice
  -t, --timeout <TIMEOUT>  Custom delay for transfer in minutes [default: 120]
  -h, --help               Print help
  -V, --version            Print version
```
Build, copy and get help.

**NOTE**: The server will make sqlite database file at the same directory.

## Running
Environment variable ```MNEMONIC``` must be set before running. It is used for generating the account for sending the tokens.

For development, run:
```bash
export MNEMONIC="your mnemonic phrase"
/<your_path>/qfn-faucet-server -r ws://127.0.0.1:9944 -d
```
or
```bash
MNEMONIC="your mnemonic phrase" /<your_path>/qfn-faucet-server -r ws://127.0.0.1:9944 -d
```

For production, run:
```bash
export MNEMONIC="your mnemonic phrase"
/<your_path>/qfn-faucet-server
```
or
```bash
MNEMONIC="your mnemonic phrase" /<your_path>/qfn-faucet-server
```
