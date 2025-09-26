# Copyright(C) Facebook, Inc. and its affiliates.
import os
import matplotlib.pyplot as plt
import matplotlib
from pathlib import Path
import numpy as np
from collections import defaultdict
from glob import glob
from os.path import join

from benchmark.aggregate import LogAggregator
from benchmark.config import PlotParameters

class PlotError(Exception):
    pass

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

    def plot_vss_bar(self, faults, kappa):
        """Generate VSS stacked bar plot for specified faults and kappa, excluding AACS."""
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
            other = [r.transcript_verification + r.reconstruct_phase for r in results]

            fig, ax = plt.subplots()
            x = np.arange(len(waiting_times))
            # Adaptive bar width to fill figure, max 90% of available space
            bar_width = min(1.5 / len(waiting_times), 0.4)

            # Less saturated colors (excluding AACS color)
            colors = ['#4a90e2', '#ff9f43', '#4caf50', '#b39ddb']

            # Set grid below bars
            ax.set_axisbelow(True)
            # Stack the bars with thinner edges
            ax.bar(x, sharing, bar_width, label='Sharing', color=colors[0], edgecolor='black', linewidth=0.3)
            ax.bar(x, reply, bar_width, bottom=sharing, label='Reply', color=colors[1], edgecolor='black', linewidth=0.3)
            ax.bar(x, transcript_comp, bar_width, bottom=np.array(sharing) + np.array(reply),
                   label='Trans. Gen', color=colors[2], edgecolor='black', linewidth=0.3)
            ax.bar(x, other, bar_width, bottom=np.array(sharing) + np.array(reply) + np.array(transcript_comp),
                   label='Trans. Veri.', color=colors[3], edgecolor='black', linewidth=0.3)

            # Customize plot
            ax.set_xlabel(f'n={committee_size}   Trans. waiting time (ms)')
            ax.set_ylabel('Duration (ms)')
            ax.set_xticks(x)
            ax.set_xticklabels(waiting_times, rotation=45)
            ax.legend(loc='upper center', bbox_to_anchor=(0.5, 1.25), ncol=2, frameon=False)
            ax.yaxis.grid(True)

            # Adjust layout to make space for legend
            plt.subplots_adjust(top=0.8)
            plt.tight_layout()

            # Save plot
            output_path = os.path.join(self.plot_dir, f'vss_bar_f{faults}_k{kappa}_c{committee_size}.png')
            try:
                plt.savefig(output_path, format='png', dpi=400, bbox_inches='tight')
                plt.close(fig)
            except Exception as e:
                raise PlotError(f"Failed to save vss bar plot: {str(e)}")

    def plot_aacs_bar(self, faults, kappa):
        """Generate bar plot for AACS phase for specified faults and kappa."""
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
            acs = [r.acs_phase for r in results]

            fig, ax = plt.subplots()
            x = np.arange(len(waiting_times))
            # Adaptive bar width to fill figure, max 90% of available space
            bar_width = min(1.5 / len(waiting_times), 0.4)

            # Use AACS color
            ax.set_axisbelow(True)
            ax.bar(x, acs, bar_width, label='AACS', color='#e57373', edgecolor='black', linewidth=0.3)

            # Customize plot
            ax.set_xlabel(f'n={committee_size}   Trans. waiting time (ms)')
            ax.set_ylabel('AACS Duration (ms)')
            ax.set_xticks(x)
            ax.set_xticklabels(waiting_times, rotation=45)
            ax.legend(loc='upper center', bbox_to_anchor=(0.5, 1.25), ncol=1, frameon=False)
            ax.yaxis.grid(True)

            # Adjust layout to make space for legend
            plt.subplots_adjust(top=0.8)
            plt.tight_layout()

            # Save plot
            output_path = os.path.join(self.plot_dir, f'aacs_bar_f{faults}_k{kappa}_c{committee_size}.png')
            try:
                plt.savefig(output_path, format='png', dpi=400, bbox_inches='tight')
                plt.close(fig)
            except Exception as e:
                raise PlotError(f"Failed to save AACS bar plot: {str(e)}")

    def plot_aacs_kappa_line(self, faults, committee_size):
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
        # Plot with warm color and black 'x' marker
        ax.plot(kappas, acs_values, marker='x', linestyle='-', color='#e67e22', markeredgecolor='black')

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
        output_path = os.path.join(self.plot_dir, f'aacs_kappa_f{faults}_c{committee_size}.png')
        try:
            plt.savefig(output_path, format='png', dpi=400, bbox_inches='tight')
            plt.close(fig)
        except Exception as e:
            raise PlotError(f"Failed to save line plot: {str(e)}")

    def plot_dkg_comparison(self, faults, transaction_waiting_time):
        """Generate line plot comparing DKG overall time across works."""
        # Filter our work's results by faults, and transaction_waiting_time
        our_results = [r for r in self.results if r.faults == faults
                      and r.transaction_waiting_time == transaction_waiting_time]
        if not our_results:
            raise PlotError(f"No results found for faults={faults}, transaction_waiting_time={transaction_waiting_time}")

        # Extract nodes and DKG overall time for our work
        our_results = sorted(our_results, key=lambda x: x.committee_size)
        our_nodes = [r.committee_size for r in our_results]
        our_times = [r.dkg_overall_time for r in our_results]

        # Read other-*.txt files
        other_data = {}
        other_files = glob(join('results', 'other-*.txt'))
        for filename in other_files:
            # Extract work name from filename (e.g., 'adkg' from 'other-adkg.txt')
            work_name = os.path.basename(filename).replace('other-', '').replace('.txt', '')
            nodes = []
            times = []
            with open(filename, 'r') as f:
                lines = f.readlines()
                i = 0
                while i < len(lines):
                    if lines[i].startswith('X ='):
                        try:
                            x = int(lines[i].split('=')[1].strip())
                            # Check if next line exists and starts with 'Y ='
                            if i + 1 < len(lines) and lines[i + 1].startswith('Y ='):
                                y = float(lines[i + 1].split('=')[1].strip())
                                nodes.append(x)
                                times.append(y)
                                i += 2  # Skip both X and Y lines
                            else:
                                i += 1
                        except (ValueError, IndexError):
                            i += 1
                            continue
                    else:
                        i += 1
            # Sort by nodes for consistent plotting
            sorted_pairs = sorted(zip(nodes, times), key=lambda x: x[0])
            if sorted_pairs:  # Only add if data is valid
                nodes, times = zip(*sorted_pairs)
                other_data[work_name] = (list(nodes), list(times))

        if not our_results and not other_data:
            raise PlotError("No valid data found for plotting DKG comparison")

        fig, ax = plt.subplots()
        # Plot our work with black 'x' marker
        ax.plot(our_nodes, our_times, marker='x', linestyle='-', color='#e67e22', markeredgecolor='black', label='Our Work')

        # Plot other works with different colors and black 'x' markers
        colors = ['#4a90e2', '#ff9f43', '#4caf50', '#b39ddb', '#e57373']
        for idx, (work_name, (nodes, times)) in enumerate(other_data.items()):
            color = colors[idx % len(colors)]
            ax.plot(nodes, times, marker='x', linestyle='-', color=color, markeredgecolor='black', label=work_name)

        # Customize plot
        ax.set_xlabel('Nodes')
        ax.set_ylabel('DKG Overall Time (ms)')
        # Set fixed x-ticks starting from 0 with step size 20
        all_nodes = sorted(set(our_nodes + [n for nodes, _ in other_data.values() for n in nodes]))
        max_nodes = max(all_nodes) if all_nodes else 100
        x_ticks = np.arange(0, max_nodes + 20, 20)
        ax.set_xticks(x_ticks)
        ax.yaxis.grid(True)

        # Add padding to y-axis
        all_times = our_times + [t for _, times in other_data.values() for t in times]
        y_max = max(all_times) if all_times else 1
        y_min = min(all_times) if all_times else 0
        ax.set_ylim(bottom=y_min * 0.9, top=y_max * 1.1)

        # Place legend above the plot
        ax.legend(loc='upper center', bbox_to_anchor=(0.5, 1.25), ncol=3, frameon=False)

        # Adjust layout to make space for legend
        plt.subplots_adjust(top=0.8)
        plt.tight_layout()

        # Save plot
        output_path = os.path.join(self.plot_dir, f'dkg_comparison_f{faults}_t{transaction_waiting_time}.png')
        try:
            plt.savefig(output_path, format='png', dpi=400, bbox_inches='tight')
            plt.close(fig)
        except Exception as e:
            raise PlotError(f"Failed to save DKG comparison plot: {str(e)}")

    def generate_plots(self, faults, kappa=None, committee_size=None, td=None):
        """Generate specified plots based on provided parameters."""
        try:
            if kappa is not None:
                self.plot_vss_bar(faults, kappa)
                self.plot_aacs_bar(faults, kappa)
            if committee_size is not None:
                self.plot_aacs_kappa_line(faults, committee_size)
            if td is not None:
                self.plot_dkg_comparison(faults, td)
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
        Plotter(aggr.results).generate_plots(params.faults, params.kappa, params.nodes[0], params.td)