# Copyright(C) Facebook, Inc. and its affiliates.
import subprocess
from math import ceil
from os.path import basename, splitext
from random import random
from time import sleep

from benchmark.commands import CommandMaker
from benchmark.config import Key, LocalCommittee, NodeParameters, BenchParameters, ConfigError, DKGParameters
from benchmark.logs import LogParser, ParseError
from benchmark.utils import Print, BenchError, PathMaker


class LocalBench:
    BASE_PORT = 6000
    RBC_BASE_PORT = 6500
    DKG_BASE_PORT = 7000
    DRB_BASE_PORT = 7500
    cl_bport = 10000
    cl_rport = 15000

    def __init__(self, bench_parameters_dict, dkg_params):
        try:
            self.bench_parameters = BenchParameters(bench_parameters_dict)
            self.dkg_params = DKGParameters(dkg_params)
        except ConfigError as e:
            raise BenchError('Invalid nodes or bench parameters', e)

    def __getattr__(self, attr):
        return getattr(self.bench_parameters, attr)

    def _background_run(self, command, log_file):
        name = splitext(basename(log_file))[0]
        cmd = f'{command} 2> {log_file}'
        # print("Command running: {}", command)
        # print(log_file)
        try:
            subprocess.run(['tmux', 'new', '-d', '-s', name, cmd], check=True)
        except subprocess.SubprocessError as e:
            raise BenchError('Failed to kill testbed', e)

    def _kill_nodes(self):
        try:
            cmd = CommandMaker.kill().split()
            subprocess.run(cmd, stderr=subprocess.DEVNULL)
        except subprocess.SubprocessError as e:
            raise BenchError('Failed to kill testbed', e)

    def run(self, debug=False):
        assert isinstance(debug, bool)
        Print.heading('Starting local benchmark')

        # Kill any previous testbed.
        self._kill_nodes()

        try:
            Print.info('Setting up testbed...')
            nodes = self.nodes[0]

            # Cleanup all files.
            cmd = f'{CommandMaker.clean_logs()} ; {CommandMaker.cleanup()}'
            subprocess.run([cmd], shell=True, stderr=subprocess.DEVNULL)
            sleep(0.5)  # Removing the store may take time.

            # Recompile the latest code.
            cmd = CommandMaker.compile().split()
            subprocess.run(cmd, check=True, cwd=PathMaker.node_crate_path())

            # Create alias for the client and nodes binary.
            cmd = CommandMaker.alias_binaries(PathMaker.binary_path())
            subprocess.run([cmd], shell=True)


            # Generate the configuration files
            cmd = CommandMaker.generate_config_files(self.BASE_PORT, self.RBC_BASE_PORT, self.DKG_BASE_PORT,
                                                     self.DRB_BASE_PORT, self.cl_bport, self.cl_rport, nodes, self.dkg_params)
            self._background_run(cmd, "err.log")

            sleep(2)

            st_time = 1000
            # Run the syncer .
            cmd = CommandMaker.run_syncer(
                PathMaker.key_file(0),
                st_time,
                debug=debug
            )
            log_file = PathMaker.syncer_log_file()
            self._background_run(cmd, log_file)

            # Run the primaries .
            for i in range(nodes):
                cmd = CommandMaker.run_primary(
                    PathMaker.key_file(i),
                    st_time,
                    debug=debug
                )
                log_file = PathMaker.primary_log_file(i)
                self._background_run(cmd, log_file)

            Print.info(f'Running benchmark ({self.duration} sec)...')
            sleep(self.duration)
            self._kill_nodes()

            # Parse logs and return the parser.
            Print.info('Parsing logs...')
            return LogParser.process(PathMaker.logs_path(), faults=self.faults)



            # # Run the workers (except the faulty ones).
            # for i, addresses in enumerate(workers_addresses):
            #     for (id, address) in addresses:
            #         cmd = CommandMaker.run_worker(
            #             PathMaker.key_file(i),
            #             PathMaker.committee_file(),
            #             PathMaker.db_path(i, id),
            #             PathMaker.parameters_file(),
            #             id,  # The worker's id.
            #             debug=debug
            #         )
            #         log_file = PathMaker.worker_log_file(i, id)
            #         self._background_run(cmd, log_file)

            # # Wait for all transactions to be processed.

            # sleep(self.duration)
            # self._kill_nodes()



        except (subprocess.SubprocessError, ParseError) as e:
            self._kill_nodes()
            raise BenchError('Failed to run benchmark', e)
