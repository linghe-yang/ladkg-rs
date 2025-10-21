# Copyright(C) Facebook, Inc. and its affiliates.
import json
import os
import subprocess
import time
from concurrent.futures import ThreadPoolExecutor
from datetime import datetime, timedelta
from math import ceil
from os.path import basename, splitext
from pathlib import Path
from time import sleep

from fabric import Connection, ThreadingGroup as Group
from fabric.exceptions import GroupException
from paramiko import RSAKey
from paramiko.ssh_exception import PasswordRequiredException, SSHException

from benchmark.commands import CommandMaker
from benchmark.config import BenchParameters, ConfigError, DKGParameters
from benchmark.instance import InstanceManager
from benchmark.logs import LogParser, ParseError
from benchmark.utils import BenchError, Print, PathMaker, progress_bar


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
            # ctx.connect_kwargs = {"key_filename": "/home/akhil/.ssh/aws", "passphrase": ""}
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
            # g.run('ls')
            g.run(' && '.join(cmd), hide=True)
            Print.heading(f'Initialized testbed of {len(hosts)} nodes')
        except (GroupException, ExecutionError) as e:
            e = FabricError(e) if isinstance(e, GroupException) else e
            import traceback
            import sys
            print("Exception", e)
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
            total_hosts = sum(len(x) for x in hosts.values())
            if total_hosts < nodes:
                Print.warn(f'Not enough hosts: required {nodes}, available {total_hosts}')
                return []

            # Select all hosts by flattening the IP lists
            ordered = [ip for region_ips in hosts.values() for ip in region_ips]
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

    def _config(self, syncer_ip):
        """Upload configuration files to AWS instances."""
        # Step 1: Read configuration files from ./configs directory
        configs_dir = Path('./configs')
        if not configs_dir.exists():
            raise BenchError(f'Configs directory {configs_dir} not found', None)

        # Collect node-x.json files
        node_configs = [f for f in configs_dir.glob('nodes-*.json') if f.is_file()]
        syncer_file = configs_dir / 'syncer'
        syncer_json = configs_dir / 'syncer.json'
        node_0_json = configs_dir / 'nodes-0.json'

        # Validate syncer files
        if not syncer_file.exists():
            raise BenchError(f'Syncer file {syncer_file} not found', None)
        if not syncer_json.exists():
            raise BenchError(f'Syncer JSON file {syncer_json} not found', None)

        Print.info(f'Found {len(node_configs)} node configuration files')

        # Step 2: Upload syncer and syncer.json to syncer_ip
        Print.info(f'Uploading syncer and syncer.json to {syncer_ip}...')
        try:
            c = Connection(syncer_ip, user='ubuntu', connect_kwargs=self.connect)
            # Create or clear /home/ubuntu/configs directory
            c.run('rm -rf /home/ubuntu/configs && mkdir -p /home/ubuntu/configs', hide=True)
            # Upload syncer and syncer.json
            c.put(str(syncer_file), '/home/ubuntu/configs/syncer')
            c.put(str(syncer_json), '/home/ubuntu/configs/syncer.json')
            c.put(str(node_0_json), '/home/ubuntu/configs/nodes-0.json')
            Print.info(f'Successfully uploaded syncer and syncer.json to {syncer_ip}')
        except Exception as e:
            raise BenchError(f'Failed to upload syncer files to {syncer_ip}', e)

        # Step 3: Process node-x.json files and map to target IPs
        ip_to_files = {}
        for config_file in node_configs:
            try:
                with open(config_file, 'r') as f:
                    config = json.load(f)
                node_id = config.get('id')
                if node_id is None:
                    raise BenchError(f'ID not found in {config_file}', None)

                # Find IP from net_map_delphi (or any net_map_*)
                net_map = config.get('net_map_delphi')
                if not net_map or str(node_id) not in net_map:
                    raise BenchError(f'net_map_delphi or ID {node_id} not found in {config_file}', None)

                # Extract IP (remove port)
                ip_port = net_map[str(node_id)]
                ip = ip_port.split(':')[0]
                ip_to_files[ip] = ip_to_files.get(ip, []) + [config_file]
            except json.JSONDecodeError as e:
                raise BenchError(f'Failed to parse JSON in {config_file}', e)
            except Exception as e:
                raise BenchError(f'Error processing {config_file}', e)

        Print.info(f'Uploading configuration files to {len(ip_to_files)} instances...')

        # Step 4: Upload node-x.json and syncer to corresponding IPs in parallel
        def upload_config(ip, files):
            """Upload node-x.json and syncer to a single instance."""
            try:
                c = Connection(ip, user='ubuntu', connect_kwargs=self.connect)
                # Create or clear /home/ubuntu/configs directory
                c.run('rm -rf /home/ubuntu/configs && mkdir -p /home/ubuntu/configs', hide=True)
                # Upload node-x.json files
                for config_file in files:
                    c.put(str(config_file), f'/home/ubuntu/configs/{config_file.name}')
                # Upload syncer
                c.put(str(syncer_file), '/home/ubuntu/configs/syncer')
            except Exception as e:
                raise BenchError(f'Failed to upload config files to {ip}', e)

        with ThreadPoolExecutor(max_workers=10) as executor:
            futures = [executor.submit(upload_config, ip, files) for ip, files in ip_to_files.items()]
            progress = progress_bar(futures, prefix='Uploading config files:')
            for future in progress:
                future.result()  # Wait for each task to complete and raise any exceptions

        Print.info(f'Successfully uploaded configuration files to {len(ip_to_files)} instances')

    def exp_setup(self, n, delta):
        import numpy as np
        # values = np.random.normal(loc=2300,scale=50,size=n)
        values = np.linspace(2200, 2200 + delta, num=n)
        arr_int = []
        for val in values:
            arr_int.append(int(val))
        return arr_int

    def _run_single(self, hosts, sleep_time, bench_parameters, debug=False):
        # Kill any potentially unfinished run and delete logs.
        self.kill(hosts=hosts, delete_logs=True)

        now = datetime.now()
        future = now + timedelta(seconds=sleep_time)
        st_time = int(future.timestamp() * 1000)

        # Prepare tasks for parallel execution
        tasks = []

        # Task for syncer (last host)
        syncer_ip = hosts[-1]
        syncer_cmd = CommandMaker.run_syncer(
            PathMaker.key_file(0),
            st_time,
            debug=debug
        )
        syncer_log = PathMaker.syncer_log_file()
        tasks.append((syncer_ip, syncer_cmd, syncer_log))

        # Tasks for primaries (all hosts except the last one)
        for i, ip in enumerate(hosts[:-1]):
            primary_cmd = CommandMaker.run_primary(
                PathMaker.key_file(i),
                st_time,
                debug=debug
            )
            primary_log = PathMaker.primary_log_file(i)
            tasks.append((ip, primary_cmd, primary_log))

        Print.info(f'Booting {len(tasks)} processes (1 syncer + {len(tasks) - 1} primaries)...')

        # Run all processes in parallel
        def run_task(ip, cmd, log_file):
            """Run a single background process on a host."""
            try:
                self._background_run(ip, cmd, log_file)
            except Exception as e:
                raise BenchError(f'Failed to start process on {ip}', e)

        with ThreadPoolExecutor(max_workers=10) as executor:
            futures = [executor.submit(run_task, ip, cmd, log_file) for ip, cmd, log_file in tasks]
            progress = progress_bar(futures, prefix='Starting processes:')
            for future in progress:
                future.result()  # Wait for each task to complete and raise any exceptions

        Print.info(f'Successfully started {len(tasks)} processes')

        # Wait for all transactions to be processed
        duration = bench_parameters.duration + sleep_time
        for _ in progress_bar(range(20), prefix=f'Running benchmark ({duration} sec):'):
            sleep(ceil(duration / 20))

        self.kill(hosts=hosts, delete_logs=False)

    def _logs(self, hosts, faults):
        # Delete local logs (if any).
        cmd = CommandMaker.clean_logs()
        subprocess.run([cmd], shell=True, stderr=subprocess.DEVNULL)
        """Download log files from remote instances in parallel."""
        # Prepare tasks for downloading logs
        tasks = []

        # Tasks for primaries (all hosts except the last one)
        primary_addresses = hosts[:-1]
        for i, ip in enumerate(primary_addresses):
            remote_log = PathMaker.primary_log_file(i)
            local_log = PathMaker.primary_log_file(i)
            tasks.append((ip, remote_log, local_log))

        # Task for syncer (last host)
        syncer_ip = hosts[-1]
        remote_syncer_log = PathMaker.syncer_log_file()
        local_syncer_log = PathMaker.syncer_log_file()
        tasks.append((syncer_ip, remote_syncer_log, local_syncer_log))

        Print.info(f'Downloading {len(tasks)} log files (1 syncer + {len(primary_addresses)} primaries)...')

        # Download logs in parallel
        def download_log(ip, remote_path, local_path):
            """Download a single log file from a remote instance."""
            try:
                c = Connection(ip, user='ubuntu', connect_kwargs=self.connect)
                c.get(remote_path, local=local_path)
            except Exception as e:
                raise BenchError(f'Failed to download log {remote_path} from {ip}', e)

        with ThreadPoolExecutor(max_workers=10) as executor:
            futures = [executor.submit(download_log, ip, remote_path, local_path) for ip, remote_path, local_path in
                       tasks]
            progress = progress_bar(futures, prefix='Downloading log files:')
            for future in progress:
                future.result()  # Wait for each task to complete and raise any exceptions

        Print.info(f'Successfully downloaded {len(tasks)} log files')

        # Parse logs and return the parser.
        Print.info('Parsing logs and computing performance...')
        return LogParser.process(PathMaker.logs_path(), faults=faults)

    def run(self, bench_parameters_dict, dkg_params, debug=False, update_bin = True, update_conf = True):
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

        host_str = " ".join(selected_hosts)

        if update_bin:
            # Update nodes.
            try:
                self._update(selected_hosts, bench_parameters.collocate)
            except (GroupException, ExecutionError) as e:
                e = FabricError(e) if isinstance(e, GroupException) else e
                raise BenchError('Failed to update nodes', e)

            # Create alias for the client and nodes binary.
            cmd = CommandMaker.alias_binaries(PathMaker.binary_path())
            subprocess.run([cmd], shell=True)


        if update_conf:
            # Generate the configuration files
            cmd = CommandMaker.generate_config_files_remote(self.BASE_PORT, self.RBC_BASE_PORT, self.DKG_BASE_PORT,
                                                            self.DRB_BASE_PORT, self.cl_bport, self.cl_rport,
                                                            bench_parameters.nodes,
                                                            dkg_params, bench_parameters.kappa, host_str)
            self._background_run_local(cmd, "err.log")
            time.sleep(2)
            # Upload all configuration files.
            try:
                self._config(selected_hosts[-1])
            except (subprocess.SubprocessError, GroupException) as e:
                e = FabricError(e) if isinstance(e, GroupException) else e
                raise BenchError('Failed to configure nodes', e)

        # Run benchmarks.
        n = bench_parameters.nodes
        k = bench_parameters.kappa
        d = dkg_params.trans_delay
        Print.heading(f'\nRunning {n} nodes (kappa: {k}, trans delay: {d})')

        for i in range(bench_parameters.runs):
            Print.heading(f'Run {i + 1}/{bench_parameters.runs}')
            try:
                self._run_single(
                    selected_hosts, 10, bench_parameters, debug
                )

                faults = bench_parameters.faults
                logger = self._logs(selected_hosts, faults)
                logger.print(PathMaker.result_file(
                    faults,
                    n,
                    k,
                    d
                ))
            except (subprocess.SubprocessError, GroupException, ParseError) as e:
                self.kill(hosts=selected_hosts)
                if isinstance(e, GroupException):
                    e = FabricError(e)
                Print.error(BenchError('Benchmark failed', e))
                continue


    def run_log(self, bench_parameters_dict, dkg_params, debug=False, update_bin = True, update_conf = True):
        assert isinstance(debug, bool)
        Print.heading('Starting remote benchmark')
        try:
            bench_parameters = BenchParameters(bench_parameters_dict)
            dkg_params = DKGParameters(dkg_params)
        except ConfigError as e:
            raise BenchError('Invalid nodes or bench parameters', e)
        selected_hosts = self._select_hosts(bench_parameters)

        faults = bench_parameters.faults
        n = bench_parameters.nodes
        k = bench_parameters.kappa
        d = dkg_params.trans_delay
        logger = self._logs(selected_hosts, faults)
        # logger = LogParser.process(PathMaker.logs_path(), faults=faults)
        logger.print(PathMaker.result_file(
            faults,
            n,
            k,
            d
        ))
