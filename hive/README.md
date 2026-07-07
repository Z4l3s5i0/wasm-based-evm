# HIVE (Custom Client Test Suite) — Docker Guide

This guide explains how to build and run the customized HIVE test suite using Docker. This eliminates the need to manually install Go or manage local dependencies.

## Prerequisites
* [Docker](https://docs.docker.com/get-docker/) installed and running.

---

## 1. Build the Docker Image

Run the following command from the directory containing the `Dockerfile` to clone, switch branches, and compile both `hive` and `hiveview` inside an isolated container:

```bash
docker build -f ./hive/hive.dockerfile -t hive-custom .
```

## 2. Running the test suite
Run the Wasix Wasm-Ethereum Client:
```bash
docker run --rm \
  -v /var/run/docker.sock:/var/run/docker.sock \
  -v $(pwd)/workspace:/hive/workspace \
  hive-custom \
  --sim ethereum/engine \
  --client wasix-w-eth \
  --sim.parallelism 20
```
Run the Wasix Rust-Ethereum Client:
```bash
docker run --rm \
  -v /var/run/docker.sock:/var/run/docker.sock \
  -v $(pwd)/workspace:/hive/workspace \
  hive-custom \
  --sim ethereum/engine \
  --client wasix-r-eth \
  --sim.parallelism 20
```

Run the Hiveview Server:
```bash
docker run --rm -it \
  -p 8080:8080 \
  -v $(pwd)/workspace:/hive/workspace \
  --entrypoint ./hiveview \
  hive-custom \
  --serve --logdir /hive/workspace/logs
```