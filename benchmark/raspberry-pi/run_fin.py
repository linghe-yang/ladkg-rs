import paramiko
import re
import numpy as np
import math
import time
import subprocess
import random
import sys
# Define the parameters for the devices
num_proc = [3,6,9,12]
password = sys.argv[1]
#print(num_proc)
for x in num_proc:
	ip_prefix = '10.42.0.'
	start_num = 0
	num_devices = 15
	each_device = x
	num_processes = (num_devices-1)*each_device + 1
	run_port = 8500
	syncer_run_port = 5000
	port_to_receive_from_syncer = 7000
	ip_start = 231
	start_val = 1000000

	# Create a list of devices with IP addresses and ports
	devices = [{'ip': f'{ip_prefix}{i+ip_start}', 'username': f'pi', 'password': str(password), 'working_directory': f'/home/pi/delphi-rs', 'app_port':run_port+i,'cli_port':syncer_run_port,'syncer_port':port_to_receive_from_syncer+i} for i in range(start_num, start_num + num_devices)]
	
	# Transmit all public keys and also individual private keys
	# Generate a list of all IP addresses with ports
	ind = 0
	total_processes = 0
	ip_list = []
	syncer_list = []
	port_r = run_port
	port_s = port_to_receive_from_syncer
	kill_ports_arr = []
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
	#ip_list = [f"{device['ip']}:{device['app_port']}" for device in devices]

	#Add syncer's ip to this file too
	ip_list.append(f"10.42.0.{ip_start}:{syncer_run_port}")

	# Write the IP list to a file
	with open('ip_file', 'w') as f:
		f.write('\n'.join(ip_list))

	# Write syncer file
	#syncer_list = [f"{device['ip']}:{device['syncer_port']}" for device in devices]

	with open('syncer','w') as f:
			f.write('\n'.join(syncer_list))

	#epsilon = np.array([1,10,50,200,500,2000,10000,30000,200000])
	#epsilon = np.array([1,10,50,200,500,2000,10000,30000,200000])
	#epsilon = np.array([1])
	#delta = np.array([1])
	#tri = np.array([300000])
	#delta = np.array([50,100,500,2000,10000,50000,200000])
	#epsilon = np.array([1,10,50,200,500,2000,10000,30000,200000])
	#delta = np.array([5000,20000,40000,80000,200000])
	#tri = np.array([36000,300000])
	epsilon=[5]
	delta = [5]
	tri = [500]
	Delta = [50]
	expo = [2]
	#epsilon = [1]
	#delta = [10]
	#tri = [100000]
	#Delta = [200]
	#epsilon = np.array([1,10,50,200,500,2000,10000,30000,200000])

	rand_num = random.randint(1,10000)
	cross_p = [[x0,y0,z0,d0] for x0 in epsilon for y0 in delta for z0 in tri for d0 in Delta if d0 <= z0/2 and d0 >= y0 ]
	iterations = 1
	latencies = []
	command_template = 'ln -sf node runnode '
	run_command = 'nohup ./runnode --config nodes-{ind}.json --ip ip_file --sleep 100 --epsilon {ep} --delta {del} --val {val} --tri {tri} --vsstype fin --syncer syncer --batch 100 --rand {rand} --expo 2 > /dev/null '
	unzip_and_copy = 'tar -xvf tkeys.tar.gz --touch && cp thresh_keys/* .'
	copy_tar = f'cp tkeys-{total_processes}.tar.gz tkeys.tar.gz'
	subprocess.run(copy_tar,shell=True)
	iterate_rand = 198789
	for arr in cross_p:
		latency_arr = []
		num_processes = (num_devices - 1)*x + 1
		for iterate in range(iterations):
			#print("Running system for the following configuration of ep,del: ",arr)
			ind = 0
			# Transfer the IP file, syncer_file, and the nodes config file to each device
			total_processes = 0
			for device in devices:
				# how many processes to run on each device?
				if ind == 2:
					# Run only one process on device measuring energy
					num_each_device = 1
				else:
					num_each_device = each_device
				if ind == 0:
					# Transfer files by number
					for proc in range(num_each_device):
						copy_files = f'cp {num_processes}/nodes-{total_processes}.json nodes-{total_processes}.json'
						subprocess.run(copy_files,shell=True)
						total_processes +=1
					ind +=1
					continue
				# Create an SSH client object
				client = paramiko.SSHClient()
				# Automatically add the server key
				client.set_missing_host_key_policy(paramiko.AutoAddPolicy())
				# Connect to the device
				client.connect(hostname=device['ip'], port=22, username=device['username'], password=device['password'])
				# Transfer the IP file to the device
				sftp = client.open_sftp()
				sftp.put('ip_file', f"{device['working_directory']}/ip_file")
				sftp.put('syncer',f"{device['working_directory']}/syncer")
				sftp.put('tkeys.tar.gz',f"{device['working_directory']}/tkeys.tar.gz")
				for proc in range(num_each_device):
					sftp.put(f"{num_processes}/nodes-{total_processes}.json",f"{device['working_directory']}/nodes-{total_processes}.json")
					total_processes += 1
				sftp.close()
				# Close the SSH connection
				client.close()
				ind +=1
			#print(f"Transferred config files to {total_processes} processes in {num_devices} devices")
			# Command to run on devices
			ind = 0
			epsilon = arr[0]
			delta = arr[1]
			values_arr = np.linspace(start_val-arr[3]/2,start_val+arr[3]/2,num_processes)
			values_arr = values_arr.astype(int)
			#vals = np.random.normal(loc=525000,scale=10000,size=(num_devices-1)*each_device + 1)
			int_val = []
			for val in values_arr:
				int_val.append(int(math.ceil(val)))
			tri = arr[2]

			# start the syncer first
			#syncer_run = 'ln -s ../target/release/node node && ../node --config nodes-0.json --ip ip_file --sleep 100 --epsilon 10 --delta 10000 --val 35000 --tri 20000 --vsstype hyb --syncer syncer --batch 100 > syncer.log'
			ind = 0
			syncer = 0
			total_processes = 0
			# Loop through the devices and execute the command
			while ind < len(devices):
				device = devices[ind]
				# Modify the command string using regex to replace placeholders with device-specific information
				command = re.sub('{ep}',str(epsilon),run_command)
				command = re.sub('{del}',str(delta),command)
				command = re.sub('{tri}',str(tri),command)
				command = re.sub('{rand}',str(iterate_rand),command)
				# add a command for syncer too
				# Add the working directory to the command string
				fin_command_template = 'cd ' + device['working_directory'] + ' && ' + command_template
				if syncer == 0:
					# Kill syncer first
					#kill_syncer = re.sub('{port}',str(cli_port),kill_template)
					command_syncer = re.sub('{ind}',str(ind),command)
					command_syncer = re.sub('{val}',str(ind),command_syncer)
					command_syncer = re.sub('fin ','sync ',command_syncer)
					#stdin,stdout,stderr = client.exec_command(kill_syncer)
					#print("kill syncer logs: stdout: "+stdout.read().decode() + "\n stderr:"+stderr.read().decode())
					# Replace log file
					command_syncer = re.sub('/dev/null','syncer.log',command_syncer)
					command_syncer = fin_command_template + ' && ' + command_syncer
					command_syncer  = command_syncer + ' 2>&1 & '
					#print(command_syncer)
					# UNCOMMENT BEFORE RUNNING
					subprocess.run(command_syncer,shell=True)
					#stdin,stdout,stderr = client.exec_command(command_syncer)
					#stdin.close()
					#time.sleep(0.1)
					#print("Syncer logs: stdout: "+ stdout.read().decode()+"\n stderr: "+ stderr.read().decode())
					#client.close()
					syncer = 1
					continue
				# Execute the modified command on the device
				# how many processes to run on each device?
				if ind == 2:
					# Run only one process on device measuring energy
					num_each_device = 1
				else:
					num_each_device = each_device
				# Create an SSH client object, Unzip tar file
				unzip_cmd = 'cd ' + device['working_directory'] + ' && ' +unzip_and_copy
				#print(unzip_cmd)
				client = paramiko.SSHClient()
				# Automatically add the server key
				client.set_missing_host_key_policy(paramiko.AutoAddPolicy())
				# Connect to the device
				client.connect(hostname=device['ip'], port=22, username=device['username'], password=device['password'])
				stdin, stdout, stderr = client.exec_command(unzip_cmd)
				err = stderr.readlines()
				#print(err)
				client.close()
				for proc in range(num_each_device):
					command_iter = re.sub('{ind}',str(total_processes),command)
					command_iter = re.sub('{val}',str(values_arr[total_processes]),command_iter)
					fin_command = fin_command_template + '&& ' + command_iter
					fin_command  = fin_command + ' 2>&1 & '
					#print(fin_command)
					if ind == 0:
						subprocess.run(fin_command,shell=True)
					else:
						# Create an SSH client object
						client = paramiko.SSHClient()
						# Automatically add the server key
						client.set_missing_host_key_policy(paramiko.AutoAddPolicy())
						# Connect to the device
						client.connect(hostname=device['ip'], port=22, username=device['username'], password=device['password'])
						stdin, stdout, stderr = client.exec_command(fin_command)
						client.close()
					total_processes += 1
				ind +=1
			print("Executed all commands, waiting for program to terminate")
			# Poll the log file and extract the latency of termination of the first node
			while True:
				with open("syncer.log","r") as f:
					lines = f.readlines()
					flag = 0
					for line in lines:
						if re.search(r"All n nodes completed the sharing protocol *?\[(.*?)\]",line):
							last_value = re.findall(r"\[(.*?)\]",line)[-1]
							last_values = last_value.split(", ")
							int_last_values = []
							for val in last_values:
								int_last_values.append(int(val))
							avg_lat = np.mean(int_last_values)
							latency_arr.append(avg_lat)
							print(f"Average latency and energy: {int(avg_lat)}")
							flag = 1
							break
					if flag ==1:
						break
				time.sleep(1)
			kill_template = 'sudo lsof -ti :{port} | sudo xargs kill -9'
			# Execute kill commands first
			ind = 0
			for device in devices:
				if ind == 0:
					ports_0_ws = kill_ports_arr[ind] + f",{device['cli_port']}"
					kill_command = re.sub('{port}',ports_0_ws,kill_template)
					#print(kill_command)
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
					#print(kill_command)
					# UNCOMMENT THIS LINE
					stdin,stdout,stderr = client.exec_command(kill_command)
					stdin.close()
					#print(stderr.read().decode())
					client.close()
				ind += 1
			iterate_rand +=1
		latencies.append(latency_arr)
	#print(cross_p)
	print(latencies)
	import csv
	with open(f"latencies_n_{x}_fin.txt","a") as f:
		csv.writer(f,delimiter=' ').writerows(latencies)
		f.close()
