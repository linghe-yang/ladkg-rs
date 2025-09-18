# Copyright(C) Facebook, Inc. and its affiliates.
from datetime import datetime
from glob import glob
from multiprocessing import Pool
from os.path import join
from re import findall, search
from statistics import mean

from benchmark.utils import Print


class ParseError(Exception):
    pass


class LogParser:
    def __init__(self, syncer, primaries, faults=0):
        inputs = [syncer, primaries]
        assert all(isinstance(x, list) for x in inputs)
        assert all(isinstance(x, str) for y in inputs for x in y)
        assert all(x for x in inputs)

        self.faults = faults
        if isinstance(faults, int):
            self.committee_size = len(primaries) + int(faults)
        else:
            self.committee_size = '?'

        # Parse syncer log
        if not syncer:
            raise ParseError("Syncer log list is empty")
        try:
            self._parse_syncer(syncer[0])
        except ParseError as e:
            raise e
        except (ValueError, IndexError, AttributeError) as e:
            raise ParseError(f'Failed to parse syncer log: {e}')

        try:
            with Pool() as p:
                results_primary = p.map(self._parse_primaries, primaries)
            averages = self._compute_primary_averages(results_primary)
            self.averages = averages
        except (ValueError, IndexError, AttributeError) as e:
            raise ParseError(f'Failed to parse primary\' logs: {e}')

    def _parse_syncer(self, log):
        # Extract configuration values
        delta_match = search(r'delta: (\d+)', log)
        epsilon_match = search(r'epsilon: (\d+)', log)
        tri_match = search(r'tri: (\d+)', log)
        kappa_match = search(r'kappa: (\d+)', log)
        trans_waiting_time_match = search(r'trans_waiting_time: (\d+)', log)
        hashrand_batch_match = search(r'hashrand batch: (\d+)', log)
        hashrand_frequency_match = search(r'hashrand frequency: (\d+)', log)

        # Check if any field is missing
        missing_fields = []
        if delta_match is None:
            missing_fields.append("delta")
        if epsilon_match is None:
            missing_fields.append("epsilon")
        if tri_match is None:
            missing_fields.append("tri")
        if kappa_match is None:
            missing_fields.append("kappa")
        if trans_waiting_time_match is None:
            missing_fields.append("trans_waiting_time")
        if hashrand_batch_match is None:
            missing_fields.append("hashrand batch")
        if hashrand_frequency_match is None:
            missing_fields.append("hashrand frequency")

        if missing_fields:
            raise ParseError(f"Failed to parse syncer log: missing fields {', '.join(missing_fields)}")

        # Set instance attributes
        self.delta = int(delta_match.group(1))
        self.epsilon = int(epsilon_match.group(1))
        self.tri = int(tri_match.group(1))
        self.kappa = int(kappa_match.group(1))
        self.trans_waiting_time = int(trans_waiting_time_match.group(1))
        self.hashrand_batch = int(hashrand_batch_match.group(1))
        self.hashrand_frequency = int(hashrand_frequency_match.group(1))


    def _parse_primaries(self, log):
        # Extract ISO timestamps for phases
        sharing_start_match = search(r'\[(.*Z) .* start sharing phase', log)
        sharing_finish_match = search(r'\[(.*Z) .* finish sharing phase', log)
        transcript_start_match = search(r'\[(.*Z) .* Start generating transcript', log)
        transcript_generated_match = search(r'\[(.*Z) .* Transcript generated', log)
        acs_result_match = search(r'\[(.*Z) .* All Transcript in final set received, ACS result formed', log)
        secret_key_match = search(r'\[(.*Z) .* secret key formed', log)
        reconstruct_start_match = search(r'\[(.*Z) .* Start reconstructing threshold public key', log)
        reconstructed_pk_match = search(r'\[(.*Z) .* Threshold public key reconstructed', log)

        # Check for missing fields
        missing_fields = []
        if sharing_start_match is None:
            missing_fields.append("start sharing phase")
        if sharing_finish_match is None:
            missing_fields.append("finish sharing phase")
        if transcript_start_match is None:
            missing_fields.append("Start generating transcript")
        if transcript_generated_match is None:
            missing_fields.append("Transcript generated")
        if acs_result_match is None:
            missing_fields.append("All Transcript in final set received, ACS result formed")
        if secret_key_match is None:
            missing_fields.append("secret key formed")
        if reconstruct_start_match is None:
            missing_fields.append("Start reconstructing threshold public key")
        if reconstructed_pk_match is None:
            missing_fields.append("Threshold public key reconstructed")

        # Skip this log if any field is missing
        if missing_fields:
            return None

        # Convert timestamps to milliseconds
        sharing_start_time = int(self._to_posix(sharing_start_match.group(1)) * 1000)
        sharing_finish_time = int(self._to_posix(sharing_finish_match.group(1)) * 1000)
        transcript_start_time = int(self._to_posix(transcript_start_match.group(1)) * 1000)
        transcript_generated_time = int(self._to_posix(transcript_generated_match.group(1)) * 1000)
        acs_result_time = int(self._to_posix(acs_result_match.group(1)) * 1000)
        secret_key_time = int(self._to_posix(secret_key_match.group(1)) * 1000)
        reconstruct_start_time = int(self._to_posix(reconstruct_start_match.group(1)) * 1000)
        reconstruct_end_time = int(self._to_posix(reconstructed_pk_match.group(1)) * 1000)

        protocol_time = max(reconstruct_end_time - sharing_start_time, secret_key_time - sharing_start_time)

        # Calculate phase durations
        result = {
            "protocol_time": protocol_time,
            "sharing_phase": sharing_finish_time - sharing_start_time,
            "reply_phase": transcript_start_time - sharing_finish_time,
            "transcript_computation": transcript_generated_time - transcript_start_time,
            "acs_phase": acs_result_time - transcript_generated_time,
            "transcript_verification": secret_key_time - acs_result_time,
            "reconstruct_phase": reconstruct_start_time - secret_key_time
        }

        return result

    def _compute_primary_averages(self, results):
        # Filter out None results
        valid_results = [r for r in results if r is not None]

        # Check if there are valid results
        if not valid_results:
            raise ParseError("No valid primary logs to compute averages")

        # Compute average for each field
        averages = {
            "protocol_time": mean([r["protocol_time"] for r in valid_results]),
            "sharing_phase": mean([r["sharing_phase"] for r in valid_results]),
            "reply_phase": mean([r["reply_phase"] for r in valid_results]),
            "transcript_computation": mean([r["transcript_computation"] for r in valid_results]),
            "acs_phase": mean([r["acs_phase"] for r in valid_results]),
            "transcript_verification": mean([r["transcript_verification"] for r in valid_results]),
            "reconstruct_phase": mean([r["reconstruct_phase"] for r in valid_results])
        }

        return averages

    def _to_posix(self, string):
        x = datetime.fromisoformat(string.replace('Z', '+00:00'))
        return datetime.timestamp(x)

    def result(self):
        # Format the summary string
        return (
            '\n'
            '-----------------------------------------\n'
            ' SUMMARY:\n'
            '-----------------------------------------\n'
            ' + CONFIG:\n'
            f' Faults: {self.faults} node(s)\n'
            f' Committee size: {self.committee_size} node(s)\n'
            f' Delphi delta: {self.delta:,}\n'
            f' Delphi epsilon: {self.epsilon:,}\n'
            f' Delphi tri: {self.tri:,}\n'
            f' AACS kappa: {self.kappa:,}\n'
            f' DKG Transaction waiting time: {self.trans_waiting_time:,} ms\n'
            f' Hashrand batch: {self.hashrand_batch:,}\n'
            f' Hashrand frequency: {self.hashrand_frequency:,}\n'
            '\n'
            ' + RESULTS:\n'
            f' DKG overall time: {round(self.averages["protocol_time"]):,} ms\n'
            f' Sharing phase: {round(self.averages["sharing_phase"]):,} ms\n'
            f' Reply phase: {round(self.averages["reply_phase"]):,} ms\n'
            f' Transcript computation: {round(self.averages["transcript_computation"]):,} ms\n'
            f' ACS phase: {round(self.averages["acs_phase"]):,} ms\n'
            f' Transcript verification: {round(self.averages["transcript_verification"]):,} ms\n'
            f' Reconstruct phase: {round(self.averages["reconstruct_phase"]):,} ms\n'
            '-----------------------------------------\n'
        )

    def print(self, filename):
        assert isinstance(filename, str)
        with open(filename, 'a') as f:
            f.write(self.result())

    @classmethod
    def process(cls, directory, faults=0):
        assert isinstance(directory, str)

        syncer = []
        for filename in sorted(glob(join(directory, 'syncer.log'))):
            with open(filename, 'r') as f:
                syncer += [f.read()]
        primaries = []
        for filename in sorted(glob(join(directory, 'primary-*.log'))):
            with open(filename, 'r') as f:
                primaries += [f.read()]

        return cls(syncer, primaries, faults=faults)
