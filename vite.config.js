import { defineConfig } from "vite";
import { spawn } from "node:child_process";

export default defineConfig({
    server: {
        headers: {
            "Cross-Origin-Opener-Policy": "same-origin",
            "Cross-Origin-Embedder-Policy": "require-corp",
        },
        fs: {
            allow: ["."],
        },
    },
    assetsInclude: ["**/*.wasm"],
    optimizeDeps: {
        exclude: ["@wasmer/sdk"],
    },
    plugins: [
        {
            name: "wasm-content-type",
            configureServer(server) {
                server.middlewares.use((req, res, next) => {
                    if (req.url && req.url.endsWith(".wasm")) {
                        res.setHeader("Content-Type", "application/wasm");
                    }
                    next();
                });
            },
        },
        {
            name: "cargo-wasix-build",
            buildStart: () => {
                console.log("\n[WASM Build] Starting cargo wasix build (this may take a few minutes)...");
                return new Promise((resolve, reject) => {
                    const child = spawn(
                        "cargo",
                        ["wasix", "build", "--release"],
                        {
                            cwd: "wasix-based-evm",
                            stdio: "inherit",
                            shell: true,
                            env: {
                                ...process.env,
                            },
                        },
                    );

                    child.on("close", (code) => {
                        if (code === 0) {
                            console.log("[WASM Build] Build completed successfully.\n");
                            resolve();
                        } else {
                            console.error(`[WASM Build] Build failed with exit code ${code}`);
                            reject(new Error(`WASM Build failed with exit code ${code}`));
                        }
                    });

                    child.on("error", (err) => {
                        console.error("[WASM Build] Failed to start process:", err);
                        reject(err);
                    });
                });
            },
        },
    ],
});