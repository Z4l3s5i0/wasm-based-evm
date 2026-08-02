import json
import pandas as pd
import matplotlib.pyplot as plt
import sys
import os

def main():
    if len(sys.argv) < 2:
        print("Usage: python plot_metrics.py <experiment_analysis_dir> [output_dir]")
        sys.exit(1)

    analysis_dir = sys.argv[1]
    output_dir = sys.argv[2] if len(sys.argv) > 2 else analysis_dir
    
    if not os.path.exists(output_dir):
        os.makedirs(output_dir)

    samples_file = os.path.join(analysis_dir, "metrics_metric_samples.json")
    nodes_file = os.path.join(analysis_dir, "metrics_nodes.json")

    if not os.path.exists(samples_file):
        print(f"File not found: {samples_file}")
        sys.exit(1)

    print(f"Loading data from {samples_file}...")
    try:
        with open(samples_file, 'r') as f:
            data = json.load(f)
    except json.JSONDecodeError as e:
        print(f"Error decoding JSON from {samples_file}: {e}")
        sys.exit(2) # Exit with code 2 to indicate data error
    except Exception as e:
        print(f"An unexpected error occurred while loading {samples_file}: {e}")
        sys.exit(1)

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
    
    # New metrics
    peers_df = df[df['name'] == 'connected_peers'].copy()
    gossip_df = df[df['name'] == 'gossip_messages_received_total'].copy()
    p2p_bytes_df = df[df['name'] == 'p2p_messages_sent_bytes_total'].copy()
    rpc_df = df[df['name'] == 'rpc_requests_total'].copy()
    target_height_df = df[df['name'] == 'sync_target_height'].copy()

    # 1. Graph: transactions_committed_total per node
    plt.figure(figsize=(12, 6))
    nodes = df['node_id'].unique()
    
    # Use a larger color map and different line styles to distinguish nodes
    color_map = plt.colormaps['tab20']
    line_styles = ['-', '--', ':', '-.']
    
    for i, node in enumerate(nodes):
        label = node[:8]
        c = color_map(i % 20)
        ls = line_styles[i % len(line_styles)]
        
        node_tx = tx_df[tx_df['node_id'] == node].sort_values('time_sec')
        if not node_tx.empty:
            plt.plot(node_tx['time_sec'], node_tx['value'], color=c, linestyle=ls, linewidth=2, label=f"Node {label} TX")

    plt.title("Transactions Committed per Node")
    plt.xlabel("Time (seconds)")
    plt.ylabel("Transactions")
    plt.legend(loc='upper left', bbox_to_anchor=(1.05, 1), borderaxespad=0.)
    plt.tight_layout()
    plt.grid(True)
    tx_plot_path = os.path.join(output_dir, "transactions_committed.png")
    plt.savefig(tx_plot_path)
    plt.close()
    print(f"Saved transactions plot to {tx_plot_path}")

    # 2. Graph: transactions_committed_total and mempool_size per node
    fig, ax1 = plt.subplots(figsize=(12, 8))

    ax2 = ax1.twinx()  # instantiate a second axes that shares the same x-axis

    for i, node in enumerate(nodes):
        label = node[:8]
        c = color_map(i % 20)
        ls = line_styles[i % len(line_styles)]
        
        # Plot TX Committed
        node_tx = tx_df[tx_df['node_id'] == node].sort_values('time_sec')
        if not node_tx.empty:
            ax1.plot(node_tx['time_sec'], node_tx['value'], color=c, linestyle=ls, linewidth=2, label=f"Node {label} TX")

        # Plot Mempool Size
        node_mp = mempool_df[mempool_df['node_id'] == node].sort_values('time_sec')
        if not node_mp.empty:
            # For mempool, we use a slightly different dash pattern if ls was solid, or just keep it distinct
            ax2.plot(node_mp['time_sec'], node_mp['value'], color=c, linestyle=ls, alpha=0.5, linewidth=1, label=f"Node {label} Mempool")

    ax1.set_xlabel('Time (seconds)')
    ax1.set_ylabel('Transactions (Thick)', color='black')
    ax2.set_ylabel('Mempool Size (Thin/Alpha)', color='gray')
    
    plt.title("Transactions Committed and Mempool Size per Node")
    
    # Combined legend
    lines1, labels1 = ax1.get_legend_handles_labels()
    lines2, labels2 = ax2.get_legend_handles_labels()
    ax1.legend(lines1 + lines2, labels1 + labels2, loc='upper left', bbox_to_anchor=(1.05, 1), borderaxespad=0., ncol=2)
    
    plt.tight_layout()
    plt.grid(True)
    combined_plot_path = os.path.join(output_dir, "transactions_and_mempool.png")
    plt.savefig(combined_plot_path)
    plt.close()
    print(f"Saved combined plot to {combined_plot_path}")

    # 3. Graph: Side-by-side subplots for Transactions, Mempool, Rejected, Blocks and Sync
    fig, (ax1, ax2, ax3, ax4, ax5) = plt.subplots(5, 1, figsize=(14, 20), sharex=True)

    # Determine when spamming starts (first time any node has mempool > 0)
    spam_start_time = None
    if not mempool_df.empty:
        spamming = mempool_df[mempool_df['value'] > 0].sort_values('time_sec')
        if not spamming.empty:
            spam_start_time = spamming['time_sec'].iloc[0]

    for i, node in enumerate(nodes):
        label = node[:8]
        c = color_map(i % 20)
        ls = line_styles[i % len(line_styles)]
        
        # Plot TX Committed on ax1
        node_tx = tx_df[tx_df['node_id'] == node].sort_values('time_sec')
        if not node_tx.empty:
            ax1.plot(node_tx['time_sec'], node_tx['value'], color=c, linestyle=ls, linewidth=2, label=f"Node {label}")

        # Plot Mempool Size on ax2
        node_mp = mempool_df[mempool_df['node_id'] == node].sort_values('time_sec')
        if not node_mp.empty:
            ax2.plot(node_mp['time_sec'], node_mp['value'], color=c, linestyle=ls, linewidth=2, label=f"Node {label}")

        # Plot Rejected TX on ax3
        node_rejected = rejected_df[rejected_df['node_id'] == node].sort_values('time_sec')
        if not node_rejected.empty:
            ax3.plot(node_rejected['time_sec'], node_rejected['value'], color=c, linestyle=ls, linewidth=2, label=f"Node {label}")

        # Plot Blocks on ax4
        node_blocks = blocks_df[blocks_df['node_id'] == node].sort_values('time_sec')
        if not node_blocks.empty:
            ax4.plot(node_blocks['time_sec'], node_blocks['value'], color=c, linestyle=ls, linewidth=2, label=f"Node {label}")

        # Plot Sync Status on ax5
        node_sync = sync_df[sync_df['node_id'] == node].sort_values('time_sec')
        if not node_sync.empty:
            ax5.plot(node_sync['time_sec'], node_sync['value'], color=c, linestyle=ls, linewidth=2, label=f"Node {label}")

    if spam_start_time is not None:
        for ax in [ax1, ax2, ax3, ax4, ax5]:
            ax.axvline(x=spam_start_time, color='r', linestyle='--', alpha=0.8)

    ax1.set_ylabel('Transactions Committed')
    ax1.set_title('Transactions Committed Total per Node')
    ax1.grid(True)
    ax1.legend(loc='upper left', bbox_to_anchor=(1.05, 1), borderaxespad=0., ncol=2)

    ax2.set_ylabel('Mempool Size')
    ax2.set_title('Mempool Size per Node')
    ax2.grid(True)
    ax2.legend(loc='upper left', bbox_to_anchor=(1.05, 1), borderaxespad=0., ncol=2)

    ax3.set_ylabel('Rejected Transactions')
    ax3.set_title('Mempool Rejected Transactions per Node')
    ax3.grid(True)
    ax3.legend(loc='upper left', bbox_to_anchor=(1.05, 1), borderaxespad=0., ncol=2)

    ax4.set_ylabel('Block Height')
    ax4.set_title('Block Height per Node')
    ax4.grid(True)
    ax4.legend(loc='upper left', bbox_to_anchor=(1.05, 1), borderaxespad=0., ncol=2)

    ax5.set_ylabel('Sync Status')
    ax5.set_title('Sync Status per Node (1=Syncing, 0=Synced)')
    ax5.set_xlabel('Time (seconds)')
    ax5.grid(True)
    ax5.legend(loc='upper left', bbox_to_anchor=(1.05, 1), borderaxespad=0., ncol=2)

    plt.tight_layout()
    side_by_side_plot_path = os.path.join(output_dir, "transactions_vs_mempool_side_by_side.png")
    plt.savefig(side_by_side_plot_path)
    plt.close()
    print(f"Saved side-by-side plot to {side_by_side_plot_path}")

    # 4. Graph: Blocks per node
    plt.figure(figsize=(12, 6))
    
    # We already determined spam_start_time above

    for i, node in enumerate(nodes):
        label = node[:8]
        c = color_map(i % 20)
        ls = line_styles[i % len(line_styles)]
        node_blocks = blocks_df[blocks_df['node_id'] == node].sort_values('time_sec')
        if not node_blocks.empty:
            plt.plot(node_blocks['time_sec'], node_blocks['value'], color=c, linestyle=ls, linewidth=2, label=f"Node {label}")
    
    if spam_start_time is not None:
        plt.axvline(x=spam_start_time, color='r', linestyle='--', alpha=0.8, label="Spam Start")

    plt.title("Block Height per Node")
    plt.xlabel("Time (seconds)")
    plt.ylabel("Block Height")
    plt.legend(loc='upper left', bbox_to_anchor=(1.05, 1), borderaxespad=0., ncol=2)
    plt.tight_layout()
    plt.grid(True)
    blocks_plot_path = os.path.join(output_dir, "blocks_per_node.png")
    plt.savefig(blocks_plot_path)
    plt.close()
    print(f"Saved blocks plot to {blocks_plot_path}")

    # 5. Graph: Sync status per node
    plt.figure(figsize=(12, 6))
    for i, node in enumerate(nodes):
        label = node[:8]
        c = color_map(i % 20)
        ls = line_styles[i % len(line_styles)]
        node_sync = sync_df[sync_df['node_id'] == node].sort_values('time_sec')
        if not node_sync.empty:
            plt.plot(node_sync['time_sec'], node_sync['value'], color=c, linestyle=ls, linewidth=2, label=f"Node {label}")
    
    plt.title("Sync Status per Node (1=Syncing, 0=Synced)")
    plt.xlabel("Time (seconds)")
    plt.ylabel("Sync Status")
    plt.legend(loc='upper left', bbox_to_anchor=(1.05, 1), borderaxespad=0., ncol=2)
    plt.tight_layout()
    sync_plot_path = os.path.join(output_dir, "sync_status_per_node.png")
    plt.savefig(sync_plot_path)
    plt.close()
    print(f"Saved sync status plot to {sync_plot_path}")

    # 6. Faceted Plots (Small Multiples) to avoid overlapping
    # We will create one image for each major metric, with subplots for each node
    metrics_to_facet = [
        ('Transactions Committed', tx_df, 'transactions_committed_faceted.png'),
        ('Mempool Size', mempool_df, 'mempool_size_faceted.png'),
        ('Rejected Transactions', rejected_df, 'rejected_transactions_faceted.png'),
        ('Block Height', blocks_df, 'block_height_faceted.png'),
        ('Connected Peers', peers_df, 'connected_peers_faceted.png'),
        ('Gossip Messages Received', gossip_df, 'gossip_messages_faceted.png'),
        ('P2P Bytes Sent', p2p_bytes_df, 'p2p_bytes_faceted.png'),
        ('RPC Requests', rpc_df, 'rpc_requests_faceted.png')
    ]

    for title, m_df, filename in metrics_to_facet:
        if m_df.empty:
            continue
            
        num_nodes = len(nodes)
        cols = 2
        rows = (num_nodes + 1) // cols
        
        fig, axes = plt.subplots(rows, cols, figsize=(15, 4 * rows), sharex=True, sharey=True)
        axes = axes.flatten()
        
        for i, node in enumerate(nodes):
            ax = axes[i]
            label = node[:8]
            c = color_map(i % 20)
            
            node_data = m_df[m_df['node_id'] == node].sort_values('time_sec')
            if not node_data.empty:
                ax.plot(node_data['time_sec'], node_data['value'], color=c, linewidth=2)
            
            ax.set_title(f"Node {label}")
            ax.grid(True, linestyle=':', alpha=0.6)
            if spam_start_time is not None:
                ax.axvline(x=spam_start_time, color='r', linestyle='--', alpha=0.5)

        # Hide unused subplots
        for j in range(i + 1, len(axes)):
            axes[j].axis('off')
            
        fig.suptitle(f"{title} per Node (Faceted)", fontsize=16)
        plt.tight_layout(rect=[0, 0.03, 1, 0.95])
        facet_path = os.path.join(output_dir, filename)
        plt.savefig(facet_path)
        plt.close()
        print(f"Saved faceted plot to {facet_path}")

    # 6b. Faceted Plot: Mempool Size vs Rejected Transactions
    if not mempool_df.empty or not rejected_df.empty:
        num_nodes = len(nodes)
        cols = 2
        rows = (num_nodes + 1) // cols
        fig, axes = plt.subplots(rows, cols, figsize=(15, 4 * rows), sharex=True, sharey=True)
        axes = axes.flatten()

        for i, node in enumerate(nodes):
            ax = axes[i]
            label = node[:8]
            
            node_mp = mempool_df[mempool_df['node_id'] == node].sort_values('time_sec')
            node_rj = rejected_df[rejected_df['node_id'] == node].sort_values('time_sec')
            
            if not node_mp.empty:
                ax.plot(node_mp['time_sec'], node_mp['value'], color='blue', label='Mempool Size', linewidth=2)
            if not node_rj.empty:
                ax.plot(node_rj['time_sec'], node_rj['value'], color='red', label='Rejected TX', linewidth=2, linestyle='--')
            
            ax.set_title(f"Node {label}")
            ax.grid(True, linestyle=':', alpha=0.6)
            if i == 0:
                ax.legend()
            if spam_start_time is not None:
                ax.axvline(x=spam_start_time, color='gray', linestyle='--', alpha=0.5)

        for j in range(i + 1, len(axes)):
            axes[j].axis('off')

        fig.suptitle("Mempool Size vs Rejected Transactions (Faceted)", fontsize=16)
        plt.tight_layout(rect=[0, 0.03, 1, 0.95])
        mp_vs_rj_path = os.path.join(output_dir, "mempool_vs_rejected_faceted.png")
        plt.savefig(mp_vs_rj_path)
        plt.close()
        print(f"Saved faceted mempool vs rejected plot to {mp_vs_rj_path}")

    # 7. Stacked faceted plot (All metrics for each node in one row)
    # This is similar to the side-by-side but organized by node.
    num_nodes = len(nodes)
    fig, axes = plt.subplots(num_nodes, 6, figsize=(25, 3 * num_nodes), sharex=True, sharey='col')
    # 6 columns: Transactions, Mempool, Rejected, Blocks, Peers, RPC
    
    for i, node in enumerate(nodes):
        label = node[:8]
        c = color_map(i % 20)
        
        # Row i axes
        ax_tx, ax_mp, ax_rj, ax_bl, ax_pr, ax_rpc = axes[i]
        
        # TX
        node_tx = tx_df[tx_df['node_id'] == node].sort_values('time_sec')
        if not node_tx.empty:
            ax_tx.plot(node_tx['time_sec'], node_tx['value'], color=c)
        ax_tx.set_ylabel(f"Node {label}")
        if i == 0: ax_tx.set_title("Transactions")
        
        # Mempool
        node_mp = mempool_df[mempool_df['node_id'] == node].sort_values('time_sec')
        if not node_mp.empty:
            ax_mp.plot(node_mp['time_sec'], node_mp['value'], color=c)
        if i == 0: ax_mp.set_title("Mempool")
        
        # Rejected
        node_rj = rejected_df[rejected_df['node_id'] == node].sort_values('time_sec')
        if not node_rj.empty:
            ax_rj.plot(node_rj['time_sec'], node_rj['value'], color=c)
        if i == 0: ax_rj.set_title("Rejected")
        
        # Blocks
        node_bl = blocks_df[blocks_df['node_id'] == node].sort_values('time_sec')
        if not node_bl.empty:
            ax_bl.plot(node_bl['time_sec'], node_bl['value'], color=c)
        if i == 0: ax_bl.set_title("Blocks")

        # Peers
        node_pr = peers_df[peers_df['node_id'] == node].sort_values('time_sec')
        if not node_pr.empty:
            ax_pr.plot(node_pr['time_sec'], node_pr['value'], color=c)
        if i == 0: ax_pr.set_title("Peers")

        # RPC
        node_rpc = rpc_df[rpc_df['node_id'] == node].sort_values('time_sec')
        if not node_rpc.empty:
            ax_rpc.plot(node_rpc['time_sec'], node_rpc['value'], color=c)
        if i == 0: ax_rpc.set_title("RPC Req")

        for ax in [ax_tx, ax_mp, ax_rj, ax_bl, ax_pr, ax_rpc]:
            ax.grid(True, linestyle=':', alpha=0.5)
            if spam_start_time is not None:
                ax.axvline(x=spam_start_time, color='black', linestyle=':', alpha=0.5)

    plt.tight_layout()
    stacked_faceted_path = os.path.join(output_dir, "node_metrics_stacked_faceted.png")
    plt.savefig(stacked_faceted_path)
    plt.close()
    print(f"Saved stacked faceted plot to {stacked_faceted_path}")

    # 8. Stacked Bar Plots
    from matplotlib.ticker import MaxNLocator
    
    def create_stacked_bar_with_lines(data_df, title, ylabel, filename):
        if data_df.empty:
            return
        
        # Round time to seconds for binning
        data_df = data_df.copy()
        data_df['time_bin'] = data_df['time_sec'].round().astype(int)
        
        # Pivot to get nodes as columns
        pivot_df = data_df.pivot_table(index='time_bin', columns='node_label', values='value', aggfunc='max')
        
        # Fill missing values for stacking
        pivot_df = pivot_df.ffill().fillna(0)
        
        if pivot_df.empty:
            return

        fig, ax = plt.subplots(figsize=(14, 8))
        
        # Plot stacked bars
        pivot_df.plot(kind='bar', stacked=True, ax=ax, width=0.8, color=[color_map(i % 20) for i in range(len(pivot_df.columns))])
        
        # Add dotted lines connecting the tops of stacks
        # Get the cumulative sums for each row to find the top of each stack
        cumsum_df = pivot_df.cumsum(axis=1)
        
        # The x-coordinates for the bars are 0, 1, 2, ...
        x_coords = range(len(pivot_df))
        
        # Plot lines for each level in the stack
        for col in cumsum_df.columns:
            y_values = cumsum_df[col].values
            # Plot dotted lines between consecutive bars
            for i in range(len(x_coords) - 1):
                ax.plot([i, i+1], [y_values[i], y_values[i+1]], color='gray', linestyle=':', linewidth=0.8, alpha=0.5)

        if spam_start_time is not None:
            # Find the bin index for spam start
            spam_bin = round(spam_start_time)
            # Find the closest index in the pivot_df
            if spam_bin in pivot_df.index:
                idx = list(pivot_df.index).index(spam_bin)
                ax.axvline(x=idx, color='r', linestyle='--', alpha=0.8, label="Spam Start")

        ax.set_title(title)
        ax.set_xlabel("Time (seconds)")
        ax.set_ylabel(ylabel)
        ax.legend(loc='upper left', bbox_to_anchor=(1.05, 1), borderaxespad=0., ncol=2)
        
        # Reduce number of x-ticks if there are too many
        if len(pivot_df) > 20:
            ax.xaxis.set_major_locator(MaxNLocator(nbins=20))
            
        plt.tight_layout()
        plot_path = os.path.join(output_dir, filename)
        plt.savefig(plot_path)
        plt.close()
        print(f"Saved stacked bar plot to {plot_path}")

    create_stacked_bar_with_lines(tx_df, "Total Committed Transactions Over Time (Stacked)", "Total Transactions", "transactions_committed_stacked_bar.png")
    create_stacked_bar_with_lines(mempool_df, "Total Mempool Size Over Time (Stacked)", "Total Mempool Size", "mempool_size_stacked_bar.png")
    create_stacked_bar_with_lines(gossip_df, "Total Gossip Messages Received Over Time (Stacked)", "Total Messages", "gossip_messages_stacked_bar.png")
    create_stacked_bar_with_lines(rpc_df, "Total RPC Requests Received Over Time (Stacked)", "Total Requests", "rpc_requests_stacked_bar.png")

    # 9. Final Cleanup
    plt.close('all')

if __name__ == "__main__":
    main()
