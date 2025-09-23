# Copyright(C) Facebook, Inc. and its affiliates.
from collections import OrderedDict
from fabric import Connection, ThreadingGroup as Group
from fabric.exceptions import GroupException
from paramiko import RSAKey
from paramiko.ssh_exception import PasswordRequiredException, SSHException
from os.path import basename, splitext
from time import sleep
from math import ceil
from copy import deepcopy
import subprocess
import time
import os
from concurrent.futures import ThreadPoolExecutor
from benchmark.config import Committee, Key, NodeParameters, BenchParameters, ConfigError, DKGParameters
from benchmark.utils import BenchError, Print, PathMaker, progress_bar
from benchmark.commands import CommandMaker
from benchmark.logs import LogParser, ParseError
from benchmark.instance import InstanceManager


class FabricError(Exception):
    ''' Wrapper for Fabric exception with a meaningfull error message. '''

    def __init__(self, error):
        assert isinstance(error, GroupException)
        message = list(error.result.values())[-1]
        super().__init__(message)


class ExecutionError(Exception):
    pass


class Bench:
    BASE_PORT = 6000
    RBC_BASE_PORT = 6001
    DKG_BASE_PORT = 6002
    DRB_BASE_PORT = 6003
    cl_bport = 5000
    cl_rport = 5001
    def __init__(self, ctx):
        self.manager = InstanceManager.make()
        self.settings = self.manager.settings
        try:
            ctx.connect_kwargs.pkey = RSAKey.from_private_key_file(
                self.manager.settings.key_path
            )
            #ctx.connect_kwargs = {"key_filename": "/home/akhil/.ssh/aws", "passphrase": ""}
            self.connect = ctx.connect_kwargs
        except (IOError, PasswordRequiredException, SSHException) as e:
            # will print this message followed by traceback
            raise BenchError('Failed to load SSH key', e)

    def _check_stderr(self, output):
        if isinstance(output, dict):
            for x in output.values():
                if x.stderr:
                    raise ExecutionError(x.stderr)
        else:
            if output.stderr:
                raise ExecutionError(output.stderr)

    def install(self):
        Print.info('Installing rust and cloning the repo...')
        cmd = [
            'sudo apt-get update',
            'sudo apt-get -y upgrade',
            'sudo apt-get -y autoremove',

            # The following dependencies prevent the error: [error: linker `cc` not found].
            'sudo apt-get -y install build-essential',
            'sudo apt-get -y install cmake',

            # Install rust (non-interactive).
            'curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y',
            'source $HOME/.cargo/env',
            'rustup default stable',

            # This is missing from the Rocksdb installer (needed for Rocksdb).
            'sudo apt-get install -y clang',

            # Clone the repo.
            f'(git clone {self.settings.repo_url} || (cd {self.settings.repo_name} ; git pull))'
        ]
        hosts = self.manager.hosts(flat=True)
        try:
            g = Group(*hosts, user='ubuntu', connect_kwargs=self.connect)
            #g.run('ls')
            g.run(' && '.join(cmd), hide=True)
            Print.heading(f'Initialized testbed of {len(hosts)} nodes')
        except (GroupException, ExecutionError) as e:
            e = FabricError(e) if isinstance(e, GroupException) else e
            import traceback
            import sys
            print("Exception",e)
            print(traceback.format_exc())
             # or
            print(sys.exc_info()[2])
            raise BenchError('Failed to install repo on testbed', e)

    def kill(self, hosts=[], delete_logs=False):
        assert isinstance(hosts, list)
        assert isinstance(delete_logs, bool)
        hosts = hosts if hosts else self.manager.hosts(flat=True)
        delete_logs = CommandMaker.clean_logs() if delete_logs else 'true'
        cmd = [delete_logs, f'({CommandMaker.kill()} || true)']
        try:
            g = Group(*hosts, user='ubuntu', connect_kwargs=self.connect)
            g.run(' && '.join(cmd), hide=True)
        except GroupException as e:
            raise BenchError('Failed to kill nodes', FabricError(e))

    def _select_hosts(self, bench_parameters):
        # Collocate the primary and its workers on the same machine.
        if bench_parameters.collocate:
            nodes = bench_parameters.nodes + 1

            # Ensure there are enough hosts.
            hosts = self.manager.hosts()
            if sum(len(x) for x in hosts.values()) < nodes:
                return []

            # Select the hosts in different data centers.
            ordered = zip(*hosts.values())
            ordered = [x for y in ordered for x in y]
            return ordered[:nodes]

        # Spawn the primary and each worker on a different machine. Each
        # authority runs in a single data center.
        else:
            primaries = bench_parameters.nodes

            # Ensure there are enough hosts.
            hosts = self.manager.hosts()
            if len(hosts.keys()) < primaries:
                return []
            for ips in hosts.values():
                if len(ips) < bench_parameters.workers + 1:
                    return []

            # Ensure the primary and its workers are in the same region.
            selected = []
            for region in list(hosts.keys())[:primaries]:
                ips = list(hosts[region])[:bench_parameters.workers + 1]
                selected.append(ips)
            return selected

    def _background_run(self, host, command, log_file):
        name = splitext(basename(log_file))[0]
        cmd = f'tmux new -d -s "{name}" "{command} |& tee {log_file}"'
        c = Connection(host, user='ubuntu', connect_kwargs=self.connect)
        output = c.run(cmd, hide=True)
        self._check_stderr(output)

    def _background_run_local(self, command, log_file):
        name = splitext(basename(log_file))[0]
        cmd = f'{command} 2> {log_file}'
        try:
            subprocess.run(['tmux', 'new', '-d', '-s', name, cmd], check=True)
        except subprocess.SubprocessError as e:
            raise BenchError('Failed to kill testbed', e)

    # def _update(self, hosts, collocate):
    #     if collocate:
    #         ips = list(set(hosts))
    #     else:
    #         ips = list(set([x for y in hosts for x in y]))
    #
    #     Print.info(
    #         f'Updating {len(ips)} machines (branch "{self.settings.branch}")...'
    #     )
    #     cmd = [
    #         f'(cd {self.settings.repo_name} && git fetch -f)',
    #         f'(cd {self.settings.repo_name} && git checkout -f {self.settings.branch})',
    #         f'(cd {self.settings.repo_name} && git pull -f)',
    #         'source $HOME/.cargo/env',
    #         'sudo apt install pkg-config && sudo apt install libssl-dev',
    #         f'(cd {self.settings.repo_name} && {CommandMaker.compile()})',
    #         CommandMaker.alias_binaries(
    #             f'./{self.settings.repo_name}/target/release/'
    #         )
    #     ]
    #     g = Group(*ips, user='ubuntu', connect_kwargs=self.connect)
    #     print(g.run(' && '.join(cmd), hide=True))

    def _update(self, hosts, collocate):
        # Step 1: Determine unique IPs
        if collocate:
            ips = list(set(hosts))
        else:
            ips = list(set([x for y in hosts for x in y]))

        Print.info(f'Testing SSH connectivity for {len(ips)} machines...')

        # Step 2: Test SSH connectivity
        g = Group(*ips, user='ubuntu', connect_kwargs=self.connect)
        test_cmd = 'echo "test"'
        failed_ips = []
        try:
            results = g.run(test_cmd, hide=True, warn=True, timeout=10)
            for ip, result in results.items():
                if result.exited != 0 or result.stderr:
                    failed_ips.append((ip, result.stderr or f'Non-zero exit code: {result.exited}'))
        except GroupException as e:
            for ip, result in e.result.items():
                if isinstance(result, Exception):
                    failed_ips.append((ip, str(result)))
                elif result.exited != 0 or result.stderr:
                    failed_ips.append((ip, result.stderr or f'Non-zero exit code: {result.exited}'))

        if failed_ips:
            for ip, error in failed_ips:
                Print.warn(f'SSH connection failed on {ip}: {error}')
            raise BenchError(
                f'SSH connectivity test failed for {len(failed_ips)}/{len(ips)} instances',
                Exception('SSH connectivity issues')
            )
        Print.info(f'Successfully connected to all {len(ips)} instances')

        Print.info(f'Updating {len(ips)} machines with local node binary...')
        # Step 3: Clean up local files
        Print.info('Cleaning up local files...')
        cmd = f'{CommandMaker.clean_logs()} ; {CommandMaker.cleanup()}'
        subprocess.run([cmd], shell=True, stderr=subprocess.DEVNULL)
        time.sleep(0.5)  # Allow time for cleanup
        # Step 4: Compile locally
        Print.info('Compiling locally...')
        try:
            cmd = CommandMaker.compile_remote().split()
            subprocess.run(cmd, check=True, cwd=PathMaker.node_crate_path())
        except subprocess.CalledProcessError as e:
            raise BenchError(f'Failed to compile locally: {e.stderr}', e)
        except Exception as e:
            raise BenchError('Unexpected error during local compilation', e)
        # Step 5: Upload node binary to remote instances in parallel
        local_repo_path = os.path.expanduser(f'~/{self.settings.repo_name}')
        binary_dir = os.path.join(local_repo_path, 'target', 'release')
        remote_binary_dir = f'/home/ubuntu/{self.settings.repo_name}/target/release/'
        binary = 'node'
        local_path = os.path.join(binary_dir, binary)

        if not os.path.exists(local_path):
            raise BenchError(f'Binary {local_path} not found after compilation', None)

        def upload_binary(ip):
            """Upload node binary to a single instance."""
            c = Connection(ip, user='ubuntu', connect_kwargs=self.connect)
            try:
                # Ensure remote directory exists
                c.run(f'mkdir -p {remote_binary_dir}', hide=True)
                # Upload node binary
                remote_path = f'{remote_binary_dir}{binary}'
                c.put(local_path, remote_path)
                # Set executable permissions
                c.run(f'chmod +x {remote_path}', hide=True)
                # Create binary alias
                c.run(
                    CommandMaker.alias_binaries(f'./{self.settings.repo_name}/target/release/'),
                    hide=True
                )
            except Exception as e:
                raise BenchError(f'Failed to upload binary to {ip}', e)

        Print.info(f'Uploading node binary to {len(ips)} instances in parallel...')
        with ThreadPoolExecutor(max_workers=10) as executor:
            futures = [executor.submit(upload_binary, ip) for ip in ips]
            progress = progress_bar(futures, prefix='Uploading node binary:')
            for future in progress:
                future.result()  # Wait for each task to complete and raise any exceptions

        Print.info(f'Successfully updated {len(ips)} machines with local node binary')


    def _config(self, hosts, node_parameters, bench_parameters):
        Print.info('Generating configuration files...')
        #print(hosts)
        # Cleanup all local configuration files.
        cmd = CommandMaker.cleanup()
        subprocess.run([cmd], shell=True, stderr=subprocess.DEVNULL)

        # Recompile the latest code.
        cmd = CommandMaker.compile().split()
        subprocess.run(cmd, check=True, cwd=PathMaker.node_crate_path())

        # Create alias for the client and nodes binary.
        cmd = CommandMaker.alias_binaries(PathMaker.binary_path())
        subprocess.run([cmd], shell=True)

        # Generate configuration files.
        # keys = []
        # key_files = [PathMaker.key_file(i) for i in range(len(hosts))]
        # for filename in key_files:
        #     cmd = CommandMaker.generate_key(filename).split()
        #     subprocess.run(cmd, check=True)
        #     keys += [Key.from_file(filename)]
        #committee = LocalCommittee(names, self.BASE_PORT)
        #ip_file.print("ip_file")

        # Generate the configuration files for HashRand
        cmd = CommandMaker.generate_config_files(self.settings.base_port,self.settings.client_base_port,self.settings.client_run_port,len(hosts))
        subprocess.run(cmd,shell=True)
        names = [str(x) for x in range(len(hosts))]
        ip_file = ""
        syncer=""
        for x in range(len(hosts)):
            port = self.settings.base_port + x
            syncer_port = self.settings.client_base_port + x
            ip_file += hosts[x]+ ":"+ str(port) + "\n"
            syncer += hosts[x] + ":" + str(syncer_port) + "\n"
        ip_file += hosts[0] + ":" + str(self.settings.client_run_port) + "\n"
        with open("ip_file", 'w') as f:
            f.write(ip_file)
        f.close()
        with open("syncer",'w') as f:
            f.write(syncer)
        f.close()
        #names = [str(x) for x in range(len(hosts))]

        if bench_parameters.collocate:
            workers = bench_parameters.workers
            addresses = OrderedDict(
                (x, [y] * (workers + 1)) for x, y in zip(names, hosts)
            )
        else:
            addresses = OrderedDict(
                (x, y) for x, y in zip(names, hosts)
            )
        committee = Committee(addresses, self.settings.base_port)
        committee.print(PathMaker.committee_file())

        node_parameters.print(PathMaker.parameters_file())
        # start the syncer on the first node first. 

        # Cleanup all nodes and upload configuration files.
        names = names[:len(names)-bench_parameters.faults]
        progress = progress_bar(names, prefix='Uploading config files:')
        for i, name in enumerate(progress):
            #for ip in committee.ips(name):
            c = Connection(hosts[i], user='ubuntu', connect_kwargs=self.connect)
            c.run(f'{CommandMaker.cleanup()} || true', hide=True)
            #c.put(PathMaker.committee_file(), '.')
            if i == 0:
                print('Node 0: writing syncer')
                c.put(PathMaker.syncer(),'.')
            c.put(PathMaker.key_file(i), '.')
            c.put(PathMaker.t_key_file(),'.')
            c.put("ip_file",'.')
            #c.put(PathMaker.parameters_file(), '.')
        Print.info('Booting primaries...')
        st_time = round(time.time() * 1000) + 60000
        n = 160
        epsilon = 2
        rho_0 = 2
        delta = 20
        Delta = 2000
        exp_vals = self.exp_setup(n,delta)
        import numpy as np
        import random
        rand = random.randint(1000000,15000000)
        for i,ip in enumerate(hosts):
            if i == 0:
                # Run syncer first
                print('Running syncer')
                cmd = CommandMaker.run_syncer(
                    PathMaker.key_file(i),
                    st_time,
                    debug=False
                )
                print(cmd)
                log_file = PathMaker.syncer_log_file()
                self._background_run(ip, cmd, log_file)
            cmd = CommandMaker.run_primary(
                PathMaker.key_file(i),
                st_time,
                epsilon,
                rho_0,
                exp_vals[i],
                Delta,
                100,
                rand,
                debug=False
            )
            unzip_cmd = CommandMaker.unzip_tkeys('tkeys.tar.gz','thresh_keys')
            print(unzip_cmd)
            self._background_run(ip,unzip_cmd,"unzip.log")
            print(cmd)
            log_file = PathMaker.primary_log_file(i)
            self._background_run(ip, cmd, log_file)
        return committee

    def exp_setup(self,n,delta):
        import numpy as np
        #values = np.random.normal(loc=2300,scale=50,size=n)
        values = np.linspace(2200,2200+delta,num=n)
        arr_int = []
        for val in values:
            arr_int.append(int(val))
        return arr_int


    def _just_run(self, hosts, node_parameters, bench_parameters):
        # Print.info('Generating configuration files...')
        # print(hosts)
        # # Cleanup all local configuration files.
        # cmd = CommandMaker.cleanup()
        # subprocess.run([cmd], shell=True, stderr=subprocess.DEVNULL)

        # # Recompile the latest code.
        # cmd = CommandMaker.compile().split()
        # subprocess.run(cmd, check=True, cwd=PathMaker.node_crate_path())

        # # Create alias for the client and nodes binary.
        # cmd = CommandMaker.alias_binaries(PathMaker.binary_path())
        # subprocess.run([cmd], shell=True)

        # # Generate configuration files.
        # # keys = []
        # # key_files = [PathMaker.key_file(i) for i in range(len(hosts))]
        # # for filename in key_files:
        # #     cmd = CommandMaker.generate_key(filename).split()
        # #     subprocess.run(cmd, check=True)
        # #     keys += [Key.from_file(filename)]
        # names = [str(x) for x in range(len(hosts))]
        # ip_file = ""
        # for x in range(len(hosts)):
        #     port = self.settings.base_port + x
        #     ip_file += hosts[x]+ ":"+ str(port) + "\n"
        # with open("ip_file", 'w') as f:
        #     f.write(ip_file)
        # f.close()
        # #committee = LocalCommittee(names, self.BASE_PORT)
        # #ip_file.print("ip_file")

        # # Generate the configuration files for HashRand
        # cmd = CommandMaker.generate_config_files(self.settings.base_port,10000,len(hosts))
        # subprocess.run(cmd,shell=True)

        # names = [str(x) for x in range(len(hosts))]

        # if bench_parameters.collocate:
        #     workers = bench_parameters.workers
        #     addresses = OrderedDict(
        #         (x, [y] * (workers + 1)) for x, y in zip(names, hosts)
        #     )
        # else:
        #     addresses = OrderedDict(
        #         (x, y) for x, y in zip(names, hosts)
        #     )
        # committee = Committee(addresses, self.settings.base_port)
        # committee.print(PathMaker.committee_file())

        # node_parameters.print(PathMaker.parameters_file())

        # # Cleanup all nodes and upload configuration files.
        # names = names[:len(names)-bench_parameters.faults]
        # progress = progress_bar(names, prefix='Uploading config files:')
        # for i, name in enumerate(progress):
        #     #for ip in committee.ips(name):
        #     c = Connection(hosts[i], user='ubuntu', connect_kwargs=self.connect)
        #     c.run(f'{CommandMaker.cleanup()} || true', hide=True)
        #     #c.put(PathMaker.committee_file(), '.')
        #     c.put(PathMaker.key_file(i), '.')
        #     c.put("ip_file",'.')
        #     #c.put(PathMaker.parameters_file(), '.')
        Print.info('Booting primaries...')
        st_time = round(time.time() * 1000) + 60000
        n=160
        epsilon = 2
        rho_0 = 2
        delta = 20
        exp_vals = self.exp_setup(n,delta)
        import numpy as np
        Delta = 20
        import random
        rand = random.randint(1000000,15000000)
        for i,ip in enumerate(hosts):
            #host = Committee.ip(address)
            if i == 0:
                # Run syncer first
                print('Running syncer')
                cmd = CommandMaker.run_syncer(
                    PathMaker.key_file(i),
                    st_time,
                    debug=False
                )
                print(cmd)
                log_file = PathMaker.syncer_log_file()
                self._background_run(ip, cmd, log_file)
            cmd = CommandMaker.run_primary(
                PathMaker.key_file(i),
                st_time,
                epsilon,
                rho_0,
                exp_vals[i],
                Delta,
                100,
                rand,
                debug=False
            )
            log_file = PathMaker.primary_log_file(i)
            self._background_run(ip, cmd, log_file)

    def _run_single(self, hosts, debug=False):
        # faults = bench_parameters.faults

        # Kill any potentially unfinished run and delete logs.
        # hosts = committee.ips()
        self.kill(hosts=hosts, delete_logs=True)

        # Run the clients (they will wait for the nodes to be ready).
        # Filter all faulty nodes from the client addresses (or they will wait
        # for the faulty nodes to be online).
        #Print.info('Booting clients...')
        #workers_addresses = committee.workers_addresses(faults)
        # rate_share = ceil(rate / committee.workers())
        # for i, addresses in enumerate(workers_addresses):
        #     for (id, address) in addresses:
        #         host = Committee.ip(address)
        #         cmd = CommandMaker.run_client(
        #             address,
        #             bench_parameters.tx_size,
        #             rate_share,
        #             [x for y in workers_addresses for _, x in y]
        #         )
        #         log_file = PathMaker.client_log_file(i, id)
        #         self._background_run(host, cmd, log_file)

        # Run the primaries (except the faulty ones).
        Print.info('Booting primaries...')
        i=0
        for ip in enumerate(hosts):
            #host = Committee.ip(address)
            cmd = CommandMaker.run_primary(
                PathMaker.key_file(i),
                debug=debug
            )
            log_file = PathMaker.primary_log_file(i)
            self._background_run(ip, cmd, log_file)
            i+=1
        # Run the workers (except the faulty ones).
        # Print.info('Booting workers...')
        # for i, addresses in enumerate(workers_addresses):
        #     for (id, address) in addresses:
        #         host = Committee.ip(address)
        #         cmd = CommandMaker.run_worker(
        #             PathMaker.key_file(i),
        #             PathMaker.committee_file(),
        #             PathMaker.db_path(i, id),
        #             PathMaker.parameters_file(),
        #             id,  # The worker's id.
        #             debug=debug
        #         )
        #         log_file = PathMaker.worker_log_file(i, id)
        #         self._background_run(host, cmd, log_file)

        # Wait for all transactions to be processed.
        # duration = bench_parameters.duration
        # for _ in progress_bar(range(20), prefix=f'Running benchmark ({duration} sec):'):
        #     sleep(ceil(duration / 20))
        # self.kill(hosts=hosts, delete_logs=False)

    def _logs(self, hosts, faults):
        # Delete local logs (if any).
        cmd = CommandMaker.clean_logs()
        subprocess.run([cmd], shell=True, stderr=subprocess.DEVNULL)

        # Download log files.
        #workers_addresses = committee.workers_addresses(faults)
        progress = progress_bar(hosts, prefix='Downloading workers logs:')
        for i, address in enumerate(progress):
            c = Connection(address, user='ubuntu', connect_kwargs=self.connect)
            if i==0:
                c.get(
                    PathMaker.syncer_log_file(),
                    local=PathMaker.syncer_log_file()
                )
                c.get(
                    PathMaker.client_log_file(i, 0), 
                    local=PathMaker.client_log_file(i, 0)
                )
            # c.get(
            #     PathMaker.client_log_file(i, 0), 
            #     local=PathMaker.client_log_file(i, 0)
            # )
            # c.get(
            #     PathMaker.worker_log_file(i, id),     
            #     local=PathMaker.worker_log_file(i, id)
            # )

        # primary_addresses = committee.primary_addresses(faults)
        # progress = progress_bar(primary_addresses, prefix='Downloading primaries logs:')
        # for i, address in enumerate(progress):
        #     host = Committee.ip(address)
        #     c = Connection(host, user='ubuntu', connect_kwargs=self.connect)
        #     c.get(
        #         PathMaker.primary_log_file(i), 
        #         local=PathMaker.primary_log_file(i)
        #    )

        # Parse logs and return the parser.
        Print.info('Parsing logs and computing performance...')
        return LogParser.process(PathMaker.logs_path(), faults=faults)

    def run(self, bench_parameters_dict, dkg_params, debug=False):
        assert isinstance(debug, bool)
        Print.heading('Starting remote benchmark')
        try:
            bench_parameters = BenchParameters(bench_parameters_dict)
            dkg_params = DKGParameters(dkg_params)
        except ConfigError as e:
            raise BenchError('Invalid nodes or bench parameters', e)

        # Select which hosts to use.
        selected_hosts = self._select_hosts(bench_parameters)
        if not selected_hosts:
            Print.warn('There are not enough instances available')
            return


        # Update nodes.
        try:
            self._update(selected_hosts, bench_parameters.collocate)
        except (GroupException, ExecutionError) as e:
            e = FabricError(e) if isinstance(e, GroupException) else e
            raise BenchError('Failed to update nodes', e)

        # Create alias for the client and nodes binary.
        cmd = CommandMaker.alias_binaries(PathMaker.binary_path())
        subprocess.run([cmd], shell=True)

        host_str = " ".join(selected_hosts)
        print("hoststr=",host_str)

        # Generate the configuration files
        cmd = CommandMaker.generate_config_files_remote(self.BASE_PORT, self.RBC_BASE_PORT, self.DKG_BASE_PORT,
                                                 self.DRB_BASE_PORT, self.cl_bport, self.cl_rport,
                                                 bench_parameters.nodes,
                                                 dkg_params, bench_parameters.kappa,host_str)
        self._background_run_local(cmd, "err.log")
        time.sleep(1)

        # # Upload all configuration files.
        # try:
        #     committee = self._config(
        #         selected_hosts, node_parameters, bench_parameters
        #     )
        # except (subprocess.SubprocessError, GroupException) as e:
        #     e = FabricError(e) if isinstance(e, GroupException) else e
        #     raise BenchError('Failed to configure nodes', e)

        # Run benchmarks.
        # for n in bench_parameters.nodes:
        #     committee_copy = deepcopy(committee)
        #     committee_copy.remove_nodes(committee.size() - n)

        #     for r in bench_parameters.rate:
        #         Print.heading(f'\nRunning {n} nodes (input rate: {r:,} tx/s)')

        #         # Run the benchmark.
        #         for i in range(bench_parameters.runs):
        #             Print.heading(f'Run {i+1}/{bench_parameters.runs}')
        #             try:
        #                 self._run_single(
        #                     r, committee_copy, bench_parameters, debug
        #                 )

        #                 faults = bench_parameters.faults
        #                 #logger = self._logs(committee_copy, faults)
        #                 logger.print(PathMaker.result_file(
        #                     faults,
        #                     n, 
        #                     bench_parameters.workers,
        #                     bench_parameters.collocate,
        #                     r, 
        #                     bench_parameters.tx_size, 
        #                 ))
        #             except (subprocess.SubprocessError, GroupException, ParseError) as e:
        #                 self.kill(hosts=selected_hosts)
        #                 if isinstance(e, GroupException):
        #                     e = FabricError(e)
        #                 Print.error(BenchError('Benchmark failed', e))
        #                 continue
    def justrun(self, bench_parameters_dict, node_parameters_dict, debug=False):
        assert isinstance(debug, bool)
        Print.heading('Starting remote benchmark')
        try:
            bench_parameters = BenchParameters(bench_parameters_dict)
            node_parameters = NodeParameters(node_parameters_dict)
        except ConfigError as e:
            raise BenchError('Invalid nodes or bench parameters', e)

        # Select which hosts to use.
        selected_hosts = self._select_hosts(bench_parameters)
        print(selected_hosts)
        if not selected_hosts:
            Print.warn('There are not enough instances available')
            return

        # Update nodes.
        try:
            self._update(selected_hosts, bench_parameters.collocate)
        except (GroupException, ExecutionError) as e:
            e = FabricError(e) if isinstance(e, GroupException) else e
            raise BenchError('Failed to update nodes', e)

        # Upload all configuration files.
        try:
            committee = self._just_run(
                selected_hosts, node_parameters, bench_parameters
            )
        except (subprocess.SubprocessError, GroupException) as e:
            e = FabricError(e) if isinstance(e, GroupException) else e
            raise BenchError('Failed to configure nodes', e)

        # Run benchmarks.
        # for n in bench_parameters.nodes:
        #     committee_copy = deepcopy(committee)
        #     committee_copy.remove_nodes(committee.size() - n)

        #     for r in bench_parameters.rate:
        #         Print.heading(f'\nRunning {n} nodes (input rate: {r:,} tx/s)')

        #         # Run the benchmark.
        #         for i in range(bench_parameters.runs):
        #             Print.heading(f'Run {i+1}/{bench_parameters.runs}')
        #             try:
        #                 self._run_single(
        #                     r, committee_copy, bench_parameters, debug
        #                 )

        #                 faults = bench_parameters.faults
        #                 #logger = self._logs(committee_copy, faults)
        #                 logger.print(PathMaker.result_file(
        #                     faults,
        #                     n, 
        #                     bench_parameters.workers,
        #                     bench_parameters.collocate,
        #                     r, 
        #                     bench_parameters.tx_size, 
        #                 ))
        #             except (subprocess.SubprocessError, GroupException, ParseError) as e:
        #                 self.kill(hosts=selected_hosts)
        #                 if isinstance(e, GroupException):
        #                     e = FabricError(e)
        #                 Print.error(BenchError('Benchmark failed', e))
        #                 continue
    def pull_logs(self, bench_parameters_dict, node_parameters_dict, debug=False):
        assert isinstance(debug, bool)
        Print.heading('Starting remote benchmark')
        try:
            bench_parameters = BenchParameters(bench_parameters_dict)
            node_parameters = NodeParameters(node_parameters_dict)
        except ConfigError as e:
            raise BenchError('Invalid nodes or bench parameters', e)

        # Select which hosts to use.
        selected_hosts = self._select_hosts(bench_parameters)
        return self._logs(selected_hosts,0)
