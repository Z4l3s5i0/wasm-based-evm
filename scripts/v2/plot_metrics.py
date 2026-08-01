import json
import pandas as pd
import matplotlib.pyplot as plt
import sys
import os

def main():
    if len(sys.argv) < 2:
        print("Usage: python plot_metrics.py <experiment_analysis_dir>")
        sys.exit(1)

    analysis_dir = sys.argv[1]
    samples_file = os.path.join(analysis_dir, "metrics_metric_samples.json")
    nodes_file = os.path.join(analysis_dir, "metrics_nodes.json")

    if not os.path.exists(samples_file):
        print(f"File not found: {samples_file}")
        sys.exit(1)

    print(f"Loading data from {samples_file}...")
    with open(samples_file, 'r') as f:
        data = json.load(f)

    # Convert to flat list of values
    rows = [item['value'] for item in data]
    df = pd.DataFrame(rows)

    if df.empty:
        print("No data found in samples file.")
        sys.exit(0)

    # Normalize timestamps: start from 0 and convert to seconds
    min_ts = df['timestamp_ms'].min()
    df['time_sec'] = (df['timestamp_ms'] - min_ts) / 1000.0

    # Shorten node IDs for better legend
    df['node_label'] = df['node_id'].str[:8]

    # Filter metrics
    tx_df = df[df['name'] == 'transactions_committed_total'].copy()
    mempool_df = df[df['name'] == 'mempool_size'].copy()
    rejected_df = df[df['name'] == 'mempool_rejected_transactions_total'].copy()
    sync_df = df[df['name'] == 'ethereum_syncing'].copy()
    blocks_df = df[df['name'] == 'current_head_block'].copy()
    if blocks_df.empty:
        blocks_df = df[df['name'] == 'blocks_imported_total'].copy()

    # 1. Graph: transactions_committed_total per node
    plt.figure(figsize=(12, 6))
    nodes = df['node_id'].unique()
    colors = plt.cm.tab10.colors
    for i, node in enumerate(nodes):
        label = node[:8]
        c = colors[i % len(colors)]
        
        node_tx = tx_df[tx_df['node_id'] == node].sort_values('time_sec')
        if not node_tx.empty:
            plt.plot(node_tx['time_sec'], node_tx['value'], color=c, linestyle='-', label=f"Node {label} TX")

    plt.title("Transactions Committed per Node")
    plt.xlabel("Time (seconds)")
    plt.ylabel("Transactions")
    plt.legend(loc='upper left', bbox_to_anchor=(1.05, 1), borderaxespad=0.)
    plt.tight_layout()
    plt.grid(True)
    tx_plot_path = os.path.join(analysis_dir, "transactions_committed.png")
    plt.savefig(tx_plot_path)
    plt.close()
    print(f"Saved transactions plot to {tx_plot_path}")

    # 2. Graph: transactions_committed_total and mempool_size per node
    fig, ax1 = plt.subplots(figsize=(12, 8))

    ax2 = ax1.twinx()  # instantiate a second axes that shares the same x-axis

    for i, node in enumerate(nodes):
        label = node[:8]
        c = colors[i % len(colors)]
        
        # Plot TX Committed
        node_tx = tx_df[tx_df['node_id'] == node].sort_values('time_sec')
        if not node_tx.empty:
            ax1.plot(node_tx['time_sec'], node_tx['value'], color=c, linestyle='-', label=f"Node {label} TX")

        # Plot Mempool Size
        node_mp = mempool_df[mempool_df['node_id'] == node].sort_values('time_sec')
        if not node_mp.empty:
            ax2.plot(node_mp['time_sec'], node_mp['value'], color=c, linestyle='--', alpha=0.6, label=f"Node {label} Mempool")

    ax1.set_xlabel('Time (seconds)')
    ax1.set_ylabel('Transactions (Solid)', color='black')
    ax2.set_ylabel('Mempool Size (Dashed)', color='gray')
    
    plt.title("Transactions Committed and Mempool Size per Node")
    
    # Combined legend
    lines1, labels1 = ax1.get_legend_handles_labels()
    lines2, labels2 = ax2.get_legend_handles_labels()
    ax1.legend(lines1 + lines2, labels1 + labels2, loc='upper left', bbox_to_anchor=(1.05, 1), borderaxespad=0.)
    
    plt.tight_layout()
    plt.grid(True)
    combined_plot_path = os.path.join(analysis_dir, "transactions_and_mempool.png")
    plt.savefig(combined_plot_path)
    plt.close()
    print(f"Saved combined plot to {combined_plot_path}")

    # 3. Graph: Side-by-side subplots for Transactions, Mempool, Rejected, Blocks and Sync
    fig, (ax1, ax2, ax3, ax4, ax5) = plt.subplots(5, 1, figsize=(12, 18), sharex=True)

    # Determine when spamming starts (first time any node has mempool > 0)
    spam_start_time = None
    if not mempool_df.empty:
        spamming = mempool_df[mempool_df['value'] > 0].sort_values('time_sec')
        if not spamming.empty:
            spam_start_time = spamming['time_sec'].iloc[0]

    for i, node in enumerate(nodes):
        label = node[:8]
        c = colors[i % len(colors)]
        
        # Plot TX Committed on ax1
        node_tx = tx_df[tx_df['node_id'] == node].sort_values('time_sec')
        if not node_tx.empty:
            ax1.plot(node_tx['time_sec'], node_tx['value'], color=c, linestyle='-', label=f"Node {label}")

        # Plot Mempool Size on ax2
        node_mp = mempool_df[mempool_df['node_id'] == node].sort_values('time_sec')
        if not node_mp.empty:
            ax2.plot(node_mp['time_sec'], node_mp['value'], color=c, linestyle='-', label=f"Node {label}")

        # Plot Rejected TX on ax3
        node_rejected = rejected_df[rejected_df['node_id'] == node].sort_values('time_sec')
        if not node_rejected.empty:
            ax3.plot(node_rejected['time_sec'], node_rejected['value'], color=c, linestyle='-', label=f"Node {label}")

        # Plot Blocks on ax4
        node_blocks = blocks_df[blocks_df['node_id'] == node].sort_values('time_sec')
        if not node_blocks.empty:
            ax4.plot(node_blocks['time_sec'], node_blocks['value'], color=c, linestyle='-', label=f"Node {label}")

        # Plot Sync Status on ax5
        node_sync = sync_df[sync_df['node_id'] == node].sort_values('time_sec')
        if not node_sync.empty:
            ax5.plot(node_sync['time_sec'], node_sync['value'], color=c, linestyle='-', label=f"Node {label}")

    if spam_start_time is not None:
        for ax in [ax1, ax2, ax3, ax4, ax5]:
            ax.axvline(x=spam_start_time, color='r', linestyle='--', alpha=0.8)

    ax1.set_ylabel('Transactions Committed')
    ax1.set_title('Transactions Committed Total per Node')
    ax1.grid(True)
    ax1.legend(loc='upper left', bbox_to_anchor=(1.05, 1), borderaxespad=0.)

    ax2.set_ylabel('Mempool Size')
    ax2.set_title('Mempool Size per Node')
    ax2.grid(True)
    ax2.legend(loc='upper left', bbox_to_anchor=(1.05, 1), borderaxespad=0.)

    ax3.set_ylabel('Rejected Transactions')
    ax3.set_title('Mempool Rejected Transactions per Node')
    ax3.grid(True)
    ax3.legend(loc='upper left', bbox_to_anchor=(1.05, 1), borderaxespad=0.)

    ax4.set_ylabel('Block Height')
    ax4.set_title('Block Height per Node')
    ax4.grid(True)
    ax4.legend(loc='upper left', bbox_to_anchor=(1.05, 1), borderaxespad=0.)

    ax5.set_ylabel('Sync Status')
    ax5.set_title('Sync Status per Node (1=Syncing, 0=Synced)')
    ax5.set_xlabel('Time (seconds)')
    ax5.grid(True)
    ax5.legend(loc='upper left', bbox_to_anchor=(1.05, 1), borderaxespad=0.)

    plt.tight_layout()
    side_by_side_plot_path = os.path.join(analysis_dir, "transactions_vs_mempool_side_by_side.png")
    plt.savefig(side_by_side_plot_path)
    plt.close()
    print(f"Saved side-by-side plot to {side_by_side_plot_path}")

    # 4. Graph: Blocks per node
    plt.figure(figsize=(12, 6))
    
    # We already determined spam_start_time above

    for i, node in enumerate(nodes):
        label = node[:8]
        c = colors[i % len(colors)]
        node_blocks = blocks_df[blocks_df['node_id'] == node].sort_values('time_sec')
        if not node_blocks.empty:
            plt.plot(node_blocks['time_sec'], node_blocks['value'], color=c, linestyle='-', label=f"Node {label}")
    
    if spam_start_time is not None:
        plt.axvline(x=spam_start_time, color='r', linestyle='--', alpha=0.8, label="Spam Start")

    plt.title("Block Height per Node")
    plt.xlabel("Time (seconds)")
    plt.ylabel("Block Height")
    plt.legend(loc='upper left', bbox_to_anchor=(1.05, 1), borderaxespad=0.)
    plt.tight_layout()
    plt.grid(True)
    blocks_plot_path = os.path.join(analysis_dir, "blocks_per_node.png")
    plt.savefig(blocks_plot_path)
    plt.close()
    print(f"Saved blocks plot to {blocks_plot_path}")

    # 5. Graph: Sync status per node
    plt.figure(figsize=(12, 6))
    for i, node in enumerate(nodes):
        label = node[:8]
        c = colors[i % len(colors)]
        node_sync = sync_df[sync_df['node_id'] == node].sort_values('time_sec')
        if not node_sync.empty:
            plt.plot(node_sync['time_sec'], node_sync['value'], color=c, linestyle='-', label=f"Node {label}")
    
    plt.title("Sync Status per Node (1=Syncing, 0=Synced)")
    plt.xlabel("Time (seconds)")
    plt.ylabel("Sync Status")
    plt.legend(loc='upper left', bbox_to_anchor=(1.05, 1), borderaxespad=0.)
    plt.tight_layout()
    plt.grid(True)
    sync_plot_path = os.path.join(analysis_dir, "sync_status_per_node.png")
    plt.savefig(sync_plot_path)
    plt.close()
    print(f"Saved sync status plot to {sync_plot_path}")

if __name__ == "__main__":
    main()
