# Implementation Plan: Multi-Node Blockchain Network Deployment

This document outlines the plan for implementing a flexible blockchain network startup system that supports local (Docker) and multi-machine deployments with configurable node ratios and consensus/validator client pairings.

## 1. Architecture Overview

The system will consist of:
- **Execution Clients**: wasix_eth (Linux, Windows/Wine, Wasix/Wasmer).
- **Consensus Clients**: Lighthouse Beacon Node.
- **Validator Clients**: Lighthouse Validator Client.
- **Bootnode**: A designated node for initial peer discovery.
- **Metrics Server**: For registration and data gathering.
- **Genesis Generator**: To create unique chain configurations.

## 2. Scripting Strategy

We will move from static scripts to a more parameterized approach.

### 2.1 Configuration Layer (scripts/config_gen.sh)
- Generates genesis.json, config.yaml, and JWT secrets.
- Takes chainId and genesisTime as parameters.
- Generates unique validator keys based on the total number of validators requested.

### 2.2 Local Docker Deployment (scripts/deploy_local.sh)
- Takes parameters: --total-nodes, --ratio-linux, --ratio-windows, --ratio-wasix.
- Dynamically generates a docker-compose.yaml or uses docker run commands.
- For each node:
    - 1 Execution Client.
    - 1 Consensus Client.
    - 0 or 1 Validator Client (ensuring exactly 50% coverage).
- Assigns unique ports and data directories.
- Registers nodes with the metrics_server.

### 2.3 Multi-Machine Deployment (scripts/deploy_remote.sh)
- Uses a configuration file (e.g., hosts.ini) listing IP addresses and node types for remote machines.
- Uses ssh or a lightweight orchestration tool to:
    - Install dependencies on remote hosts (install.sh).
    - Copy genesis and configuration files.
    - Start the node stack (Execution + Consensus + [Validator]).
- Supports the same ratio logic as the local deployment.

## 3. Key Components Implementation

### 3.1 Dynamic Chain ID
- Update configure.sh (or the new config_gen.sh) to accept --chain-id.
- Ensure the genesis generator uses this ID.

### 3.2 Metrics Registration
- Nodes self-register with the `metrics_server` on startup via HTTP POST.
- Nodes fetch bootstrap peers from the `metrics_server` via HTTP GET.

### 3.3 Node Ratio Logic
- A helper function to calculate how many nodes of each type to spawn based on percentages or absolute numbers.
- A helper to distribute validator duties to exactly 50% of the nodes.

## 4. Tasks & Milestones

1. Phase 1: Configuration Refactoring ✓
    - [✓] Create scripts/v2/config_gen.sh with --chain-id and dynamic validator key generation.
2. Phase 2: Metrics Integration ✓
    - [✓] Add registration call to the execution client startup sequence.
3. Phase 3: Local Orchestrator ✓
    - [✓] Create scripts/v2/start_local.sh.
    - [✓] Implement docker-compose template generation.
4. Phase 4: Remote Orchestrator ✓
    - [✓] Create scripts/v2/start_remote.sh.
    - [✓] Implement SSH-based deployment logic.
5. Phase 5: Automated Installation & Setup Integration ✓
    - [✓] Create scripts/v2/setup.sh for environment preparation.
    - [✓] Integrate --setup flag into start scripts.
6. Phase 6: Docker Registry Integration ✓
    - [✓] Create scripts/v2/push_images.sh for image pushing.
    - [✓] Update deployment scripts to support --registry and --tag.
7. Phase 7: Documentation & Validation *
    - [✓] Finalize USER_GUIDE.md.
    - [ ] Test local deployment with 4 nodes (2 Linux, 1 Windows, 1 Wasix).
    - [ ] Test validator distribution.

## 5. Directory Structure Changes
`
scripts/
  v2/
    config_gen.sh
    start_local.sh
    start_remote.sh
    common.sh (shared logic for ratios/ports)
    templates/
      docker-compose.yml.template
    push_images.sh
    setup.sh
`

