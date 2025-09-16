# Reproducing results on the Raspberry Pi testbed
Delphi, Abraham et al. and FIN have been evaluated on a Raspberry-Pi testbed of 15 Raspberry Pi 4B devices connected through a Local Area Network (LAN). This file contains instructions on reproducing the results of the paper. 

## Testbed Access
1. The Raspberry Pi testbed is behind a firewall to prevent network scams and botnets taking over the devices. A user wishing to gain access should email the owner at akhilsai2712@gmail.com to gain access to the testbed. 

2. The 15 devices are accessible on ips `10.43.0.231` to `10.43.0.245` with username `pi`. Password should be obtained from akhilsai2712@gmail.com. 

3. Upon obtaining access, the user should SSH into the first node at `10.43.0.231` with the specified username and password. The artifact has been installed in the `/home/pi/delphi` directory. 

## Configuration setup
4. The following command should be run to generate configuration files. This command should be run from the directory containing the main `Cargo.toml` file. The number 43 can be replaced with $n=85,127,169$ to generate the results in the paper for different values of $n$. 
```
$ ./target/release/genconfig --base_port 8500 --client_base_port 7000 --client_run_port 5000 --NumNodes 43 --target benchmark/raspberry-pi/43/ --local true --blocksize 100 --delay 10
```
The required configuration files have already been generated in the device at `10.43.0.231`. 

## Running experiments
5. Each RPi device runs multiple processes of the protocol. For example, running a distributed system of $n=43$ processes requires 14 RPi devices to run three processes each. The files `run_delphi.py,run_fin.py,run_abraham.py` should be copied into the node `10.43.0.231` using `scp` (These files are already present in the device, however, if the user wants to run specific configurations, they can change the information in this file and transfer it to the machine). This number should be specified in the `run_delphi.py` file on line 8, denoted by the variable `num_proc`. For $n=43,85,127,169$, `num_proc` should be `3,6,9,12`, respectively. Further, the password must be set in the password field on line 23 in the file. 

6. The following command should be run on the device `10.43.0.231`. This python script uses the following dependencies: `paramiko,re,numpy,math,time,subprocess`. These libraries can be installed using `pip`. This script uses SCP and SSH to upload configuration files onto the devices and start the processes, respectively. The script runs a configuration three times and reports the vector of latencies. 
```
python3 run_delphi.py
```
We ran `delphi` with the configuration $\epsilon=5,\rho_0=5,\Delta=500,\delta=50,500$ for the drone-based object detection usecase. We also ran Abraham et al. at $\epsilon=5,\delta=5,\Delta=20$. Notice that $\delta$ in Delphi is different than Abraham et al. In Abraham et al., $\Delta$ is the real difference between honest inputs (It is the maximum difference in Delphi).  

7. After running the experiment, the processes can be killed using the `kill.py` file. The number of processes on each device should be specified in line 12 of the file. 
```
python3 kill.py
```

8. The experiment prints the termination latencies in a `latencies` file. 

9. The described steps so far are the same for running Abraham et al.  

## Running FIN
10. Running FIN requires a BLS threshold setup. Check the main README file for a detailed description. Each folder has a `tkeys-{n}.tar.gz` file. The following command should be run before running FIN.
```
$ cp tkeys-{n}.tar.gz tkeys.tar.gz
```

11. Then, FIN can be run by running the following command. 
```
$ python3 run_fin.py
```