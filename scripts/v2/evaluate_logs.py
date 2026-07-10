import os
import re
import sys
import json
from collections import defaultdict
from datetime import datetime

def parse_log_line(line):
    # Regex to match: [EXP] EVENT_NAME key1=value1 key2=value2 ...
    # Note: Some values might be in quotes like hash="0x..."
    # Improved pattern to handle [EXP] preceded by timestamp/metadata and more robust kv parsing
    match = re.search(r'\[EXP\]\s+(\w+)\s+(.*)', line)
    if not match:
        return None
    
    event_name = match.group(1)
    kv_str = match.group(2)
    
    # Extract key-value pairs
    kv_pairs = {}
    # Handles key=value and key="value"
    # Improved pattern to handle various value formats: hex, numbers, strings in quotes, etc.
    pattern = r'(\w+)=({[^}]+}|"[^"]*"|0x[a-fA-F0-0]+|\S+)'
    for k, v in re.findall(pattern, kv_str):
        # Remove quotes if present
        if v.startswith('"') and v.endswith('"'):
            v = v[1:-1]
        kv_pairs[k] = v
        
    return event_name, kv_pairs

def process_logs(log_dir, target_chain_id=None):
    tx_recv_times = defaultdict(dict)  # tx_hash -> {peer_id: timestamp}
    tx_exec_times = {}                 # tx_hash -> timestamp
    block_recv_times = defaultdict(dict) # block_hash -> {peer_id: timestamp}
    block_exec_times = {}                # block_hash -> timestamp
    block_txs = {}                       # block_hash -> [tx_hashes]
    
    node_stats = defaultdict(lambda: {
        'tx_received': 0,
        'tx_sent': 0,
        'blocks_received': 0,
        'blocks_sent': 0,
        'blocks_executed': 0,
        'tx_executed': 0,
        'total_gas': 0,
        'total_exec_ms': 0
    })

    # Sort files to process them in chronological order per node
    all_files = sorted([f for f in os.listdir(log_dir) if f.endswith('.log')])
    files_to_process = []
    
    for filename in all_files:
        # Assuming filename format: {container_name}_{chain_id}_{file_number}.log
        parts = filename.split('_')
        if len(parts) < 2:
            continue
            
        node_id = parts[0]
        file_chain_id = parts[1]
        
        if target_chain_id and file_chain_id != target_chain_id:
            continue
            
        files_to_process.append((filename, node_id))
    
    for filename, node_id in files_to_process:
        filepath = os.path.join(log_dir, filename)
        with open(filepath, 'r', errors='ignore') as f:
            for line in f:
                # Basic timestamp extraction if present in the log line
                # Standard log format often has timestamps at the beginning
    # [2026-07-09 21:16:00,123] or [2026-07-09T21:16:00.123Z] or Jul 10 08:16:07.859
                ts_match = re.search(r'(?:\[(\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}[.,]\d+Z?)\]|^([A-Z][a-z]{2}\s+\d+\s+\d{2}:\d{2}:\d{2}\.\d{3}))', line)
                if ts_match:
                    timestamp = ts_match.group(1) or ts_match.group(2)
                else:
                    timestamp = None
                
                parsed = parse_log_line(line)
                if not parsed:
                    continue
                
                event, data = parsed
                
                if event in ['P2P_RECV_TX', 'TX_RPC_RECV']:
                    tx_hash = data.get('hash')
                    node_stats[node_id]['tx_received'] += 1
                    if tx_hash and timestamp:
                        if tx_hash not in tx_recv_times:
                            tx_recv_times[tx_hash]['first_seen'] = timestamp
                        tx_recv_times[tx_hash][node_id] = timestamp
                        
                elif event == 'P2P_SEND_TX':
                    node_stats[node_id]['tx_sent'] += 1
                    
                elif event == 'TX_EXEC_START':
                    tx_hash = data.get('hash')
                    node_stats[node_id]['tx_executed'] += 1
                    if tx_hash and timestamp:
                        if tx_hash not in tx_exec_times:
                            tx_exec_times[tx_hash] = timestamp
                        # If we never saw this TX via P2P (e.g. it came in a block or was local),
                        # use its execution time as a fallback for 'first_seen'
                        if tx_hash not in tx_recv_times:
                            tx_recv_times[tx_hash]['first_seen'] = timestamp
                            
                elif event == 'TX_EXEC_END':
                    node_stats[node_id]['total_gas'] += int(data.get('gas_used', 0))
                    node_stats[node_id]['total_exec_ms'] += int(data.get('elapsed_ms', 0))

                elif event in ['P2P_RECV_BLOCK', 'BLOCK_RPC_RECV']:
                    block_hash = data.get('hash')
                    if event == 'P2P_RECV_BLOCK':
                        node_stats[node_id]['blocks_received'] += 1
                    if block_hash and timestamp:
                        if block_hash not in block_recv_times:
                            block_recv_times[block_hash]['first_seen'] = timestamp
                        block_recv_times[block_hash][node_id] = timestamp

                elif event == 'P2P_SEND_BLOCK':
                    node_stats[node_id]['blocks_sent'] += 1

                elif event == 'BLOCK_EXEC_START':
                    block_hash = data.get('hash')
                    if block_hash and timestamp:
                        # Fallback for block first_seen if we didn't get P2P_RECV_BLOCK
                        if block_hash not in block_recv_times:
                            block_recv_times[block_hash]['first_seen'] = timestamp

                elif event == 'BLOCK_EXEC_END':
                    block_hash = data.get('hash')
                    node_stats[node_id]['blocks_executed'] += 1
                    if block_hash and timestamp:
                        block_exec_times[block_hash] = timestamp

    return {
        'node_stats': node_stats,
        'tx_recv_times': tx_recv_times,
        'tx_exec_times': tx_exec_times,
        'block_recv_times': block_recv_times,
        'block_exec_times': block_exec_times
    }

def calculate_metrics(data):
    def parse_ts(ts_str):
        # Handle format like Jul 10 08:16:07.859
        if re.match(r'^[A-Z][a-z]{2}\s+\d+\s+\d{2}:\d{2}:\d{2}\.\d{3}$', ts_str):
            # Assume current year if not provided
            current_year = 2026
            return datetime.strptime(f"{current_year} {ts_str}", "%Y %b %d %H:%M:%S.%f")

        ts_str = ts_str.replace(',', '.').replace(' ', 'T')
        if not ts_str.endswith('Z'):
            ts_str += 'Z'
        try:
            return datetime.strptime(ts_str, "%Y-%m-%dT%H:%M:%S.%fZ")
        except:
            # Try without milliseconds if needed
            return datetime.strptime(ts_str.split('.')[0] + 'Z', "%Y-%m-%dT%H:%M:%SZ")

    # 1. Inclusion Latency (First Seen -> Executed)
    inclusion_latencies = []
    for tx_hash, exec_ts_str in data['tx_exec_times'].items():
        if tx_hash in data['tx_recv_times'] and 'first_seen' in data['tx_recv_times'][tx_hash]:
            start = parse_ts(data['tx_recv_times'][tx_hash]['first_seen'])
            end = parse_ts(exec_ts_str)
            latency = (end - start).total_seconds()
            if latency >= 0:
                inclusion_latencies.append(latency)

    # 2. Block Propagation Latency
    propagation_latencies = []
    for block_hash, nodes in data['block_recv_times'].items():
        if 'first_seen' in nodes:
            first_ts = parse_ts(nodes['first_seen'])
            for node_id, ts_str in nodes.items():
                if node_id == 'first_seen': continue
                lat = (parse_ts(ts_str) - first_ts).total_seconds()
                if lat >= 0:  # Changed from > 0 to >= 0
                    propagation_latencies.append(lat)

    # Summarize Node Stats
    total_tx_exec = sum(s['tx_executed'] for s in data['node_stats'].values())
    total_blocks = sum(s['blocks_executed'] for s in data['node_stats'].values())
    
    results = {
        'inclusion_latency': {
            'avg': sum(inclusion_latencies) / len(inclusion_latencies) if inclusion_latencies else 0,
            'min': min(inclusion_latencies) if inclusion_latencies else 0,
            'max': max(inclusion_latencies) if inclusion_latencies else 0,
            'count': len(inclusion_latencies)
        },
        'propagation_latency': {
            'avg': sum(propagation_latencies) / len(propagation_latencies) if propagation_latencies else 0,
            'max': max(propagation_latencies) if propagation_latencies else 0,
            'count': len(propagation_latencies)
        },
        'total_stats': {
            'transactions_executed': total_tx_exec,
            'blocks_executed': total_blocks,
        },
        'node_breakdown': {node: stats for node, stats in data['node_stats'].items()}
    }
    
    return results

if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Usage: python evaluate_logs.py <log_dir> [chain_id]")
        sys.exit(1)
        
    log_dir = sys.argv[1]
    if not os.path.isdir(log_dir):
        print(f"Error: {log_dir} is not a directory")
        sys.exit(1)
        
    target_chain_id = sys.argv[2] if len(sys.argv) > 2 else None
    
    if target_chain_id:
        print(f"Processing logs in {log_dir} for chain_id {target_chain_id}...")
    else:
        print(f"Processing all logs in {log_dir}...")
        
    data = process_logs(log_dir, target_chain_id)
    metrics = calculate_metrics(data)
    
    print("\n=== Experiment Metrics ===")
    print(f"Total Transactions Executed: {metrics['total_stats']['transactions_executed']}")
    print(f"Total Blocks Executed: {metrics['total_stats']['blocks_executed']}")
    
    il = metrics['inclusion_latency']
    print(f"\nInclusion Latency (Time from P2P recv to EVM exec):")
    print(f"  Avg: {il['avg']:.3f}s")
    print(f"  Min: {il['min']:.3f}s")
    print(f"  Max: {il['max']:.3f}s")
    print(f"  Count: {il['count']}")
    
    pl = metrics['propagation_latency']
    print(f"\nBlock Propagation Latency (Time from first node recv to other nodes recv):")
    print(f"  Avg: {pl['avg']:.3f}s")
    print(f"  Max: {pl['max']:.3f}s")
    
    print("\nNode Breakdown:")
    for node, stats in metrics['node_breakdown'].items():
        print(f"  {node}:")
        print(f"    TX Recv/Sent: {stats['tx_received']}/{stats['tx_sent']}")
        print(f"    Blocks Executed: {stats['blocks_executed']}")
        if stats['blocks_executed'] > 0:
            avg_gas = stats['total_gas'] / stats['blocks_executed']
            print(f"    Avg Gas/Block: {avg_gas:.0f}")
        
    with open('experiment_results.json', 'w') as f:
        json.dump(metrics, f, indent=2)
    print("\nFull results saved to experiment_results.json")
