# Copyright(C) Facebook, Inc. and its affiliates.
from os.path import join

from benchmark.utils import PathMaker


class CommandMaker:

    @staticmethod
    def cleanup():
        return (
            f'rm -r .db-* ; rm -rf ./configs ; mkdir ./configs ; mkdir -p {PathMaker.results_path()}'
        )

    @staticmethod
    def clean_logs():
        return f'rm -rf {PathMaker.logs_path()} ; mkdir -p {PathMaker.logs_path()}'

    @staticmethod
    def compile():
        return 'cargo build --quiet --release'

    @staticmethod
    def compile_remote():
        return 'cargo build --quiet --release --features remote'

    @staticmethod
    def generate_key(filename):
        assert isinstance(filename, str)
        return f'./node generate_keys --filename {filename}'

    @staticmethod
    def generate_config_files(bport, rbc_bport, dkg_bport, drb_bport, client_bport, client_run_port, num_nodes, params,
                              kappa):
        return (
            f'./genconfig --blocksize 100 --delay 100 --base_port {bport} --rbc_base_port {rbc_bport} --dkg_base_port {dkg_bport} --drb_base_port {drb_bport} --client_base_port {client_bport} '
            f'--NumNodes {num_nodes} --target ./configs --client_run_port {client_run_port} --local true --kappa {kappa} --delta {params.delta} --epsilon {params.epsilon} --tri {params.tri} --expo 2 --hashrand_batch {params.hr_batch} --hashrand_freq {params.hr_freq} --trans_delay {params.trans_delay}'
        )

    @staticmethod
    def generate_config_files_remote(bport, rbc_bport, dkg_bport, drb_bport, client_bport, client_run_port, num_nodes,
                                     params,
                                     kappa, remote_ips):
        return (
            f'./genconfig --blocksize 100 --delay 100 --base_port {bport} --rbc_base_port {rbc_bport} --dkg_base_port {dkg_bport} --drb_base_port {drb_bport} --client_base_port {client_bport} '
            f'--NumNodes {num_nodes} --target ./configs --client_run_port {client_run_port} --local true --kappa {kappa} --delta {params.delta} --epsilon {params.epsilon} --tri {params.tri} --expo 2 --hashrand_batch {params.hr_batch} --hashrand_freq {params.hr_freq} --trans_delay {params.trans_delay} ----remote_ips "{remote_ips}"'
        )

    @staticmethod
    def run_primary(conf, delay, debug=False):
        assert isinstance(conf, str)
        assert isinstance(debug, bool)
        # v = '-vvv' if debug else '-vv'
        return (f'./node --config {conf} '
                f'--sleep {delay} --batch 100 --vsstype dkg --syncer ./configs/syncer --rand 10000')

    @staticmethod
    def unzip_tkeys(fileloc, dir, debug=False):
        return (f'tar -xvzf {fileloc} && cp {dir}/* .')

    @staticmethod
    def run_syncer(conf, delay, debug=False):
        assert isinstance(conf, str)
        assert isinstance(debug, bool)
        # v = '-vvv' if debug else '-vv'
        return (f'./node --config {conf} '
                f'--sleep {delay} --batch 100 --vsstype sync --syncer ./configs/syncer --rand 10000')

    @staticmethod
    def run_worker(keys, committee, store, parameters, id, debug=False):
        assert isinstance(keys, str)
        assert isinstance(committee, str)
        assert isinstance(parameters, str)
        assert isinstance(debug, bool)
        v = '-vvv' if debug else '-vv'
        return (f'./node {v} run --keys {keys} --committee {committee} '
                f'--store {store} --parameters {parameters} worker --id {id}')

    @staticmethod
    def run_client(address, size, rate, nodes):
        assert isinstance(address, str)
        assert isinstance(size, int) and size > 0
        assert isinstance(rate, int) and rate >= 0
        assert isinstance(nodes, list)
        assert all(isinstance(x, str) for x in nodes)
        nodes = f'--nodes {" ".join(nodes)}' if nodes else ''
        return f'./benchmark_client {address} --size {size} --rate {rate} {nodes}'

    @staticmethod
    def kill():
        return 'tmux kill-server'

    @staticmethod
    def alias_binaries(origin):
        assert isinstance(origin, str)
        node, genconfig = join(origin, 'node'), join(origin, 'genconfig')
        return f'rm node ; rm genconfig ; ln -s {node} . ; ln -s {genconfig} .'
