import { defineConfig, type PluginOption, type ViteDevServer } from "vite";
import react from "@vitejs/plugin-react";
import { fileURLToPath, URL } from "node:url";
import net from "node:net";
import { spawn } from "node:child_process";

const host = process.env.TAURI_DEV_HOST;

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [react(), reactDevtools()],
  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
    },
  },
  // Tauri expects a fixed port; if this is unavailable we fail fast
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // Tell Vite to ignore watching src-tauri
      ignored: ["**/src-tauri/**"],
    },
  },
  // Produce sourcemaps for debug builds
  build: {
    target: "es2021",
    minify: "esbuild",
    sourcemap: false,
  },
});

// React DevTools standalone bridge (dev only).
// Spawns the react-devtools window on :8097 and injects the bridge script
// ahead of React so it works in both the browser and the Tauri WebView2.
const DEVTOOLS_PORT = 8097;

function reactDevtools(): PluginOption {
  const enabled = process.env.RDT !== "false";
  let child: ReturnType<typeof spawn> | null = null;

  const waitForPort = (port: number, timeoutMs = 15000) =>
    new Promise<boolean>((resolve) => {
      const start = Date.now();
      const probe = net.createConnection({ port });
      const done = (ok: boolean) => {
        probe.destroy();
        resolve(ok);
      };
      const retry = () => {
        if (Date.now() - start >= timeoutMs) return done(false);
        setTimeout(check, 300);
      };
      const check = () => {
        const c = net.createConnection({ port });
        c.once("connect", () => done(true));
        c.once("error", retry);
      };
      probe.once("connect", () => done(true));
      probe.once("error", retry);
    });

  return {
    name: "react-devtools",
    apply: "serve",
    configureServer(server: ViteDevServer) {
      if (!enabled) return;

      // Resolve the bin entry directly so we don't depend on PATH/.cmd lookup.
      const binPath = fileURLToPath(
        new URL("./node_modules/react-devtools/bin.js", import.meta.url),
      );

      // Spawn immediately; don't block on async checks before launching.
      child = spawn(process.execPath, [binPath], {
        stdio: "inherit",
        windowsHide: false,
      });
      child.on("error", (err) => {
        server.config.logger.error(
          `react-devtools failed to launch: ${err.message}`,
        );
      });

      // Best-effort readiness log (non-blocking).
      waitForPort(DEVTOOLS_PORT).then((ok) =>
        server.config.logger.info(
          ok
            ? "react-devtools ready on :8097"
            : "react-devtools not reachable on :8097 (bridge inactive)",
        ),
      );

      server.httpServer?.on("close", () => {
        try {
          child?.kill();
        } catch {
          /* ignore */
        }
      });
    },
    transformIndexHtml() {
      if (!enabled) return undefined;
      return [
        {
          tag: "script",
          attrs: { src: `http://localhost:${DEVTOOLS_PORT}` },
          injectTo: "head-prepend",
        },
      ];
    },
  };
}
