# Copyright(C) Facebook, Inc. and its affiliates.
from re import search
from collections import defaultdict
from statistics import mean, stdev
from glob import glob
from copy import deepcopy
from os.path import join
import os

from benchmark.utils import PathMaker

from pathlib import Path
import re


class Setup:
    def __init__(self, faults, nodes, workers, collocate, rate, tx_size):
        self.nodes = nodes
        self.workers = workers
        self.collocate = collocate
        self.rate = rate
        self.tx_size = tx_size
        self.faults = faults
        self.max_latency = 'any'

    def __str__(self):
        return (
            f' Faults: {self.faults}\n'
            f' Committee size: {self.nodes}\n'
            f' Workers per node: {self.workers}\n'
            f' Collocate primary and workers: {self.collocate}\n'
            f' Input rate: {self.rate} tx/s\n'
            f' Transaction size: {self.tx_size} B\n'
            f' Max latency: {self.max_latency} ms\n'
        )

    def __eq__(self, other):
        return isinstance(other, Setup) and str(self) == str(other)

    def __hash__(self):
        return hash(str(self))

    @classmethod
    def from_str(cls, raw):
        faults = int(search(r'Faults: (\d+)', raw).group(1))
        nodes = int(search(r'Committee size: (\d+)', raw).group(1))
        workers = int(search(r'Worker\(s\) per node: (\d+)', raw).group(1))
        collocate = 'True' == search(
            r'Collocate primary and workers: (True|False)', raw
        ).group(1)
        rate = int(search(r'Input rate: (\d+)', raw).group(1))
        tx_size = int(search(r'Transaction size: (\d+)', raw).group(1))
        return cls(faults, nodes, workers, collocate, rate, tx_size)


# class Result:
#     def __init__(self, mean_tps, mean_latency, std_tps=0, std_latency=0):
#         self.mean_tps = mean_tps
#         self.mean_latency = mean_latency
#         self.std_tps = std_tps
#         self.std_latency = std_latency
#
#     def __str__(self):
#         return(
#             f' TPS: {self.mean_tps} +/- {self.std_tps} tx/s\n'
#             f' Latency: {self.mean_latency} +/- {self.std_latency} ms\n'
#         )
#
#     @classmethod
#     def from_str(cls, raw):
#         tps = int(search(r'End-to-end TPS: (\d+)', raw).group(1))
#         latency = int(search(r'End-to-end latency: (\d+)', raw).group(1))
#         return cls(tps, latency)
#
#     @classmethod
#     def aggregate(cls, results):
#         if len(results) == 1:
#             return results[0]
#
#         mean_tps = round(mean([x.mean_tps for x in results]))
#         mean_latency = round(mean([x.mean_latency for x in results]))
#         std_tps = round(stdev([x.mean_tps for x in results]))
#         std_latency = round(stdev([x.mean_latency for x in results]))
#         return cls(mean_tps, mean_latency, std_tps, std_latency)


class Result:
    def __init__(self, faults, committee_size, kappa, transaction_waiting_time,
                 dkg_overall_time, sharing_phase, reply_phase, transcript_computation,
                 acs_phase, transcript_verification, reconstruct_phase):
        self.faults = faults
        self.committee_size = committee_size
        self.kappa = kappa
        self.transaction_waiting_time = transaction_waiting_time
        self.dkg_overall_time = dkg_overall_time
        self.sharing_phase = sharing_phase
        self.reply_phase = reply_phase
        self.transcript_computation = transcript_computation
        self.acs_phase = acs_phase
        self.transcript_verification = transcript_verification
        self.reconstruct_phase = reconstruct_phase


class LogAggregator:
    def __init__(self, faults):
        self.faults = faults
        self.results = self.aggregate_results()

    def print(self):
        for r in self.results:
            print(r.faults)
            print(r.committee_size)
            print(r.kappa)
            print(r.transaction_waiting_time)
            print(r.dkg_overall_time)
            print(r.sharing_phase)
            print(r.reply_phase)
            print(r.transcript_computation)
            print(r.acs_phase)
            print(r.transcript_verification)
            print(r.reconstruct_phase)


    def aggregate_results(self):
        # Dictionary to store results grouped by CONFIG key
        config_groups = {}

        # Regex to parse filename
        filename_pattern = re.compile(r'bench-(\d+)-(\d+)-(\d+)-(\d+)\.txt')

        # Read files matching the faults parameter
        for filename in glob(join(PathMaker.results_path(), f'bench-{self.faults}-*.txt')):
            match = filename_pattern.search(filename)
            if not match:
                continue
            faults, committee_size, waiting_time, kappa = map(int, match.groups())

            with open(filename, 'r') as f:
                data = f.read()

            # Split data into SUMMARY blocks
            summaries = data.split('-----------------------------------------')[:-1]
            for summary in summaries:
                if '+ CONFIG:' not in summary or '+ RESULTS:' not in summary:
                    continue

                # Extract CONFIG
                config_lines = summary.split('+ CONFIG:')[1].split('+ RESULTS:')[0].strip().split('\n')
                config = {}
                for line in config_lines:
                    if ':' in line:
                        key, value = line.split(':', 1)
                        config[key.strip()] = value.strip()

                # Extract RESULTS
                result_lines = summary.split('+ RESULTS:')[1].strip().split('\n')
                results = {}
                for line in result_lines:
                    if ':' in line:
                        key, value = line.split(':', 1)
                        results[key.strip()] = float(value.split()[0])  # Extract numeric value

                # Create config key for grouping
                config_key = (faults, committee_size, int(config['AACS kappa'].split()[0]),
                            int(config['DKG Transaction waiting time'].split()[0]))

                if config_key not in config_groups:
                    config_groups[config_key] = []
                config_groups[config_key].append(results)

        # Aggregate results by computing averages
        aggregated_results = []
        for config_key, result_list in config_groups.items():
            faults, committee_size, kappa, waiting_time = config_key
            n = len(result_list)
            if n == 0:
                continue

            # Compute averages
            avg_results = {}
            for key in result_list[0].keys():
                avg_results[key] = sum(r[key] for r in result_list) / n

            # Create Result object
            result = Result(
                faults=faults,
                committee_size=committee_size,
                kappa=kappa,
                transaction_waiting_time=waiting_time,
                dkg_overall_time=avg_results['DKG overall time'],
                sharing_phase=avg_results['Sharing phase'],
                reply_phase=avg_results['Reply phase'],
                transcript_computation=avg_results['Transcript computation'],
                acs_phase=avg_results['ACS phase'],
                transcript_verification=avg_results['Transcript verification'],
                reconstruct_phase=avg_results['Reconstruct phase']
            )
            aggregated_results.append(result)

        return aggregated_results