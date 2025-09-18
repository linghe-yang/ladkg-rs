# Copyright(C) Facebook, Inc. and its affiliates.
import os
import matplotlib.pyplot as plt
import matplotlib
from pathlib import Path
import numpy as np
from collections import defaultdict

from benchmark.aggregate import LogAggregator
from benchmark.config import PlotParameters


# @tick.FuncFormatter
# def default_major_formatter(x, pos):
#     if pos is None:
#         return
#     if x >= 1_000:
#         return f'{x/1000:.0f}k'
#     else:
#         return f'{x:.0f}'
#
#
# @tick.FuncFormatter
# def sec_major_formatter(x, pos):
#     if pos is None:
#         return
#     return f'{float(x)/1000:.1f}'
#
#
# @tick.FuncFormatter
# def mb_major_formatter(x, pos):
#     if pos is None:
#         return
#     return f'{x:,.0f}'


class PlotError(Exception):
    pass


# class Ploter:
#     def __init__(self, filenames):
#         if not filenames:
#             raise PlotError('No data to plot')
#
#         self.results = []
#         try:
#             for filename in filenames:
#                 with open(filename, 'r') as f:
#                     self.results += [f.read().replace(',', '')]
#         except OSError as e:
#             raise PlotError(f'Failed to load log files: {e}')
#
#     def _natural_keys(self, text):
#         def try_cast(text): return int(text) if text.isdigit() else text
#         return [try_cast(c) for c in split('(\d+)', text)]
#
#     def _tps(self, data):
#         values = findall(r' TPS: (\d+) \+/- (\d+)', data)
#         values = [(int(x), int(y)) for x, y in values]
#         return list(zip(*values))
#
#     def _latency(self, data, scale=1):
#         values = findall(r' Latency: (\d+) \+/- (\d+)', data)
#         values = [(float(x)/scale, float(y)/scale) for x, y in values]
#         return list(zip(*values))
#
#     def _variable(self, data):
#         return [int(x) for x in findall(r'Variable value: X=(\d+)', data)]
#
#     def _tps2bps(self, x):
#         data = self.results[0]
#         size = int(search(r'Transaction size: (\d+)', data).group(1))
#         return x * size / 10**6
#
#     def _bps2tps(self, x):
#         data = self.results[0]
#         size = int(search(r'Transaction size: (\d+)', data).group(1))
#         return x * 10**6 / size
#
#     def _plot(self, x_label, y_label, y_axis, z_axis, type):
#         plt.figure()
#         markers = cycle(['o', 'v', 's', 'p', 'D', 'P'])
#         self.results.sort(key=self._natural_keys, reverse=(type == 'tps'))
#         for result in self.results:
#             y_values, y_err = y_axis(result)
#             x_values = self._variable(result)
#             if len(y_values) != len(y_err) or len(y_err) != len(x_values):
#                 raise PlotError('Unequal number of x, y, and y_err values')
#
#             plt.errorbar(
#                 x_values, y_values, yerr=y_err, label=z_axis(result),
#                 linestyle='dotted', marker=next(markers), capsize=3
#             )
#
#         plt.legend(loc='lower center', bbox_to_anchor=(0.5, 1), ncol=3)
#         plt.xlim(xmin=0)
#         plt.ylim(bottom=0)
#         plt.xlabel(x_label, fontweight='bold')
#         plt.ylabel(y_label[0], fontweight='bold')
#         plt.xticks(weight='bold')
#         plt.yticks(weight='bold')
#         plt.grid()
#         ax = plt.gca()
#         ax.xaxis.set_major_formatter(default_major_formatter)
#         ax.yaxis.set_major_formatter(default_major_formatter)
#         if 'latency' in type:
#             ax.yaxis.set_major_formatter(sec_major_formatter)
#         if len(y_label) > 1:
#             secaxy = ax.secondary_yaxis(
#                 'right', functions=(self._tps2bps, self._bps2tps)
#             )
#             secaxy.set_ylabel(y_label[1])
#             secaxy.yaxis.set_major_formatter(mb_major_formatter)
#
#         for x in ['pdf', 'png']:
#             plt.savefig(PathMaker.plot_file(type, x), bbox_inches='tight')
#
#     @staticmethod
#     def nodes(data):
#         x = search(r'Committee size: (\d+)', data).group(1)
#         f = search(r'Faults: (\d+)', data).group(1)
#         faults = f'({f} faulty)' if f != '0' else ''
#         return f'{x} nodes {faults}'
#
#     @staticmethod
#     def workers(data):
#         x = search(r'Workers per node: (\d+)', data).group(1)
#         f = search(r'Faults: (\d+)', data).group(1)
#         faults = f'({f} faulty)' if f != '0' else ''
#         return f'{x} workers {faults}'
#
#     @staticmethod
#     def max_latency(data):
#         x = search(r'Max latency: (\d+)', data).group(1)
#         f = search(r'Faults: (\d+)', data).group(1)
#         faults = f'({f} faulty)' if f != '0' else ''
#         return f'Max latency: {float(x) / 1000:,.1f} s {faults}'
#
#     @classmethod
#     def plot_latency(cls, files, scalability):
#         assert isinstance(files, list)
#         assert all(isinstance(x, str) for x in files)
#         z_axis = cls.workers if scalability else cls.nodes
#         x_label = 'Throughput (tx/s)'
#         y_label = ['Latency (s)']
#         ploter = cls(files)
#         ploter._plot(x_label, y_label, ploter._latency, z_axis, 'latency')
#
#     @classmethod
#     def plot_tps(cls, files, scalability):
#         assert isinstance(files, list)
#         assert all(isinstance(x, str) for x in files)
#         z_axis = cls.max_latency
#         x_label = 'Workers per node' if scalability else 'Committee size'
#         y_label = ['Throughput (tx/s)', 'Throughput (MB/s)']
#         ploter = cls(files)
#         ploter._plot(x_label, y_label, ploter._tps, z_axis, 'tps')
#
#     @classmethod
#     def plot(cls, params_dict):
#         try:
#             params = PlotParameters(params_dict)
#         except PlotError as e:
#             raise PlotError('Invalid nodes or bench parameters', e)
#
#         # Aggregate the logs.
#         LogAggregator(params.faults).print()
#         #
#         # # Make the latency, tps, and robustness graphs.
#         # iterator = params.workers if params.scalability() else params.nodes
#         # latency_files, tps_files = [], []
#         # for f in params.faults:
#         #     for x in iterator:
#         #         latency_files += glob(
#         #             PathMaker.agg_file(
#         #                 'latency',
#         #                 f,
#         #                 x if not params.scalability() else params.nodes[0],
#         #                 x if params.scalability() else params.workers[0],
#         #                 params.collocate,
#         #                 'any',
#         #                 params.tx_size,
#         #             )
#         #         )
#         #
#         #     for l in params.max_latency:
#         #         tps_files += glob(
#         #             PathMaker.agg_file(
#         #                 'tps',
#         #                 f,
#         #                 'x' if not params.scalability() else params.nodes[0],
#         #                 'x' if params.scalability() else params.workers[0],
#         #                 params.collocate,
#         #                 'any',
#         #                 params.tx_size,
#         #                 max_latency=l
#         #             )
#         #         )
#         #
#         # cls.plot_latency(latency_files, params.scalability())
#         # cls.plot_tps(tps_files, params.scalability())


class Plotter:

    def __init__(self, results):
        self.results = results
        self.plot_dir = "./plots"
        # Create plots directory if it doesn't exist
        Path(self.plot_dir).mkdir(exist_ok=True)
        # Set IEEE journal style with DejaVu Serif as fallback
        plt.style.use('classic')
        matplotlib.rcParams.update({
            'font.family': 'DejaVu Serif',  # Fallback for Times New Roman
            'font.size': 10,
            'axes.labelsize': 5,  # Smaller axis labels
            'xtick.labelsize': 5,  # Smaller tick labels
            'ytick.labelsize': 5,  # Smaller tick labels
            'legend.fontsize': 6,  # Smaller legend font
            'lines.linewidth': 1.5,
            'figure.figsize': (3.5, 2.5),  # IEEE single-column width
            'axes.grid': True,
            'grid.linestyle': ':',  # Denser dotted grid
            'grid.alpha': 0.5,  # Slightly darker gray grid
            'grid.color': '#b0b0b0',  # Darker gray
            'axes.linewidth': 0.5,  # Thinner axes
            'xtick.direction': 'out',  # Ticks outward
            'ytick.direction': 'out',  # Ticks outward
            'xtick.major.size': 3,  # Shorter tick length
            'ytick.major.size': 3,  # Shorter tick length
        })

    def plot_stacked_bar(self, faults, kappa):
        """Generate stacked bar plot for specified faults and kappa."""
        # Filter results by faults and kappa
        filtered_results = [r for r in self.results if r.faults == faults and r.kappa == kappa]
        if not filtered_results:
            raise PlotError(f"No results found for faults={faults} and kappa={kappa}")

        # Group by committee_size
        committee_groups = defaultdict(list)
        for result in filtered_results:
            committee_groups[result.committee_size].append(result)

        for committee_size, results in committee_groups.items():
            # Sort by transaction_waiting_time for x-axis
            results = sorted(results, key=lambda x: x.transaction_waiting_time)
            waiting_times = [r.transaction_waiting_time for r in results]
            sharing = [r.sharing_phase for r in results]
            reply = [r.reply_phase for r in results]
            transcript_comp = [r.transcript_computation for r in results]
            acs = [r.acs_phase for r in results]
            other = [r.transcript_verification + r.reconstruct_phase for r in results]

            fig, ax = plt.subplots()
            x = np.arange(len(waiting_times))
            # Adaptive bar width to fill figure, max 80% of available space
            bar_width = min(0.9 / len(waiting_times), 0.15)

            # Less saturated colors
            colors = ['#4a90e2', '#ff9f43', '#4caf50', '#e57373', '#b39ddb']

            # Stack the bars with thinner edges
            ax.set_axisbelow(True)
            ax.bar(x, sharing, bar_width, label='Sharing', color=colors[0], edgecolor='black', linewidth=0.3)
            ax.bar(x, reply, bar_width, bottom=sharing, label='Reply', color=colors[1], edgecolor='black', linewidth=0.3)
            ax.bar(x, transcript_comp, bar_width, bottom=np.array(sharing) + np.array(reply),
                   label='Trans. Gen', color=colors[2], edgecolor='black', linewidth=0.3)
            ax.bar(x, acs, bar_width, bottom=np.array(sharing) + np.array(reply) + np.array(transcript_comp),
                   label='AACS', color=colors[3], edgecolor='black', linewidth=0.3)
            ax.bar(x, other, bar_width, bottom=np.array(sharing) + np.array(reply) + np.array(transcript_comp) + np.array(acs),
                   label='Trans. Veri.', color=colors[4], edgecolor='black', linewidth=0.3)

            # Customize plot
            ax.set_xlabel(f'n={committee_size}   Trans. waiting time (ms)')
            ax.set_ylabel('Duration (ms)')
            ax.set_xticks(x)
            ax.set_xticklabels(waiting_times, rotation=45)
            ax.legend(loc='upper center', bbox_to_anchor=(0.5, 1.25), ncol=3, frameon=False)
            ax.yaxis.grid(True)

            # Adjust layout to make space for legend
            plt.subplots_adjust(top=0.8)
            plt.tight_layout()

            # Save plot
            output_path = os.path.join(self.plot_dir, f'stacked_bar_f{faults}_k{kappa}_c{committee_size}.pdf')
            try:
                plt.savefig(output_path, format='pdf', bbox_inches='tight')
                plt.close(fig)
            except Exception as e:
                raise PlotError(f"Failed to save stacked bar plot: {str(e)}")

    def plot_line(self, faults, committee_size):
        """Generate line plot for ACS phase vs kappa for specified faults and committee_size."""
        # Filter results by faults and committee_size
        filtered_results = [r for r in self.results if r.faults == faults and r.committee_size == committee_size]
        if not filtered_results:
            raise PlotError(f"No results found for faults={faults} and committee_size={committee_size}")

        # Group by kappa
        kappa_groups = defaultdict(list)
        for result in filtered_results:
            kappa_groups[result.kappa].append(result)

        # Sort kappas for x-axis
        kappas = sorted(kappa_groups.keys())
        acs_values = []
        for kappa in kappas:
            # Average ACS phase for same kappa
            acs_avg = np.mean([r.acs_phase for r in kappa_groups[kappa]])
            acs_values.append(acs_avg)

        fig, ax = plt.subplots()
        # Plot with warm color and 'x' marker
        ax.plot(kappas, acs_values, marker='x', linestyle='-', color='#e67e22')

        # Customize plot
        ax.set_xlabel('kappa')
        ax.set_ylabel('AACS Duration (ms)')
        ax.set_xticks(kappas)
        ax.yaxis.grid(True)

        # Add padding to axes and ensure y-axis max is above max data point
        ax.margins(x=0.1)  # 10% padding on x-axis
        y_max = max(acs_values)
        ax.set_ylim(bottom=min(acs_values) * 0.9, top=y_max * 1.1)

        # Adjust layout
        plt.tight_layout()

        # Save plot
        output_path = os.path.join(self.plot_dir, f'line_acs_f{faults}_c{committee_size}.pdf')
        try:
            plt.savefig(output_path, format='pdf', bbox_inches='tight')
            plt.close(fig)
        except Exception as e:
            raise PlotError(f"Failed to save line plot: {str(e)}")

    def generate_plots(self, faults, kappa=None, committee_size=None):
        """Generate specified plots based on provided parameters."""
        try:
            if kappa is not None:
                self.plot_stacked_bar(faults, kappa)
            if committee_size is not None:
                self.plot_line(faults, committee_size)
        except Exception as e:
            raise PlotError(f"Error generating plots: {str(e)}")

    @classmethod
    def plot(cls, params_dict):
        try:
            params = PlotParameters(params_dict)
        except PlotError as e:
            raise PlotError('Invalid nodes or bench parameters', e)

        # Aggregate the logs.
        aggr = LogAggregator(params.faults)
        Plotter(aggr.results).generate_plots(params.faults,params.kappa, params.nodes[0])


        #
        # # Make the latency, tps, and robustness graphs.
        # iterator = params.workers if params.scalability() else params.nodes
        # latency_files, tps_files = [], []
        # for f in params.faults:
        #     for x in iterator:
        #         latency_files += glob(
        #             PathMaker.agg_file(
        #                 'latency',
        #                 f,
        #                 x if not params.scalability() else params.nodes[0],
        #                 x if params.scalability() else params.workers[0],
        #                 params.collocate,
        #                 'any',
        #                 params.tx_size,
        #             )
        #         )
        #
        #     for l in params.max_latency:
        #         tps_files += glob(
        #             PathMaker.agg_file(
        #                 'tps',
        #                 f,
        #                 'x' if not params.scalability() else params.nodes[0],
        #                 'x' if params.scalability() else params.workers[0],
        #                 params.collocate,
        #                 'any',
        #                 params.tx_size,
        #                 max_latency=l
        #             )
        #         )
        #
        # cls.plot_latency(latency_files, params.scalability())
        # cls.plot_tps(tps_files, params.scalability())