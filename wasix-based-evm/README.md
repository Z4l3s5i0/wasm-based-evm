# Experimental Proposal README

**Title:** Performance & Security Evaluation of Native vs WASM Ethereum Nodes on a Raspberry Pi Cluster

---

## 1. Overview

This project evaluates the **performance, efficiency, and system behavior** of Ethereum execution clients compiled as:

* **Native Rust binaries**
* **WebAssembly (WASM) binaries**

The experiments are conducted on a controlled cluster of **16–32 Raspberry Pi nodes**, enabling reproducible and hardware-consistent benchmarking.

The central goal is to quantify the **“cost of abstraction”** introduced by WASM and determine whether WASM-based nodes are viable in **real-world blockchain deployments**.

---

## 2. Experimental Philosophy

We separate concerns into two dimensions:

* **Intrinsic execution cost** → measured in isolated environments
* **System-level interaction effects** → measured in hybrid environments
* **Adversarial resilience** → stress and attack workloads


This leads to three core setups:

| Setup                    | Purpose                                |
| ------------------------ | -------------------------------------- |
| A & B (Control)          | Measure pure execution differences     |
| C (Hybrid)               | Measure real-world interaction effects |
| D (Multinode per device) | Evaluate isolation and contention      |

---

## 3. Experimental Setups

### Setup A & B — Baseline Comparison (Control)

Two independent blockchains:

* Network 1: **n Native nodes**
* Network 2: **n WASM nodes**

Both:

* run identical configurations
* receive identical workloads

#### Goal

Isolate **pure execution overhead** and establish baseline performance.

---

### Setup C — Hybrid Network

Single blockchain:

* n/2 Native nodes
* n/2 WASM nodes

All share:

* consensus
* mempool
* block production

#### Goal

Measure:

* heterogeneous execution environments
* fairness and participation
* systemic bottlenecks

---

### Setup D — Multinode per Device

* n WASM nodes on n/2 physical devices

#### Goal

Evaluate:

* isolation guarantees
* resource contention
* scalability per device

---

## 4. Experimental Variables

### Workload Types
We use **six structured workload scenarios**, covering both **real-world usage patterns** and **adversarial stress conditions**.

| Workload | Type           | Scenario                   | Duration (s) | TPS          |
| -------- | -------------- | -------------------------- | ------------ | ------------ |
| DDoS     | Transfer Tx    | Constant high-rate attack  | 120          | 10,000       |
| FIFA     | Smart Contract | Sustained high throughput  | 100          | 45,000       |
| GAFAM    | Smart Contract | Burst traffic (decay)      | 180          | 20,000 → 100 |
| Gaming   | Smart Contract | Intensive compute workload | 276          | 13,000       |
| PayPal   | Transfer Tx    | Low steady rate            | 300          | 200          |
| VISA     | Transfer Tx    | Medium steady rate         | 300          | 1,800        |


### Workload Roles

#### Real-World Baselines

* **PayPal** → low throughput financial usage
* **VISA** → moderate throughput payments

#### High-Performance Scenarios

* **FIFA** → sustained peak demand
* **Gaming** → compute-heavy smart contract execution

#### Dynamic Systems

* **GAFAM** → burst behavior with decay

#### Adversarial Scenario

* **DDoS** → sustained high-rate spam transactions
---
### Independent Variables (Controlled)

* Execution type (Native / WASM / Hybrid ratio)
* Workload scenario (see table above)
* Transaction rate (defined per workload)
* Smart contract complexity (gas usage)

> Note: Network conditions are held constant to isolate execution effects.


### Dependent Variables (Measured)

#### Performance

* Block import rate
* Transaction throughput (TPS)
* Transaction latency (p50 / p95 / p99)

#### Resource Efficiency

* CPU cycles per transaction
* Instructions per transaction
* Memory usage

#### System Pressure

* Mempool size
* Mempool growth rate

#### Synchronization

* Sync gap per node
* Sync gap variance

#### Stability

* Reorg frequency
* Block processing variance

#### Fairness (Hybrid only)

* Block proposal distribution per node type

#### Security / Adversarial Resilience

(especially for DDoS workload)

* transaction drop rate
* mempool overflow behavior
* latency degradation factor
* CPU saturation

---

## 5. Metric Collection Strategy

Metrics are collected from **two complementary sources**:

1. **Execution Client (application-level)**
2. **node_exporter (system-level)**

This separation is critical for distinguishing:

* *protocol behavior* vs
* *system resource constraints*

---

# 6. Metrics — Execution Client vs node_exporter

## 6.1 Execution Client Metrics (Application-Level)

These are **blockchain-aware metrics** and form the **core of the research**.

### Execution Metrics

* `tx_execution_seconds`
* `cpu_cycles_total`
* `instruction_count_total`

Derived:

* cycles per transaction
* cycles per gas
* IPC (instructions per cycle)

---

### Latency Metrics

* `execution_latency_cdf`

Extract:

* p50 / p95 / p99
* tail amplification

---

### Throughput & Consensus

* `block_import_rate`
* `block_processing_seconds`
* `gas_used_per_block`

---

### Synchronization

* `current_head_block`
* `network_head_block`

Derived:

* sync gap
* sync variance

---

### Mempool / Pressure

* `mempool_size`
* transaction enqueue/dequeue rates

---

### Fairness (Hybrid Critical)

* blocks proposed per node
* blocks accepted per node

Derived:

* % contribution (WASM vs Native)

---

### Cache / State Access

* `cache_hits_total`
* `cache_misses_total`

Derived:

* cache hit ratio
* misses per transaction

---

### WASM-Specific

* `wasm_compilation_seconds_total`

---

## 6.2 node_exporter Metrics (System-Level)

These provide **hardware and OS-level visibility**.

They explain *why* performance differences occur.

---

### CPU Metrics

* CPU usage (user, system, idle)
* load average
* context switches

Derived:

* CPU saturation
* scheduling overhead

---

### Memory Metrics

* total memory usage
* RSS (process-level if available)
* page faults

Derived:

* memory per transaction
* memory growth rate

---

### Disk / I/O

* read/write bytes
* IOPS
* disk latency

Derived:

* I/O per transaction
* write amplification

---

### Network

* bytes in/out
* packet rates

Derived:

* bandwidth usage per node

---

## 6.3 What MUST Come From Where

### Execution Client (MANDATORY)

These cannot be replaced by node_exporter:

* transaction execution time
* gas usage
* instruction count
* consensus metrics
* mempool state
* block production

---

### node_exporter (SUPPORTING)

These provide explanation but not core results:

* CPU utilization
* memory pressure
* disk bottlenecks

---

## 6.4 Summary Table

| Metric Type       | Source           | Purpose              |
| ----------------- | ---------------- | -------------------- |
| Execution time    | Execution client | Core comparison      |
| CPU cycles        | Execution client | Efficiency           |
| Instruction count | Execution client | Microarchitecture    |
| TPS / throughput  | Execution client | System performance   |
| Latency (p99)     | Execution client | User impact          |
| Mempool           | Execution client | Backpressure         |
| CPU usage         | node_exporter    | Bottleneck diagnosis |
| Memory usage      | node_exporter    | Resource pressure    |
| Disk I/O          | node_exporter    | State access cost    |

---

# 7. Normalization Strategy (Critical)

All metrics must be normalized to enable fair comparison:

### Primary

* per transaction

### Secondary

* per unit of gas

### Derived Examples

* cycles per gas
* time per gas
* memory per transaction
* latency per gas

---

# 8. Experimental Methodology

For each setup:

1. Initialize network(s) with identical configuration
2. Deploy identical workloads
3. Run experiment for fixed duration (30–60 min)
4. Collect:

    * per-node metrics
    * aggregated network metrics
5. Repeat:

    * **≥ 5 runs per configuration**
6. Compute:

    * mean
    * variance
    * confidence intervals

---

# 9. Expected Outcomes

This framework enables answering:

* What is the **true overhead** of WASM execution?
* Does WASM scale **linearly or non-linearly**?
* Does WASM degrade **tail latency (p99)**?
* How does WASM behave under **adversarial workloads (DDoS)**?
* Does WASM affect **consensus fairness**?
* Can WASM nodes operate in **real-world heterogeneous networks**?
* Can WASM nodes sustain **realistic and high-load conditions**?

---

# 10. Future Extensions

* Browser-based WASM nodes vs runtime (Wasmer)
* Advanced adversarial attacks (state bloat, contract abuse)
* Network perturbation experiments
* Determinism validation (state root consistency)

---

# 11. Final Note

This proposal emphasizes:

* **controlled experimentation**
* **clear separation of concerns**
* **strong normalization**
* **realistic workload modeling**
* **adversarial stress testing**
* **multi-layer observability**

