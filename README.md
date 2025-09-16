# Delphi: Asynchronous Approximate Agreement for Distributed Oracles
This repository contains a Rust implementation of the following distributed oracle agreement protocols. 

1. Delphi AAA protocol
2. FIN ACS protocol [1]
3. Abraham et al. AAA protocol [2]

The repository uses the libchatter networking library available [here](https://github.com/libdist-rs/libchatter-rs). This code has been written as a research prototype and has not been vetted for security. Therefore, this repository can contain serious security vulnerabilities. Please use at your own risk. 

Please consider citing our paper if you use this artifact. 
```
Delphi: Efficient Asynchronous Approximate Agreement for Distributed Oracles
Akhil Bandarupalli, Adithya Bhat, Saurabh Bagchi, Aniket Kate, Chen-Da Liu-Zhang, and Michael K. Reiter
To appear at 54th Annual IEEE/IFIP International Conference on Dependable Systems and Networks (DSN), 2024.
```
## Dataset
The repository also contains a dataset containing values of prominent cryptocurrencies polled from 12 cryptocurrency exchanges. Details are available in the `dataset` folder. 

# Quick Start
We describe the steps to run this artifact. 

## Hardware and OS setup
1. This artifact has been run and tested on `x86_64` and `x64` architectures. However, we are unaware of any issues that would prevent this artifact from running on `x86` architectures. 

2. This artifact has been run and tested on Ubuntu `20.04.5 LTS` OS and Raspbian Linux version released on `2023-02-21`, both of which follow the Debian distro. However, we are unaware of any issues that would prevent this artifact from running on Fedora distros like CentOS and Red Hat Linux. 

## Rust installation and Cargo setup
The repository uses the `Cargo` build tool. The compatibility between dependencies has been tested for Rust version `1.63`.

3. Run the set of following commands to install the toolchain required to compile code written in Rust and create binary executable files. 
```
$ sudo apt-get update
$ sudo apt-get -y upgrade
$ sudo apt-get -y autoremove
$ sudo apt-get -y install build-essential
$ sudo apt-get -y install cmake
$ sudo apt-get -y install curl
# Install rust (non-interactive)
$ curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
$ source $HOME/.cargo/env
$ rustup install 1.63.0
$ rustup override set 1.63.0
```
4. Build the repository using the following command. The command should be run in the directory containing the `Cargo.toml` file. 
```
$ cargo build --release
$ mkdir logs
```

5. Next, generate configuration files for nodes in the system using the following command. Make sure to create the directory (in this example, `testdata/hyb_4/`) before running this command. 
```
$ ./target/release/genconfig --base_port 8500 --client_base_port 7000 --client_run_port 9000 --NumNodes 4 --blocksize 100 --delay 100 --target testdata/hyb_4/ --local true
```

6. After generating the configuration files, run the script `appxcon-test.sh` in the scripts folder with the following command line arguments. This command starts Delphi with four nodes.
```
$ ./scripts/appxcon-test.sh {epsilon} {rho} {Delta} testdata/hyb_4/syncer
```
7. Substitute desired values of $\epsilon,\rho_0,\Delta$. Example values include $\epsilon=1,\rho_0=10,\Delta=100000$. The script randomly assigns input values $v_i$ to each node. This logic can be changed to make nodes start with custom input values. 

8. The outputs are logged into the `syncer.log` file in logs directory. The outputs of each node are printed in a JSON format, along with the amount of time the node took to terminate the protocol. 

9. Running the FIN ACS protocol requires additional configuration. FIN uses BLS threshold signatures to generate common coins necessary for proposal election and Binary Byzantine Agreement. This setup includes a master public key in the `pub` file, $n$ partial secret keys (one for each node) as `sec0,...,sec3` files, and the $n$ partial public keys as `pub0,...,pub3` files. We utilized the `crypto_blstrs` library in the [apss](https://github.com/ISTA-SPiDerS/apss) repository to generate these keys. We pregenerated these files for $n=16,64,112,160$ in the benchmark folder, in zip files `tkeys-{n}.tar.gz`. After generating these files, place them in the configuration directory (`testdata/hyb_4` in this example) and run the following command (We already performed this step and have these files ready in `testdata/hyb_4` folder). 
```
# Kill previous processes running on these ports
$ sudo lsof -ti:7000-7015 | xargs kill -9
$ ./scripts/fin-test.sh testdata/hyb_4/syncer
```

10. Similarly, Abraham et al.'s Approximate Agreement protocol can be run using the following command.
```
# Kill previous processes running on these ports
$ sudo lsof -ti:7000-7015 | xargs kill -9
$ ./scripts/abraham-test.sh {epsilon} {delta} {Delta} testdata/hyb_4/syncer
```
The parameters {epsilon} and {delta} must be equal in this context to yield Abraham et al.'s protocol. {Delta} must be set to be equal to the difference between honest inputs `M-m`. Example configuration run includes the following command.
```
$ ./scripts/abraham-test.sh 2 2 20 testdata/hyb_4/syncer
```

## Running in AWS
We utilize the code in the [Narwhal](https://github.com/MystenLabs/sui/tree/main/narwhal/benchmark) repository to execute code in AWS. This repository uses `fabric` to spawn AWS instances, install Rust, and build the repository on individual machines. Please refer to the `benchmark` directory for more instructions about reproducing the results in the paper. 

## Running in Raspberry-Pi testbed
We described detailed instructions to reproduce the results in the paper in the Raspberry-Pi device testbed in the `benchmark/raspberry-pi` directory. 

# System architecture
Each node runs as an independent process, which communicates with other nodes through sockets. Apart from the $n$ nodes running the protocol, the system also spawns a process called `syncer`. The `syncer` is responsible for measuring latency of completion. It reliably measures the system's latency by issuing `START` and `STOP` commands to all nodes. The nodes begin executing the protocol only after the `syncer` verifies that all nodes are online, and issues the `START` command by sending a message to all nodes. Further, the nodes send a `TERMINATED` message to the `syncer` once they terminate the protocol. The `syncer` records both start and termination times of all processes, which allows it to accurately measure the latency of each protocol. 

# Dependencies
The artifact uses multiple Rust libraries for various functionalities. We give a list of all dependencies used by the artifact in the `Cargo.lock` file. `Cargo` automatically manages these dependencies and fetches the specified versions from the `crates.io` repository manager. 

# Code Organization
The artifact is organized into the following modules of code. 
1. The `config` directory contains code pertaining to configuring each node in the distributed system. Each node requires information about port to use, network addresses of other nodes, symmetric keys to establish pairwise authenticated channels between nodes, and protocol specific configuration parameters like values of $\epsilon,\Delta,\rho$. Code related to managing and parsing these parameters is in the `config` directory. This library has been borrowed from the `libchatter` (https://github.com/libdist-rs/libchatter-rs) repository. 

2. The `crypto` directory contains code that manages the pairwise authenticated channels between nodes. Mainly, nodes use Message Authentication Codes (MACs) for message authentication. This repo manages the required secret keys and mechanisms for generating MACs. This library has been borrowed from the `libchatter` (https://github.com/libdist-rs/libchatter-rs) repository. 

3. The `crypto_blstrs` directory contains code that enables nodes to toss common coins from BLS threshold signatures. This library has been borrowed from the `apss` (https://github.com/ISTA-SPiDerS/apss) repository. 

4. The `types` directory governs the message serialization and deserialization. Each message sent between nodes is serialized into bytecode to be sent over the network. Upon receiving a message, each node deserializes the received bytecode into the required message type after receiving. This library has been written on top of the library from `libchatter` (https://github.com/libdist-rs/libchatter-rs) repository. 

5. *Networking*: This repository uses the `libnet-rs` (https://github.com/libdist-rs/libnet-rs) networking library. Similar libraries include networking library from the `narwhal` (https://github.com/MystenLabs/sui/tree/main/narwhal/) repository. The nodes use the `tcp` protocol to send messages to each other. 

6. The `tools` directory consists of code that generates configuration files for nodes. This library has been borrowed from the `libchatter` (https://github.com/libdist-rs/libchatter-rs) repository. 

7. The `consensus` directory contains the implementations of various protocols. Primarily, it contains implementations of Abraham et al.'s approximate agreement protocol in the `hyb_appxcon` subdirectory, `delphi` protocol in the `delphi` subdirectory, and FIN protocol in `fin` subdirectory. Each protocol contains a `context.rs` file, which contains a function named `spawn` from where the protocol's execution starts. This function is called by the `node` library in the `node` folder. This library contains a `main.rs` file, which spawns an instance of a node running the respective protocol by invoking the `spawn` function. 

# References
```
[1] Abraham, Ittai, Yonatan Amit, and Danny Dolev. "Optimal resilience asynchronous approximate agreement." In Principles of Distributed Systems: 8th International Conference, OPODIS 2004, Grenoble, France, December 15-17, 2004, Revised Selected Papers 8, pp. 229-239. Springer Berlin Heidelberg, 2005.

[2] Duan, Sisi, Xin Wang, and Haibin Zhang. "Fin: Practical signature-free asynchronous common subset in constant time." In Proceedings of the 2023 ACM SIGSAC Conference on Computer and Communications Security, pp. 815-829. 2023.
```