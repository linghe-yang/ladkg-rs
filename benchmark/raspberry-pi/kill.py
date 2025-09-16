import paramiko
import re
import numpy as np
import math
import time
import subprocess

# Define the parameters for the devices
ip_prefix = '10.43.0.'
start_num = 0
num_devices = 15
each_device = 12
num_processes = (num_devices-1)*each_device + 1
run_port = 8500
cli_port = 5000
syncer_port = 7000
ip_start = 231

# Create a list of devices with IP addresses and ports
devices = [{'ip': f'{ip_prefix}{i+ip_start}', 'username': f'pi', 'password': f'dcsl_1234', 'working_directory': f'/home/pi/async-cc-hash', 'app_port':run_port+i,'cli_port':cli_port,'syncer_port':syncer_port+i} for i in range(start_num, start_num + num_devices)]

# Generate a list of all IP addresses with ports
ind = 0
total_processes = 0
ip_list = []
syncer_list = []
port_r = run_port
port_s = syncer_port
kill_ports_arr = []
kill_template = 'sudo lsof -ti :{port} | sudo xargs kill -9'
for device in devices:
	if ind == 2:
		num_each_device = 1
	else:
		num_each_device = each_device
	kill_ports = f"{port_r},{port_s}"
	for proc in range(num_each_device):
		ip_proc = f"{device['ip']}:{port_r}"
		ip_sync_proc = f"{device['ip']}:{port_s}"
		ip_list.append(ip_proc)
		syncer_list.append(ip_sync_proc)
		port_r +=1
		port_s += 1
		kill_ports += f",{port_r},{port_s}"
	kill_ports_arr.append(kill_ports)
	ind += 1
	
ind = 0
for device in devices:
	if ind == 0:
		ports_0_ws = kill_ports_arr[ind] + f",{device['cli_port']}"
		kill_command = re.sub('{port}',ports_0_ws,kill_template)
		print(kill_command)
		# UNCOMMENT THIS LINE
		subprocess.run(kill_command,shell=True)
	else:
		kill_command = re.sub('{port}',kill_ports_arr[ind],kill_template)
		# Create an SSH client object
		client = paramiko.SSHClient()
		# Automatically add the server key
		client.set_missing_host_key_policy(paramiko.AutoAddPolicy())
		# Connect to the device
		client.connect(hostname=device['ip'], port=22, username=device['username'], password=device['password'])
		print(kill_command)
		# UNCOMMENT THIS LINE
		stdin,stdout,stderr = client.exec_command(kill_command)
		stdin.close()
		print(stderr.read().decode())
		client.close()
	ind += 1
