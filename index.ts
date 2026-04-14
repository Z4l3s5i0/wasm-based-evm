import { init, runWasix, initializeLogger } from "@wasmer/sdk";
import wasixBaseEvmUrl from "./wasix-based-evm/target/wasm32-wasmer-wasi/release/wasix-based-evm.wasi.wasm?url";

function appendLog(message: string, type: "stdout" | "stderr" | "system" = "stdout") {
    const logsElement = document.getElementById("logs");
    const containerElement = document.getElementById("log-container");
    if (logsElement && containerElement) {
        const line = document.createElement("div");
        line.className = `log-line log-${type}`;
        line.textContent = message;
        logsElement.appendChild(line);

        // Auto-scroll to bottom
        containerElement.scrollTop = containerElement.scrollHeight;
    }
    // Also log to console for backup
    if (type === "stderr") {
        console.error(message);
    } else {
        console.log(message);
    }
}

async function initialize() {
    appendLog(`SharedArrayBuffer available: ${typeof SharedArrayBuffer !== "undefined"}`, "system");
    appendLog(`Is secure context: ${window.isSecureContext}`, "system");
    appendLog(`Cross-Origin-Opener-Policy: same-origin (checked via headers)`, "system");
    appendLog(`Cross-Origin-Embedder-Policy: require-corp (checked via headers)`, "system");

    await init({});
    // Initialize the logger for more verbose output from the Wasmer SDK itself
    try {
        initializeLogger("info,wasmer_wasix=debug");
    } catch (e) {
        // initializeLogger can only be called once
        appendLog(`Logger already initialized or failed to initialize: ${e}`, "stderr");
    }
    appendLog(`Fetching WASM from: ${wasixBaseEvmUrl}`, "system");
    const response = await fetch(wasixBaseEvmUrl);
    if (!response.ok) {
        throw new Error(`Failed to fetch WASM from ${wasixBaseEvmUrl}: ${response.status} ${response.statusText}`);
    }
    const contentType = response.headers.get("Content-Type");
    appendLog(`WASM Response Content-Type: ${contentType}`, "system");
    if (contentType && !contentType.includes("application/wasm")) {
        const text = await response.text();
        appendLog(`WASM response is not application/wasm. Body starts with: ${text.slice(0, 100)}`, "stderr");
        throw new Error(`Unexpected Content-Type: ${contentType}. Expected application/wasm.`);
    }
    const module = await WebAssembly.compileStreaming(response);
    return module;
}

async function pipeToConsole(
    stream: ReadableStream<Uint8Array>,
    label: string,
) {
    const reader = stream.getReader();
    const decoder = new TextDecoder();
    let remaining = "";

    const type = label === "WASM-STDERR" ? "stderr" : "stdout";

    try {
        while (true) {
            const { value, done } = await reader.read();
            if (done) {
                if (remaining.trim()) {
                    appendLog(`[${label}] ${remaining}`, type);
                }
                break;
            }

            const text = remaining + decoder.decode(value);
            const lines = text.split("\n");
            remaining = lines.pop() || "";

            for (const line of lines) {
                if (line.trim()) {
                    appendLog(`[${label}] ${line}`, type);
                }
            }
        }
    } catch (e) {
        appendLog(`Error reading ${label} stream: ${e}`, "stderr");
    } finally {
        reader.releaseLock();
    }
}

async function runClient(module: WebAssembly.Module) {
    appendLog("Starting WASM client...", "system");
    const instance = await runWasix(module, {
        program: "wasix-based-evm",
        args: [
            "--verbose", "2",
            "--data-dir", "/data-dir",
            "--p2p-port", "9002",
            "--discovery-port", "9001",
            "--eth-rpc-port", "8545",
            "--auth-rpc-port", "8551",
            "--max-peers", "50",
            "--chain", "devnet"
        ],
        env: {
            RUST_LOG: "info",
        },
        mount: {
            "/data-dir": {},
        },
        capabilities: {
            net: true,        // equivalent to --net
        },
        threading: true,
        async: true,
    });

    // Pipe stdout and stderr to the screen
    pipeToConsole(instance.stdout, "WASM-STDOUT");
    pipeToConsole(instance.stderr, "WASM-STDERR");

    const result = await instance.wait();
    appendLog(`WASM Process Finished: ${JSON.stringify(result)}`, "system");
}

async function main() {
    const startBtn = document.getElementById("start-btn") as HTMLButtonElement;
    if (startBtn) {
        startBtn.disabled = true;
        startBtn.textContent = "Running...";
    }

    try {
        const module = await initialize();
        await runClient(module);
    } catch (error) {
        appendLog(`Critical Error: ${error}`, "stderr");
    } finally {
        if (startBtn) {
            startBtn.disabled = false;
            startBtn.textContent = "Start WASM Client";
        }
    }
}

const startBtn = document.getElementById("start-btn");
if (startBtn) {
    startBtn.addEventListener("click", () => {
        // Clear logs for fresh start
        const logs = document.getElementById("logs");
        if (logs) logs.innerHTML = "";
        main();
    });
} else {
    // Fallback for direct execution if button is missing
    main();
}